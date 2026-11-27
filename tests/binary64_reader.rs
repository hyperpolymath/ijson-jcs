// SPDX-License-Identifier: MPL-2.0
// SPDX-FileCopyrightText: 2026 Jonathan D.A. Jewell
use ijson_jcs::{JsonMode, Value, parse_jcs, parse_json, to_jcs_string};
use proptest::prelude::*;

#[test]
fn binary64_model_applies_to_every_parse_mode() {
    for (input, expected) in [
        ("9007199254740992", "9007199254740992"),
        ("9007199254740993", "9007199254740992"),
        ("-9007199254740993", "-9007199254740992"),
        ("100000000000000000000", "100000000000000000000"),
        ("18446744073709551616", "18446744073709552000"),
    ] {
        assert_eq!(to_jcs_string(&parse_jcs(input).unwrap()).unwrap(), expected);
        for mode in [JsonMode::Strict, JsonMode::Canonical] {
            let value = parse_json(input, mode).unwrap();
            assert_eq!(to_jcs_string(&value).unwrap(), expected, "{mode}: {input}");
        }
    }
    let source = "1e16";
    let canonical = to_jcs_string(&parse_json(source, JsonMode::Strict).unwrap()).unwrap();
    assert_eq!(canonical, "10000000000000000");
    assert!(parse_json(&canonical, JsonMode::Strict).is_ok());
    assert_eq!(
        to_jcs_string(&parse_jcs(&canonical).unwrap()).unwrap(),
        canonical
    );
}

#[test]
fn binary64_applies_inside_arrays_and_objects() {
    let parsed =
        parse_jcs(r#"{"a":[9007199254740993,{"b":-9007199254740993}],"s":"9007199254740993"}"#)
            .unwrap();
    assert_eq!(
        to_jcs_string(&parsed).unwrap(),
        r#"{"a":[9007199254740992,{"b":-9007199254740992}],"s":"9007199254740993"}"#
    );
    assert_eq!(
        parse_jcs("-0").unwrap().as_f64().unwrap().to_bits(),
        (-0.0f64).to_bits()
    );
}

#[test]
fn binary64_retains_structural_and_unicode_rejections() {
    for input in [
        r#"{"a":1,"\u0061":2}"#,
        r#"{"nested":{"x":1,"x":2}}"#,
        r#""\ud800""#,
        r#""\ufdd0""#,
        r#"{"\uffff":0}"#,
        "1e400",
        "-1e400",
        "NaN",
        "Infinity",
        "0 1",
        "[1,]",
        "{",
    ] {
        assert!(parse_jcs(input).is_err(), "{input}");
    }
}

#[test]
fn binary64_underflow_rounds_to_zero_and_drops_sign() {
    for input in ["1e-400", "-1e-400", "0e0", "-0.0", "2e-324", "[1e-400]"] {
        for mode in [JsonMode::Strict, JsonMode::Canonical] {
            let value = parse_json(input, mode).unwrap();
            let expected = if input.starts_with('[') { "[0]" } else { "0" };
            assert_eq!(to_jcs_string(&value).unwrap(), expected, "{mode}: {input}");
        }
        assert!(parse_jcs(input).is_ok(), "{input}");
    }
    assert_eq!(parse_jcs("5e-324").unwrap().as_f64(), Some(5e-324));
}

#[test]
fn binary64_reader_array_nesting_boundary() {
    let nested_arrays = |depth| format!("{}0{}", "[".repeat(depth), "]".repeat(depth));

    parse_jcs(&nested_arrays(127)).expect("127 nested arrays should parse");
    for depth in [128, 129] {
        let error = parse_jcs(&nested_arrays(depth)).unwrap_err();
        assert!(
            error.is_parse_error(),
            "expected a parse error at depth {depth}, got {error}"
        );
    }
}

proptest! {
    #[test]
    fn finite_binary64_canonical_bytes_round_trip(bits in any::<u64>()) {
        let number = f64::from_bits(bits);
        if let Some(number) = serde_json::Number::from_f64(number) {
            let canonical = to_jcs_string(&Value::Number(number)).unwrap();
            let reparsed = parse_jcs(&canonical).unwrap();
            prop_assert_eq!(to_jcs_string(&reparsed).unwrap(), canonical);
            let expected_bits = if bits == 0x8000_0000_0000_0000 { 0 } else { bits };
            prop_assert_eq!(reparsed.as_f64().unwrap().to_bits(), expected_bits);
        }
    }
}
