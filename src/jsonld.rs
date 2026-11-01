//! SPDX-License-Identifier: MPL-2.0
//! Copyright (c) 2026 Hyperpolymath Estate
//!
//! JSON-LD documents: byte canonicalisation of the document, and nothing more.
//!
//! The estate route in the README keeps JSON-LD "contexts and graph
//! semantics". That cuts both ways:
//!
//! * a JSON-LD document is JSON, so its **document bytes** can be JCS
//!   canonicalised exactly like any other JSON — members sorted, numbers per
//!   ECMAScript, no insignificant whitespace, `@context` members and their
//!   array order preserved verbatim;
//! * JCS is **not** RDF dataset canonicalisation. It does not expand or
//!   compact contexts, it does not fetch a remote `@context` (no network
//!   access, ever), it does not label blank nodes, and two documents that
//!   describe the same RDF dataset in different JSON-LD shapes produce two
//!   different byte strings. RDFC-1.0 / URDNA2015 is a different algorithm and
//!   is not implemented here.
//!
//! [`supports_rdf_dataset_canonicalization`] answers that question in code, so
//! a consumer that needs dataset canonicalisation cannot infer it from a
//! function name. Nothing in this crate claims it.
//!
//! ```rust
//! use ijson_jcs::{canonicalize_jsonld_document, supports_rdf_dataset_canonicalization};
//!
//! // Member order is normalised; `@context` content is left exactly as written.
//! let input = br#"{"@id": "https://example.invalid/alice", "@context": {"name": "http://schema.org/name"}, "name": "Alice"}"#;
//! assert_eq!(
//!     canonicalize_jsonld_document(input).unwrap(),
//!     br#"{"@context":{"name":"http://schema.org/name"},"@id":"https://example.invalid/alice","name":"Alice"}"#
//! );
//! assert!(!supports_rdf_dataset_canonicalization());
//! ```

use serde_json::Value;

use crate::error::IJsonError;
use crate::profile::canonicalize_bytes;

/// The scope a canonicalisation guarantee covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JsonLdScope {
    /// The JSON-LD **document** bytes: JCS over the member and value tree,
    /// with contexts left inert.
    DocumentBytes,
}

impl JsonLdScope {
    /// The stable name, for receipts and differential vectors.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DocumentBytes => "jsonld-document-bytes",
        }
    }

    /// Whether this scope covers RDF dataset canonicalisation. It never does.
    pub const fn covers_rdf_dataset(self) -> bool {
        false
    }
}

/// What this crate canonicalises for JSON-LD input: document bytes only.
pub const JSONLD_SCOPE: JsonLdScope = JsonLdScope::DocumentBytes;

/// Whether this crate canonicalises RDF datasets (RDFC-1.0 / URDNA2015).
///
/// Always `false`. It exists so that "does it?" is a question with a
/// machine-checkable answer rather than an inference from a function name.
pub const fn supports_rdf_dataset_canonicalization() -> bool {
    false
}

/// Canonicalise a JSON-LD **document**: strict I-JSON parse, then JCS bytes.
///
/// No trailing line feed (that is the library profile; see
/// [`crate::estate_file_from_bytes`] for the on-disk form). A remote
/// `@context` is never fetched: the value is treated as data. Invalid JSON, a
/// duplicate member, an unsafe integer or a lone surrogate is an error, as for
/// any other I-JSON document.
pub fn canonicalize_jsonld_document(input: &[u8]) -> Result<Vec<u8>, IJsonError> {
    canonicalize_bytes(input)
}

/// Canonicalise a JSON-LD document to a `String`.
pub fn canonicalize_jsonld_document_to_string(input: &[u8]) -> Result<String, IJsonError> {
    let bytes = canonicalize_jsonld_document(input)?;
    String::from_utf8(bytes).map_err(|e| {
        IJsonError::CanonicalizationError(crate::error::CanonicalizationError::StringEncodingError(
            e.to_string(),
        ))
    })
}

/// Whether a value declares a JSON-LD `@context` member at its top level.
///
/// Informational only. Canonicalisation is byte-level and treats the member
/// like any other: this never changes the output bytes.
pub fn declares_context(value: &Value) -> bool {
    matches!(value, Value::Object(map) if map.contains_key("@context"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{CanonicalProfile, classify};
    use crate::{JsonMode, parse_json_bytes};

    /// Canonicalising a JSON-LD document is JCS over the document tree.
    #[test]
    fn document_bytes_are_jcs_bytes() {
        let input = br#"{"b": 2, "@context": ["https://example.invalid/a", {"@vocab": "https://example.invalid/"}], "a": 1}"#;
        // (one line of input keeps the expected bytes readable below)
        let canonical = canonicalize_jsonld_document(input).unwrap();
        assert_eq!(
            canonical,
            br#"{"@context":["https://example.invalid/a",{"@vocab":"https://example.invalid/"}],"a":1,"b":2}"#
        );
        // Library bytes, explicitly: no trailing line feed.
        assert!(!canonical.ends_with(b"\n"));
        assert_eq!(
            canonicalize_jsonld_document_to_string(input).unwrap(),
            String::from_utf8(canonical.clone()).unwrap()
        );
        assert!(classify(&canonical, CanonicalProfile::LibraryBytes)
            .unwrap()
            .is_canonical());
    }

    /// Array order inside `@context` is data, not something to sort.
    #[test]
    fn context_array_order_is_preserved() {
        let a = canonicalize_jsonld_document(br#"{"@context":["a","b"]}"#).unwrap();
        let b = canonicalize_jsonld_document(br#"{"@context":["b","a"]}"#).unwrap();
        assert_ne!(a, b);
        assert_eq!(a, br#"{"@context":["a","b"]}"#);
    }

    /// Two documents with the same RDF meaning are NOT one byte string.
    ///
    /// This is the honest limit of a byte-level canonicalisation: it is why the
    /// scope constant says document bytes and why nothing here is called RDF
    /// canonicalisation.
    #[test]
    fn equivalent_rdf_shapes_differ_as_bytes() {
        let expanded = canonicalize_jsonld_document(
            br#"{"@context":{"name":"https://schema.org/name"},"@id":"https://example.invalid/a","name":"A"}"#,
        )
        .unwrap();
        let value_form = canonicalize_jsonld_document(
            br#"{"@context":{"name":"https://schema.org/name"},"@id":"https://example.invalid/a","name":{"@value":"A"}}"#,
        )
        .unwrap();
        assert_ne!(expanded, value_form);
        assert!(!supports_rdf_dataset_canonicalization());
        assert!(!JSONLD_SCOPE.covers_rdf_dataset());
        assert_eq!(JSONLD_SCOPE.as_str(), "jsonld-document-bytes");
    }

    /// A JSON-LD document is validated like any other I-JSON document.
    #[test]
    fn invalid_documents_are_refused() {
        for bad in [
            &br#"{"@id":"a","@id":"b"}"#[..],
            br#"{"@id":"\ud800"}"#,
            br#"{"@value":18446744073709551616}"#,
            b"{\"@id\": }",
        ] {
            assert!(
                canonicalize_jsonld_document(bad).is_err(),
                "{}",
                String::from_utf8_lossy(bad)
            );
        }
    }

    /// Context detection is a shape question, not a validation step.
    #[test]
    fn context_detection() {
        let with = parse_json_bytes(br#"{"@context":{}}"#, JsonMode::Strict).unwrap();
        let without = parse_json_bytes(br#"{"a":1}"#, JsonMode::Strict).unwrap();
        assert!(declares_context(&with));
        assert!(!declares_context(&without));
        assert!(!declares_context(&Value::Array(vec![with])));
    }

    /// No network, ever: a remote context stays an inert string.
    #[test]
    fn remote_context_is_not_fetched() {
        let input = br#"{"@context":"https://example.invalid/nonexistent-context","@id":"x"}"#;
        assert_eq!(
            canonicalize_jsonld_document(input).unwrap(),
            br#"{"@context":"https://example.invalid/nonexistent-context","@id":"x"}"#
        );
    }
}
