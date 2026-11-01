// SPDX-License-Identifier: MPL-2.0
// Copyright (c) 2026 Hyperpolymath Estate
//
// The library profile and the estate file profile, at library and CLI level.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

use ijson_jcs::{
    CanonicalProfile, Conformance, classify, estate_file_bytes, estate_file_from_bytes,
    strip_estate_final_lf, to_jcs,
};
use serde_json::json;

/// Path of the binary under test, built by cargo for integration tests.
fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ijson-jcs"))
}

/// Create a fresh file named `name` with `contents` in a unique temp dir.
fn temp_file(name: &str, contents: &[u8]) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "profile-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    fs::write(&path, contents).unwrap();
    path
}

/// Run the binary with `args`, feeding `input` on stdin.
fn run_stdin(args: &[&str], input: &[u8]) -> Output {
    let mut child = bin()
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn ijson-jcs");
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

/// The exit code the CLI documents for a verdict.
fn want_exit(conformance: Conformance) -> i32 {
    match conformance {
        Conformance::Canonical => 0,
        Conformance::NotCanonical => 1,
    }
}

/// The (library, estate-file, tolerant) rules disagree exactly as the policy says.
#[test]
fn profiles_disagree_only_about_the_final_line_feed() {
    // (bytes, library, estate-file, tolerant, description)
    let cases: &[(&[u8], i32, i32, i32, &str)] = &[
        (b"{\"a\":1}", 0, 1, 0, "bare canonical bytes"),
        (b"{\"a\":1}\n", 1, 0, 0, "canonical plus one LF"),
        (b"{\"a\":1}\n\n", 1, 1, 1, "two final LFs"),
        (b"{\"a\":1}\r\n", 1, 1, 1, "CRLF"),
        (b"{\"a\":1} ", 1, 1, 1, "trailing space"),
        (b" {\"a\":1}\n", 1, 1, 1, "leading space"),
        (b"{\"b\":1,\"a\":2}\n", 1, 1, 1, "unsorted members"),
        (b"{\"a\":1,\"a\":2}\n", 2, 2, 2, "duplicate key"),
        (b"{\"a\":\n", 2, 2, 2, "truncated"),
    ];
    for (bytes, library, estate, tolerant, why) in cases {
        let path = temp_file("case.json", bytes);
        for (profile, want) in [
            ("library", *library),
            ("estate-file", *estate),
            ("tolerant", *tolerant),
        ] {
            let out = bin()
                .args(["check", "--profile", profile])
                .arg(&path)
                .output()
                .unwrap();
            assert_eq!(
                out.status.code(),
                Some(want),
                "{why}: check --profile {profile} on {:?}",
                String::from_utf8_lossy(bytes)
            );
        }
        // The default is the historical tolerant rule; nothing changed for
        // consumers that never pass --profile.
        let out = bin().arg("check").arg(&path).output().unwrap();
        assert_eq!(out.status.code(), Some(*tolerant), "{why}: default check");
    }
}

/// The library classifier and the CLI verdicts cannot drift apart.
#[test]
fn library_and_cli_agree_on_every_profile() {
    let inputs: &[&[u8]] = &[
        b"{}",
        b"{}\n",
        b"{ }\n",
        b"[]\n\n",
        b"null",
        b"\"x\"\n",
        b"[1,2,3]\n",
        b"[ 1, 2, 3 ]\n",
        b"{\"b\":1,\"a\":2}\n",
        b"[1e400]\n",
        b"[9007199254740993]\n",
        b"",
        b"{} trailing\n",
    ];
    for input in inputs {
        let path = temp_file("agree.json", input);
        for profile in [
            CanonicalProfile::LibraryBytes,
            CanonicalProfile::EstateFile,
            CanonicalProfile::Tolerant,
        ] {
            let library = classify(input, profile);
            let out = bin()
                .args(["check", "--profile", profile.as_str()])
                .arg(&path)
                .output()
                .unwrap();
            match library {
                Ok(conformance) => assert_eq!(
                    out.status.code(),
                    Some(want_exit(conformance)),
                    "{profile} on {:?}",
                    String::from_utf8_lossy(input)
                ),
                Err(_) => assert_eq!(
                    out.status.code(),
                    Some(2),
                    "{profile} on {:?}: invalid input must exit 2",
                    String::from_utf8_lossy(input)
                ),
            }
        }
    }
}

/// `fix` installs the estate file profile; `canon` prints library bytes.
#[test]
fn fix_writes_the_file_profile_and_canon_writes_library_bytes() {
    let path = temp_file("fix.json", b"{ \"b\" : 1, \"a\" : 2 }");
    assert_eq!(
        bin().arg("fix").arg(&path).output().unwrap().status.code(),
        Some(0)
    );
    assert_eq!(fs::read(&path).unwrap(), b"{\"a\":2,\"b\":1}\n");
    assert_eq!(
        bin()
            .args(["check", "--profile", "estate-file"])
            .arg(&path)
            .output()
            .unwrap()
            .status
            .code(),
        Some(0)
    );

    let out = run_stdin(&["canon"], b"{ \"b\" : 1, \"a\" : 2 }\n");
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(out.stdout, b"{\"a\":2,\"b\":1}");

    // A file holding exactly `canon` output is library-canonical and
    // estate-file-*not*-canonical; that is the whole distinction.
    let bare = temp_file("bare.json", &out.stdout);
    assert_eq!(
        bin()
            .args(["check", "--profile", "library"])
            .arg(&bare)
            .output()
            .unwrap()
            .status
            .code(),
        Some(0)
    );
    assert_eq!(
        bin()
            .args(["check", "--profile", "estate-file"])
            .arg(&bare)
            .output()
            .unwrap()
            .status
            .code(),
        Some(1)
    );

    // Hashing a checked file means hashing the library bytes inside it.
    let file = fs::read(&path).unwrap();
    let stripped = strip_estate_final_lf(&file).expect("estate files end with one LF");
    assert_eq!(stripped, &out.stdout[..]);
    let value = json!({"b": 1, "a": 2});
    assert_eq!(to_jcs(&value).unwrap(), out.stdout);
    assert_eq!(estate_file_bytes(&value).unwrap(), file);
    assert_eq!(estate_file_from_bytes(&file).unwrap(), file);
}

/// `--profile` is refused where it would mean nothing.
#[test]
fn profile_flag_is_refused_outside_check() {
    let path = temp_file("flag.json", b"{}\n");
    for args in [
        &["fix", "--profile", "library"][..],
        &["canon", "--profile", "library"],
        &["jsonld", "--profile", "estate-file"],
        &["check-jsonl", "--profile", "estate-file"],
        &["fix-jsonl", "--profile", "tolerant"],
    ] {
        let mut argv = args.to_vec();
        if !matches!(args[0], "canon" | "jsonld") {
            argv.push(path.to_str().unwrap());
        }
        let out = bin().args(&argv).output().unwrap();
        assert_eq!(out.status.code(), Some(2), "{argv:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("--profile applies to 'check' only"),
            "{argv:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    // A profile name that is not one of the three is a usage error, not a default.
    let out = bin()
        .args(["check", "--profile", "strict"])
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--profile needs"));
    // The aliases the module documents are accepted.
    for alias in ["library-bytes", "file", "check"] {
        let out = bin()
            .args(["check", "--profile", alias])
            .arg(&path)
            .output()
            .unwrap();
        assert_ne!(out.status.code(), Some(2), "--profile {alias}");
    }
}
