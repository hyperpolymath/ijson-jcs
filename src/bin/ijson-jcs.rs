// SPDX-License-Identifier: MPL-2.0
// Copyright (c) 2026 Hyperpolymath Estate
//
// I-JSON/JCS file operations. No shell, network, or implicit format migration.

#![forbid(unsafe_code)]

use ijson_jcs::{
    CanonicalProfile, IJsonError, JsonMode, canonicalize_jsonld_document, canonicalize_records,
    classify, classify_records, parse_json_bytes, to_jcs,
};
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicU64, Ordering};

const EXIT_OK: u8 = 0;
const EXIT_NOT_CANONICAL: u8 = 1;
const EXIT_INVALID: u8 = 2;
const DEFAULT_MAX_BYTES: u64 = 64 * 1024 * 1024;

const USAGE: &str = "\
ijson-jcs - I-JSON (RFC 7493) parsing and JCS (RFC 8785) canonicalisation

USAGE:
    ijson-jcs check FILE...    Report whether each FILE is canonical
    ijson-jcs fix FILE...      Atomically replace valid regular files
    ijson-jcs canon            Canonicalise stdin to stdout
    ijson-jcs check-jsonl FILE...  Report whether each JSON Lines FILE is canonical
    ijson-jcs fix-jsonl FILE...    Rewrite each JSON Lines FILE, one record per line
    ijson-jcs jsonld           Canonicalise a JSON-LD document on stdin
    ijson-jcs --help           Show this help
    ijson-jcs --version        Show the version

OPTIONS (before --):
    --max-bytes N              Input limit per file / stdin (default 67108864)
    --profile P                check only: tolerant, library or estate-file
    --                        Treat all remaining arguments as file names

PROFILES (check only):
    tolerant      canonical bytes, optionally followed by one newline (default)
    library       canonical bytes exactly, no trailing newline (what you hash)
    estate-file   canonical bytes and exactly one final newline (JSON-POLICY J-1)

Input is parsed strictly as I-JSON: duplicate object keys, lone surrogates,
noncharacters, invalid UTF-8, numbers outside the IEEE-754 double range and
integers outside +/-(2^53-1) are rejected.

check accepts the JCS form optionally followed by ONE newline (\\n), so it is
not the whole estate on-disk rule: pass --profile estate-file when the policy's
exactly-one-final-LF requirement is what you mean to enforce.
fix writes exactly one final newline; canon writes none.
check-jsonl and fix-jsonl apply the same rule to one record per line: no blank
lines, one record per line, exactly one final newline. jsonld canonicalises a
JSON-LD *document* only: contexts are never fetched, expanded or compacted, and
this is not RDF dataset canonicalisation.
fix refuses symlinks, hard-linked files (Unix), read-only files and inputs
whose JCS output would be rejected by the strict integer-token profile.

EXIT STATUS:
    0  all inputs valid and canonical (check), or all fixed (fix, canon, jsonld)
    1  check: some input valid but not canonical (and none invalid)
    2  invalid/unreadable input, resource limit, write failure, or usage error
";

/// Render a parse/validation failure the way this CLI has always printed it.
fn describe(error: &IJsonError) -> String {
    match error.as_validation_error().map(|validation| validation.path()) {
        Some(path) if !path.is_empty() => format!("{error} (at {path})"),
        _ => error.to_string(),
    }
}

/// Parse strictly and render JCS bytes, including the legacy error path.
fn canonical_form(input: &[u8]) -> Result<Vec<u8>, String> {
    let value = parse_json_bytes(input, JsonMode::Canonical).map_err(|e| describe(&e))?;
    to_jcs(&value).map_err(|e| e.to_string())
}

/// Canonicalise a JSON-LD document, keeping the CLI's error wording.
fn jsonld_form(input: &[u8]) -> Result<Vec<u8>, String> {
    canonicalize_jsonld_document(input).map_err(|e| describe(&e))
}

/// Read at most one byte beyond the limit, including from an unbounded pipe.
fn read_bounded(reader: impl Read, limit: u64) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(io::Error::other(format!("input exceeds --max-bytes {limit}")));
    }
    Ok(bytes)
}

/// Non-regular files (e.g. FIFOs) must not make a file check hang indefinitely.
fn read_file(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    if !fs::metadata(path)?.is_file() {
        return Err(io::Error::other("not a regular file"));
    }
    read_bounded(File::open(path)?, limit)
}

/// Remove an uncommitted temporary file on every ordinary failure path.
struct PendingFile(PathBuf);

impl Drop for PendingFile {
    fn drop(&mut self) {
        // After a successful rename this path no longer exists. Cleanup is
        // best-effort: it must not hide the primary write/rename error.
        let _ = fs::remove_file(&self.0);
    }
}

/// Replace in the same directory only after a complete, synced write.
///
/// This is atomic replacement, not a lock against concurrent/malicious writers.
/// Ownership, ACLs and extended attributes are not copied; permission bits are.
fn atomic_replace(path: &Path, bytes: &[u8], permissions: fs::Permissions) -> io::Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    for _ in 0..128 {
        let temp = parent.join(format!(
            ".ijson-jcs.{}.{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = match options.open(&temp) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        };
        let pending = PendingFile(temp);
        file.write_all(bytes)?;
        file.set_permissions(permissions)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&pending.0, path)?;
        return Ok(());
    }
    Err(io::Error::other("could not create a unique temporary file"))
}

/// Check the mutation preconditions before opening or replacing the target.
fn fix_permissions(path: &Path) -> io::Result<fs::Permissions> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() {
        return Err(io::Error::other("fix requires a regular file, not a symlink or device"));
    }
    if metadata.permissions().readonly() {
        return Err(io::Error::other("fix refuses a read-only file"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err(io::Error::other("fix refuses a hard-linked file"));
        }
    }
    Ok(metadata.permissions())
}

/// Raise the exit code to the worst verdict seen so far.
fn record_verdict(verdict: &str, code: &mut u8) {
    if verdict == "NOT CANONICAL" {
        *code = (*code).max(EXIT_NOT_CANONICAL);
    }
}

/// Process single-document files independently; one bad file does not stop the rest.
fn files_command(
    files: &[PathBuf],
    fix: bool,
    limit: u64,
    profile: CanonicalProfile,
    out: &mut impl Write,
) -> io::Result<u8> {
    let mut code = EXIT_OK;
    for path in files {
        let result = (|| -> Result<&str, String> {
            let permissions = if fix {
                Some(fix_permissions(path).map_err(|e| e.to_string())?)
            } else {
                None
            };
            let bytes = read_file(path, limit).map_err(|e| format!("cannot read: {e}"))?;
            if !fix {
                let verdict = classify(&bytes, profile).map_err(|e| describe(&e))?;
                return Ok(if verdict.is_canonical() { "OK" } else { "NOT CANONICAL" });
            }
            let mut wanted = canonical_form(&bytes)?;
            // A finite float can render as a bare integer beyond 2^53-1.
            // Retain the existing strict token policy, but never replace a
            // valid input with a file that our own checker would reject.
            canonical_form(&wanted).map_err(|e| format!(
                "canonical output violates the strict file profile: {e}; use a string for exact integers, or canon for JCS bytes"
            ))?;
            wanted.push(b'\n');
            if wanted == bytes {
                return Ok("OK");
            }
            if let Some(permissions) = permissions {
                atomic_replace(path, &wanted, permissions).map_err(|e| format!("cannot replace: {e}"))?;
            }
            Ok("FIXED")
        })();
        match result {
            Ok(verdict) => {
                writeln!(out, "{verdict} {}", path.display())?;
                record_verdict(verdict, &mut code);
            }
            Err(reason) => {
                writeln!(out, "INVALID {}: {reason}", path.display())?;
                code = EXIT_INVALID;
            }
        }
    }
    Ok(code)
}

/// Process JSON Lines files independently, one record per line.
fn records_command(
    files: &[PathBuf],
    fix: bool,
    limit: u64,
    out: &mut impl Write,
) -> io::Result<u8> {
    let mut code = EXIT_OK;
    for path in files {
        let result = (|| -> Result<&str, String> {
            let permissions = if fix {
                Some(fix_permissions(path).map_err(|e| e.to_string())?)
            } else {
                None
            };
            let bytes = read_file(path, limit).map_err(|e| format!("cannot read: {e}"))?;
            if !fix {
                let verdict = classify_records(&bytes).map_err(|e| e.to_string())?;
                return Ok(if verdict.is_canonical() { "OK" } else { "NOT CANONICAL" });
            }
            let wanted = canonicalize_records(&bytes).map_err(|e| e.to_string())?;
            // Same guard as `fix`: never install bytes the strict token profile
            // would refuse to read back.
            canonicalize_records(&wanted).map_err(|e| format!(
                "canonical output violates the strict file profile: {e}; use a string for exact integers, or canon for JCS bytes"
            ))?;
            if wanted == bytes {
                return Ok("OK");
            }
            if let Some(permissions) = permissions {
                atomic_replace(path, &wanted, permissions).map_err(|e| format!("cannot replace: {e}"))?;
            }
            Ok("FIXED")
        })();
        match result {
            Ok(verdict) => {
                writeln!(out, "{verdict} {}", path.display())?;
                record_verdict(verdict, &mut code);
            }
            Err(reason) => {
                writeln!(out, "INVALID {}: {reason}", path.display())?;
                code = EXIT_INVALID;
            }
        }
    }
    Ok(code)
}

/// Canonicalise all of stdin to stdout: `canon` (JCS bytes) and `jsonld`.
fn stdin_command(
    limit: u64,
    out: &mut impl Write,
    canonicalise: fn(&[u8]) -> Result<Vec<u8>, String>,
) -> io::Result<u8> {
    let result = read_bounded(io::stdin().lock(), limit)
        .map_err(|e| format!("cannot read: {e}"))
        .and_then(|bytes| canonicalise(&bytes));
    match result {
        Ok(bytes) => {
            out.write_all(&bytes)?;
            Ok(EXIT_OK)
        }
        Err(reason) => {
            eprintln!("INVALID <stdin>: {reason}");
            Ok(EXIT_INVALID)
        }
    }
}

/// Print usage failures to stderr; never misclassify them as validation success.
fn usage_error(message: &str) -> u8 {
    eprintln!("ijson-jcs: {message}\nTry 'ijson-jcs --help' for usage.");
    EXIT_INVALID
}

/// Parse OS strings so non-UTF-8 file names work, respecting the `--` boundary.
fn run(args: &[OsString], out: &mut impl Write) -> io::Result<u8> {
    let mut positional = Vec::new();
    let mut options = true;
    let mut limit = DEFAULT_MAX_BYTES;
    let mut profile = CanonicalProfile::default();
    let mut profile_given = false;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if options && arg == "--" {
            options = false;
        } else if options && (arg == "--help" || arg == "-h") {
            write!(out, "{USAGE}")?;
            return Ok(EXIT_OK);
        } else if options && (arg == "--version" || arg == "-V") {
            writeln!(out, "ijson-jcs {}", env!("CARGO_PKG_VERSION"))?;
            return Ok(EXIT_OK);
        } else if options && arg == "--max-bytes" {
            match args.next().and_then(|v| v.to_str()).and_then(|v| v.parse::<u64>().ok()) {
                Some(n) if n > 0 && n < usize::MAX as u64 => limit = n,
                _ => return Ok(usage_error("--max-bytes needs a positive, addressable byte count")),
            }
        } else if options && arg == "--profile" {
            match args.next().and_then(|v| v.to_str()).and_then(CanonicalProfile::from_name) {
                Some(value) => {
                    profile = value;
                    profile_given = true;
                }
                None => {
                    return Ok(usage_error("--profile needs tolerant, library or estate-file"));
                }
            }
        } else if options && arg.to_string_lossy().starts_with('-') && arg != "-" {
            return Ok(usage_error(&format!("unknown option '{}'", arg.to_string_lossy())));
        } else {
            positional.push(arg);
        }
    }
    let Some((command, files)) = positional.split_first() else {
        return Ok(usage_error("missing command"));
    };
    let files: Vec<PathBuf> = files.iter().map(|path| PathBuf::from(*path)).collect();
    let command_name = command.to_str();
    if profile_given && command_name != Some("check") {
        return Ok(usage_error("--profile applies to 'check' only"));
    }
    match command_name {
        Some("check" | "fix") if files.is_empty() => Ok(usage_error("check/fix needs at least one FILE")),
        Some("check" | "fix") => files_command(&files, command_name == Some("fix"), limit, profile, out),
        Some("check-jsonl" | "fix-jsonl") if files.is_empty() => {
            Ok(usage_error("check-jsonl/fix-jsonl needs at least one FILE"))
        }
        Some("check-jsonl" | "fix-jsonl") => {
            records_command(&files, command_name == Some("fix-jsonl"), limit, out)
        }
        Some("canon") if !files.is_empty() => Ok(usage_error("'canon' reads stdin and takes no FILE")),
        Some("canon") => stdin_command(limit, out, canonical_form),
        Some("jsonld") if !files.is_empty() => Ok(usage_error("'jsonld' reads stdin and takes no FILE")),
        Some("jsonld") => stdin_command(limit, out, jsonld_form),
        _ => Ok(usage_error(&format!("unknown command '{}'", command.to_string_lossy()))),
    }
}

/// Convert output errors (including broken pipes) into the documented exit 2.
fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let mut out = io::stdout().lock();
    match run(&args, &mut out).and_then(|code| out.flush().map(|()| code)) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("ijson-jcs: cannot write stdout: {error}");
            ExitCode::from(EXIT_INVALID)
        }
    }
}
