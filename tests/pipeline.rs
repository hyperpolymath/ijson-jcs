// SPDX-License-Identifier: MPL-2.0
// Copyright (c) 2026 Hyperpolymath Estate
//
// The standards pipeline's own self-test, run from cargo test so that a change
// to this crate cannot leave the pipeline unproven. It needs bash and a POSIX
// userland, not a Rust toolchain, a binary or the network.

#![cfg(unix)]

use std::process::Command;

/// The repository root the test binary is running in.
fn root() -> &'static str {
    env!("CARGO_MANIFEST_DIR")
}

/// The pipeline self-test passes, and says so in its own summary.
///
/// This is deliberately a real subprocess: the scripts are the deliverable, so
/// a test that re-implemented their logic in Rust would prove nothing about the
/// thing that actually runs in CI.
#[test]
fn standards_pipeline_can_fail() {
    let script = format!("{}/scripts/tests/standards-inputs-test.sh", root());
    let output = Command::new("bash")
        .arg(&script)
        .output()
        .unwrap_or_else(|e| panic!("cannot run {script}: {e}"));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "the pipeline self-test failed\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    let summary = stdout.lines().last().unwrap_or("");
    assert!(
        summary.starts_with("passed "),
        "the self-test printed no summary; last line: {summary:?}"
    );
    assert!(
        summary.ends_with(", failed 0"),
        "the self-test reported failures: {summary}"
    );
    // A self-test that ran no controls would also print `failed 0`.
    let passed: usize = summary
        .strip_prefix("passed ")
        .and_then(|rest| rest.split(',').next())
        .and_then(|count| count.trim().parse().ok())
        .unwrap_or(0);
    assert!(passed >= 40, "only {passed} controls ran");
}

/// The profile, JSONL and JSON-LD fixture trees the pipeline gates are here.
#[test]
fn fixture_trees_are_present() {
    for path in [
        "tests/fixtures/standards/good",
        "tests/fixtures/standards/good/canonical.json",
        "tests/fixtures/standards/good/jsonld.json",
        "tests/fixtures/standards/good/records.jsonl",
    ] {
        let full = format!("{}/{}", root(), path);
        assert!(
            std::path::Path::new(&full).exists(),
            "missing fixture: {path}"
        );
    }
}
