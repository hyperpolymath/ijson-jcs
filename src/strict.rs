//! SPDX-License-Identifier: MPL-2.0
//! Copyright (c) 2026 Hyperpolymath Estate
//!
//! Duplicate-key-rejecting JSON parser
//
// `serde_json::Value` silently keeps the *last* member when an object has a
// repeated key, so a duplicate can never be detected after the fact by
// inspecting a `Value`. RFC 7493 §2.3 makes duplicate names a MUST NOT, so
// the strict parse modes go through this deserializer instead, which builds
// the same `Value` but fails as soon as a key repeats within one object.
//
// The same holds for an integer token beyond the `i64`/`u64` range:
// serde_json reads it as a double, so the `Value` holds a float and the
// integer it came from is gone. `check_integer_tokens` therefore reads the
// raw text after the parse and refuses any integer token outside
// ±(2^53−1).
//
// Everything else (double range, lone-surrogate escapes, UTF-8, trailing
// characters) is still enforced by serde_json's own tokenizer.

use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};
use std::fmt;

use crate::error::ValidationError;
use crate::ijson::{SAFE_UINT_MAX, json_pointer_encode};

/// A `serde_json::Value` that was deserialized with duplicate keys rejected.
struct StrictValue(Value);

impl<'de> Deserialize<'de> for StrictValue {
    /// Deserialize any JSON value, failing on a duplicate object key.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(StrictVisitor).map(StrictValue)
    }
}

/// Visitor that builds a `Value`, rejecting objects with repeated keys.
struct StrictVisitor;

impl<'de> Visitor<'de> for StrictVisitor {
    type Value = Value;

    /// Describe what this visitor accepts, for serde error messages.
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("any valid JSON value")
    }

    /// Accept a JSON boolean.
    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }

    /// Accept a JSON number that fits in an `i64`.
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }

    /// Accept a JSON number that fits in a `u64`.
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }

    /// Accept a JSON number parsed as a double; non-finite values are refused.
    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Value, E> {
        Number::from_f64(v)
            .map(Value::Number)
            .ok_or_else(|| E::custom("non-finite number"))
    }

    /// Accept a JSON string (borrowed or transient).
    fn visit_str<E: de::Error>(self, v: &str) -> Result<Value, E> {
        Ok(Value::String(v.to_owned()))
    }

    /// Accept a JSON string (owned).
    fn visit_string<E: de::Error>(self, v: String) -> Result<Value, E> {
        Ok(Value::String(v))
    }

    /// Accept JSON `null`.
    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    /// Accept JSON `null` reported as an absent option.
    fn visit_none<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    /// Accept a JSON array, deserializing each element strictly.
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut items = Vec::new();
        while let Some(StrictValue(item)) = seq.next_element()? {
            items.push(item);
        }
        Ok(Value::Array(items))
    }

    /// Accept a JSON object, failing if any key occurs more than once.
    fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Value, A::Error> {
        let mut map = Map::new();
        while let Some(key) = access.next_key::<String>()? {
            if map.contains_key(&key) {
                return Err(de::Error::custom(format!(
                    "duplicate object key {key:?} (forbidden by RFC 7493 §2.3)"
                )));
            }
            let StrictValue(value) = access.next_value()?;
            map.insert(key, value);
        }
        Ok(Value::Object(map))
    }
}

/// Parse a complete JSON text, rejecting duplicate object keys.
///
/// Returns the same `Value` `serde_json::from_str` would, except that an
/// object containing the same key twice is an error rather than being
/// collapsed to its last member.
pub(crate) fn from_str_no_duplicates(input: &str) -> Result<Value, serde_json::Error> {
    let mut deserializer = serde_json::Deserializer::from_str(input);
    let StrictValue(value) = StrictValue::deserialize(&mut deserializer)?;
    deserializer.end()?;
    Ok(value)
}

/// A container the integer-token scan is inside, kept to report a path.
///
/// A key is held as its raw literal, quotes and escapes included, and is
/// decoded only if an error needs the path.
enum Frame<'a> {
    /// An array, holding the index of the element being read.
    Array(usize),
    /// An object, holding the key of the member being read, or `None`
    /// while the next string is its key.
    Object(Option<&'a str>),
}

/// Refuse an integer token outside the I-JSON safe range ±(2^53−1).
///
/// `input` must already have parsed as JSON. Digits inside a string are
/// not a token. A token with `.`, `e` or `E` is a float and is accepted, as
/// `validate_i_json` accepts floats. The error is the `UnsafeInteger` that
/// `validate_i_json` reports, with the token as written and its path in
/// the same format.
pub(crate) fn check_integer_tokens(input: &str) -> Result<(), ValidationError> {
    let bytes = input.as_bytes();
    let mut stack: Vec<Frame<'_>> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                // Valid JSON: an escape is `\` plus one byte that is never `"`.
                let start = i;
                i += 1;
                while bytes[i] != b'"' {
                    i += if bytes[i] == b'\\' { 2 } else { 1 };
                }
                if let Some(Frame::Object(key @ None)) = stack.last_mut() {
                    *key = Some(&input[start..=i]);
                }
            }
            b'-' | b'0'..=b'9' => {
                let start = i;
                while i + 1 < bytes.len()
                    && matches!(bytes[i + 1], b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E')
                {
                    i += 1;
                }
                let token = &input[start..=i];
                if !token.contains(['.', 'e', 'E']) && !is_safe_integer(token) {
                    return Err(ValidationError::UnsafeInteger {
                        value: token.to_string(),
                        path: path_of(&stack),
                    });
                }
            }
            b'[' => stack.push(Frame::Array(0)),
            b'{' => stack.push(Frame::Object(None)),
            b']' | b'}' => {
                stack.pop();
            }
            b',' => match stack.last_mut() {
                Some(Frame::Array(index)) => *index += 1,
                Some(Frame::Object(key)) => *key = None,
                None => {}
            },
            _ => {}
        }
        i += 1;
    }
    Ok(())
}

/// Whether a JSON integer token lies within ±(2^53−1).
fn is_safe_integer(token: &str) -> bool {
    let magnitude = token.strip_prefix('-').unwrap_or(token);
    magnitude.parse::<u64>().is_ok_and(|m| m <= SAFE_UINT_MAX)
}

/// The path of the value being scanned, `/`-joined as `validate_i_json`
/// writes it (no leading `/`; the top-level value is the empty path).
fn path_of(stack: &[Frame<'_>]) -> String {
    stack
        .iter()
        .map(|frame| match frame {
            Frame::Array(index) => index.to_string(),
            Frame::Object(raw) => {
                let key: Option<String> = raw.and_then(|r| serde_json::from_str(r).ok());
                json_pointer_encode(&key.unwrap_or_default())
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A repeated key is rejected at top level and when nested.
    #[test]
    fn duplicate_keys_rejected_at_any_depth() {
        assert!(from_str_no_duplicates(r#"{"a":1,"a":2}"#).is_err());
        assert!(from_str_no_duplicates(r#"[{"x":{"k":1,"k":1}}]"#).is_err());
    }

    /// The same key in two different objects is not a duplicate.
    #[test]
    fn same_key_in_sibling_objects_is_fine() {
        let v = from_str_no_duplicates(r#"[{"k":1},{"k":2}]"#).unwrap();
        assert_eq!(v, serde_json::json!([{"k": 1}, {"k": 2}]));
    }

    /// On duplicate-free input the result equals `serde_json::from_str`.
    #[test]
    fn matches_serde_json_on_valid_input() {
        let text = r#"{"b":[1,-2,3.5,1e30,"s",null,true],"a":{"n":-0}}"#;
        let expected: Value = serde_json::from_str(text).unwrap();
        assert_eq!(from_str_no_duplicates(text).unwrap(), expected);
    }

    /// Characters after the JSON value are rejected.
    #[test]
    fn trailing_garbage_rejected() {
        assert!(from_str_no_duplicates("{} x").is_err());
    }

    /// The token scan skips strings and tracks keys and indices across
    /// siblings, whitespace and nesting.
    #[test]
    fn integer_token_path_follows_keys_and_indices() {
        let text = "{\"a\": [ {}, [], \"x,]\" ,\n {\"k\": 1, \"m\": 18446744073709551616} ]}";
        let err = check_integer_tokens(text).unwrap_err();
        assert_eq!(err.path(), "a/3/m");
        // A key with a unicode escape (for `b`) and an escaped quote is
        // decoded only for the path.
        let input = concat!(r#"{"a"#, "\x5Cu0062", r#"\"":[18446744073709551616]}"#);
        let err = check_integer_tokens(input).unwrap_err();
        assert_eq!(err.path(), "ab\"/0");
        assert!(check_integer_tokens("{\"a\": [ {}, [], \"x,]\", {\"k\": 1}]}").is_ok());
    }
}
