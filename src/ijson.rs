//! SPDX-License-Identifier: MPL-2.0
//! Copyright (c) 2026 Hyperpolymath Estate
//!
//! I-JSON (RFC 7493) validation
//
// # I-JSON Requirements
//
// I-JSON is a restricted profile of JSON (RFC 8259) that addresses
// interoperability issues by imposing additional constraints:
//
// ## MUST Requirements:
// - UTF-8 encoding
// - No duplicate object keys
// - No unpaired surrogates (U+D800..U+DFFF) in strings or keys
// - No noncharacters in strings or keys
//
// ## SHOULD Requirements:
// - Numbers limited to IEEE-754 binary64 exact representation
// - Top-level value should be object or array
// - Follow RFC 3339 for timestamps, base64url for binary, etc.
//
// ## Other Recommendations:
// - Object key order is insignificant
// - Unknown keys should be ignored (MUST-IGNORE)
//
// # Implementation
//
// This module validates that serde_json::Value data conforms to I-JSON.

use serde_json::Value;

use crate::error::ValidationError;

/// The safe integer limits for IEEE-754 binary64
/// Numbers outside this range cannot be represented exactly as doubles
/// and SHOULD be represented as strings in I-JSON for exactness
///
/// These values are precomputed to avoid runtime calculation:
/// - 2^53 = 9007199254740992
/// - Safe range: [-(2^53)+1, (2^53)-1] = [-9007199254740991, 9007199254740991]
pub const SAFE_INT_MIN: i64 = -9007199254740991; // -(2^53 - 1)
pub const SAFE_INT_MAX: i64 = 9007199254740991; // 2^53 - 1
pub const SAFE_UINT_MAX: u64 = 9007199254740991; // 2^53 - 1

/// Validate that a serde_json::Value is I-JSON compliant
///
/// This function checks the I-JSON requirements that a `Value` can still
/// show, and returns a detailed error for any violation found.
///
/// # A built `Value`, not parsed text
///
/// Use this on a `Value` you built, before you emit or canonicalise it;
/// [`to_jcs`](crate::to_jcs) calls it for that reason. Do not use it to
/// validate text. A `Value` that `serde_json` parsed from text has already
/// lost two things this check would need: a repeated key has collapsed to
/// its last value, and an integer beyond the `i64`/`u64` range has become an
/// `f64`. To validate text, use [`parse_json`](crate::parse_json) with
/// [`JsonMode::Strict`](crate::JsonMode::Strict), which reads the text
/// itself, or [`is_valid_i_json_string`](crate::is_valid_i_json_string).
///
/// ```rust
/// use ijson_jcs::{parse_json, validate_i_json, JsonMode};
///
/// let text = r#"{"k": 1, "k": 2}"#;
/// // serde_json keeps only the last "k", so the repeat is invisible here.
/// let collapsed: serde_json::Value = serde_json::from_str(text).unwrap();
/// assert!(validate_i_json(&collapsed).is_ok());
/// // Parsing the text itself refuses it.
/// assert!(parse_json(text, JsonMode::Strict).is_err());
/// ```
///
/// # Arguments
/// * `value` - The JSON value to validate
///
/// # Returns
/// * `Ok(())` - The value is I-JSON compliant
/// * `Err(ValidationError)` - The value violates I-JSON rules
///
/// # Examples
///
/// ```rust
/// use ijson_jcs::validate_i_json;
/// use serde_json::json;
///
/// // Valid I-JSON
/// let valid = json!({"a": 1, "b": [2, 3]});
/// assert!(validate_i_json(&valid).is_ok());
/// ```
pub fn validate_i_json(value: &Value) -> Result<(), ValidationError> {
    validate_value(value, &mut Vec::new(), 0)
}

/// Validate a built value with a bounded call stack and lossless path segments.
/// Duplicate names cannot exist in a `Value::Object`; only the text parser
/// can check them. Do not maintain a second, misleading duplicate detector.
fn validate_value(
    value: &Value,
    segments: &mut Vec<String>,
    depth: usize,
) -> Result<(), ValidationError> {
    if matches!(value, Value::Object(_) | Value::Array(_)) && depth >= crate::MAX_DEPTH {
        return Err(ValidationError::NestingTooDeep {
            limit: crate::MAX_DEPTH,
            path: segments.join("/"),
        });
    }
    match value {
        Value::Object(map) => {
            for (key, val) in map {
                segments.push(json_pointer_encode(key));
                validate_string_is_i_json(key, &segments.join("/"))?;
                validate_value(val, segments, depth + 1)?;
                segments.pop();
            }
        }
        Value::Array(arr) => {
            for (i, val) in arr.iter().enumerate() {
                segments.push(i.to_string());
                validate_value(val, segments, depth + 1)?;
                segments.pop();
            }
        }
        Value::String(s) => validate_string_is_i_json(s, &segments.join("/"))?,
        Value::Number(n) => validate_number_is_i_json(n, &segments.join("/"))?,
        Value::Bool(_) | Value::Null => {}
    }
    Ok(())
}

/// Validate that a string is valid I-JSON
///
/// Checks for:
/// - Unpaired surrogates (U+D800..U+DFFF)
/// - Noncharacters (U+FDD0..U+FDEF, U+FFFE, U+FFFF, etc.)
///
/// Escaped control characters are permitted (RFC 7493 §2.1 does not ban them).
fn validate_string_is_i_json(s: &str, path: &str) -> Result<(), ValidationError> {
    for (index, ch) in s.chars().enumerate() {
        let code_point = ch as u32;

        // Check for noncharacters
        if is_noncharacter(code_point) {
            return Err(ValidationError::Noncharacter {
                location: format!("U+{:04X} at index {}", code_point, index),
                path: path.to_string(),
            });
        }

        // Control characters (U+0000..U+001F) are NOT checked: RFC 8259
        // requires them to be escaped in the JSON text (serde_json enforces
        // that), and RFC 7493 §2.1 only forbids surrogates and noncharacters.
        // RFC 8785 §3.2.3's own sample contains an escaped U+000F.
    }

    Ok(())
}

/// Check if a code point is a noncharacter
///
/// Noncharacters are code points that are explicitly defined as non-characters
/// by the Unicode standard and MUST NOT appear in interchange:
///
/// - U+FDD0..U+FDEF (last two of each plane)
/// - U+FFFE, U+FFFF (last two of BMP)
/// - U+1FFFE, U+1FFFF, U+2FFFE, U+2FFFF, ... U+10FFFE, U+10FFFF
///   (last two of each supplementary plane)
fn is_noncharacter(code_point: u32) -> bool {
    // U+FDD0..U+FDEF
    if (0xFDD0..=0xFDEF).contains(&code_point) {
        return true;
    }

    // U+FFFE, U+FFFF
    if code_point == 0xFFFE || code_point == 0xFFFF {
        return true;
    }

    // U+1FFFE, U+1FFFF, U+2FFFE, U+2FFFF, etc.
    // These are all code points where the last 16 bits are FFFE or FFFF
    // and the code point is within the valid Unicode range (U+0000..U+10FFFF)
    if code_point <= 0x10FFFF && (code_point & 0xFFFF) >= 0xFFFE {
        return true;
    }

    false
}

/// Validate that a number is valid I-JSON
///
/// I-JSON recommends that numbers be limited to those exactly representable
/// in IEEE-754 binary64 (double precision). Numbers outside this range should
/// be represented as strings for exactness.
///
/// Safe integer range: [-(2^53)+1, (2^53)-1]
/// - 2^53 = 9007199254740992
/// - Safe range: [-9007199254740991, 9007199254740991]
///
/// Also checks for NaN and Infinity which are not allowed in RFC 8259 JSON
/// but might appear in some implementations.
fn validate_number_is_i_json(n: &serde_json::Number, path: &str) -> Result<(), ValidationError> {
    // Check for integer representation first (even if it can also be float)
    if let Some(i) = n.as_i64() {
        // Check safe integer range
        if !(SAFE_INT_MIN..=SAFE_INT_MAX).contains(&i) {
            return Err(ValidationError::UnsafeInteger {
                value: i.to_string(),
                path: path.to_string(),
            });
        }
    } else if let Some(u) = n.as_u64() {
        // Check safe unsigned integer range
        if u > SAFE_UINT_MAX {
            return Err(ValidationError::UnsafeInteger {
                value: u.to_string(),
                path: path.to_string(),
            });
        }
    } else if let Some(f) = n.as_f64() {
        // Check for special float values (NaN, Infinity)
        if !f.is_finite() {
            return Err(ValidationError::NonFiniteNumber {
                value: n.to_string(),
                path: path.to_string(),
            });
        }

        // For floats that are not integers, we could be more strict and check
        // if they have exact representation, but I-JSON allows floats in general.
        // The recommendation is primarily for integers outside the safe range.
        //
        // For now, we allow all finite floats as I-JSON compliant.
        // Users who need exact representation for specific use cases
        // should use integers or strings.
    }

    // Note: serde_json::Number should always be i64, u64, or f64
    // If we get here, the number is valid I-JSON
    Ok(())
}

/// JSON Pointer encoding for a key (RFC 6901)
pub(crate) fn json_pointer_encode(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

/// Validate that a value can be canonicalized to JCS
///
/// This performs additional checks beyond I-JSON validation to ensure
/// the value is suitable for canonicalization.
pub fn validate_canonicalizable(value: &Value) -> Result<(), ValidationError> {
    // For now, any I-JSON is canonicalizable
    // In the future, we might add additional constraints
    validate_i_json(value)
}

/// Check if a value is I-JSON compliant (non-failing version)
///
/// Same scope as [`validate_i_json`]: for a `Value` you built, not one that
/// `serde_json` parsed from text.
pub fn is_i_json(value: &Value) -> bool {
    validate_i_json(value).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_valid_i_json_objects_and_arrays() {
        // Basic valid I-JSON
        assert!(validate_i_json(&json!({"a": 1, "b": 2})).is_ok());
        assert!(validate_i_json(&json!([1, 2, 3])).is_ok());
        assert!(validate_i_json(&json!({"nested": {"object": true}})).is_ok());
    }

    #[test]
    fn test_valid_i_json_primitives_at_top_level() {
        // These are valid JSON but not recommended by I-JSON for top-level
        // However, we currently allow them for compatibility
        // The strict top-level check is configurable

        // For now, let's test that our validation works for nested primitives
        let obj = json!({"null": null, "bool": true, "string": "hello"});
        assert!(validate_i_json(&obj).is_ok());
    }

    #[test]
    fn test_noncharacters() {
        for code in (0xFDD0..=0xFDEF)
            .chain((0..=16).flat_map(|plane| [plane * 0x10000 + 0xFFFE, plane * 0x10000 + 0xFFFF]))
        {
            let nonchar = char::from_u32(code).unwrap();
            for value in [
                json!({"text": nonchar.to_string()}),
                json!({(nonchar.to_string()): 1}),
            ] {
                let err = validate_i_json(&value).unwrap_err();
                assert!(matches!(err, ValidationError::Noncharacter { .. }));
            }
        }
    }

    /// Escaped control characters are valid I-JSON.
    #[test]
    fn test_control_characters() {
        // Escaped control characters are valid I-JSON (RFC 7493 §2.1 only
        // forbids surrogates and noncharacters); RFC 8785 §3.2.3 uses U+000F.
        let parsed: Value = serde_json::from_str(r#"{"text": "\u0000\u000f\u001f"}"#).unwrap();
        assert!(validate_i_json(&parsed).is_ok());
    }

    #[test]
    fn test_allowed_control_characters() {
        // TAB, LF, CR should be allowed
        let value = json!({"text": "line1\nline2\tword\r"});
        assert!(validate_i_json(&value).is_ok());
    }

    #[test]
    fn test_unsafe_integers() {
        // Test large integers outside safe range
        // SAFE_INT_MAX = 9007199254740991 = 2^53 - 1
        // So we need a number larger than this

        // Use a number that's definitely larger than SAFE_INT_MAX
        // i64::MAX might be represented as a float by serde_json, so let's use a specific value
        let large_int = 9223372036854775807i64; // i64::MAX
        let value = json!({"big": large_int});

        let result = validate_i_json(&value);
        assert!(
            result.is_err(),
            "Expected validation error for large integer, got: {:?}",
            result
        );
        if let Err(e) = result {
            assert!(e.is_number_error());
        }
    }

    #[test]
    fn test_safe_integers() {
        // Test integers within safe range
        let safe_int = SAFE_INT_MAX; // 2^53 - 1, maximum safe integer
        let value = json!({"safe": safe_int});

        assert!(validate_i_json(&value).is_ok());
    }

    #[test]
    fn test_floats() {
        // Test that floats are allowed (I-JSON allows them)
        let value = json!({"ratio": 0.5, "pi": 1.2345});
        assert!(validate_i_json(&value).is_ok());
    }

    #[test]
    fn test_string_validation_helpers() {
        assert!(crate::is_valid_i_json_string(r#"{"a": 1}"#));
        assert!(!crate::is_valid_i_json_string(r#"invalid json"#));
    }

    #[test]
    fn test_value_validation_helpers() {
        let valid = json!({"a": 1});
        assert!(is_i_json(&valid));
    }

    #[test]
    fn test_nested_structures() {
        let complex = json!({
            "users": [
                {"name": "Alice", "age": 30},
                {"name": "Bob", "age": 25}
            ],
            "metadata": {
                "version": "1.0",
                "timestamp": "2026-01-01T00:00:00Z"
            }
        });

        assert!(validate_i_json(&complex).is_ok());
    }

    #[test]
    fn test_empty_structures() {
        assert!(validate_i_json(&json!({})).is_ok());
        assert!(validate_i_json(&json!([])).is_ok());
        assert!(validate_i_json(&json!("")).is_ok());
        assert!(validate_i_json(&json!(0)).is_ok());
        assert!(validate_i_json(&json!(true)).is_ok());
        assert!(validate_i_json(&json!(false)).is_ok());
        assert!(validate_i_json(&json!(null)).is_ok());
    }

    #[test]
    fn test_path_tracking() {
        let value = json!({"level1": {"level2": {"invalid": "\u{ffff}"}}});
        assert_eq!(
            validate_i_json(&value).unwrap_err().path(),
            "level1/level2/invalid"
        );
        assert!(validate_i_json(&json!({"level1": {"control": "\u{0000}"}})).is_ok());
    }
}
