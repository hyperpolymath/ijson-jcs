// SPDX-License-Identifier: MPL-2.0 AND CC-BY-SA-4.0
// Code is MPL-2.0; prose is CC-BY-SA-4.0.
//! Counted tests that pin behaviour a `cargo mutants` run showed was unpinned.
//!
//! Each test names the mutant it kills (file:line of the original run, on the
//! base plus the coverage patch).
mod support;

use ijson_jcs::{
    IJsonError, JsonMode, ValidationError, Value, parse_json, to_jcs, validate_i_json,
};
use std::fs;
use std::path::Path;
use support::{run, scratch};

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn unsafe_integer_path(input: &str) -> String {
    match parse_json(input, JsonMode::Strict) {
        Err(IJsonError::ValidationError(ValidationError::UnsafeInteger { path, .. })) => path,
        other => panic!("expected UnsafeInteger for {input}, got {other:?}"),
    }
}

// ---- library ---------------------------------------------------------------

/// error.rs:182 `is_duplicate_key -> false`.
#[test]
fn duplicate_key_is_reported_as_such() {
    let dup = parse_json(r#"{"a":1,"a":2}"#, JsonMode::Strict);
    // Duplicates are refused while parsing, as a parse error.
    assert!(dup.is_err());
    let err = ValidationError::DuplicateKey {
        key: "a".into(),
        path: String::new(),
    };
    assert!(err.is_duplicate_key());
    let other = ValidationError::UnsafeInteger {
        value: "1".into(),
        path: String::new(),
    };
    assert!(!other.is_duplicate_key());
}

/// ijson.rs:174 `|| -> &&` in is_noncharacter: each of U+FFFE and U+FFFF alone.
#[test]
fn each_bmp_noncharacter_is_refused() {
    for ch in ['\u{FFFE}', '\u{FFFF}', '\u{FDD0}', '\u{FDEF}'] {
        let value = Value::String(ch.to_string());
        let err = validate_i_json(&value).expect_err("noncharacter must be refused");
        assert!(err.is_string_error(), "U+{:04X}", ch as u32);
    }
    // The neighbours are ordinary characters.
    for ch in ['\u{FFFD}', '\u{E000}', '\u{FDCF}', '\u{FDF0}'] {
        assert!(validate_i_json(&Value::String(ch.to_string())).is_ok());
    }
}

/// jcs.rs:124 `<= -> >` is not killable: write_number_jcs only sees values that
/// validation already accepted, and a u64 reaches that branch only above
/// i64::MAX, which validation refuses. Pin the refusal that makes it so.
#[test]
fn integers_above_safe_range_are_refused_before_writing() {
    for token in ["1152921504606846976", "18446744073709551615"] {
        let value: Value = serde_json::from_str(token).unwrap();
        assert!(to_jcs(&value).is_err(), "{token}");
    }
}

/// jcs.rs:214 `sort_keys_utf16 -> ()`: UTF-16 order differs from code point
/// order when a supplementary character meets U+E000..U+FFFF.
#[test]
fn keys_sort_by_utf16_code_units() {
    let value: Value = serde_json::from_str("{\"\u{FFE0}\":1,\"\u{10000}\":2}").unwrap();
    let out = to_jcs(&value).unwrap();
    assert_eq!(text(&out), "{\"\u{10000}\":2,\"\u{FFE0}\":1}");
}

/// strict.rs:181 (`]`/`}` pops), :184 (`,` arm), :185 (`+= -> *=`) and the
/// path walk: the reported path of an unsafe integer follows the document.
#[test]
fn unsafe_integer_paths_follow_the_document() {
    // Index advances on every comma (:184, :185): index 2 distinguishes += 1
    // from *= 1 and from no increment.
    assert_eq!(unsafe_integer_path("[1,2,18446744073709551616]"), "2");
    assert_eq!(unsafe_integer_path("[1,18446744073709551616]"), "1");
    // A closed container is popped (:181): the sibling after it is at index 1.
    assert_eq!(unsafe_integer_path("[[1],18446744073709551616]"), "1");
    assert_eq!(unsafe_integer_path("[{\"a\":1},18446744073709551616]"), "1");
    // A comma resets the object key (:184).
    assert_eq!(
        unsafe_integer_path("{\"a\":1,\"b\":18446744073709551616}"),
        "b"
    );
    assert_eq!(
        unsafe_integer_path("{\"a\":[1,2],\"b\":18446744073709551616}"),
        "b"
    );
    // Nested path joins with `/`.
    assert_eq!(
        unsafe_integer_path("{\"a\":[0,{\"c\":18446744073709551616}]}"),
        "a/1/c"
    );
}

// ---- CLI -------------------------------------------------------------------

/// bin:22 `* -> +` (both): the default limit is 64 MiB, not about 1 MiB or 64 KiB.
#[test]
fn default_max_bytes_admits_a_two_mib_file() {
    let dir = scratch("default-limit");
    let mut doc = Vec::with_capacity(2 * 1024 * 1024 + 8);
    doc.extend_from_slice(b"[\"");
    doc.resize(doc.len() + 2 * 1024 * 1024, b'a');
    doc.extend_from_slice(b"\"]\n");
    fs::write(dir.join("big.json"), &doc).unwrap();
    let out = run(&dir, &["check", "big.json"], b"");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stdout));
    assert_eq!(text(&out.stdout), "OK big.json\n");
}

/// bin:94 `> -> >=` in read_bounded: exactly the limit is accepted, one more is not.
#[test]
fn max_bytes_boundary_is_inclusive() {
    let dir = scratch("limit-edge");
    let at = run(&dir, &["--max-bytes", "2", "canon"], b"[]");
    assert_eq!(at.status.code(), Some(0), "{}", text(&at.stderr));
    assert_eq!(text(&at.stdout), "[]");
    let over = run(&dir, &["--max-bytes", "1", "canon"], b"[]");
    assert_eq!(over.status.code(), Some(2));
    assert!(text(&over.stderr).contains("exceeds --max-bytes 1"));
}

/// bin:347 `< -> <=`: u64::MAX is not an addressable byte count on this target
/// (usize::MAX as u64), one less is.
#[cfg(target_pointer_width = "64")]
#[test]
fn max_bytes_refuses_the_largest_count() {
    let dir = scratch("limit-max");
    let refused = run(
        &dir,
        &["--max-bytes", "18446744073709551615", "canon"],
        b"[]",
    );
    assert_eq!(refused.status.code(), Some(2));
    assert!(text(&refused.stderr).contains("--max-bytes needs"));
    let accepted = run(
        &dir,
        &["--max-bytes", "18446744073709551614", "canon"],
        b"[]",
    );
    assert_eq!(
        accepted.status.code(),
        Some(0),
        "{}",
        text(&accepted.stderr)
    );
}

/// bin:74 match guard `!path.is_empty() -> true`: an error with no path prints
/// no " (at )" suffix, and an error with a path does.
#[test]
fn error_path_suffix_only_when_there_is_a_path() {
    let dir = scratch("describe");
    let rooted = run(&dir, &["canon"], b"9007199254740992");
    assert_eq!(rooted.status.code(), Some(2));
    assert!(
        !text(&rooted.stderr).contains("(at"),
        "{}",
        text(&rooted.stderr)
    );
    let nested = run(&dir, &["canon"], b"[1,9007199254740992]");
    assert_eq!(nested.status.code(), Some(2));
    assert!(
        text(&nested.stderr).contains("(at 1)"),
        "{}",
        text(&nested.stderr)
    );
}

fn messy(dir: &Path) -> std::path::PathBuf {
    let path = dir.join("messy.json");
    fs::write(&path, b"{ \"b\" : 1 , \"a\" : 2 }").unwrap();
    path
}

/// bin:117 `Drop for PendingFile -> ()`: a failed write leaves no temporary file.
/// The write fails with EFBIG under RLIMIT_FSIZE (SIGXFSZ ignored).
#[cfg(unix)]
#[test]
fn failed_write_removes_its_temporary_file() {
    use std::process::Command;
    let dir = scratch("cleanup");
    let big = dir.join("big.json");
    let mut doc = b"[ ".to_vec();
    for n in 0..4000 {
        doc.extend_from_slice(format!("{n} , ").as_bytes());
    }
    doc.extend_from_slice(b"0 ]");
    fs::write(&big, &doc).unwrap();
    let out = Command::new("sh")
        .arg("-c")
        .arg("ulimit -f 1; trap '' XFSZ; exec \"$0\" fix \"$1\"")
        .arg(env!("CARGO_BIN_EXE_ijson-jcs"))
        .arg(&big)
        .current_dir(&dir)
        // The size limit would truncate the instrumented binary's own profile.
        .env("LLVM_PROFILE_FILE", "/dev/null")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stdout));
    assert!(
        text(&out.stdout).contains("cannot replace"),
        "{}",
        text(&out.stdout)
    );
    assert_eq!(fs::read(&big).unwrap(), doc, "the target is untouched");
    let leftovers: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(".ijson-jcs."))
        .collect();
    assert!(
        leftovers.is_empty(),
        "temporary file left behind: {leftovers:?}"
    );
}

/// bin:129 `delete ! ` in atomic_replace: the temporary file goes beside an
/// absolute target, not in the working directory (which is gone here).
#[cfg(unix)]
#[test]
fn temporary_file_is_created_beside_the_target() {
    use std::process::Command;
    let dir = scratch("beside");
    let target = messy(&dir);
    let out = Command::new("sh")
        .arg("-c")
        .arg("mkdir \"$2/gone\" && cd \"$2/gone\" && rmdir \"$2/gone\" && exec \"$0\" fix \"$1\"")
        .arg(env!("CARGO_BIN_EXE_ijson-jcs"))
        .arg(&target)
        .arg(&dir)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stdout));
    assert_eq!(fs::read(&target).unwrap(), b"{\"a\":2,\"b\":1}\n");
}

/// bin:146 guard `AlreadyExists -> true` (and the exclusion for line 147): an
/// open failure other than EEXIST is reported as itself, at once. The target
/// path fits PATH_MAX but the temporary name beside it does not (ENAMETOOLONG).
#[cfg(target_os = "linux")]
#[test]
fn temporary_name_too_long_is_reported_as_the_os_error() {
    let base = scratch("deep");
    let mut dir = base.clone();
    let want = 4084;
    let len = |p: &Path| p.as_os_str().len();
    while len(&dir) + 201 <= want {
        dir.push("d".repeat(200));
    }
    let rest = want - len(&dir);
    assert!(rest >= 2, "scratch path too long for this layout");
    dir.push("e".repeat(rest - 1));
    assert_eq!(len(&dir), want);
    fs::create_dir_all(&dir).unwrap();
    let target = dir.join("a.json");
    fs::write(&target, b"[ 1 ]").unwrap();
    assert!(len(&target) < 4096);
    let out = run(&base, &["fix", target.to_str().unwrap()], b"");
    let stdout = text(&out.stdout);
    let outcome = (out.status.code(), stdout.contains("File name too long"));
    let _ = fs::remove_dir_all(&base);
    assert_eq!(outcome, (Some(2), true), "{stdout}");
}
