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
    for number in ["9007199254740992", "18446744073709551616"] {
        let input = format!(r#"{{"a":{{"":{{"":{{"b~/c":{number}}}}}}}}}"#);
        let error = parse_json(&input, JsonMode::Strict).unwrap_err();
        assert_eq!(error.as_validation_error().unwrap().path(), "a///b~0~1c");
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
fn historical_float_and_integer_token_distinction_is_explicit() {
    // Do NOT silently weaken the estate integer-token rule to make a broad
    // round-trip property pass. `fix` refuses to install such an output.
    let value = parse_json("1e20", JsonMode::Canonical).unwrap();
    let bytes = to_jcs(&value).unwrap();
    assert_eq!(bytes, b"100000000000000000000");
    assert!(parse_json_bytes(&bytes, JsonMode::Canonical).is_err());
    assert!(parse_json_bytes(&bytes, JsonMode::Loose).is_ok());
}

#[test]
fn object_container_depth_is_bounded_independently_of_array_depth() {
    let mut value = Value::Null;
    for _ in 0..MAX_DEPTH {
        value = Value::Object([("".into(), value)].into_iter().collect());
    }
    assert!(validate_i_json(&value).is_ok());
    assert!(to_jcs(&value).is_ok());
    value = Value::Object([("".into(), value)].into_iter().collect());
    let error = validate_i_json(&value).unwrap_err();
    assert_eq!(error.path(), "/".repeat(MAX_DEPTH - 1));
    assert!(matches!(error, ValidationError::NestingTooDeep { .. }));
    assert!(to_jcs(&value).is_err());
}

#[test]
fn built_unsigned_numbers_and_boolean_validator_have_firing_controls() {
    for number in [1u64 << 63, u64::MAX - 1, u64::MAX] {
        let value = json!({"unsigned": number});
        let error = validate_i_json(&value).unwrap_err();
        assert_eq!(error.path(), "unsigned");
        assert!(error.is_number_error());
        assert!(!ijson_jcs::is_i_json(&value));
        assert!(to_jcs(&value).is_err());
    }
    assert!(!ijson_jcs::is_i_json(&json!("\u{ffff}")));
    assert!(ijson_jcs::is_i_json(&json!(9_007_199_254_740_991u64)));
}
