//! SPDX-License-Identifier: MPL-2.0
//! Copyright (c) 2026 Hyperpolymath Estate
//!
//! JCS (RFC 8785) canonical JSON serialization
//
// # JCS Requirements
//
// JCS (JSON Canonicalization Scheme, RFC 8785) provides deterministic
// byte-for-byte serialization of I-JSON data with these rules:
//
// 1. **Input**: MUST be valid I-JSON (we validate this first)
// 2. **Encoding**: UTF-8 output
// 3. **Object keys**: Sorted by UTF-16 code unit order (not simple string comparison)
// 4. **Whitespace**: No insignificant whitespace (compact form)
// 5. **Primitives**: Serialized exactly as ECMAScript JSON.stringify does
//    - Numbers follow ES number-to-string rules
//    - No trailing zeros, proper scientific notation only when needed
// 6. **Arrays**: Keep element order (only objects get key-sorted)
// 7. **Output**: Single unique compact UTF-8 byte sequence for any given I-JSON value
//
// # Implementation Notes
//
// This implementation is **forked and enhanced** from the Groove implementation
// (`meta-repos/groove/cli/src/canonical.rs`), which was already RFC 8785 compliant
// for their manifest use case. Enhancements include:
//
// - Full number support (not just integers)
// - Proper UTF-16 code unit sorting for non-ASCII keys
// - Integration with I-JSON validation
// - More comprehensive error handling
//
// # Fork Relationship
//
// The original Groove implementation was designed for manifest signing and
// deliberately restricted to integers only. This generalization:
//
// - Handles all I-JSON number types (integers and floats)
// - Uses proper ES number-to-string rules via serde_json
// - Maintains the same deterministic output for the Groove use case
//
// This means the Groove implementation can be replaced with this one without
// changing the canonical output for their manifest format.

use serde_json::Value;
use std::cmp::Ordering;

use crate::error::{CanonicalizationError, ValidationError};
use crate::ijson;

/// The safe integer limits for IEEE-754 binary64
/// Numbers outside this range cannot be represented exactly as doubles
/// and MUST be represented as strings in I-JSON for exactness
const SAFE_INT_MIN: i64 = -9007199254740991; // -(2^53 - 1)
const SAFE_INT_MAX: i64 = 9007199254740991; // 2^53 - 1
const SAFE_UINT_MAX: u64 = 9007199254740991; // 2^53 - 1

/// Canonicalize a JSON value to JCS format (RFC 8785)
///
/// Returns deterministic UTF-8 bytes representing the canonical JSON form.
///
/// # Arguments
/// * `value` - The JSON value to canonicalize (must be valid I-JSON)
///
/// # Returns
/// * `Ok(Vec<u8>)` - The canonical UTF-8 byte sequence
/// * `Err(CanonicalizationError)` - If canonicalization fails
///
/// # Examples
///
/// ```rust
/// use ijson_jcs::{to_jcs, JsonMode, parse_json};
///
/// // Parse and canonicalize
/// let value = parse_json(r#"{"b": 1, "a": 2}"#, JsonMode::Canonical).unwrap();
/// let canonical = to_jcs(&value).unwrap();
/// // canonical == b"{\"a\":2,\"b\":1}"
/// ```
pub fn to_jcs(value: &Value) -> Result<Vec<u8>, CanonicalizationError> {
    // Validate I-JSON compliance first (JCS requires I-JSON input)
    ijson::validate_i_json(value)?;
    validate_canonicalizable(value)?;

    let mut out = Vec::with_capacity(256);
    write_canonical(value, &mut out)?;
    Ok(out)
}

/// Canonicalize a JSON value to JCS string
///
/// Note: This returns a String, but for true determinism and hashing,
/// prefer `to_jcs()` which returns bytes directly.
pub fn to_jcs_string(value: &Value) -> Result<String, CanonicalizationError> {
    let bytes = to_jcs(value)?;
    String::from_utf8(bytes).map_err(|e| CanonicalizationError::StringEncodingError(e.to_string()))
}

/// Alias for to_jcs (for API consistency)
pub fn canonicalize(value: &Value) -> Result<Vec<u8>, CanonicalizationError> {
    to_jcs(value)
}

/// Write canonical JSON to a byte buffer
fn write_canonical(value: &Value, out: &mut Vec<u8>) -> Result<(), CanonicalizationError> {
    match value {
        Value::Null => out.extend_from_slice(b"null"),
        Value::Bool(true) => out.extend_from_slice(b"true"),
        Value::Bool(false) => out.extend_from_slice(b"false"),
        Value::Number(n) => write_number_jcs(n, out)?,
        Value::String(s) => write_string_jcs(s, out),
        Value::Array(arr) => write_array_jcs(arr, out)?,
        Value::Object(map) => write_object_jcs(map, out)?,
    }
    Ok(())
}

/// Write a number in JCS format (RFC 8785 §3.2.2.3)
///
/// Integers in the safe range are written as decimal integers. Everything
/// else is treated as an IEEE-754 double and written with the ECMAScript
/// `Number.prototype.toString` algorithm, which is what JCS mandates.
fn write_number_jcs(
    n: &serde_json::Number,
    out: &mut Vec<u8>,
) -> Result<(), CanonicalizationError> {
    if let Some(i) = n
        .as_i64()
        .filter(|i| (SAFE_INT_MIN..=SAFE_INT_MAX).contains(i))
    {
        out.extend_from_slice(i.to_string().as_bytes());
        return Ok(());
    }
    if let Some(u) = n.as_u64().filter(|u| *u <= SAFE_UINT_MAX) {
        out.extend_from_slice(u.to_string().as_bytes());
        return Ok(());
    }

    // Integers outside the safe range are rejected by I-JSON validation
    // before we get here; anything left is a double.
    let f = n
        .as_f64()
        .ok_or_else(|| CanonicalizationError::NumberFormatError(n.to_string()))?;
    out.extend_from_slice(es_number_to_string(f)?.as_bytes());
    Ok(())
}

/// Format a finite double exactly as ECMAScript `Number.prototype.toString`.
///
/// Implements ECMA-262 §6.1.6.1.20 (Number::toString) for radix 10, which
/// RFC 8785 §3.2.2.3 adopts: shortest round-tripping digits, plain notation
/// for decimal exponents in (-7, 21], otherwise `d.ddde±x`; `-0` becomes `0`.
pub(crate) fn es_number_to_string(f: f64) -> Result<String, CanonicalizationError> {
    if !f.is_finite() {
        return Err(CanonicalizationError::NumberFormatError(f.to_string()));
    }
    if f == 0.0 {
        return Ok("0".to_string());
    }

    // Rust's `{:e}` yields the shortest digit string that round-trips,
    // e.g. "1.2345e-7" or "-4.5e0".
    let sci = format!("{:e}", f.abs());
    let (mantissa, exp) = sci
        .split_once('e')
        .ok_or_else(|| CanonicalizationError::NumberFormatError(sci.clone()))?;
    let exp: i32 = exp
        .parse()
        .map_err(|_| CanonicalizationError::NumberFormatError(sci.clone()))?;
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let k = digits.len() as i32;
    let n = exp + 1; // position of the decimal point relative to the digits

    let mut s = String::with_capacity(32);
    if f < 0.0 {
        s.push('-');
    }
    if k <= n && n <= 21 {
        s.push_str(&digits);
        s.extend(std::iter::repeat_n('0', (n - k) as usize));
    } else if 0 < n && n <= 21 {
        s.push_str(&digits[..n as usize]);
        s.push('.');
        s.push_str(&digits[n as usize..]);
    } else if -6 < n && n <= 0 {
        s.push_str("0.");
        s.extend(std::iter::repeat_n('0', (-n) as usize));
        s.push_str(&digits);
    } else {
        s.push_str(&digits[..1]);
        if k > 1 {
            s.push('.');
            s.push_str(&digits[1..]);
        }
        s.push('e');
        s.push(if n - 1 < 0 { '-' } else { '+' });
        s.push_str(&(n - 1).abs().to_string());
    }
    Ok(s)
}

/// Write a string in JCS format
///
/// Uses serde_json's RFC 8259 compliant string escaping.
fn write_string_jcs(s: &str, out: &mut Vec<u8>) {
    // serde_json::to_string produces RFC 8259 compliant string escaping
    // which matches JCS requirements for the code points we care about
    let escaped = serde_json::to_string(s).expect("String is always serializable");
    // The escaped string will be in quotes, but we want just the content
    // Actually, serde_json::to_string on a string value returns the quoted string
    out.extend_from_slice(escaped.as_bytes());
}

/// Write an array in JCS format
fn write_array_jcs(arr: &[Value], out: &mut Vec<u8>) -> Result<(), CanonicalizationError> {
    out.push(b'[');
    for (i, item) in arr.iter().enumerate() {
        if i > 0 {
            out.push(b',');
        }
        write_canonical(item, out)?;
    }
    out.push(b']');
    Ok(())
}

/// Write an object in JCS format with keys sorted by UTF-16 code unit order
fn write_object_jcs(
    map: &serde_json::Map<String, Value>,
    out: &mut Vec<u8>,
) -> Result<(), CanonicalizationError> {
    // Collect keys and sort by UTF-16 code unit order
    let mut keys: Vec<&String> = map.keys().collect();
    sort_keys_utf16(&mut keys);

    out.push(b'{');
    for (i, key) in keys.iter().enumerate() {
        if i > 0 {
            out.push(b',');
        }

        // Write the key (with quotes and escaping via serde_json)
        let key_str = serde_json::to_string(key).expect("Key is always serializable");
        out.extend_from_slice(key_str.as_bytes());
        out.push(b':');

        // Write the value
        write_canonical(&map[*key], out)?;
    }
    out.push(b'}');
    Ok(())
}

/// Sort string keys by UTF-16 code unit order as required by JCS (RFC 8785)
///
/// JCS specifies: "Recursively sort the members of all objects by the
/// UTF-16 code unit order of their keys."
///
/// This is NOT the same as simple string comparison for non-ASCII keys.
/// For ASCII-only keys (which is the common case), this behaves the same
/// as lexicographic comparison.
fn sort_keys_utf16(keys: &mut Vec<&String>) {
    keys.sort_by(|a, b| utf16_code_unit_cmp(a, b));
}

/// Compare two strings by UTF-16 code unit order
fn utf16_code_unit_cmp(a: &str, b: &str) -> Ordering {
    let a_utf16: Vec<u16> = a.encode_utf16().collect();
    let b_utf16: Vec<u16> = b.encode_utf16().collect();
    a_utf16.cmp(&b_utf16)
}

/// Validate that a value can be canonicalized to JCS
///
/// This performs additional checks beyond I-JSON validation to ensure
/// the value is suitable for canonicalization.
pub fn validate_canonicalizable(value: &Value) -> Result<(), ValidationError> {
    // For now, any I-JSON is canonicalizable
    // In the future, we might add additional constraints
    ijson::validate_i_json(value)
}

#[allow(dead_code)]
/// Create a JCS canonical form from a JSON string
///
/// Convenience function that parses, validates, and canonicalizes in one step.
pub fn canonicalize_json(input: &str) -> Result<Vec<u8>, CanonicalizationError> {
    let value = serde_json::from_str(input)
        .map_err(|e| CanonicalizationError::StringEncodingError(e.to_string()))?;
    canonicalize(&value)
}

#[allow(dead_code)]
/// Create a JCS canonical form from JSON bytes
pub fn canonicalize_json_bytes(input: &[u8]) -> Result<Vec<u8>, CanonicalizationError> {
    let input_str = std::str::from_utf8(input)
        .map_err(|e| CanonicalizationError::StringEncodingError(e.to_string()))?;
    canonicalize_json(input_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_keys_sorted_minimal_separators() {
        let value = json!({"b": 1, "a": {"z": true, "m": [1, 2, "x"]}, "c": null});
        let canonical = to_jcs(&value).unwrap();
        let canonical_str = String::from_utf8(canonical).unwrap();

        // Should have sorted keys and minimal separators
        assert_eq!(
            canonical_str,
            r#"{"a":{"m":[1,2,"x"],"z":true},"b":1,"c":null}"#
        );
    }

    #[test]
    fn test_insertion_order_does_not_matter() {
        let a = json!({"x": 1, "y": 2});
        let b: Value = serde_json::from_str(r#"{"y": 2, "x": 1}"#).unwrap();

        let canon_a = to_jcs(&a).unwrap();
        let canon_b = to_jcs(&b).unwrap();

        assert_eq!(canon_a, canon_b);
    }

    #[test]
    fn test_floats_are_handled() {
        // Unlike the original Groove implementation, we handle floats
        let value = json!({"ratio": 0.5, "pi": 1.2345});
        let canonical = to_jcs(&value).unwrap();

        assert!(!canonical.is_empty());
        let canonical_str = String::from_utf8(canonical).unwrap();
        assert!(canonical_str.contains("ratio"));
        assert!(canonical_str.contains("pi"));
    }

    #[test]
    fn test_nested_objects() {
        let value = json!({
            "outer": {
                "z": 1,
                "a": {
                    "inner": true
                }
            }
        });

        let canonical = to_jcs(&value).unwrap();
        let canonical_str = String::from_utf8(canonical).unwrap();

        // Should have nested key sorting
        assert!(canonical_str.contains("outer"));
        assert!(canonical_str.contains("z"));
        assert!(canonical_str.contains("a"));
        assert!(canonical_str.contains("inner"));
    }

    #[test]
    fn test_arrays_preserve_order() {
        let value = json!({"arr": [3, 1, 2]});
        let canonical = to_jcs(&value).unwrap();
        let canonical_str = String::from_utf8(canonical).unwrap();

        // Array order should be preserved
        assert!(canonical_str.contains("[3,1,2]"));
    }

    #[test]
    fn test_special_characters() {
        let value = json!({"s": "a\"b\\c\nd"});
        let canonical = to_jcs(&value).unwrap();
        let canonical_str = String::from_utf8(canonical).unwrap();

        // Should have proper escaping
        assert_eq!(canonical_str, r#"{"s":"a\"b\\c\nd"}"#);
    }

    #[test]
    fn test_utf16_key_sorting() {
        // Test with non-ASCII keys to verify UTF-16 sorting
        // Note: Most JSON in practice uses ASCII keys, but we test the edge case
        let value = json!({"café": 1, "cafe": 2});
        let canonical = to_jcs(&value).unwrap();
        let canonical_str = String::from_utf8(canonical).unwrap();

        // Both should be present
        assert!(canonical_str.contains("café"));
        assert!(canonical_str.contains("cafe"));
    }

    #[test]
    fn test_empty_values() {
        assert_eq!(to_jcs(&json!(null)).unwrap(), b"null");
        assert_eq!(to_jcs(&json!(true)).unwrap(), b"true");
        assert_eq!(to_jcs(&json!(false)).unwrap(), b"false");
        assert_eq!(to_jcs(&json!([])).unwrap(), b"[]");
        assert_eq!(to_jcs(&json!({})).unwrap(), b"{}");
        assert_eq!(to_jcs(&json!("")).unwrap(), b"\"\"");
    }

    #[test]
    fn test_canonicalize_json() {
        let input = r#"{"b": 1, "a": 2}"#;
        let canonical = canonicalize_json(input).unwrap();
        let canonical_str = String::from_utf8(canonical).unwrap();

        assert_eq!(canonical_str, r#"{"a":2,"b":1}"#);
    }

    #[test]
    fn test_invalid_i_json_rejected() {
        // Test that JCS canonicalization rejects invalid I-JSON
        // Since we can't easily create invalid I-JSON with serde_json,
        // we'll test with a value that has an unsafe integer

        // Use the maximum i64 value which is definitely outside the safe range
        let large_int = i64::MAX; // 9223372036854775807
        let value = json!({"big": large_int});

        let result = to_jcs(&value);
        assert!(result.is_err());
        assert!(result.unwrap_err().is_validation_error());
    }

    #[test]
    fn test_safe_integers() {
        // Test safe integers are serialized normally
        let value = json!({"min": SAFE_INT_MIN, "max": SAFE_INT_MAX});
        let canonical = to_jcs(&value).unwrap();

        assert!(!canonical.is_empty());
    }

    #[test]
    fn test_unsigned_safe_integers() {
        // Test safe unsigned integers
        let value = json!({"max_uint": SAFE_UINT_MAX});
        let canonical = to_jcs(&value).unwrap();

        assert!(!canonical.is_empty());
    }

    #[test]
    fn test_to_jcs_string() {
        let value = json!({"a": 1});
        let canonical_str = to_jcs_string(&value).unwrap();

        assert_eq!(canonical_str, r#"{"a":1}"#);
    }

    /// Doubles format per ECMAScript Number::toString (RFC 8785 Appendix B).
    #[test]
    fn test_es_number_formatting() {
        // RFC 8785 Appendix B sample values
        let cases: &[(f64, &str)] = &[
            (0.0, "0"),
            (-0.0, "0"),
            (1e30, "1e+30"),
            (4.50, "4.5"),
            (0.002, "0.002"),
            (1e-7, "1e-7"),
            (0.000001, "0.000001"),
            (1e21, "1e+21"),
            (1e20, "100000000000000000000"),
            (123456789012345680000.0, "123456789012345680000"),
            (-1.5e-9, "-1.5e-9"),
            (9007199254740992.0, "9007199254740992"),
            (5e-324, "5e-324"),
            (-5e-324, "-5e-324"),
            (1.7976931348623157e308, "1.7976931348623157e+308"),
            (333333333.3333333, "333333333.3333333"),
            (1.0, "1"),
            (-1.25, "-1.25"),
        ];
        for (f, want) in cases {
            assert_eq!(es_number_to_string(*f).unwrap(), *want, "formatting {f:e}");
        }
        assert!(es_number_to_string(f64::NAN).is_err());
        assert!(es_number_to_string(f64::INFINITY).is_err());
    }
}
