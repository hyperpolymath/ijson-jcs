// SPDX-License-Identifier: MPL-2.0
// Copyright (c) 2026 Hyperpolymath Estate
//
// Integration tests for the `ijson-jcs` binary.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

/// RFC 8785 §3.2.3 sample input (numbers, string escapes, literals).
const RFC_SAMPLE_IN: &str = r#"{
  "numbers": [333333333.33333329, 1E30, 4.50, 2e-3, 0.000000000000000000000000001],
  "string": "\u20ac$\u000F\u000aA'\u0042\u0022\u005c\\\"\/",
  "literals": [null, true, false]
}"#;

/// RFC 8785 §3.2.3 expected canonical output for `RFC_SAMPLE_IN`.
const RFC_SAMPLE_OUT: &str = "{\"literals\":[null,true,false],\"numbers\":[333333333.3333333,1e+30,4.5,0.002,1e-27],\"string\":\"\u{20ac}$\\u000f\\nA'B\\\"\\\\\\\\\\\"/\"}";

/// Path of the binary under test, built by cargo for integration tests.
fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ijson-jcs"))
}

/// Run `ijson-jcs canon` with `input` on stdin and return its output.
fn canon(input: &[u8]) -> Output {
    let mut child = bin()
        .arg("canon")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn ijson-jcs");
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

/// Run `ijson-jcs canon` on `input`, assert success, return stdout as UTF-8.
fn canon_ok(input: &str) -> String {
    let out = canon(input.as_bytes());
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

/// Create a fresh file named `name` with `contents` in a unique temp dir.
fn temp_file(name: &str, contents: &[u8]) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "cli-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    fs::write(&path, contents).unwrap();
    path
}

/// Run `ijson-jcs <cmd> <paths...>` and return its output.
fn run_on(cmd: &str, paths: &[&PathBuf]) -> Output {
    bin().arg(cmd).args(paths).output().expect("run ijson-jcs")
}

/// The RFC 8785 §3.2.3 sample canonicalises to the RFC's expected bytes.
#[test]
fn rfc8785_sample_canonicalises() {
    assert_eq!(canon_ok(RFC_SAMPLE_IN), RFC_SAMPLE_OUT);
}

/// Numbers are written with the ECMAScript algorithm JCS requires.
#[test]
fn number_serialisation() {
    assert_eq!(
        canon_ok("[1e30, 4.50, 0.002, -0, -0.0, 1.0, 1e21, 1e20, 1e-7]"),
        "[1e+30,4.5,0.002,0,0,1,1e+21,100000000000000000000,1e-7]"
    );
}

/// Object keys sort by UTF-16 code units, including a non-BMP key.
#[test]
fn keys_sorted_by_utf16_code_units_including_non_bmp() {
    // RFC 8785 §3.2.3 sorting example. U+1F600 (surrogates D83D DE00) sorts
    // BEFORE U+FB33 in UTF-16 order, although it is after it by code point.
    let input = r#"{
      "\u20ac": "Euro Sign",
      "\r": "Carriage Return",
      "\ufb33": "Hebrew Letter Dalet With Dagesh",
      "1": "One",
      "\ud83d\ude00": "Emoji: Grinning Face",
      "\u0080": "Control",
      "\u00f6": "Latin Small Letter O With Diaeresis"
    }"#;
    let want = "{\"\\r\":\"Carriage Return\",\"1\":\"One\",\"\u{80}\":\"Control\",\
                \"\u{f6}\":\"Latin Small Letter O With Diaeresis\",\"\u{20ac}\":\"Euro Sign\",\
                \"\u{1f600}\":\"Emoji: Grinning Face\",\"\u{fb33}\":\"Hebrew Letter Dalet With Dagesh\"}";
    assert_eq!(canon_ok(input), want);
}

/// `canon` exits 2 with an INVALID message for each class of bad input.
#[test]
fn canon_rejects_invalid_input_with_exit_2() {
    for bad in [
        &br#"{"a":1,"a":2}"#[..],     // duplicate key
        br#"["\ud800"]"#,             // lone surrogate
        br#"[1e400]"#,                // outside double range
        br#"[9007199254740993]"#,     // integer outside +/-(2^53-1)
        br#"[18446744073709551616]"#, // integer beyond u64, read by serde as a float
        b"[\"\xff\"]",                // invalid UTF-8
        br#"{"a":1} trailing"#,       // trailing garbage
        br#"["\uffff"]"#,             // noncharacter
    ] {
        let out = canon(bad);
        assert_eq!(
            out.status.code(),
            Some(2),
            "input {:?}",
            String::from_utf8_lossy(bad)
        );
        assert!(out.stdout.is_empty());
        assert!(String::from_utf8_lossy(&out.stderr).starts_with("INVALID <stdin>:"));
    }
}

/// `check` exits 0 when all OK, 1 when any not canonical, 2 when any invalid.
#[test]
fn check_exit_codes() {
    let ok = temp_file("ok.json", br#"{"a":1,"b":[true]}"#);
    let ok_nl = temp_file("ok-nl.json", b"{\"a\":1}\n");
    let ugly = temp_file("ugly.json", br#"{ "b": 1, "a": 2 }"#);
    let bad = temp_file("bad.json", br#"{"a":1,"a":2}"#);
    let missing = ok.with_file_name("does-not-exist.json");

    let out = run_on("check", &[&ok, &ok_nl]);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(
        stdout,
        format!("OK {}\nOK {}\n", ok.display(), ok_nl.display())
    );

    let out = run_on("check", &[&ok, &ugly]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        String::from_utf8(out.stdout)
            .unwrap()
            .contains(&format!("NOT CANONICAL {}", ugly.display()))
    );

    let out = run_on("check", &[&ugly, &bad, &ok]);
    assert_eq!(out.status.code(), Some(2));
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        stdout.contains(&format!("INVALID {}: ", bad.display())),
        "{stdout}"
    );
    assert!(stdout.contains("duplicate object key"), "{stdout}");

    let out = run_on("check", &[&missing]);
    assert_eq!(out.status.code(), Some(2));
}

/// `check` accepts exactly one optional trailing `\n` and nothing else.
#[test]
fn single_trailing_newline_rule() {
    let cases: &[(&[u8], i32)] = &[
        (b"{\"a\":1}", 0),
        (b"{\"a\":1}\n", 0),
        (b"{\"a\":1}\n\n", 1),
        (b"{\"a\":1}\r\n", 1),
        (b"{\"a\":1} ", 1),
        (b"\n{\"a\":1}", 1),
    ];
    for (bytes, want) in cases {
        let f = temp_file("nl.json", bytes);
        let out = run_on("check", &[&f]);
        assert_eq!(
            out.status.code(),
            Some(*want),
            "input {:?}",
            String::from_utf8_lossy(bytes)
        );
    }
}

/// `fix` writes JCS + `\n`, and a second `fix` changes nothing.
#[test]
fn fix_rewrites_and_is_idempotent() {
    let f = temp_file("fix.json", RFC_SAMPLE_IN.as_bytes());
    let out = run_on("fix", &[&f]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        format!("FIXED {}\n", f.display())
    );
    let first = fs::read(&f).unwrap();
    assert_eq!(first, format!("{RFC_SAMPLE_OUT}\n").into_bytes());

    let out = run_on("fix", &[&f]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        format!("OK {}\n", f.display())
    );
    assert_eq!(fs::read(&f).unwrap(), first);

    assert_eq!(run_on("check", &[&f]).status.code(), Some(0));
}

/// `fix` normalises a bare canonical file to end with one `\n`.
#[test]
fn fix_adds_newline_to_bare_canonical_form() {
    let f = temp_file("bare.json", b"[1,2]");
    assert_eq!(run_on("fix", &[&f]).status.code(), Some(0));
    assert_eq!(fs::read(&f).unwrap(), b"[1,2]\n");
}

/// `fix` exits 2 on invalid input, leaving it untouched but fixing others.
#[test]
fn fix_leaves_invalid_files_untouched() {
    let bad_bytes = br#"{ "k": 1, "k": 2 }"#;
    let bad = temp_file("dup.json", bad_bytes);
    let good = temp_file("good.json", br#"{"z":1,"a":0}"#);
    let out = run_on("fix", &[&bad, &good]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(fs::read(&bad).unwrap(), bad_bytes);
    // Valid files given alongside an invalid one are still fixed.
    assert_eq!(fs::read(&good).unwrap(), b"{\"a\":0,\"z\":1}\n");
}

/// `canon` output carries no trailing newline.
#[test]
fn canon_has_no_trailing_newline() {
    assert_eq!(canon_ok("{\"a\" : [ 1 ] }\n"), r#"{"a":[1]}"#);
}

/// `--help` exits 0; usage errors exit 2 with a message on stderr.
#[test]
fn help_and_usage_errors() {
    let out = bin().arg("--help").output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let help = String::from_utf8(out.stdout).unwrap();
    assert!(help.contains("ijson-jcs check FILE..."));
    assert!(help.contains("EXIT STATUS"));

    for args in [
        &[][..],
        &["bogus"],
        &["check"],
        &["fix"],
        &["canon", "x.json"],
        &["check", "--nope"],
    ] {
        let out = bin().args(args).output().unwrap();
        assert_eq!(out.status.code(), Some(2), "args {args:?}");
        assert!(!out.stderr.is_empty());
    }
}
