// SPDX-License-Identifier: MPL-2.0
// Copyright (c) 2026 Hyperpolymath Estate
//
// `ijson-jcs` command-line tool: check, fix or produce RFC 8785 (JCS)
// canonical JSON, parsing input as I-JSON (RFC 7493).
//
// Arguments are parsed by hand to keep the crate's dependency set unchanged.

use ijson_jcs::{JsonMode, parse_json_bytes, to_jcs};
use std::fs;
use std::io::{self, Read, Write};
use std::process::ExitCode;

/// Exit status: every input was valid and canonical (or was fixed).
const EXIT_OK: u8 = 0;
/// Exit status: `check` found at least one valid but non-canonical file.
const EXIT_NOT_CANONICAL: u8 = 1;
/// Exit status: invalid or unreadable input, a write failure, or a usage error.
const EXIT_INVALID: u8 = 2;

const USAGE: &str = "\
ijson-jcs - I-JSON (RFC 7493) parsing and JCS (RFC 8785) canonicalisation

USAGE:
    ijson-jcs check FILE...    Report whether each FILE is canonical
    ijson-jcs fix FILE...      Rewrite each valid FILE in canonical form
    ijson-jcs canon            Canonicalise stdin to stdout
    ijson-jcs --help           Show this help
    ijson-jcs --version        Show the version

Input is parsed strictly as I-JSON: duplicate object keys, lone surrogates,
noncharacters, invalid UTF-8, numbers outside the IEEE-754 double range and
integers outside +/-(2^53-1) are rejected.

A file counts as canonical if its bytes are exactly the JCS form, optionally
followed by ONE trailing newline (\\n). `fix` writes the JCS form plus one
trailing newline; `canon` writes the JCS form with no trailing newline.

`check` prints one line per file: `OK path`, `NOT CANONICAL path` or
`INVALID path: reason`. `fix` prints `OK path`, `FIXED path` or
`INVALID path: reason`, and leaves invalid files untouched.

EXIT STATUS:
    0  all inputs valid and canonical (check), or all fixed (fix, canon)
    1  check: some input valid but not canonical (and none invalid)
    2  some input invalid or unreadable, a write failed, or a usage error
";

/// Outcome of examining one input's bytes.
enum Verdict {
    /// The bytes are the canonical form, optionally plus one trailing `\n`.
    Canonical,
    /// The input is valid I-JSON but its bytes differ from the JCS form.
    NotCanonical,
    /// The input could not be parsed or is not I-JSON; holds the reason.
    Invalid(String),
}

/// Parse `input` as I-JSON and return its JCS canonical bytes, or a reason.
fn canonical_form(input: &[u8]) -> Result<Vec<u8>, String> {
    let value = parse_json_bytes(input, JsonMode::Canonical).map_err(|e| {
        match e.as_validation_error().map(|v| v.path()) {
            Some(path) if !path.is_empty() => format!("{e} (at {path})"),
            _ => e.to_string(),
        }
    })?;
    to_jcs(&value).map_err(|e| e.to_string())
}

/// Decide whether `input` is canonical, allowing exactly one trailing `\n`.
fn judge(input: &[u8]) -> Verdict {
    match canonical_form(input) {
        Err(reason) => Verdict::Invalid(reason),
        Ok(canon) => {
            let body = input.strip_suffix(b"\n").unwrap_or(input);
            if body == canon.as_slice() {
                Verdict::Canonical
            } else {
                Verdict::NotCanonical
            }
        }
    }
}

/// `check` subcommand: report each file's status and return the exit code.
fn cmd_check(files: &[String]) -> u8 {
    let mut code = EXIT_OK;
    for path in files {
        let verdict = match fs::read(path) {
            Ok(bytes) => judge(&bytes),
            Err(e) => Verdict::Invalid(format!("cannot read: {e}")),
        };
        match verdict {
            Verdict::Canonical => println!("OK {path}"),
            Verdict::NotCanonical => {
                println!("NOT CANONICAL {path}");
                code = code.max(EXIT_NOT_CANONICAL);
            }
            Verdict::Invalid(reason) => {
                println!("INVALID {path}: {reason}");
                code = EXIT_INVALID;
            }
        }
    }
    code
}

/// `fix` subcommand: rewrite valid files as JCS + `\n`, return the exit code.
fn cmd_fix(files: &[String]) -> u8 {
    let mut code = EXIT_OK;
    for path in files {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) => {
                println!("INVALID {path}: cannot read: {e}");
                code = EXIT_INVALID;
                continue;
            }
        };
        let mut wanted = match canonical_form(&bytes) {
            Ok(canon) => canon,
            Err(reason) => {
                println!("INVALID {path}: {reason}");
                code = EXIT_INVALID;
                continue;
            }
        };
        wanted.push(b'\n');
        if wanted == bytes {
            println!("OK {path}");
        } else if let Err(e) = fs::write(path, &wanted) {
            println!("INVALID {path}: cannot write: {e}");
            code = EXIT_INVALID;
        } else {
            println!("FIXED {path}");
        }
    }
    code
}

/// `canon` subcommand: canonicalise stdin to stdout, return the exit code.
fn cmd_canon() -> u8 {
    let mut input = Vec::new();
    if let Err(e) = io::stdin().read_to_end(&mut input) {
        eprintln!("INVALID <stdin>: cannot read: {e}");
        return EXIT_INVALID;
    }
    match canonical_form(&input) {
        Ok(canon) => {
            let mut out = io::stdout().lock();
            if let Err(e) = out.write_all(&canon).and_then(|()| out.flush()) {
                eprintln!("ijson-jcs: cannot write stdout: {e}");
                return EXIT_INVALID;
            }
            EXIT_OK
        }
        Err(reason) => {
            eprintln!("INVALID <stdin>: {reason}");
            EXIT_INVALID
        }
    }
}

/// Print a usage error plus a hint to stderr and return the usage exit code.
fn usage_error(msg: &str) -> u8 {
    eprintln!("ijson-jcs: {msg}\nTry 'ijson-jcs --help' for usage.");
    EXIT_INVALID
}

/// Dispatch the command line (without the program name) to a subcommand.
fn run(args: &[String]) -> u8 {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print!("{USAGE}");
        return EXIT_OK;
    }
    if args.iter().any(|a| a == "-V" || a == "--version") {
        println!("ijson-jcs {}", env!("CARGO_PKG_VERSION"));
        return EXIT_OK;
    }
    let Some((cmd, rest)) = args.split_first() else {
        return usage_error("missing command");
    };
    // `--` ends option parsing so files whose names start with '-' work.
    let files: Vec<String> = match rest.first().map(String::as_str) {
        Some("--") => rest[1..].to_vec(),
        _ => {
            if let Some(opt) = rest.iter().find(|a| a.starts_with('-') && a.len() > 1) {
                return usage_error(&format!("unknown option '{opt}'"));
            }
            rest.to_vec()
        }
    };
    match cmd.as_str() {
        "check" | "fix" if files.is_empty() => {
            usage_error(&format!("'{cmd}' needs at least one FILE"))
        }
        "check" => cmd_check(&files),
        "fix" => cmd_fix(&files),
        "canon" if !files.is_empty() => usage_error("'canon' reads stdin and takes no FILE"),
        "canon" => cmd_canon(),
        other => usage_error(&format!("unknown command '{other}'")),
    }
}

/// Entry point: run the CLI and convert its status to a process exit code.
fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    ExitCode::from(run(&args))
}
