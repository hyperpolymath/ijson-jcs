// SPDX-License-Identifier: MPL-2.0
//! Proven-tests doctrine: each declared defect has clean and firing controls.

mod fixtures;

use ijson_jcs::{
    JsonMode, MAX_DEPTH, ValidationError, Value, parse_json, parse_json_bytes, to_jcs,
    validate_i_json,
};
use serde_json::json;

#[test]
fn clean_fault_pairs_have_the_expected_diagnosis() {
    assert!(!fixtures::PAIRS.is_empty());
    for pair in fixtures::PAIRS {
        for mode in [JsonMode::Strict, JsonMode::Canonical] {
            let clean = parse_json_bytes(pair.clean, mode)
                .unwrap_or_else(|e| panic!("harness fired on {}: {e}", pair.class));
            assert!(to_jcs(&clean).is_ok(), "{}", pair.class);
            let failure = parse_json_bytes(pair.fault, mode).expect_err(pair.class);
            assert!(
                failure.to_string().contains(pair.diagnostic),
                "{} was misclassified: {failure}",
                pair.class
            );
        }
    }
}

#[test]
fn empty_path_segments_survive_nesting_and_sibling_visits() {
    for number in [9007199254740992u64, 18446744073709551615] {
        let value = json!({"a": {"": {"": {"b~/c": number}}}});
        let error = validate_i_json(&value).unwrap_err();
        assert_eq!(error.path(), "a///b~0~1c");
    }
    let value = json!({"a": {"": [], "b": {"": "\u{ffff}"}}});
    assert_eq!(validate_i_json(&value).unwrap_err().path(), "a/b/");
}

#[test]
fn constructed_values_are_depth_bounded_before_serialization() {
    let mut value = Value::Null;
    for _ in 0..MAX_DEPTH {
        value = Value::Array(vec![value]);
    }
    assert!(validate_i_json(&value).is_ok());
    assert!(to_jcs(&value).is_ok());
    value = Value::Array(vec![value]);
    assert!(matches!(
        validate_i_json(&value),
        Err(ValidationError::NestingTooDeep { .. })
    ));
    assert!(to_jcs(&value).is_err());
}

#[test]
fn text_depth_limit_is_not_disabled() {
    let deep = format!("{}0{}", "[".repeat(256), "]".repeat(256));
    for mode in [JsonMode::Loose, JsonMode::Strict, JsonMode::Canonical] {
        assert!(parse_json(&deep, mode).unwrap_err().is_parse_error());
    }
}

#[test]
fn integral_doubles_round_trip_through_every_strict_mode() {
    // Owner ruling (0.2.0): Strict and Canonical read the RFC 8785 binary64
    // model, so every byte string `to_jcs` emits reparses.
    let value = parse_json("1e20", JsonMode::Canonical).unwrap();
    let bytes = to_jcs(&value).unwrap();
    assert_eq!(bytes, b"100000000000000000000");
    for mode in [JsonMode::Strict, JsonMode::Canonical, JsonMode::Loose] {
        let again = parse_json_bytes(&bytes, mode).unwrap();
        assert_eq!(to_jcs(&again).unwrap(), bytes, "{mode}");
    }
}
