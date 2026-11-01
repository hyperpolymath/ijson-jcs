// SPDX-License-Identifier: MPL-2.0
// Copyright (c) 2026 Hyperpolymath Estate
//
// JSON Lines / NDJSON record framing, at library and CLI level.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

use ijson_jcs::{
    Conformance, FramingError, canonicalize_records, classify_records, record_count, records,
};

/// Path of the binary under test, built by cargo for integration tests.
fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ijson-jcs"))
}

/// Create a fresh file named `name` with `contents` in a unique temp dir.
fn temp_file(name: &str, contents: &[u8]) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "records-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    fs::write(&path, contents).unwrap();
    path
}

/// Run `ijson-jcs check-jsonl|fix-jsonl` over `paths`.
fn run_on(command: &str, paths: &[&PathBuf]) -> Output {
    bin().arg(command).args(paths).output().expect("run ijson-jcs")
}

/// The framing matrix: what the estate gate prints, and what the CLI exits with.
#[test]
fn framing_matrix() {
    // (input, check-jsonl exit, gate verdict)
    let cases: &[(&[u8], i32, &str)] = &[
        (b"", 0, "CANONICAL"),
        (b"{\"a\":1}\n{\"b\":[1,2]}\n", 0, "CANONICAL"),
        (b"1\n2\n\"three\"\n", 0, "CANONICAL"),
        (b"{\"a\":1}\n{\"b\":[1,2]}", 1, "NOT_CANONICAL"),
        (b"{\"a\":1}\n{\"z\":1,\"b\":2}\n", 1, "NOT_CANONICAL"),
        (b"{\"a\":1}\n{\"b\": 2}\n", 1, "NOT_CANONICAL"),
        (b"{\"a\":1}\r\n{\"b\":2}\n", 1, "NOT_CANONICAL"),
        (b"{\"a\":1}\n\n{\"b\":2}\n", 2, "INVALID"),
        (b"\n", 2, "INVALID"),
        (b"{\"a\":1}\n{\"b\":\n", 2, "INVALID"),
        (b"{\"a\":1}\n{\"a\":1,\"a\":2}\n", 2, "INVALID"),
        (b"[18446744073709551616]\n", 2, "INVALID"),
    ];
    for (input, want_exit, want_verdict) in cases {
        let shown = String::from_utf8_lossy(input);
        let library = match classify_records(input) {
            Ok(Conformance::Canonical) => "CANONICAL",
            Ok(Conformance::NotCanonical) => "NOT_CANONICAL",
            Err(_) => "INVALID",
        };
        assert_eq!(library, *want_verdict, "library verdict for {:?}", shown);

        let path = temp_file("records.jsonl", input);
        let out = run_on("check-jsonl", &[&path]);
        assert_eq!(out.status.code(), Some(*want_exit), "CLI exit for {:?}", shown);
        assert!(
            String::from_utf8_lossy(&out.stdout).starts_with(match *want_exit {
                0 => "OK ",
                1 => "NOT CANONICAL ",
                _ => "INVALID ",
            }),
            "CLI output for {:?}: {}",
            shown,
            String::from_utf8_lossy(&out.stdout)
        );
    }
}

/// A record error names the line it is about.
#[test]
fn errors_name_the_line() {
    let blank = classify_records(b"{}\n\n{}\n").unwrap_err();
    assert!(matches!(blank, FramingError::BlankLine { line: 2 }));
    assert_eq!(blank.line(), 2);

    let invalid = canonicalize_records(b"{}\n{}\nnot json\n").unwrap_err();
    assert_eq!(invalid.line(), 3);
    assert!(invalid.to_string().starts_with("line 3 is not valid I-JSON"));

    let duplicate = canonicalize_records(b"{\"k\":1}\n{\"k\":1,\"k\":2}\n").unwrap_err();
    assert_eq!(duplicate.line(), 2);
    assert!(duplicate.to_string().contains("duplicate object key"));

    let count = record_count(b"1\n2\n3\n").unwrap();
    assert_eq!(count, 3);
    assert_eq!(records(b"1\n2\n3\n").unwrap().len(), 3);
    // A file without a final LF still has three records; it is a finding,
    // not a framing error.
    assert_eq!(records(b"1\n2\n3").unwrap().len(), 3);
}

/// `fix-jsonl` rewrites canonical records, and a second run changes nothing.
#[test]
fn fix_jsonl_rewrites_and_is_idempotent() {
    let path = temp_file("fix.jsonl", b"{\"b\":1,\"a\":2}\n[ 3, 1 ]\n{\"c\":null}");
    let out = run_on("fix-jsonl", &[&path]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        format!("FIXED {}\n", path.display())
    );
    let once = fs::read(&path).unwrap();
    // Keys sort, values travel with them: `a` (2) before `b` (1).
    assert_eq!(once, b"{\"a\":2,\"b\":1}\n[3,1]\n{\"c\":null}\n");

    let out = run_on("fix-jsonl", &[&path]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        format!("OK {}\n", path.display())
    );
    assert_eq!(fs::read(&path).unwrap(), once);
    assert_eq!(run_on("check-jsonl", &[&path]).status.code(), Some(0));
}

/// `fix-jsonl` refuses a file it would have to change structure to fix.
#[test]
fn fix_jsonl_leaves_invalid_files_untouched() {
    for (name, contents) in [
        ("blank.jsonl", &b"{}\n\n{}\n"[..]),
        ("bad.jsonl", b"{}\n{\"a\":\n"),
        ("dup.jsonl", b"{\"k\":1,\"k\":2}\n"),
    ] {
        let bad = temp_file(name, contents);
        let good = temp_file("good.jsonl", b"{\"z\":1,\"a\":0}\n");
        let out = run_on("fix-jsonl", &[&bad, &good]);
        assert_eq!(out.status.code(), Some(2), "{name}");
        assert_eq!(fs::read(&bad).unwrap(), contents, "{name} must not be rewritten");
        // A valid file next to an invalid one is still processed.
        assert_eq!(fs::read(&good).unwrap(), b"{\"a\":0,\"z\":1}\n", "{name}");
    }
}

/// A record whose canonical form breaks the strict token profile is refused.
#[test]
fn fix_jsonl_refuses_output_outside_the_strict_token_profile() {
    let path = temp_file("float.jsonl", b"1e20\n");
    let out = run_on("fix-jsonl", &[&path]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(fs::read(&path).unwrap(), b"1e20\n");
    assert!(
        String::from_utf8(out.stdout)
            .unwrap()
            .contains("canonical output violates the strict file profile")
    );
    // `canon` still prints the record's canonical bytes; only the file
    // rewrite refuses them, because a later `check` would reject the result.
    let out = canon(b"1e20\n");
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(out.stdout, b"100000000000000000000");
}

/// Run `ijson-jcs canon` with `input` on stdin.
fn canon(input: &[u8]) -> Output {
    let mut child = bin()
        .arg("canon")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

/// Directories, missing files and usage errors are exit 2 for both commands.
#[test]
fn unusable_inputs_are_exit_two() {
    let path = temp_file("dir.jsonl", b"{}\n");
    let directory = path.parent().unwrap().to_path_buf();
    let missing = path.with_file_name("missing.jsonl");
    for command in ["check-jsonl", "fix-jsonl"] {
        assert_eq!(run_on(command, &[&directory]).status.code(), Some(2), "{command} dir");
        assert_eq!(run_on(command, &[&missing]).status.code(), Some(2), "{command} missing");
        assert_eq!(bin().arg(command).output().unwrap().status.code(), Some(2), "{command} no args");
    }
}

#[cfg(unix)]
#[test]
fn fix_jsonl_refuses_links_and_read_only_files() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let original = b"{\"b\":2,\"a\":1}\n";
    let path = temp_file("target.jsonl", original);
    let link = path.with_file_name("symlink.jsonl");
    symlink(&path, &link).unwrap();
    assert_eq!(run_on("fix-jsonl", &[&link]).status.code(), Some(2));
    assert_eq!(fs::read(&path).unwrap(), original);
    let hardlink = path.with_file_name("hardlink.jsonl");
    fs::hard_link(&path, &hardlink).unwrap();
    assert_eq!(run_on("fix-jsonl", &[&path]).status.code(), Some(2));
    assert_eq!(fs::read(&hardlink).unwrap(), original);
    fs::remove_file(hardlink).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o400)).unwrap();
    assert_eq!(run_on("fix-jsonl", &[&path]).status.code(), Some(2));
    assert_eq!(fs::read(&path).unwrap(), original);
}
