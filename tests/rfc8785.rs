// SPDX-License-Identifier: MPL-2.0
//! RFC 8785 Appendix B: raw binary64 inputs, not rounded decimal literals.
//! These are standards examples, not a claim of proof over all binary64 values.

use ijson_jcs::{JsonMode, Value, parse_json, to_jcs_string};

#[test]
fn appendix_b_binary64_vectors() {
    let cases: &[(u64, &str)] = &[
        (0x0000000000000000, "0"),
        (0x8000000000000000, "0"),
        (0x0000000000000001, "5e-324"),
        (0x8000000000000001, "-5e-324"),
        (0x7fefffffffffffff, "1.7976931348623157e+308"),
        (0xffefffffffffffff, "-1.7976931348623157e+308"),
        (0x4340000000000000, "9007199254740992"),
        (0xc340000000000000, "-9007199254740992"),
        (0x4430000000000000, "295147905179352830000"),
        (0x44b52d02c7e14af5, "9.999999999999997e+22"),
        (0x44b52d02c7e14af6, "1e+23"),
        (0x44b52d02c7e14af7, "1.0000000000000001e+23"),
        (0x444b1ae4d6e2ef4e, "999999999999999700000"),
        (0x444b1ae4d6e2ef4f, "999999999999999900000"),
        (0x444b1ae4d6e2ef50, "1e+21"),
        (0x3eb0c6f7a0b5ed8c, "9.999999999999997e-7"),
        (0x3eb0c6f7a0b5ed8d, "0.000001"),
        (0x41b3de4355555553, "333333333.3333332"),
        (0x41b3de4355555554, "333333333.33333325"),
        (0x41b3de4355555555, "333333333.3333333"),
        (0x41b3de4355555556, "333333333.3333334"),
        (0x41b3de4355555557, "333333333.33333343"),
        (0xbecbf647612f3696, "-0.0000033333333333333333"),
        (0x43143ff3c1cb0959, "1424953923781206.2"),
    ];
    for &(bits, want) in cases {
        let number = serde_json::Number::from_f64(f64::from_bits(bits)).unwrap();
        assert_eq!(to_jcs_string(&Value::Number(number)).unwrap(), want, "{bits:016x}");
    }
}

#[test]
fn decimal_ties_choose_even_not_rust_generic_formatting() {
    let value = parse_json("1424953923781206.25", JsonMode::Canonical).unwrap();
    assert_eq!(to_jcs_string(&value).unwrap(), "1424953923781206.2");
}

#[test]
fn utf16_order_is_not_scalar_order_and_arrays_do_not_sort() {
    let input = r#"{"\u20ac":"Euro Sign","\r":"Carriage Return","\ufb33":"Hebrew Letter Dalet With Dagesh","1":"One","\ud83d\ude00":"Emoji: Grinning Face","\u0080":"Control","\u00f6":"Latin Small Letter O With Diaeresis"}"#;
    let value = parse_json(input, JsonMode::Canonical).unwrap();
    assert_eq!(to_jcs_string(&value).unwrap(), "{\"\\r\":\"Carriage Return\",\"1\":\"One\",\"\u{0080}\":\"Control\",\"ö\":\"Latin Small Letter O With Diaeresis\",\"€\":\"Euro Sign\",\"😀\":\"Emoji: Grinning Face\",\"דּ\":\"Hebrew Letter Dalet With Dagesh\"}");
    let value = parse_json(r#"[{"\ue000":1,"\ud800\udc00":2},3,1,2]"#, JsonMode::Canonical).unwrap();
    assert_eq!(to_jcs_string(&value).unwrap(), "[{\"𐀀\":2,\"\u{e000}\":1},3,1,2]");
}

#[test]
fn canonicalization_does_not_normalize_unicode_or_resolve_json_ld() {
    let value = parse_json(r#"{"é":1,"e\u0301":2,"@context":"https://example.invalid/context"}"#, JsonMode::Canonical).unwrap();
    assert_eq!(to_jcs_string(&value).unwrap(), "{\"@context\":\"https://example.invalid/context\",\"e\u{0301}\":2,\"é\":1}");
}

#[test]
fn all_escaped_controls_are_allowed_but_raw_controls_are_not() {
    for code in 0..32 {
        let escaped = format!(r#""\u{code:04x}""#);
        let value = parse_json(&escaped, JsonMode::Canonical).unwrap();
        let canonical = to_jcs_string(&value).unwrap();
        assert_eq!(parse_json(&canonical, JsonMode::Strict).unwrap(), value);
        let raw = format!("\"{}\"", char::from_u32(code).unwrap());
        assert!(parse_json(&raw, JsonMode::Strict).is_err());
    }
}
