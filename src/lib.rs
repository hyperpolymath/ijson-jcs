//! SPDX-License-Identifier: MPL-2.0
//! Copyright (c) 2026 Hyperpolymath Estate
//!
//! I-JSON (RFC 7493) validator + JCS (RFC 8785) canonicalizer
//
// # Overview
//
// This crate provides **elegant, correct** JSON handling for scenarios requiring:
// - **Interoperability**: I-JSON compliance (RFC 7493)
// - **Determinism**: JCS canonicalization (RFC 8785)
// - **Safety**: Validation of JSON data before hashing, signing, or storage
//
// # Design Principles
//
// 1. **Correctness First**: Strict adherence to RFC specifications
// 2. **Elegance**: Clean, minimal APIs that are intuitive to use
// 3. **Dual-Mode**: Support both loose parsing (compatibility) and strict modes
// 4. **Deterministic**: JCS output is byte-for-byte identical for same input
// 5. **Zero-Copy**: Minimize allocations where possible
//
// # Fork Relationship
//
// This implementation is **forked and enhanced** from the excellent
// `meta-repos/groove/cli/src/canonical.rs` implementation, which was already
// RFC 8785 compliant for the Groove use case. We've generalized it to:
// - Support full I-JSON validation (duplicate keys, Unicode safety, etc.)
// - Handle all JSON number types (not just integers)
// - Provide a comprehensive, unified API
// - Add extensive test coverage
//
// # Usage
//
// ```rust
// use ijson_jcs::{JsonMode, parse_json, to_jcs, validate_i_json};
// use serde_json::json;
//
// // Parse with I-JSON validation
// let value = parse_json(r#"{"b": 1, "a": 2}"#, JsonMode::Strict)?;
//
// // Canonicalize to JCS (deterministic bytes)
// let canonical_bytes = to_jcs(&value)?;
// // canonical_bytes == b"{\"a\":2,\"b\":1}"
// ```
//
// # Bytes are not files
//
// Three byte-level questions are separate here, because conflating them is a
// real defect class:
//
// 1. `to_jcs` (and `canonicalize_bytes`) — **library canonical bytes**: what
//    you hash or sign, with no trailing line feed.
// 2. `estate_file_bytes` (and `estate_file_from_bytes`) — the **estate file
//    profile**: those bytes plus exactly one `\n`, as JSON-POLICY J-1 requires
//    on disk.
// 3. `classify` with a `CanonicalProfile` — the verdict for bytes you already
//    have. `check`'s historical rule is `CanonicalProfile::Tolerant` (either
//    form); a gate that claims J-1 must ask for `CanonicalProfile::EstateFile`.
//
// `records` splits JSON Lines / NDJSON into one record per line under the same
// verdicts, and `canonicalize_jsonld_document` canonicalises a JSON-LD
// **document** without ever claiming RDF dataset canonicalisation — see
// `supports_rdf_dataset_canonicalization`, which is `false`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::all)]
#![allow(clippy::needless_doctest_main)]

mod error;
mod ijson;
mod jcs;
mod jsonld;
mod profile;
mod records;
mod strict;

use std::fmt;

/// Maximum container depth accepted for a programmatically constructed value.
/// Text input is additionally bounded by serde_json's recursion limit.
pub const MAX_DEPTH: usize = 128;

/// JSON parsing and validation modes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum JsonMode {
    /// Parse any valid JSON (standard serde_json behavior)
    /// No I-JSON validation; a repeated key keeps its last value
    #[default]
    Loose,

    /// Parse JSON and validate I-JSON compliance
    /// Fails on duplicate keys, unsafe numbers, bad Unicode, etc.
    Strict,

    /// Parse JSON, validate I-JSON, and prepare for JCS canonicalization
    /// Most strict mode - ensures data is suitable for hashing/signing
    Canonical,
}

impl fmt::Display for JsonMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JsonMode::Loose => write!(f, "Loose"),
            JsonMode::Strict => write!(f, "Strict"),
            JsonMode::Canonical => write!(f, "Canonical"),
        }
    }
}

pub use error::{CanonicalizationError, IJsonError, ParseMode, ValidationError};
pub use ijson::{is_i_json, validate_canonicalizable, validate_i_json};
pub use jcs::{canonicalize, to_jcs, to_jcs_string};
pub use jsonld::{
    JSONLD_SCOPE, JsonLdScope, canonicalize_jsonld_document,
    canonicalize_jsonld_document_to_string, declares_context,
    supports_rdf_dataset_canonicalization,
};
pub use profile::{
    CanonicalProfile, Conformance, FINAL_LF, canonicalize_bytes, classify, estate_file_bytes,
    estate_file_from_bytes, strip_estate_final_lf,
};
pub use records::{
    FramingError, Records, canonicalize_records, classify_records, record_count, records,
};

pub use serde_json::Value;

/// Parse JSON string with the specified mode
///
/// # Arguments
/// * `input` - The JSON string to parse
/// * `mode` - The parsing mode (Loose, Strict, or Canonical)
///
/// # Returns
/// * `Ok(Value)` - Parsed JSON value
/// * `Err(IJsonError)` - Parse error or validation failure
///
/// # Examples
///
/// ```rust
/// use ijson_jcs::{parse_json, JsonMode};
///
/// // Loose parsing - accepts any valid JSON (compatibility mode)
/// let value = parse_json(r#"{"b": 1, "a": 2}"#, JsonMode::Loose).unwrap();
///
/// // Strict parsing - validates I-JSON compliance
/// let value = parse_json(r#"{"b": 1, "a": 2}"#, JsonMode::Strict).unwrap();
///
/// // Canonical mode - validates and prepares for JCS canonicalization
/// let value = parse_json(r#"{"b": 1, "a": 2}"#, JsonMode::Canonical).unwrap();
/// ```
///
/// In `Strict` and `Canonical` modes an object with a repeated key is
/// rejected (RFC 7493 §2.3) during parsing, since a parsed `Value` can no
/// longer show that a duplicate existed.
///
/// Those modes read every number as IEEE-754 binary64, the RFC 8785 number
/// model, so the full output of [`to_jcs`] reparses. An integer token beyond
/// ±(2^53−1) rounds to the nearest double (`9007199254740993` becomes
/// `9007199254740992`), a nonzero token below the smallest subnormal such as
/// `1e-400` rounds to zero, and a token that overflows (`1e400`) is a parse
/// error. Use strings for identifiers or arbitrary-precision values.
/// Container nesting is limited to 127 levels, the serde_json recursion
/// limit; the same bound applies to [`parse_jcs`].
pub fn parse_json(input: &str, mode: JsonMode) -> Result<Value, IJsonError> {
    let parsed = match mode {
        JsonMode::Loose => serde_json::from_str(input),
        JsonMode::Strict | JsonMode::Canonical => strict::from_str_no_duplicates(input),
    };
    let value = parsed.map_err(|e| IJsonError::ParseError { source: e })?;

    match mode {
        JsonMode::Loose => Ok(value),
        JsonMode::Strict => {
            ijson::validate_i_json(&value)?;
            Ok(value)
        }
        JsonMode::Canonical => {
            ijson::validate_canonicalizable(&value)?;
            Ok(value)
        }
    }
}

/// Parse JSON using the RFC 8785 binary64 number model.
///
/// Equivalent to [`parse_json`] with [`JsonMode::Canonical`]; kept as an
/// explicit entry point for callers reparsing [`to_jcs`] output. Every number
/// is rounded to binary64. Duplicate keys, noncharacters, lone surrogates,
/// excessive nesting (more than 127 levels), non-finite numbers and trailing
/// input are rejected. Whitespace and key order need not be canonical.
///
/// ```
/// use ijson_jcs::{parse_jcs, to_jcs_string};
/// let bytes = "10000000000000000";
/// assert_eq!(to_jcs_string(&parse_jcs(bytes)?).unwrap(), bytes);
/// assert_eq!(to_jcs_string(&parse_jcs("1e-400")?).unwrap(), "0");
/// # Ok::<(), ijson_jcs::IJsonError>(())
/// ```
pub fn parse_jcs(input: &str) -> Result<Value, IJsonError> {
    parse_json(input, JsonMode::Canonical)
}

/// Parse JSON bytes with the specified mode
pub fn parse_json_bytes(input: &[u8], mode: JsonMode) -> Result<Value, IJsonError> {
    let input_str =
        std::str::from_utf8(input).map_err(|e| IJsonError::EncodingError { source: e, mode })?;
    parse_json(input_str, mode)
}

/// Serialize JSON value to standard string (non-canonical)
pub fn to_json_string(value: &Value) -> String {
    serde_json::to_string(value).expect("Value is always serializable")
}

/// Serialize JSON value to pretty-printed string
pub fn to_json_pretty_string(value: &Value) -> String {
    serde_json::to_string_pretty(value).expect("Value is always serializable")
}

/// Check if a string contains valid I-JSON
pub fn is_valid_i_json_string(input: &str) -> bool {
    parse_json(input, JsonMode::Strict).is_ok()
}

/// Check if bytes contain valid I-JSON
pub fn is_valid_i_json_bytes(input: &[u8]) -> bool {
    parse_json_bytes(input, JsonMode::Strict).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_modes() {
        // Loose mode accepts any valid JSON
        let value = parse_json(r#"{"b": 1, "a": 2}"#, JsonMode::Loose).unwrap();
        assert!(value.is_object());

        // Strict mode validates I-JSON
        let value = parse_json(r#"{"b": 1, "a": 2}"#, JsonMode::Strict).unwrap();
        assert!(value.is_object());

        // Canonical mode is strictest
        let value = parse_json(r#"{"b": 1, "a": 2}"#, JsonMode::Canonical).unwrap();
        assert!(value.is_object());
    }

    /// Strict helpers reject duplicate keys; loose parsing still accepts them.
    #[test]
    fn test_validation_helpers() {
        assert!(is_valid_i_json_string(r#"{"a": 1}"#));
        // RFC 7493 §2.3: duplicate keys are not I-JSON
        assert!(!is_valid_i_json_string(r#"{"k":1,"k":2}"#));
        // ...but loose mode keeps serde_json's last-wins behaviour
        assert!(parse_json(r#"{"k":1,"k":2}"#, JsonMode::Loose).is_ok());

        assert!(is_valid_i_json_bytes(b"{\"a\": 1}"));
        assert!(!is_valid_i_json_bytes(b"{\"k\":1,\"k\":2}"));
    }

    /// Strict and Canonical read every number as binary64: integer tokens
    /// beyond ±(2^53−1) round, underflow rounds to zero, overflow is refused.
    #[test]
    fn test_binary64_number_model() {
        for (input, canonical) in [
            ("9007199254740991", "9007199254740991"),
            ("9007199254740993", "9007199254740992"),
            ("-9007199254740993", "-9007199254740992"),
            ("18446744073709551616", "18446744073709552000"),
            ("[-9223372036854775809]", "[-9223372036854776000]"),
            (
                r#"{"ttl":18446744073709551616}"#,
                r#"{"ttl":18446744073709552000}"#,
            ),
            ("1e-400", "0"),
            ("-1e-400", "0"),
            ("[5e-324,0e0,0.0]", "[5e-324,0,0]"),
        ] {
            for mode in [JsonMode::Strict, JsonMode::Canonical] {
                let value =
                    parse_json(input, mode).unwrap_or_else(|e| panic!("{mode}: {input}: {e}"));
                assert_eq!(to_jcs_string(&value).unwrap(), canonical, "{mode}: {input}");
                let bytes = parse_json_bytes(input.as_bytes(), mode).unwrap();
                assert_eq!(bytes, value, "{mode}: {input}");
            }
            assert!(is_valid_i_json_string(input), "{input}");
        }
        for input in ["1e400", "-1e400", "[1e999]", "123e999999"] {
            for mode in [JsonMode::Strict, JsonMode::Canonical] {
                assert!(parse_json(input, mode).is_err(), "{mode}: {input}");
            }
        }
    }

    /// Safe-range integers, float tokens and over-range digits inside
    /// strings are accepted, with the same `Value` Loose parsing gives.
    #[test]
    fn test_integer_token_check_accepts() {
        for input in [
            "9007199254740991",
            "-9007199254740991",
            "[1e30,1.5,-0.0,-0,0,1e+20,2E-3]",
            // A float token is exempt like every float, whatever its value.
            "[18446744073709551616.0]",
            r#"{"s":"18446744073709551616"}"#,
            r#"{"s":"a\"18446744073709551616","18446744073709551617":"\\"}"#,
            r#"{"\\\"":[-9007199254740991],"t":true,"f":false,"n":null}"#,
        ] {
            let loose = parse_json(input, JsonMode::Loose).unwrap();
            for mode in [JsonMode::Strict, JsonMode::Canonical] {
                assert_eq!(
                    parse_json(input, mode).ok(),
                    Some(loose.clone()),
                    "{mode}: {input}"
                );
            }
        }
    }

    #[test]
    fn test_serialization() {
        let value = json!({"a": 1, "b": 2});
        assert_eq!(to_json_string(&value), r#"{"a":1,"b":2}"#);

        let pretty = to_json_pretty_string(&value);
        assert!(pretty.contains('\n'));
    }
}
