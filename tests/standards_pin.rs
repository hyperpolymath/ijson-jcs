// SPDX-License-Identifier: MPL-2.0
// Copyright (c) 2026 Hyperpolymath Estate
//
// The recorded standards revision, its contract, its receipts and the kyq drop
// have to agree with each other. These are the checks that keep a pin, a
// manifest and a log from drifting apart between pipeline runs; they need no
// network, no toolchain and no binary.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use ijson_jcs::{CanonicalProfile, Conformance, classify};

/// The repository root.
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Read a repository file, failing loudly when it is missing.
fn read(relative: &str) -> String {
    let path = root().join(relative);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// The value of a `readonly NAME=...` assignment in .ci/pins.sh.
fn pin(name: &str) -> String {
    let pins = read(".ci/pins.sh");
    let prefix = format!("readonly {name}=");
    let line = pins
        .lines()
        .find(|line| line.starts_with(&prefix))
        .unwrap_or_else(|| panic!("{name} is not declared in .ci/pins.sh"));
    line[prefix.len()..]
        .trim()
        .trim_matches(|c| c == '"' || c == '\'')
        .to_string()
}

/// A full 40-hex commit, lowercase, and nothing else.
fn is_revision(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Every regular file under `dir`, recursively.
fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display())) {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            files_under(&path, out);
        } else {
            out.push(path);
        }
    }
}

/// (digest, path) pairs from a sha256sum-format manifest.
fn manifest_entries(relative: &str) -> Vec<(String, String)> {
    let mut entries = Vec::new();
    for line in read(relative).lines() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let (digest, path) = line
            .split_once("  ")
            .unwrap_or_else(|| panic!("{relative}: not `<sha256>  <path>`: {line:?}"));
        assert_eq!(digest.len(), 64, "{relative}: {line:?}");
        assert!(
            digest.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "{relative}: digest is not lowercase hex: {digest}"
        );
        assert!(
            !path.starts_with('/') && !path.contains(".."),
            "{relative}: path escapes the tree: {path}"
        );
        entries.push((digest.to_string(), path.to_string()));
    }
    entries
}

/// (role, path, literal) records from the contract.
fn contract_entries() -> Vec<(String, String, String)> {
    let mut entries = Vec::new();
    for line in read(".ci/standards-contract.tsv").lines() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        assert_eq!(
            fields.len(),
            3,
            "contract line is not role<TAB>path<TAB>literal: {line:?}"
        );
        entries.push((
            fields[0].to_string(),
            fields[1].to_string(),
            fields[2].to_string(),
        ));
    }
    entries
}

/// The pin names a full commit of the one repository the pipeline tracks.
#[test]
fn pin_records_an_immutable_revision() {
    let revision = pin("STANDARDS_REV");
    assert!(
        is_revision(&revision),
        "STANDARDS_REV must be a full 40-hex commit, got {revision:?}"
    );
    assert_eq!(pin("STANDARDS_REPOSITORY"), "metadatastician/standards");
    assert!(
        pin("STANDARDS_URL_BASE").contains("codeload.github.com")
            || pin("STANDARDS_URL_BASE").contains("STANDARDS_URL_BASE"),
        "STANDARDS_URL_BASE must default to the archive host"
    );
    // The other estate revisions this repository records are full commits too.
    for name in ["PROVEN_REV", "PONS_REV", "PANIC_REV", "KYQ_REV"] {
        assert!(is_revision(&pin(name)), "{name} is not a full commit");
    }
}

/// The manifest is a sorted, duplicate-free sha256sum file.
#[test]
fn manifest_is_sorted_and_unique() {
    let entries = manifest_entries(".ci/standards.sha256");
    assert!(!entries.is_empty(), "the manifest pins nothing");
    let paths: Vec<&str> = entries.iter().map(|(_, path)| path.as_str()).collect();
    let mut sorted = paths.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(paths, sorted, ".ci/standards.sha256 is not sorted or has duplicates");
}

/// Every path the contract depends on is pinned, and every role is known.
#[test]
fn contract_is_pinned_and_uses_known_roles() {
    let pinned: BTreeSet<String> = manifest_entries(".ci/standards.sha256")
        .into_iter()
        .map(|(_, path)| path)
        .collect();
    let mut roles = BTreeSet::new();
    let entries = contract_entries();
    assert!(!entries.is_empty(), "the contract asserts nothing");
    for (role, path, literal) in entries {
        roles.insert(role);
        assert!(
            pinned.contains(&path),
            "contract path is not pinned in .ci/standards.sha256: {path}"
        );
        assert!(!literal.trim().is_empty(), "empty literal for {path}");
    }
    let known: BTreeSet<String> = ["rule-j-1", "rule-j-2", "gate", "gate-test"]
        .iter()
        .map(|role| role.to_string())
        .collect();
    assert!(
        roles.is_subset(&known),
        "unknown contract roles: {:?}",
        roles.difference(&known).collect::<Vec<_>>()
    );
    // The contract must cover the JSON policy, the gate and the gate's test:
    // a revision that only moved a spec file would otherwise pass unnoticed.
    let paths: BTreeSet<String> = entries_paths();
    for required in [
        "3-practice/JSON-POLICY.adoc",
        "scripts/check-ijson-jcs.sh",
        "scripts/tests/check-ijson-jcs-test.sh",
    ] {
        assert!(
            paths.contains(required),
            "the contract does not depend on {required}"
        );
    }
}

/// The contract's distinct paths, in a form the tests can compare.
fn entries_paths() -> BTreeSet<String> {
    let pinned: BTreeSet<String> = manifest_entries(".ci/standards.sha256")
        .into_iter()
        .map(|(_, path)| path)
        .collect();
    contract_entries()
        .into_iter()
        .map(|(_, path, _)| path)
        .filter(|path| pinned.contains(path))
        .collect()
}

/// The revision log starts with its header and ends at the pin.
#[test]
fn revision_log_ends_at_the_pin() {
    let log = read(".ci/standards-revisions.tsv");
    let mut lines = log.lines().filter(|line| !line.trim().is_empty());
    let header = lines.next().expect("the revision log is empty");
    assert_eq!(
        header, "date\trevision\tprevious\tmanifest_sha256\tsource",
        "the revision log has no header"
    );
    let records: Vec<&str> = lines.collect();
    for record in &records {
        let fields: Vec<&str> = record.split('\t').collect();
        assert_eq!(fields.len(), 5, "revision record is not five fields: {record:?}");
        assert!(is_revision(fields[1]), "revision is not a commit: {record:?}");
        assert!(is_revision(fields[2]), "previous revision is not a commit: {record:?}");
        assert_eq!(fields[3].len(), 64, "manifest digest is not sha256: {record:?}");
        assert!(
            fields[0].ends_with('Z') && fields[0].contains('T'),
            "date is not ISO 8601 UTC: {record:?}"
        );
    }
    if let Some(last) = records.last() {
        let revision = last.split('\t').nth(1).expect("revision field");
        assert_eq!(
            revision,
            pin("STANDARDS_REV"),
            "the newest revision record is not the pinned revision"
        );
    }
}

/// Every receipt under .ci/receipts/ is an estate-canonical JSON document that
/// records a full standards revision and an accepted verdict.
#[test]
fn receipts_are_estate_canonical_and_accepted() {
    let dir = root().join(".ci/receipts");
    if !dir.is_dir() {
        return;
    }
    for entry in fs::read_dir(&dir).expect("read .ci/receipts") {
        let path = entry.expect("directory entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let bytes = fs::read(&path).expect("read receipt");
        let verdict = classify(&bytes, CanonicalProfile::EstateFile)
            .unwrap_or_else(|e| panic!("{} is not I-JSON: {e}", path.display()));
        assert_eq!(
            verdict,
            Conformance::Canonical,
            "{} is not canonical bytes plus exactly one LF",
            path.display()
        );
        let value: serde_json::Value =
            serde_json::from_slice(&bytes).expect("receipt is JSON");
        assert_eq!(
            value["schema"], "ijson-jcs/standards-receipt/1",
            "{} has an unknown schema",
            path.display()
        );
        assert_eq!(
            value["verdict"], "accepted",
            "{} is not an accepted receipt",
            path.display()
        );
        assert!(
            is_revision(value["standards"]["revision"].as_str().unwrap_or("")),
            "{} does not record a full standards revision",
            path.display()
        );
        assert!(
            value["checks"]
                .as_array()
                .is_some_and(|checks| !checks.is_empty()),
            "{} records no checks",
            path.display()
        );
    }
}

/// Every vector directory is complete, and the manifest covers exactly them.
#[test]
fn kyq_vectors_are_complete_and_sealed() {
    let vectors = root().join("interop/kyq/vectors");
    assert!(vectors.is_dir(), "interop/kyq/vectors is missing");
    let commands = ["canon", "check", "fix", "check-jsonl", "fix-jsonl", "jsonld"];
    let mut dirs = 0;
    for entry in fs::read_dir(&vectors).expect("read interop/kyq/vectors") {
        let dir = entry.expect("directory entry").path();
        if !dir.is_dir() {
            continue;
        }
        dirs += 1;
        let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("<non-utf8>");
        let cmd = fs::read_to_string(dir.join("cmd"))
            .unwrap_or_else(|e| panic!("{name}: no cmd file: {e}"));
        let first = cmd.split_whitespace().next().unwrap_or("");
        assert!(commands.contains(&first), "{name}: unknown command {first:?}");
        assert!(dir.join("stdin").is_file(), "{name}: no stdin file");
        let exit = fs::read_to_string(dir.join("expect.exit"))
            .unwrap_or_else(|e| panic!("{name}: no expect.exit: {e}"));
        let exit = exit.trim();
        assert!(
            matches!(exit, "0" | "1" | "2"),
            "{name}: expect.exit must be 0, 1 or 2, got {exit:?}"
        );
    }
    assert!(dirs > 0, "no vectors");
    let entries = manifest_entries("interop/kyq/vectors.sha256");
    let sealed: BTreeSet<&str> = entries.iter().map(|(_, path)| path.as_str()).collect();
    let mut on_disk = Vec::new();
    files_under(&vectors, &mut on_disk);
    let drop = root().join("interop/kyq");
    for path in &on_disk {
        let relative = path
            .strip_prefix(&drop)
            .expect("vector file under interop/kyq")
            .to_string_lossy()
            .to_string();
        assert!(
            sealed.contains(relative.as_str()),
            "vector file is not sealed by vectors.sha256: {relative}"
        );
    }
    assert_eq!(
        sealed.len(),
        on_disk.len(),
        "vectors.sha256 names files that do not exist"
    );
}
