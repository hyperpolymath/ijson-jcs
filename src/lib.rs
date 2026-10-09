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

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::all)]
#![allow(clippy::needless_doctest_main)]

mod error;
mod ijson;
mod jcs;
mod strict;

use std::fmt;

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
/// For the same reason those modes check integer tokens in the text itself:
/// an integer beyond the `i64`/`u64` range reaches the `Value` as a float,
/// so a token outside ±(2^53−1) such as `18446744073709551616` is an
/// `UnsafeInteger` error read from `input`, not from the `Value`. Float
/// tokens (`1.5`, `1e30`) are accepted.
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
            strict::check_integer_tokens(input)?;
            Ok(value)
        }
        JsonMode::Canonical => {
            ijson::validate_i_json(&value)?;
            strict::check_integer_tokens(input)?;
            jcs::validate_canonicalizable(&value)?;
            Ok(value)
        }
    }
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

    /// Strict and Canonical refuse an integer token beyond `u64`/`i64`, at
    /// top level and nested, naming the token and its path; Loose accepts it.
    #[test]
    fn test_over_range_integer_tokens_refused() {
        for (input, token, path) in [
            ("18446744073709551616", "18446744073709551616", ""),
            ("[18446744073709551617]", "18446744073709551617", "0"),
            ("[-9223372036854775809]", "-9223372036854775809", "0"),
            (
                r#"{"ttl":18446744073709551616}"#,
                "18446744073709551616",
                "ttl",
            ),
            (
                r#"{"a":[1,{"b":[2,-99999999999999999999999]}]}"#,
                "-99999999999999999999999",
                "a/1/b/1",
            ),
        ] {
            let want = ValidationError::UnsafeInteger {
                value: token.to_string(),
                path: path.to_string(),
            };
            for mode in [JsonMode::Strict, JsonMode::Canonical] {
                let err = parse_json(input, mode).unwrap_err();
                assert_eq!(err.as_validation_error(), Some(&want), "{mode}: {input}");
                let err = parse_json_bytes(input.as_bytes(), mode).unwrap_err();
                assert_eq!(err.as_validation_error(), Some(&want), "{mode}: {input}");
            }
            assert!(parse_json(input, JsonMode::Loose).is_ok(), "Loose: {input}");
        }
        assert!(!is_valid_i_json_string(r#"{"ttl":18446744073709551616}"#));
        assert!(!is_valid_i_json_bytes(b"[18446744073709551616]"));
    }

    /// The token check reports the same path as `validate_i_json` reports
    /// for an unsafe integer at the same place.
    #[test]
    fn test_over_range_path_matches_validate_i_json() {
        let path = |n: &str| {
            let input = format!(r#"{{"a":[1,{{"b~/c":[2,{n}]}}]}}"#);
            let err = parse_json(&input, JsonMode::Strict).unwrap_err();
            err.as_validation_error().unwrap().path().to_string()
        };
        // 2^53 + 1 fits a u64, so `validate_i_json` refuses it from the Value.
        assert_eq!(path("9007199254740993"), "a/1/b~0~1c/1");
        assert_eq!(path("18446744073709551616"), "a/1/b~0~1c/1");
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
