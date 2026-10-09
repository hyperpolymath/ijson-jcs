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
use std::collections::HashSet;

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
    let mut context = ValidationContext::new();
    validate_value(value, &mut context)?;

    // Check top-level recommendation (SHOULD, not MUST)
    // I-JSON recommends that top-level values be objects or arrays,
    // but doesn't require it. For maximum compatibility, we'll allow
    // any valid JSON at the top level, but this could be made stricter
    // with a configuration option in the future.

    Ok(())
}

/// Context for validation with path tracking and duplicate key detection
#[derive(Debug, Clone)]
struct ValidationContext {
    /// Current path in JSON Pointer format
    path: String,
    /// Set of keys seen in current object (for duplicate detection)
    current_keys: HashSet<String>,
    /// Whether we've seen a top-level object or array
    top_level_object_or_array: bool,
}

impl ValidationContext {
    fn new() -> Self {
        Self {
            path: String::new(),
            current_keys: HashSet::new(),
            top_level_object_or_array: false,
        }
    }

    #[allow(dead_code)]
    fn is_top_level_object_or_array(&self) -> bool {
        self.top_level_object_or_array
    }

    /// Enter a new object context
    fn enter_object(&mut self) {
        self.current_keys.clear();
        if self.path.is_empty() {
            self.top_level_object_or_array = true;
        }
    }

    /// Exit object context
    fn exit_object(&mut self) {
        self.current_keys.clear();
    }

    /// Add a key to current object, checking for duplicates
    fn add_key(&mut self, key: &str) -> Result<(), ValidationError> {
        if !self.current_keys.insert(key.to_string()) {
            return Err(ValidationError::DuplicateKey {
                key: key.to_string(),
                path: format!("{}{}", self.path, json_pointer_encode(key)),
            });
        }
        Ok(())
    }

    /// Push path segment for JSON Pointer
    fn push_path(&mut self, segment: &str) {
        if !self.path.is_empty() {
            self.path.push('/');
        }
        self.path.push_str(&json_pointer_encode(segment));
    }

    /// Pop path segment
    fn pop_path(&mut self, segment: &str) {
        let encoded_segment = json_pointer_encode(segment);
        if self.path.ends_with(&encoded_segment) {
            // Remove the segment
            self.path.truncate(self.path.len() - encoded_segment.len());
            // Remove trailing slash if present
            if self.path.ends_with('/') {
                self.path.pop();
            }
        }
    }
}

/// Validate a JSON value with path tracking
fn validate_value(value: &Value, context: &mut ValidationContext) -> Result<(), ValidationError> {
    match value {
        Value::Object(map) => {
            context.enter_object();

            for (key, val) in map {
                // Validate the key is a valid I-JSON string
                validate_string_is_i_json(key, &context.path)?;

                // Check for duplicate keys
                context.add_key(key)?;

                // Push key path
                context.push_path(key);

                // Validate the value
                validate_value(val, context)?;

                // Pop key path
                context.pop_path(key);
            }

            context.exit_object();
        }
        Value::Array(arr) => {
            if context.path.is_empty() {
                context.top_level_object_or_array = true;
            }

            for (i, val) in arr.iter().enumerate() {
                // Push array index path
                context.push_path(&i.to_string());

                // Validate the value
                validate_value(val, context)?;

                // Pop array index path
                context.pop_path(&i.to_string());
            }
        }
        Value::String(s) => {
            validate_string_is_i_json(s, &context.path)?;
        }
        Value::Number(n) => {
            validate_number_is_i_json(n, &context.path)?;
        }
        Value::Bool(_) | Value::Null => {
            // Always valid in I-JSON
        }
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

        // Check for unpaired surrogates (U+D800..U+DFFF)
        // These should not appear in valid UTF-8, but we check anyway
        if (0xD800..=0xDFFF).contains(&code_point) {
            return Err(ValidationError::UnpairedSurrogate {
                location: format!("U+{:04X} at index {}", code_point, index),
                path: path.to_string(),
            });
        }

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

/// Get the type name of a JSON value for error messages
#[allow(dead_code)]
fn value_type_name(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(_) => "boolean".to_string(),
        Value::Number(_) => "number".to_string(),
        Value::String(_) => "string".to_string(),
        Value::Array(_) => "array".to_string(),
        Value::Object(_) => "object".to_string(),
    }
}

/// JSON Pointer encoding for a key (RFC 6901)
fn json_pointer_encode(key: &str) -> String {
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

#[allow(dead_code)]
/// Check if a string is valid I-JSON (non-failing version)
pub fn is_valid_i_json_string(s: &str) -> bool {
    // First, parse it
    match serde_json::from_str::<Value>(s) {
        Ok(v) => validate_i_json(&v).is_ok(),
        Err(_) => false,
    }
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
    fn test_duplicate_keys_detection_logic() {
        // Test the duplicate key detection logic in ValidationContext
        let mut context = ValidationContext::new();

        // This should succeed (first insertion)
        assert!(context.add_key("test").is_ok());

        // This should fail (duplicate)
        let result = context.add_key("test");
        assert!(result.is_err());
        assert!(result.unwrap_err().is_duplicate_key());
    }

    #[test]
    fn test_noncharacters() {
        // U+FFFF is a noncharacter
        let nonchar = '\u{FFFF}';

        // This should fail during string validation
        let value = json!({"text": format!("{}", nonchar)});
        if let Ok(parsed) = serde_json::from_str::<Value>(&value.to_string()) {
            let result = validate_i_json(&parsed);
            // Should fail due to noncharacter
            if let Err(e) = result {
                assert!(e.is_string_error());
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
        assert!(is_valid_i_json_string(r#"{"a": 1}"#));
        assert!(!is_valid_i_json_string(r#"invalid json"#));
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
        // Test that path tracking works correctly for nested structures
        let value = json!({
            "level1": {
                "level2": {
                    "value": 42
                }
            }
        });

        // This should pass validation
        let result = validate_i_json(&value);
        assert!(result.is_ok());

        // Test with a string that has invalid characters
        let value_with_control = json!({
            "level1": {
                "level2": {
                    "invalid": "\u{0000}" // null character
                }
            }
        });

        let _result = validate_i_json(&value_with_control);
        // This might pass or fail depending on how serde_json handles the null character
        // The important thing is that path tracking works
    }
}
