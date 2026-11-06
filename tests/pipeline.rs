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
    // Exercise the real script with hostile host settings, rather than relying
    // on a CI runner whose Git configuration happens to be empty.
    let sandbox = std::env::temp_dir().join(format!(
        "ijson-pipeline-host-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&sandbox).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(sandbox.clone());
    let home = sandbox.join("home");
    let xdg = sandbox.join("xdg");
    let template = sandbox.join("template");
    let hooks = template.join("hooks");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(xdg.join("git")).unwrap();
    std::fs::create_dir_all(&hooks).unwrap();
    let config = "[commit]\n\tgpgsign = true\n[gpg]\n\tprogram = /bin/false\n";
    for path in [
        home.join(".gitconfig"),
        xdg.join("git/config"),
        sandbox.join("system"),
    ] {
        std::fs::write(path, config).unwrap();
    }
    let marker = sandbox.join("hook-ran");
    let host_git = sandbox.join("host-git");
    let host_worktree = sandbox.join("host-worktree");
    std::fs::create_dir(&host_git).unwrap();
    std::fs::create_dir(&host_worktree).unwrap();
    std::fs::write(host_git.join("HEAD"), "host HEAD must remain untouched\n").unwrap();
    std::fs::write(host_git.join("index"), "host index must remain untouched\n").unwrap();
    let hook = hooks.join("pre-commit");
    std::fs::write(&hook, "#!/bin/sh\n: >\"$IJSON_HOST_HOOK_MARKER\"\nexit 1\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    let script = format!("{}/scripts/tests/standards-inputs-test.sh", root());
    let output = Command::new("bash")
        .arg(&script)
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", &xdg)
        .env("GIT_CONFIG_GLOBAL", home.join(".gitconfig"))
        .env("GIT_CONFIG_SYSTEM", sandbox.join("system"))
        .env("GIT_CONFIG_NOSYSTEM", "0")
        .env("GIT_TEMPLATE_DIR", &template)
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "core.hooksPath")
        .env("GIT_CONFIG_VALUE_0", &hooks)
        .env("IJSON_HOST_HOOK_MARKER", &marker)
        .env("GIT_DIR", &host_git)
        .env("GIT_COMMON_DIR", &host_git)
        .env("GIT_WORK_TREE", &host_worktree)
        .env("GIT_INDEX_FILE", host_git.join("index"))
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
    assert!(
        stdout.contains("a proposal with no staged change is refused"),
        "the Git fixture controls did not run"
    );
    assert!(!marker.exists(), "a host-supplied hook executed");
    assert_eq!(
        std::fs::read_to_string(host_git.join("HEAD")).unwrap(),
        "host HEAD must remain untouched\n"
    );
    assert_eq!(
        std::fs::read_to_string(host_git.join("index")).unwrap(),
        "host index must remain untouched\n"
    );
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
