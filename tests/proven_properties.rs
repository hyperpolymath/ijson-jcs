// SPDX-License-Identifier: MPL-2.0 AND CC-BY-SA-4.0
// Code is MPL-2.0; prose is CC-BY-SA-4.0.
//! Counted property evidence (kind R): every public entry point and every
//! CLI command, under a recorded seed (`support::SEED`) and >= 100 cases.
//! Provenance: sampled, not a proof.

mod support;

use ijson_jcs::{
    CanonicalProfile, CanonicalizationError, Conformance, FramingError, IJsonError, JSONLD_SCOPE,
    JsonLdScope, JsonMode, MAX_DEPTH, ParseMode, ValidationError, Value, canonicalize,
    canonicalize_bytes, canonicalize_jsonld_document, canonicalize_jsonld_document_to_string,
    canonicalize_records, classify, classify_records, declares_context, estate_file_bytes,
    estate_file_from_bytes, is_i_json, is_valid_i_json_bytes, is_valid_i_json_string, parse_json,
    parse_json_bytes, record_count, records, strip_estate_final_lf,
    supports_rdf_dataset_canonicalization, to_jcs, to_json_pretty_string, to_json_string,
    validate_i_json,
};
use proptest::prelude::*;
use support::{config, run, scratch, text, value};

const PROFILES: [CanonicalProfile; 3] = [
    CanonicalProfile::LibraryBytes,
    CanonicalProfile::EstateFile,
    CanonicalProfile::Tolerant,
];

fn with_lf(bytes: &[u8], n: usize) -> Vec<u8> {
    let mut out = bytes.to_vec();
    out.extend(std::iter::repeat_n(b'\n', n));
    out
}

/// One JSON Lines document from compact records (never containing a newline).
fn jsonl(values: &[Value], final_lf: bool) -> Vec<u8> {
    let lines: Vec<String> = values.iter().map(to_json_string).collect();
    let mut doc = lines.join("\n").into_bytes();
    if final_lf && !values.is_empty() {
        doc.push(b'\n');
    }
    doc
}

/// Every `ValidationError` variant, carrying `path`.
fn every_validation_error(path: &str) -> Vec<ValidationError> {
    let p = || path.to_string();
    vec![
        ValidationError::NestingTooDeep {
            limit: 128,
            path: p(),
        },
        ValidationError::DuplicateKey {
            key: "k".into(),
            path: p(),
        },
        ValidationError::UnpairedSurrogate {
            location: "l".into(),
            path: p(),
        },
        ValidationError::Noncharacter {
            location: "l".into(),
            path: p(),
        },
        ValidationError::UnsafeInteger {
            value: "1".into(),
            path: p(),
        },
        ValidationError::NonFiniteNumber {
            value: "1".into(),
            path: p(),
        },
        ValidationError::ControlCharacter {
            character: 1,
            location: "l".into(),
            path: p(),
        },
        ValidationError::InvalidUnicodeEscape {
            sequence: "s".into(),
            path: p(),
        },
        ValidationError::TopLevelNotObjectOrArray { r#type: "t".into() },
    ]
}

proptest! {
    #![proptest_config(config(256))]

    /// Profiles: canonical bytes, final-LF rules, and the name round trip.
    #[test]
    fn profiles_agree_on_canonical_bytes_and_final_lf(v in value()) {
        let jcs = to_jcs(&v).unwrap();
        prop_assert_eq!(canonicalize(&v).unwrap(), jcs.clone());
        let pretty = to_json_pretty_string(&v).into_bytes();
        prop_assert_eq!(canonicalize_bytes(&pretty).unwrap(), jcs.clone());
        prop_assert_eq!(estate_file_from_bytes(&pretty).unwrap(), with_lf(&jcs, 1));
        prop_assert_eq!(estate_file_bytes(&v).unwrap(), with_lf(&jcs, 1));
        let terminated = with_lf(&jcs, 1);
        prop_assert_eq!(strip_estate_final_lf(&terminated), Some(&jcs[..]));
        prop_assert_eq!(strip_estate_final_lf(&jcs), None);
        let verdict = |bytes: &[u8], p| classify(bytes, p).unwrap();
        use CanonicalProfile::*;
        use Conformance::*;
        prop_assert_eq!(verdict(&jcs, LibraryBytes), Canonical);
        prop_assert_eq!(verdict(&with_lf(&jcs, 1), LibraryBytes), NotCanonical);
        prop_assert_eq!(verdict(&jcs, EstateFile), NotCanonical);
        prop_assert_eq!(verdict(&with_lf(&jcs, 1), EstateFile), Canonical);
        prop_assert_eq!(verdict(&jcs, Tolerant), Canonical);
        prop_assert_eq!(verdict(&with_lf(&jcs, 1), Tolerant), Canonical);
        prop_assert_eq!(verdict(&with_lf(&jcs, 2), Tolerant), NotCanonical);
        for p in PROFILES {
            prop_assert_eq!(CanonicalProfile::from_name(p.as_str()), Some(p));
            prop_assert_eq!(p.to_string(), p.as_str());
            let c = verdict(&pretty, p);
            prop_assert_eq!(c.to_string(), c.as_str());
            prop_assert_eq!(c.is_canonical(), c == Canonical);
            prop_assert_eq!(
                c.as_str(),
                if c.is_canonical() { "canonical" } else { "not-canonical" }
            );
        }
    }

    /// Profile names outside the documented set and aliases are refused.
    #[test]
    fn unknown_profile_names_are_refused(name in "[a-z-]{0,14}") {
        let known = ["library", "library-bytes", "estate-file", "file", "tolerant", "check"];
        prop_assert_eq!(
            CanonicalProfile::from_name(&name).is_some(),
            known.contains(&name.as_str())
        );
    }

    /// Records: framing, counting, canonical rewrite and the line of each fault.
    #[test]
    fn records_frame_count_and_locate_faults(
        vs in prop::collection::vec(value(), 1..6),
        at in any::<prop::sample::Index>(),
    ) {
        let n = vs.len();
        let canonical: Vec<u8> = vs.iter().flat_map(|v| with_lf(&to_jcs(v).unwrap(), 1)).collect();
        let doc = jsonl(&vs, true);
        let parsed = records(&doc).unwrap();
        prop_assert_eq!(parsed.len(), n);
        prop_assert!(!parsed.is_empty());
        prop_assert_eq!(parsed.as_slice().len(), n);
        prop_assert_eq!(parsed.iter().count(), n);
        prop_assert_eq!(parsed.to_string(), format!("{n} record(s)"));
        prop_assert_eq!(record_count(&doc).unwrap(), n);
        prop_assert_eq!(canonicalize_records(&doc).unwrap(), canonical.clone());
        prop_assert_eq!(classify_records(&canonical).unwrap(), Conformance::Canonical);
        prop_assert_eq!(canonicalize_records(&canonical).unwrap(), canonical.clone());
        // A missing final line feed is a finding, not an error.
        let unterminated = jsonl(&vs, false);
        prop_assert_eq!(record_count(&unterminated).unwrap(), n);
        prop_assert_eq!(classify_records(&unterminated).unwrap(), Conformance::NotCanonical);
        // A blank line or a broken record is refused at its one-based line.
        let k = at.index(n);
        let mut lines: Vec<Vec<u8>> = vs.iter().map(|v| to_json_string(v).into_bytes()).collect();
        let mut blank = lines.clone();
        blank.insert(k, Vec::new());
        let blank_doc = with_lf(&blank.join(&b'\n'), 1);
        let err = records(&blank_doc).unwrap_err();
        prop_assert!(matches!(err, FramingError::BlankLine { .. }), "line 135");
        prop_assert_eq!(err.line(), k + 1);
        prop_assert!(err.to_string().contains(&format!("line {}", k + 1)), "line 137");
        prop_assert_eq!(record_count(&blank_doc).unwrap_err().line(), k + 1);
        lines[k] = b"{".to_vec();
        let broken = with_lf(&lines.join(&b'\n'), 1);
        let err = canonicalize_records(&broken).unwrap_err();
        prop_assert!(matches!(err, FramingError::InvalidRecord { .. }), "line 142");
        prop_assert_eq!(err.line(), k + 1);
        prop_assert_eq!(classify_records(&broken).unwrap_err().line(), k + 1);
    }

    /// The whole-input conversion lands on line 1.
    #[test]
    fn whole_input_failures_are_line_one(bytes in prop::collection::vec(0x80u8..=0xff, 1..8)) {
        let source = parse_json_bytes(&bytes, JsonMode::Strict).unwrap_err();
        let framed = FramingError::from(source);
        prop_assert_eq!(framed.line(), 1);
        prop_assert!(records(b"").unwrap().is_empty());
        prop_assert_eq!(classify_records(b"").unwrap(), Conformance::Canonical);
    }

    /// JSON-LD: document-bytes scope only; `@context` is detected, never resolved.
    #[test]
    fn jsonld_is_document_jcs_and_detects_context(v in value(), ctx in text()) {
        let pretty = to_json_pretty_string(&v).into_bytes();
        let jcs = to_jcs(&v).unwrap();
        prop_assert_eq!(canonicalize_jsonld_document(&pretty).unwrap(), jcs.clone());
        prop_assert_eq!(
            canonicalize_jsonld_document_to_string(&pretty).unwrap(),
            String::from_utf8(jcs).unwrap()
        );
        let has = matches!(&v, Value::Object(m) if m.contains_key("@context"));
        prop_assert_eq!(declares_context(&v), has);
        let mut doc = serde_json::Map::new();
        doc.insert("@context".into(), Value::String(ctx));
        doc.insert("body".into(), v);
        prop_assert!(declares_context(&Value::Object(doc)));
        prop_assert!(canonicalize_jsonld_document_to_string(b"{").is_err(), "line 173");
        prop_assert_eq!(JSONLD_SCOPE, JsonLdScope::DocumentBytes);
        prop_assert_eq!(JSONLD_SCOPE.as_str(), "jsonld-document-bytes");
        prop_assert!(!JSONLD_SCOPE.covers_rdf_dataset());
        prop_assert!(!supports_rdf_dataset_canonicalization());
    }

    /// Every mode round-trips serialised values; the boolean helpers agree.
    #[test]
    fn modes_round_trip_and_helpers_agree(v in value()) {
        let compact = to_json_string(&v);
        let pretty = to_json_pretty_string(&v);
        for mode in [JsonMode::Loose, JsonMode::Strict, JsonMode::Canonical] {
            prop_assert_eq!(parse_json(&compact, mode).unwrap(), v.clone());
            prop_assert_eq!(parse_json_bytes(pretty.as_bytes(), mode).unwrap(), v.clone());
            prop_assert_eq!(ParseMode { mode }.to_string(), format!("{mode:?}"));
            prop_assert_eq!(mode.to_string(), format!("{mode:?}"));
        }
        prop_assert!(is_valid_i_json_string(&compact));
        prop_assert!(is_valid_i_json_bytes(pretty.as_bytes()));
    }

    /// Unsafe integers fail as number validation errors with a path, in text
    /// and in a constructed `Value` (which no parser can produce).
    #[test]
    fn unsafe_integers_are_number_errors(
        n in 9_007_199_254_740_992u64..=u64::MAX,
        key in "[a-z]{1,6}",
    ) {
        let text = format!("{{\"{key}\":[{n}]}}");
        for mode in [JsonMode::Strict, JsonMode::Canonical] {
            let err = parse_json(&text, mode).unwrap_err();
            prop_assert!(err.is_validation_error());
            prop_assert!(!err.is_parse_error() && !err.is_encoding_error());
            let inner = err.as_validation_error().unwrap();
            prop_assert!(inner.is_number_error() && !inner.is_string_error());
            prop_assert!(!inner.is_duplicate_key());
            prop_assert_eq!(inner.path(), format!("{key}/0"));
        }
        prop_assert!(!is_valid_i_json_string(&text));
        let constructed = Value::Array(vec![Value::Number(n.into())]);
        let err = validate_i_json(&constructed).unwrap_err();
        prop_assert!(err.is_number_error());
        prop_assert_eq!(err.path(), "0");
        prop_assert!(to_jcs(&constructed).unwrap_err().is_validation_error());
    }

    /// Invalid UTF-8 is an encoding error, via the parser and via `From`.
    #[test]
    fn invalid_utf8_is_an_encoding_error(
        prefix in "[a-z]{0,4}",
        bad in 0x80u8..=0xbf,
    ) {
        let mut bytes = prefix.clone().into_bytes();
        bytes.push(bad);
        let err = parse_json_bytes(&bytes, JsonMode::Canonical).unwrap_err();
        prop_assert!(err.is_encoding_error() && !err.is_parse_error());
        prop_assert!(err.as_validation_error().is_none());
        prop_assert!(!is_valid_i_json_bytes(&bytes));
        let utf8 = std::str::from_utf8(&bytes).unwrap_err();
        let converted = IJsonError::from(utf8);
        prop_assert!(converted.is_encoding_error() && !converted.is_validation_error());
        let malformed = format!("[{prefix}");
        let err = parse_json(&malformed, JsonMode::Strict).unwrap_err();
        prop_assert!(err.is_parse_error() && !err.is_validation_error());
    }

    /// Error classification is total over every variant.
    #[test]
    fn error_classification_is_total(path in "[a-z0-9/~]{0,12}") {
        for e in every_validation_error(&path) {
            let top = matches!(e, ValidationError::TopLevelNotObjectOrArray { .. });
            prop_assert_eq!(e.path(), if top { "" } else { path.as_str() });
            let classes = [e.is_duplicate_key(), e.is_string_error(), e.is_number_error()];
            prop_assert!(classes.iter().filter(|c| **c).count() <= 1);
            let wrapped = IJsonError::from(e.clone());
            prop_assert!(wrapped.is_validation_error());
            prop_assert_eq!(wrapped.as_validation_error(), Some(&e));
            let canon = CanonicalizationError::from(e);
            prop_assert!(canon.is_validation_error());
            let outer = IJsonError::from(canon);
            prop_assert!(outer.is_validation_error() && outer.as_validation_error().is_none());
        }
        for other in [
            CanonicalizationError::NumberFormatError(path.clone()),
            CanonicalizationError::StringEncodingError(path.clone()),
            CanonicalizationError::CyclicReference,
            CanonicalizationError::BufferError,
        ] {
            prop_assert!(!other.is_validation_error());
            prop_assert!(!other.to_string().is_empty());
        }
    }
}

proptest! {
    #![proptest_config(config(256))]

    /// Constructed values: nesting at the limit passes, one deeper fails at
    /// the path of the first container past the limit.
    #[test]
    fn constructed_nesting_obeys_the_depth_limit(
        extra in 0usize..40,
        objects in prop::collection::vec(any::<bool>(), MAX_DEPTH + 40),
    ) {
        let build = |depth: usize| {
            let mut v = Value::Null;
            for level in (0..depth).rev() {
                v = if objects[level] {
                    Value::Object([("k".to_string(), v)].into_iter().collect())
                } else {
                    Value::Array(vec![v])
                };
            }
            v
        };
        prop_assert!(is_i_json(&build(MAX_DEPTH)));
        let deep = build(MAX_DEPTH + 1 + extra);
        prop_assert!(!is_i_json(&deep));
        let err = validate_i_json(&deep).unwrap_err();
        prop_assert!(matches!(err, ValidationError::NestingTooDeep { limit: MAX_DEPTH, .. }), "nesting");
        let want: Vec<&str> = objects[..MAX_DEPTH].iter().map(|o| if *o { "k" } else { "0" }).collect();
        prop_assert_eq!(err.path(), want.join("/"));
    }

    /// Constructed strings holding any Unicode noncharacter are refused.
    #[test]
    fn constructed_noncharacters_are_string_errors(
        class in 0u32..3,
        offset in 0u32..32,
        plane in 0u32..17,
        low in any::<bool>(),
        key in "[a-z]{1,4}",
    ) {
        let cp = match class {
            0 => 0xfdd0 + offset,
            1 => 0xfffe + u32::from(low),
            _ => (plane << 16) | (0xfffe + u32::from(low)),
        };
        let c = char::from_u32(cp).unwrap();
        let v = Value::Object([(key.clone(), Value::String(format!("x{c}")))].into_iter().collect());
        let err = validate_i_json(&v).unwrap_err();
        prop_assert!(matches!(err, ValidationError::Noncharacter { .. }), "noncharacter");
        prop_assert!(err.is_string_error());
        prop_assert_eq!(err.path(), key.as_str());
        prop_assert!(!is_i_json(&v));
        prop_assert!(to_jcs(&v).is_err());
        let ok = Value::Object([(key, Value::String(format!("x{}", char::from_u32(cp - 2).unwrap())))].into_iter().collect());
        prop_assert_eq!(is_i_json(&ok), !(0xfdd2..=0xfdef).contains(&cp));
    }

    /// A raw serde_json parse error converts to a parse-class error.
    #[test]
    fn serde_errors_convert_to_parse_errors(junk in "[a-z{}\\[\\]:,]{1,8}") {
        if let Err(source) = serde_json::from_str::<Value>(&junk) {
            let err = IJsonError::from(source);
            prop_assert!(err.is_parse_error() && !err.is_validation_error() && !err.is_encoding_error());
            prop_assert!(err.to_string().starts_with("JSON parse error"));
        }
    }
}

proptest! {
    #![proptest_config(config(128))]

    /// CLI: canon/jsonld print JCS; check/fix agree with the library per profile.
    #[test]
    fn cli_commands_agree_with_the_library(v in value(), vs in prop::collection::vec(value(), 1..4)) {
        let dir = scratch("prop");
        let jcs = to_jcs(&v).unwrap();
        let pretty = to_json_pretty_string(&v).into_bytes();

        let out = run(&dir, &["canon"], &pretty);
        prop_assert_eq!(out.status.code(), Some(0));
        prop_assert_eq!(&out.stdout, &jcs);
        let out = run(&dir, &["jsonld"], &pretty);
        prop_assert_eq!(out.status.code(), Some(0));
        prop_assert_eq!(&out.stdout, &jcs);

        std::fs::write(dir.join("doc.json"), &pretty).unwrap();
        for (profile, want) in [
            ("library", classify(&pretty, CanonicalProfile::LibraryBytes)),
            ("estate-file", classify(&pretty, CanonicalProfile::EstateFile)),
            ("tolerant", classify(&pretty, CanonicalProfile::Tolerant)),
        ] {
            let out = run(&dir, &["--profile", profile, "check", "doc.json"], b"");
            let canonical = want.unwrap().is_canonical();
            prop_assert_eq!(out.status.code(), Some(if canonical { 0 } else { 1 }));
            let line = if canonical { "OK doc.json\n" } else { "NOT CANONICAL doc.json\n" };
            prop_assert_eq!(String::from_utf8_lossy(&out.stdout), line);
        }
        let out = run(&dir, &["fix", "doc.json"], b"");
        prop_assert_eq!(out.status.code(), Some(0));
        prop_assert_eq!(std::fs::read(dir.join("doc.json")).unwrap(), with_lf(&jcs, 1));
        let out = run(&dir, &["fix", "doc.json"], b"");
        prop_assert_eq!(String::from_utf8_lossy(&out.stdout), "OK doc.json\n");

        let doc = jsonl(&vs, false);
        std::fs::write(dir.join("doc.jsonl"), &doc).unwrap();
        let want = classify_records(&doc).unwrap();
        let out = run(&dir, &["check-jsonl", "doc.jsonl"], b"");
        prop_assert_eq!(out.status.code(), Some(if want.is_canonical() { 0 } else { 1 }));
        let out = run(&dir, &["fix-jsonl", "doc.jsonl"], b"");
        prop_assert_eq!(out.status.code(), Some(0));
        prop_assert_eq!(String::from_utf8_lossy(&out.stdout), "FIXED doc.jsonl\n");
        let fixed = std::fs::read(dir.join("doc.jsonl")).unwrap();
        prop_assert_eq!(&fixed, &canonicalize_records(&doc).unwrap());
        let out = run(&dir, &["check-jsonl", "doc.jsonl"], b"");
        prop_assert_eq!(String::from_utf8_lossy(&out.stdout), "OK doc.jsonl\n");
        let out = run(&dir, &["fix-jsonl", "doc.jsonl"], b"");
        prop_assert_eq!(String::from_utf8_lossy(&out.stdout), "OK doc.jsonl\n");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
