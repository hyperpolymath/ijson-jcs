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
// Everything else (number range, lone-surrogate escapes, UTF-8, trailing
// characters) is still enforced by serde_json's own tokenizer.

use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};
use std::fmt;

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
}
