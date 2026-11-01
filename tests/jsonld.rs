// SPDX-License-Identifier: MPL-2.0
// Copyright (c) 2026 Hyperpolymath Estate
//
// JSON-LD byte canonicalisation, and the RDF claim this crate does not make.

use std::io::Write;
use std::process::{Command, Output, Stdio};

use ijson_jcs::{
    JSONLD_SCOPE, JsonLdScope, canonicalize_jsonld_document,
    canonicalize_jsonld_document_to_string, declares_context, supports_rdf_dataset_canonicalization,
};

/// Run `ijson-jcs jsonld` with `input` on stdin.
fn jsonld(input: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ijson-jcs"))
        .arg("jsonld")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn ijson-jcs");
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

/// A document canonicalises to JCS bytes, with `@context` treated as data.
#[test]
fn document_bytes_are_canonical_and_keep_their_context() {
    let input = br#"{
        "name": {"@value": "Alice", "@language": "en"},
        "@id": "https://example.invalid/alice",
        "@context": {"name": "https://schema.org/name"}
    }"#;
    let want = br#"{"@context":{"name":"https://schema.org/name"},"@id":"https://example.invalid/alice","name":{"@language":"en","@value":"Alice"}}"#;
    assert_eq!(canonicalize_jsonld_document(input).unwrap(), want);
    assert_eq!(
        canonicalize_jsonld_document_to_string(input).unwrap(),
        String::from_utf8(want.to_vec()).unwrap()
    );
    let out = jsonld(input);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(out.stdout, want);
}

/// The CLI hashes documents, not files: no trailing line feed is added.
#[test]
fn cli_prints_library_bytes() {
    let out = jsonld(b"{\"@id\":\"x\"}\n");
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(out.stdout, b"{\"@id\":\"x\"}");
    assert!(!out.stdout.ends_with(b"\n"));
    assert!(out.stderr.is_empty());
}

/// A remote `@context` is never fetched: it stays an inert string.
#[test]
fn remote_contexts_are_not_fetched() {
    let input = br#"{"@context": "https://example.invalid/context-that-does-not-exist", "@id": "x"}"#;
    let out = jsonld(input);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        out.stdout,
        br#"{"@context":"https://example.invalid/context-that-does-not-exist","@id":"x"}"#
    );
}

/// Invalid documents are refused by the CLI with exit 2 and the usual wording.
#[test]
fn invalid_documents_exit_two() {
    for bad in [
        &br#"{"@id":"a","@id":"b"}"#[..],
        br#"{"@id":"\ud800"}"#,
        br#"{"@value":18446744073709551616}"#,
        br#"{"@id":"x"} trailing"#,
        b"{\"@id\":",
        b"[\"\\xff\"]",
    ] {
        let out = jsonld(bad);
        assert_eq!(out.status.code(), Some(2), "{:?}", String::from_utf8_lossy(bad));
        assert!(out.stdout.is_empty());
        assert!(
            String::from_utf8_lossy(&out.stderr).starts_with("INVALID <stdin>:"),
            "{:?}",
            String::from_utf8_lossy(bad)
        );
    }
}

/// The RDF question has a machine-checkable answer: no.
#[test]
fn rdf_dataset_canonicalisation_is_not_claimed() {
    assert!(!supports_rdf_dataset_canonicalization());
    assert!(!JSONLD_SCOPE.covers_rdf_dataset());
    assert_eq!(JSONLD_SCOPE.as_str(), "jsonld-document-bytes");
    assert_eq!(JsonLdScope::DocumentBytes.as_str(), "jsonld-document-bytes");
    assert!(!JsonLdScope::DocumentBytes.covers_rdf_dataset());

    // Two shapes that describe the same RDF dataset are different byte strings.
    let value_form = canonicalize_jsonld_document(
        br#"{"@context":{"name":"https://schema.org/name"},"@id":"https://example.invalid/a","name":{"@value":"A"}}"#,
    )
    .unwrap();
    let string_form = canonicalize_jsonld_document(
        br#"{"@context":{"name":"https://schema.org/name"},"@id":"https://example.invalid/a","name":"A"}"#,
    )
    .unwrap();
    assert_ne!(value_form, string_form);
}

/// `@context` detection is a shape question, and never changes the bytes.
#[test]
fn context_detection_does_not_change_output() {
    let input = b"{\"@context\":{},\"b\":1,\"a\":2}";
    let value = ijson_jcs::parse_json_bytes(input, ijson_jcs::JsonMode::Canonical).unwrap();
    assert!(declares_context(&value));
    assert_eq!(jsonld(input).stdout, b"{\"@context\":{},\"a\":2,\"b\":1}");

    // `--` protects a file name that looks like an option; `jsonld` takes none.
    let out = Command::new(env!("CARGO_BIN_EXE_ijson-jcs"))
        .args(["jsonld", "file.json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("'jsonld' reads stdin"));
}
