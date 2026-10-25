// SPDX-License-Identifier: MPL-2.0
//! Shrinking, replayable sampled properties. Provenance: Unproven, not a proof.

use ijson_jcs::{JsonMode, Value, canonicalize, parse_json, parse_json_bytes, to_jcs};
use proptest::prelude::*;
use proptest::test_runner::RngSeed;

fn text() -> impl Strategy<Value = String> {
    prop::collection::vec(any::<char>(), 0..32).prop_map(|chars| {
        chars.into_iter().filter(|c| {
            let n = *c as u32;
            !(0xfdd0..=0xfdef).contains(&n) && n & 0xffff < 0xfffe
        }).collect()
    })
}

fn value() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        (-9_007_199_254_740_991i64..=9_007_199_254_740_991).prop_map(|n| Value::Number(n.into())),
        // This property covers the subset closed under the strict token rule.
        (-1e15f64..1e15).prop_map(|f| Value::Number(serde_json::Number::from_f64(f).unwrap())),
        text().prop_map(Value::String),
    ];
    leaf.prop_recursive(4, 64, 8, |inner| prop_oneof![
        prop::collection::vec(inner.clone(), 0..8).prop_map(Value::Array),
        prop::collection::btree_map(text(), inner, 0..8)
            .prop_map(|entries| Value::Object(entries.into_iter().collect())),
    ])
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 512,
        rng_seed: RngSeed::Fixed(0x494a_534f_4e4a_4353),
        ..ProptestConfig::default()
    })]

    #[test]
    fn strict_profile_canonicalization_is_idempotent(input in value()) {
        let first = to_jcs(&input).unwrap();
        let parsed = parse_json_bytes(&first, JsonMode::Canonical).unwrap();
        prop_assert_eq!(&first, &to_jcs(&parsed).unwrap());
        prop_assert_eq!(&first, &canonicalize(&input).unwrap());
    }

    #[test]
    fn insignificant_whitespace_does_not_change_signed_bytes(input in value()) {
        let pretty = serde_json::to_string_pretty(&input).unwrap();
        let parsed = parse_json(&pretty, JsonMode::Strict).unwrap();
        prop_assert_eq!(to_jcs(&parsed).unwrap(), to_jcs(&input).unwrap());
    }

    #[test]
    fn duplicate_names_never_survive_strict_parsing(key in text()) {
        let quoted = serde_json::to_string(&key).unwrap();
        let input = format!("{{{quoted}:1,{quoted}:2}}");
        prop_assert!(parse_json(&input, JsonMode::Strict).is_err());
        prop_assert!(parse_json(&input, JsonMode::Canonical).is_err());
        prop_assert_eq!(parse_json(&input, JsonMode::Loose).unwrap()[key], 2);
    }

    #[test]
    fn arbitrary_bytes_never_panic(bytes in prop::collection::vec(any::<u8>(), 0..4096)) {
        if let Ok(value) = parse_json_bytes(&bytes, JsonMode::Canonical) {
            prop_assert!(serde_json::from_slice::<Value>(&bytes).is_ok());
            prop_assert!(to_jcs(&value).is_ok());
        }
    }
}
