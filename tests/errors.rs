// SPDX-License-Identifier: MPL-2.0
//! Exercise public diagnostic classifiers, including legacy constructible types.
use ijson_jcs::{CanonicalizationError, IJsonError, JsonMode, ParseMode, ValidationError};

#[test]
fn validation_classifiers_and_paths_have_positive_and_negative_controls() {
    let path = "a~1b/".to_owned();
    let cases = [
        (
            ValidationError::DuplicateKey {
                key: "key".into(),
                path: path.clone(),
            },
            true,
            false,
            false,
        ),
        (
            ValidationError::UnpairedSurrogate {
                location: "lone".into(),
                path: path.clone(),
            },
            false,
            true,
            false,
        ),
        (
            ValidationError::Noncharacter {
                location: "U+FFFF".into(),
                path: path.clone(),
            },
            false,
            true,
            false,
        ),
        (
            ValidationError::ControlCharacter {
                character: 0,
                location: "legacy".into(),
                path: path.clone(),
            },
            false,
            true,
            false,
        ),
        (
            ValidationError::InvalidUnicodeEscape {
                sequence: "bad".into(),
                path: path.clone(),
            },
            false,
            true,
            false,
        ),
        (
            ValidationError::UnsafeInteger {
                value: "9007199254740992".into(),
                path: path.clone(),
            },
            false,
            false,
            true,
        ),
        (
            ValidationError::NonFiniteNumber {
                value: "NaN".into(),
                path: path.clone(),
            },
            false,
            false,
            true,
        ),
        (
            ValidationError::NestingTooDeep {
                limit: 128,
                path: path.clone(),
            },
            false,
            false,
            false,
        ),
    ];
    for (error, duplicate, string, number) in cases {
        assert_eq!(error.path(), path);
        assert_eq!(error.is_duplicate_key(), duplicate);
        assert_eq!(error.is_string_error(), string);
        assert_eq!(error.is_number_error(), number);
        assert!(!error.to_string().is_empty());
    }
    let top = ValidationError::TopLevelNotObjectOrArray {
        r#type: "null".into(),
    };
    assert_eq!(top.path(), "");
    assert!(!top.is_duplicate_key() && !top.is_string_error() && !top.is_number_error());
}

#[test]
fn top_level_error_classification_is_not_just_a_positive_assertion() {
    let parse_error = serde_json::from_str::<serde_json::Value>("{").unwrap_err();
    let parse: IJsonError = parse_error.into();
    assert!(parse.is_parse_error());
    assert!(!parse.is_validation_error() && !parse.is_encoding_error());
    assert!(parse.as_validation_error().is_none());

    let invalid = ijson_jcs::parse_json_bytes(&[0xff], JsonMode::Canonical).unwrap_err();
    let IJsonError::EncodingError { source, mode } = invalid else {
        panic!("wrong error type")
    };
    assert_eq!(mode, JsonMode::Canonical);
    let encoding: IJsonError = source.into();
    assert!(encoding.is_encoding_error());
    assert!(!encoding.is_parse_error() && !encoding.is_validation_error());
    assert!(encoding.as_validation_error().is_none());
    assert!(matches!(
        encoding,
        IJsonError::EncodingError {
            mode: JsonMode::Loose,
            ..
        }
    ));

    let validation = ValidationError::Noncharacter {
        location: "U+FFFF".into(),
        path: "x".into(),
    };
    let error: IJsonError = validation.clone().into();
    assert_eq!(error.as_validation_error(), Some(&validation));
    assert!(error.is_validation_error());
    assert!(!error.is_parse_error() && !error.is_encoding_error());

    let canonical: CanonicalizationError = validation.into();
    assert!(canonical.is_validation_error());
    let error: IJsonError = canonical.into();
    assert!(error.is_validation_error());
    assert!(!error.is_parse_error() && !error.is_encoding_error());
    assert!(error.as_validation_error().is_none());
    for error in [
        CanonicalizationError::NumberFormatError("number".into()),
        CanonicalizationError::StringEncodingError("string".into()),
        CanonicalizationError::CyclicReference,
        CanonicalizationError::BufferError,
    ] {
        assert!(!error.is_validation_error());
        assert!(!error.to_string().is_empty());
    }
    for (mode, expected) in [
        (JsonMode::Loose, "Loose"),
        (JsonMode::Strict, "Strict"),
        (JsonMode::Canonical, "Canonical"),
    ] {
        assert_eq!(ParseMode { mode }.to_string(), expected);
    }
}
