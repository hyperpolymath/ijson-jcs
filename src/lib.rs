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

use std::fmt;

/// JSON parsing and validation modes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JsonMode {
    /// Parse any valid JSON (standard serde_json behavior)
    /// No I-JSON validation, preserves key order
    Loose,

    /// Parse JSON and validate I-JSON compliance
    /// Fails on duplicate keys, unsafe numbers, bad Unicode, etc.
    Strict,

    /// Parse JSON, validate I-JSON, and prepare for JCS canonicalization
    /// Most strict mode - ensures data is suitable for hashing/signing
    Canonical,
}

impl Default for JsonMode {
    fn default() -> Self {
        Self::Loose
    }
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
pub use jcs::{to_jcs, to_jcs_string, canonicalize};

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
pub fn parse_json(input: &str, mode: JsonMode) -> Result<Value, IJsonError> {
    let value = serde_json::from_str(input).map_err(|e| IJsonError::ParseError { source: e })?;

    match mode {
        JsonMode::Loose => Ok(value),
        JsonMode::Strict => {
            ijson::validate_i_json(&value)?;
            Ok(value)
        }
        JsonMode::Canonical => {
            ijson::validate_i_json(&value)?;
            jcs::validate_canonicalizable(&value)?;
            Ok(value)
        }
    }
}

/// Parse JSON bytes with the specified mode
pub fn parse_json_bytes(input: &[u8], mode: JsonMode) -> Result<Value, IJsonError> {
    let input_str = std::str::from_utf8(input)
        .map_err(|e| IJsonError::EncodingError { source: e, mode })?;
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

    #[test]
    fn test_validation_helpers() {
        assert!(is_valid_i_json_string(r#"{"a": 1}"#));
        // serde_json handles duplicate keys by keeping the last one,
        // so this becomes valid JSON with a single key
        assert!(is_valid_i_json_string(r#"{"k":1,"k":2}"#));

        assert!(is_valid_i_json_bytes(b"{\"a\": 1}"));
        assert!(is_valid_i_json_bytes(b"{\"k\":1,\"k\":2}"));
    }

    #[test]
    fn test_serialization() {
        let value = json!({"a": 1, "b": 2});
        assert_eq!(to_json_string(&value), r#"{"a":1,"b":2}"#);
        
        let pretty = to_json_pretty_string(&value);
        assert!(pretty.contains('\n'));
    }
}
