// SPDX-License-Identifier: MPL-2.0 AND CC-BY-SA-4.0
// Code is MPL-2.0; prose is CC-BY-SA-4.0.
//! Counted conformance evidence (kind C): vectors whose expected outputs are
//! pinned by SHA-256. Each vector also states its exit status and a phrase
//! it must contain, so a re-pin cannot silently accept a wrong verdict.
//!
//! To re-pin after an intended change, run with IJSON_JCS_PRINT_DIGESTS=1:
//! the run prints every digest and then fails, so it can never pass that way.

mod support;

use ijson_jcs::{JsonMode, Value, parse_json_bytes, to_jcs, to_jcs_string};
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use support::sha256;

fn printing() -> bool {
    std::env::var_os("IJSON_JCS_PRINT_DIGESTS").is_some()
}

/// Compare or print one pinned digest; returns whether it matched.
fn pinned(name: &str, actual: &str, want: &str, report: &mut Vec<String>) {
    if printing() {
        report.push(format!("{name}\t{actual}"));
    } else if actual != want {
        report.push(format!("{name}: digest {actual}, pinned {want}"));
    }
}

fn finish(report: Vec<String>) {
    assert!(
        report.is_empty() && !printing(),
        "\n{}\n(print mode never passes)",
        report.join("\n")
    );
}

#[test]
fn sha256_matches_fips_180_4_examples() {
    let mut million = Vec::new();
    million.resize(1_000_000, b'a');
    for (input, want) in [
        (
            &b""[..],
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ),
        (
            &b"abc"[..],
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ),
        (
            &b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"[..],
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
        ),
        (
            &million[..],
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0",
        ),
    ] {
        assert_eq!(sha256::hex(input), want);
    }
}

/// RFC 8785 section 3.2.3 sample and Appendix B binary64 vectors, as one corpus.
#[test]
fn rfc8785_corpus_is_pinned() {
    let sample_in = br#"{
  "numbers": [333333333.33333329, 1E30, 4.50, 2e-3, 0.000000000000000000000000001],
  "string": "\u20ac$\u000F\u000aA'\u0042\u0022\u005c\\\"\/",
  "literals": [null, true, false]
}"#;
    let sample_out = "{\"literals\":[null,true,false],\"numbers\":[333333333.3333333,1e+30,4.5,0.002,1e-27],\"string\":\"\u{20ac}$\\u000f\\nA'B\\\"\\\\\\\\\\\"/\"}";
    let value = parse_json_bytes(sample_in, JsonMode::Canonical).unwrap();
    let mut corpus = to_jcs(&value).unwrap();
    assert_eq!(corpus, sample_out.as_bytes());
    for bits in [
        0x0000000000000000u64,
        0x8000000000000000,
        0x0000000000000001,
        0x8000000000000001,
        0x7fefffffffffffff,
        0xffefffffffffffff,
        0x4340000000000000,
        0xc340000000000000,
        0x4430000000000000,
        0x44b52d02c7e14af5,
        0x44b52d02c7e14af6,
        0x44b52d02c7e14af7,
        0x444b1ae4d6e2ef4e,
        0x444b1ae4d6e2ef4f,
        0x444b1ae4d6e2ef50,
        0x3eb0c6f7a0b5ed8c,
        0x3eb0c6f7a0b5ed8d,
        0x41b3de4355555553,
        0x41b3de4355555554,
        0x41b3de4355555555,
        0x41b3de4355555556,
        0x41b3de4355555557,
        0xbecbf647612f3696,
        0x43143ff3c1cb0959,
    ] {
        let n = serde_json::Number::from_f64(f64::from_bits(bits)).unwrap();
        corpus.push(b'\n');
        corpus.extend(format!("{bits:016x} ").bytes());
        corpus.extend(to_jcs_string(&Value::Number(n)).unwrap().bytes());
    }
    let mut report = Vec::new();
    pinned(
        "rfc8785-corpus",
        &sha256::hex(&corpus),
        "e7484f9ca175da3ba3831a84fd11d23d53d71613dca6dc99936779a66bef3f39",
        &mut report,
    );
    finish(report);
}

/// Library verdicts in all three modes for inputs at each defect boundary.
#[test]
fn library_vectors_are_pinned() {
    let deep_ok = format!("{}{}", "[".repeat(128), "]".repeat(128));
    let deep_bad = format!("{}{}", "[".repeat(129), "]".repeat(129));
    let vectors: Vec<(&str, Vec<u8>, &str)> = vec![
        (
            "empty-object",
            b"{}".to_vec(),
            "90345a3bb86e7977e5b0b48597def02365dfe88156b936e499831a665230eefd",
        ),
        (
            "unsorted",
            br#"{"b":1,"a":[true,false,null]}"#.to_vec(),
            "53f53b3ba89a1247146574f4ef5e236c11163aa394db166670b9d98041f9b56e",
        ),
        (
            "depth-128",
            deep_ok.into_bytes(),
            "9a56b5a7716faf33afc3447d06353b39eb23a28fd5fe6eaa2c26cc5a00464a62",
        ),
        (
            "depth-129",
            deep_bad.into_bytes(),
            "9a56b5a7716faf33afc3447d06353b39eb23a28fd5fe6eaa2c26cc5a00464a62",
        ),
        (
            "duplicate-nested",
            br#"{"x":[{"a":1,"a":2}]}"#.to_vec(),
            "ce2c5c71d4e9ee09376ba9fcf264969bc63c8f4a3aeeec537b4b0b58e64b5aaf",
        ),
        (
            "duplicate-escaped",
            br#"{"a":1,"\u0061":2}"#.to_vec(),
            "40d46aee8dc27627dcd708cda92bcee6f59b3175047a73c94af1bd69799664aa",
        ),
        (
            "lone-surrogate",
            br#"["\ud800"]"#.to_vec(),
            "01d99dfe2adcaa48091a1d17f7d503eb39010fb74bed93822cce1d66749fc4a2",
        ),
        (
            "noncharacter",
            br#"["\ufdd0"]"#.to_vec(),
            "09af83d21947a80a04e83657f93c93cb8c04f179eec3afeb036b7921b18bcdec",
        ),
        (
            "raw-control",
            b"[\"\x01\"]".to_vec(),
            "f634d779aa8edcd8a86c1593bf232d7a49fdab5547ebf559654e800592d7d632",
        ),
        (
            "int-2^53",
            b"[9007199254740992]".to_vec(),
            "24757b771d445ca66ef992e28e4c1ed1ddcd0f990c9c9bdefe79fbb614e6bc92",
        ),
        (
            "int-2^64",
            br#"{"k":[18446744073709551616]}"#.to_vec(),
            "5994c57c0db4ad8cf2f31b60a7162f0a0201a9d1c61080ff7ac593df81fcf398",
        ),
        (
            "neg-2^53",
            b"[-9007199254740992]".to_vec(),
            "dff4985ee2715bb98d60b10aabd5f0e2d37b50499c7e20f358e3433b61905bfe",
        ),
        (
            "float-1e400",
            b"[1e400]".to_vec(),
            "aaf6344972643e7c07583b02bcadd94308cbf5fd4e0e77f4bd129c1ab5ecb786",
        ),
        (
            "float-1e16",
            b"[1e16]".to_vec(),
            "6bcc4ca0c45c50a2b7fb3d99477b443e541cfe40d356da3c41aefe507010cf19",
        ),
        (
            "top-scalar",
            b"\"s\"".to_vec(),
            "68339bf1bac797e27ef62ebd819c4d7b04c86a387f4eb6d45879c5a89d74f09f",
        ),
        (
            "trailing",
            b"[] x".to_vec(),
            "a8f8d68f7cedafe80c1cbd150d713c67db8f9680bcc353be56e2f8cc09a01add",
        ),
        (
            "not-utf8",
            b"[\"\xff\"]".to_vec(),
            "6960846194c1e70b11d7af496fc6ca126488197a93400eae01eded3a90517cf5",
        ),
    ];
    let mut report = Vec::new();
    for (name, input, want) in vectors {
        let mut transcript = String::new();
        for mode in [JsonMode::Loose, JsonMode::Strict, JsonMode::Canonical] {
            let line = match parse_json_bytes(&input, mode) {
                Ok(v) => match to_jcs_string(&v) {
                    Ok(s) => format!("{mode}: ok {s}"),
                    Err(e) => format!("{mode}: parsed, jcs refused: {e}"),
                },
                Err(e) => {
                    let path = e.as_validation_error().map(|v| v.path().to_string());
                    format!("{mode}: err {e} path={path:?}")
                }
            };
            transcript.push_str(&line);
            transcript.push('\n');
        }
        pinned(name, &sha256::hex(transcript.as_bytes()), want, &mut report);
    }
    finish(report);
}

struct Cli {
    name: &'static str,
    args: &'static [&'static str],
    stdin: &'static [u8],
    prep: fn(&Path),
    exit: i32,
    phrase: &'static str,
    digest: &'static str,
}

fn none(_: &Path) {}
fn write(dir: &Path, name: &str, bytes: &[u8]) {
    fs::write(dir.join(name), bytes).unwrap();
}
fn canonical_doc(d: &Path) {
    write(d, "a.json", b"{\"a\":1}\n");
}
fn messy_doc(d: &Path) {
    write(d, "a.json", b"{ \"b\": 2, \"a\": 1 }");
}
fn mixed_docs(d: &Path) {
    write(d, "ok.json", b"{\"a\":1}");
    write(d, "messy.json", b"[1, 2]");
    write(d, "bad.json", b"{\"a\":1,\"a\":2}");
    fs::create_dir(d.join("dir.json")).unwrap();
}
fn odd_names(d: &Path) {
    write(d, "-x.json", b"{}\n");
    write(d, "-", b"[ ]");
}
fn float_doc(d: &Path) {
    write(d, "a.json", b"[1e16]");
}
fn invalid_doc(d: &Path) {
    write(d, "a.json", b"{\"a\":");
}
#[cfg(unix)]
fn symlinked(d: &Path) {
    canonical_doc(d);
    std::os::unix::fs::symlink("a.json", d.join("link.json")).unwrap();
}
#[cfg(unix)]
fn hardlinked(d: &Path) {
    messy_doc(d);
    fs::hard_link(d.join("a.json"), d.join("b.json")).unwrap();
}
fn read_only(d: &Path) {
    messy_doc(d);
    let path = d.join("a.json");
    let mut perms = fs::metadata(&path).unwrap().permissions();
    perms.set_readonly(true);
    fs::set_permissions(path, perms).unwrap();
}
fn jsonl_canonical(d: &Path) {
    write(d, "r.jsonl", b"{\"a\":1}\n[]\n");
}
fn jsonl_messy(d: &Path) {
    write(d, "r.jsonl", b"{\"b\":1, \"a\":2}\r\n[ ]");
}
fn jsonl_blank(d: &Path) {
    write(d, "r.jsonl", b"{}\n\n[]\n");
}
fn jsonl_float(d: &Path) {
    write(d, "r.jsonl", b"{}\n[1e16]\n");
}
#[cfg(unix)]
fn jsonl_symlinked(d: &Path) {
    jsonl_messy(d);
    std::os::unix::fs::symlink("r.jsonl", d.join("link.jsonl")).unwrap();
}

const RFC_IN: &[u8] = br#"{"numbers":[333333333.33333329,1E30,4.50,2e-3,0.000000000000000000000000001],"literals":[null,true,false]}"#;

fn cli_vectors() -> Vec<Cli> {
    macro_rules! v {
        ($name:literal, [$($a:literal),*], $stdin:expr, $prep:expr, $exit:literal, $phrase:literal, $digest:literal) => {
            Cli { name: $name, args: &[$($a),*], stdin: $stdin, prep: $prep, exit: $exit,
                  phrase: $phrase, digest: $digest }
        };
    }
    vec![
        v!(
            "help",
            ["--help"],
            b"",
            none,
            0,
            "USAGE:",
            "8922627e819d5effc2578efeaa9557b8d45eb5c2b8f333bf76c5c36ae259e934"
        ),
        v!(
            "help-short",
            ["-h"],
            b"",
            none,
            0,
            "EXIT STATUS:",
            "8922627e819d5effc2578efeaa9557b8d45eb5c2b8f333bf76c5c36ae259e934"
        ),
        v!(
            "version",
            ["--version"],
            b"",
            none,
            0,
            "ijson-jcs ",
            "d76dbe3ca903bb1ee2962416637cfb07d3669452d548a7fa0a30333010de4548"
        ),
        v!(
            "version-short",
            ["-V"],
            b"",
            none,
            0,
            "ijson-jcs ",
            "d76dbe3ca903bb1ee2962416637cfb07d3669452d548a7fa0a30333010de4548"
        ),
        v!(
            "no-command",
            [],
            b"",
            none,
            2,
            "missing command",
            "6c34ab3dfc4134664ce714bea9c2ad1fc80eb72e5d52131050b8877a05118ccd"
        ),
        v!(
            "max-bytes-zero",
            ["--max-bytes", "0", "canon"],
            b"{}",
            none,
            2,
            "--max-bytes needs",
            "f82b4fd2049c7ca7c14c4439fecaae799481f479ddd8440df4f0d9848163e24d"
        ),
        v!(
            "max-bytes-text",
            ["--max-bytes", "x", "canon"],
            b"{}",
            none,
            2,
            "--max-bytes needs",
            "f82b4fd2049c7ca7c14c4439fecaae799481f479ddd8440df4f0d9848163e24d"
        ),
        v!(
            "max-bytes-missing",
            ["--max-bytes"],
            b"",
            none,
            2,
            "--max-bytes needs",
            "f82b4fd2049c7ca7c14c4439fecaae799481f479ddd8440df4f0d9848163e24d"
        ),
        v!(
            "max-bytes-exceeded",
            ["--max-bytes", "4", "canon"],
            b"{\"a\":1}",
            none,
            2,
            "exceeds --max-bytes 4",
            "6d2b4f2f01491f48b36d8864e6e29a08077957beb1d9eeb0e6e90dd3f83e1179"
        ),
        v!(
            "max-bytes-fits",
            ["--max-bytes", "64", "canon"],
            b"{\"b\":1,\"a\":2}",
            none,
            0,
            "{\"a\":2,\"b\":1}",
            "d223505f50d06a1894a3dc68912143fb0c93d6aa0998c84c6fc10e8814e43888"
        ),
        v!(
            "profile-unknown",
            ["--profile", "x", "check", "a.json"],
            b"",
            canonical_doc,
            2,
            "--profile needs",
            "b563dc8921f4210806ef3e1f680af5ea86a907217ef3ef2783f1be36b8fbcc39"
        ),
        v!(
            "profile-missing",
            ["--profile"],
            b"",
            none,
            2,
            "--profile needs",
            "ee291664c81289182b07940424759495c9288b120dc84649143aedb5fb27a37e"
        ),
        v!(
            "profile-not-check",
            ["--profile", "library", "fix", "a.json"],
            b"",
            canonical_doc,
            2,
            "'check' only",
            "d188b05cf9ca9b305bd1f9e4f56fdac5e5f7a339e5d0b964e71bcb04dc6a79e0"
        ),
        v!(
            "unknown-option",
            ["-x"],
            b"",
            none,
            2,
            "unknown option '-x'",
            "b0b2af353bd418e2338497fd62dde996d23b958a89d44e24a855e39fe9b6a5d5"
        ),
        v!(
            "check-no-file",
            ["check"],
            b"",
            none,
            2,
            "at least one FILE",
            "d4b9c7d3a48531972ff0f48f1c776a6f0962443d7fef042bac1c7cef90cd5db3"
        ),
        v!(
            "fix-no-file",
            ["fix"],
            b"",
            none,
            2,
            "at least one FILE",
            "d4b9c7d3a48531972ff0f48f1c776a6f0962443d7fef042bac1c7cef90cd5db3"
        ),
        v!(
            "check-jsonl-no-file",
            ["check-jsonl"],
            b"",
            none,
            2,
            "at least one FILE",
            "822f7cb0141f486dc6c8bb7c17465c3ed2a81b602a25a43b2b1fb74f7a6d32e2"
        ),
        v!(
            "fix-jsonl-no-file",
            ["fix-jsonl"],
            b"",
            none,
            2,
            "at least one FILE",
            "822f7cb0141f486dc6c8bb7c17465c3ed2a81b602a25a43b2b1fb74f7a6d32e2"
        ),
        v!(
            "canon-with-file",
            ["canon", "a.json"],
            b"",
            none,
            2,
            "takes no FILE",
            "3fcc4e4c4d8115a158f74a521ada5eb4ce78544db8f663a9a7f0c40e935a4848"
        ),
        v!(
            "jsonld-with-file",
            ["jsonld", "a.json"],
            b"",
            none,
            2,
            "takes no FILE",
            "7ff02ca4fb594fff30f8296076464c04785623359cef6371e3e70b768d2fadab"
        ),
        v!(
            "unknown-command",
            ["frob"],
            b"",
            none,
            2,
            "unknown command 'frob'",
            "97c74a1d7fde58e4955f8fb6d692be12cfcbca168c6627ab7d78afe106b15199"
        ),
        v!(
            "canon-rfc8785",
            ["canon"],
            RFC_IN,
            none,
            0,
            "333333333.3333333,1e+30,4.5,0.002,1e-27",
            "26aa2ab3297cab3be171b45cca5a0f3f723a954221023d041d6f3c72ec3a8f27"
        ),
        v!(
            "canon-duplicate-path",
            ["canon"],
            b"{\"x\":[{\"a\":1,\"a\":2}]}",
            none,
            2,
            "INVALID <stdin>",
            "223aa988e4499d0bb2f60f3cbe448191bc8748b815c26acab86374b013f922ee"
        ),
        v!(
            "canon-unsafe-path",
            ["canon"],
            b"{\"k\":[9007199254740992]}",
            none,
            2,
            "(at k/0)",
            "012406fa209cbbd638957cfb321b7cda8b291cb3ee1b4daaa2396c1c88d8763f"
        ),
        v!(
            "canon-not-utf8",
            ["canon"],
            b"[\"\xff\"]",
            none,
            2,
            "INVALID <stdin>",
            "19557dd2b8f310ce324f8ac53a1617331ddb44227f35152b2f8d99f3fe669cbd"
        ),
        v!(
            "jsonld-document",
            ["jsonld"],
            b"{\"@context\":\"urn:x\",\"b\":1,\"a\":2}",
            none,
            0,
            "{\"@context\":\"urn:x\",\"a\":2,\"b\":1}",
            "c2a978d27c0d75fca0ac7790b49b1f40fb2e870ee9e997b4d836f7d059a64304"
        ),
        v!(
            "jsonld-invalid",
            ["jsonld"],
            b"{\"@id\": }",
            none,
            2,
            "INVALID <stdin>",
            "a8ee337051ddd901bfce772f0cad4fadf9e4b4c1fbf5087e41cbaf8247dbe8d4"
        ),
        v!(
            "check-canonical",
            ["check", "a.json"],
            b"",
            canonical_doc,
            0,
            "OK a.json",
            "bbd948aa713fc127792e6f78d22eb7264ead169250e0af14704efbaebe1fb0d3"
        ),
        v!(
            "check-library",
            ["--profile", "library", "check", "a.json"],
            b"",
            canonical_doc,
            1,
            "NOT CANONICAL a.json",
            "c7478b52421577e569547d5b11492690dbda01c13517e409308f8a375ca21ddb"
        ),
        v!(
            "check-messy",
            ["check", "a.json"],
            b"",
            messy_doc,
            1,
            "NOT CANONICAL a.json",
            "1b2b46f49fe8f6ffc60e2867a71efe216c96d3e2ad2d75905a5ebd530e3e9533"
        ),
        v!(
            "check-mixed",
            [
                "check",
                "ok.json",
                "messy.json",
                "bad.json",
                "dir.json",
                "gone.json"
            ],
            b"",
            mixed_docs,
            2,
            "not a regular file",
            "9c7336fe6453990ace95aa5d5227792a61d83d55b6187ab7777c43e932006634"
        ),
        v!(
            "check-dashdash",
            ["--", "check", "-x.json", "-"],
            b"",
            odd_names,
            1,
            "OK -x.json",
            "4c2fd737d064dac6b37f8da7b397f4d024bfffd5063f0760c606fa14c866acfe"
        ),
        v!(
            "check-dash",
            ["check", "-"],
            b"",
            odd_names,
            1,
            "NOT CANONICAL -",
            "d4e897fa7ba064b7c740c5966f5b9d2a31ae7c040b1edcfd89c24402d5c22862"
        ),
        v!(
            "fix-messy",
            ["fix", "a.json"],
            b"",
            messy_doc,
            0,
            "FIXED a.json",
            "7eb835fa942b5a3c2c9856516dd3055a4165e631a75215f8600e10ea52eda91e"
        ),
        v!(
            "fix-canonical",
            ["fix", "a.json"],
            b"",
            canonical_doc,
            0,
            "OK a.json",
            "bbd948aa713fc127792e6f78d22eb7264ead169250e0af14704efbaebe1fb0d3"
        ),
        v!(
            "fix-invalid",
            ["fix", "a.json"],
            b"",
            invalid_doc,
            2,
            "INVALID a.json",
            "2b621d69e376031e2d9fc262ebffd2fd49feca0f99418b39b623efb2a36eec59"
        ),
        v!(
            "fix-missing",
            ["fix", "gone.json"],
            b"",
            none,
            2,
            "INVALID gone.json",
            "c0130bc96c981419fb211649d5ffe2740e858169d3adb39c66424ea7b02f357d"
        ),
        v!(
            "fix-float-guard",
            ["fix", "a.json"],
            b"",
            float_doc,
            2,
            "violates the strict file profile",
            "b8bf3e1832f56f75fbafe8c5625602dea5492a7031dba47ca87dc3f57c511fa8"
        ),
        v!(
            "fix-symlink",
            ["fix", "link.json"],
            b"",
            symlinked,
            2,
            "not a symlink",
            "9b82dfe4e51d3d1c4f93cd277420ce064acc8799418e6e022f2bd412851a05d6"
        ),
        v!(
            "fix-hardlink",
            ["fix", "a.json"],
            b"",
            hardlinked,
            2,
            "hard-linked",
            "1132a758cf0c24f06221857ed245c685f08baf27e6d51afcdc2bd327d619cef9"
        ),
        v!(
            "fix-read-only",
            ["fix", "a.json"],
            b"",
            read_only,
            2,
            "read-only",
            "deba251249092de757e202ea29e872980ef9cbb7f9a4a88ff6a7609c13129c3e"
        ),
        v!(
            "check-jsonl-canonical",
            ["check-jsonl", "r.jsonl"],
            b"",
            jsonl_canonical,
            0,
            "OK r.jsonl",
            "9bada4395120bf2a667d4e89ba17070a9402c361ea77d101850c97e158680747"
        ),
        v!(
            "check-jsonl-messy",
            ["check-jsonl", "r.jsonl"],
            b"",
            jsonl_messy,
            1,
            "NOT CANONICAL r.jsonl",
            "24aced0b8b7a79cd6c7d989981dab7a4887fee371018887619018e2bfebf0aab"
        ),
        v!(
            "check-jsonl-blank",
            ["check-jsonl", "r.jsonl"],
            b"",
            jsonl_blank,
            2,
            "line 2 is blank",
            "6b4759edfa04a45e34524614bfe061277d825eb6ccdfca3823945b1a1a241d9f"
        ),
        v!(
            "check-jsonl-missing",
            ["check-jsonl", "gone.jsonl"],
            b"",
            none,
            2,
            "cannot read",
            "0e59b3cfe21b6825384157e2b333a918509cc87741339a690476d289521ba337"
        ),
        v!(
            "fix-jsonl-messy",
            ["fix-jsonl", "r.jsonl"],
            b"",
            jsonl_messy,
            0,
            "FIXED r.jsonl",
            "e7b10385d28f1bcdd4401c37434a581b57ffbc272912156e5fdd6e440e5d5c84"
        ),
        v!(
            "fix-jsonl-canonical",
            ["fix-jsonl", "r.jsonl"],
            b"",
            jsonl_canonical,
            0,
            "OK r.jsonl",
            "9bada4395120bf2a667d4e89ba17070a9402c361ea77d101850c97e158680747"
        ),
        v!(
            "fix-jsonl-blank",
            ["fix-jsonl", "r.jsonl"],
            b"",
            jsonl_blank,
            2,
            "line 2 is blank",
            "6b4759edfa04a45e34524614bfe061277d825eb6ccdfca3823945b1a1a241d9f"
        ),
        v!(
            "fix-jsonl-float-guard",
            ["fix-jsonl", "r.jsonl"],
            b"",
            jsonl_float,
            2,
            "violates the strict file profile",
            "97fc48e2851c6a5002ff8a5b9856a8a920c0f526f2e59398f7bc4aa91321d4d8"
        ),
        v!(
            "fix-jsonl-symlink",
            ["fix-jsonl", "link.jsonl"],
            b"",
            jsonl_symlinked,
            2,
            "not a symlink",
            "92e5cf9ebd4d385d25f8c3cf07f9c72690d2f764edd6fab80e62ad92be3904b3"
        ),
        v!(
            "fix-jsonl-missing",
            ["fix-jsonl", "gone.jsonl"],
            b"",
            none,
            2,
            "INVALID gone.jsonl",
            "481eeb4bb25762ca82d5da585cb634f315fa63fc6596f9bd8562a56b5a6f3f28"
        ),
    ]
}

/// Transcript: exit, stdout, stderr, then every regular file left in `dir`.
fn transcript(dir: &Path, code: Option<i32>, stdout: &[u8], stderr: &[u8]) -> Vec<u8> {
    let mut t = format!("exit {code:?}\n").into_bytes();
    t.extend_from_slice(stdout);
    t.extend_from_slice(b"\n--stderr--\n");
    t.extend_from_slice(stderr);
    let mut names: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    names.sort();
    for name in names {
        let path = dir.join(&name);
        if fs::symlink_metadata(&path).unwrap().is_file() {
            t.extend(format!("\n--file {}--\n", name.to_string_lossy()).bytes());
            t.extend(fs::read(path).unwrap());
        }
    }
    t
}

#[cfg(unix)]
#[test]
fn cli_vectors_are_pinned() {
    let mut report = Vec::new();
    for vector in cli_vectors() {
        let dir = support::scratch("vec");
        (vector.prep)(&dir);
        let out = support::run(&dir, vector.args, vector.stdin);
        let shown = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        if out.status.code() != Some(vector.exit) || !shown.contains(vector.phrase) {
            report.push(format!(
                "{}: exit {:?} (want {}), missing {:?} in {shown:?}",
                vector.name,
                out.status.code(),
                vector.exit,
                vector.phrase
            ));
        }
        let t = transcript(&dir, out.status.code(), &out.stdout, &out.stderr);
        pinned(vector.name, &sha256::hex(&t), vector.digest, &mut report);
        let _ = fs::remove_dir_all(&dir);
    }
    finish(report);
}

/// A failing stdout (here, a full device) is exit 2 with a diagnostic.
#[cfg(target_os = "linux")]
#[test]
fn unwritable_stdout_is_exit_two() {
    let full = fs::OpenOptions::new()
        .write(true)
        .open("/dev/full")
        .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_ijson-jcs"))
        .arg("--help")
        .stdout(Stdio::from(full))
        .stderr(Stdio::piped())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let mut report = Vec::new();
    pinned(
        "stdout-full",
        &sha256::hex(&out.stderr),
        "f195f3668c8f64b81f5eacd156d211faeccdd53326f87abdccb407ec329e36db",
        &mut report,
    );
    finish(report);
}

/// Every temporary name taken: `fix` and `fix-jsonl` report the failure,
/// leave the target untouched and remove nothing they did not create.
/// The shell keeps its PID across `exec`, so the names are predictable.
#[cfg(unix)]
#[test]
fn exhausted_temporary_names_fail_closed() {
    let mut report = Vec::new();
    for (name, command, file, prep, digest) in [
        (
            "fix-temp-exhausted",
            "fix",
            "a.json",
            messy_doc as fn(&Path),
            "efeab34f9dd4100db206e368f0226452299eaaf13672f7e52c1b0216ea1649e9",
        ),
        (
            "fix-jsonl-temp-exhausted",
            "fix-jsonl",
            "r.jsonl",
            jsonl_messy,
            "dd46d8ef485aa1467ba0b0b16665837e98a7f827cc13168acc3bdb0267e23cb3",
        ),
    ] {
        let dir = support::scratch("tmp");
        prep(&dir);
        let before = fs::read(dir.join(file)).unwrap();
        let out = Command::new("sh")
            .arg("-c")
            .arg("i=0; while [ $i -lt 128 ]; do : > .ijson-jcs.$$.$i.tmp; i=$((i+1)); done; exec \"$0\" \"$1\" \"$2\"")
            .args([env!("CARGO_BIN_EXE_ijson-jcs"), command, file])
            .current_dir(&dir)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2), "{name}");
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(
            text.contains("cannot replace: could not create a unique temporary file"),
            "{name}: {text}"
        );
        assert_eq!(fs::read(dir.join(file)).unwrap(), before, "{name}");
        let mut temps = 0;
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".ijson-jcs.")
            {
                assert!(
                    fs::read(&path).unwrap().is_empty(),
                    "{name}: temp overwritten"
                );
                fs::remove_file(path).unwrap();
                temps += 1;
            }
        }
        assert_eq!(temps, 128, "{name}");
        let t = transcript(&dir, out.status.code(), &out.stdout, &out.stderr);
        pinned(name, &sha256::hex(&t), digest, &mut report);
        let _ = fs::remove_dir_all(&dir);
    }
    finish(report);
}
