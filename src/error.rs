//! SPDX-License-Identifier: MPL-2.0
//! Copyright (c) 2026 Hyperpolymath Estate
//!
//! Comprehensive error types for I-JSON and JCS operations

use serde_json::Error as SerdeJsonError;
use std::fmt;
use std::str::Utf8Error;
use thiserror::Error;

use crate::JsonMode;

/// Top-level error type for I-JSON operations
#[derive(Debug, Error)]
pub enum IJsonError {
    /// JSON parsing failed
    #[error("JSON parse error: {source}")]
    ParseError {
        /// The underlying serde_json parse error
        source: SerdeJsonError,
    },

    /// Input is not valid UTF-8
    #[error("Input encoding error: {source}")]
    EncodingError {
        /// The UTF-8 error
        source: Utf8Error,
        /// The parsing mode in use
        mode: JsonMode,
    },

    /// I-JSON validation failed
    #[error("I-JSON validation error: {0}")]
    ValidationError(#[from] ValidationError),

    /// Canonicalization error
    #[error("Canonicalization error: {0}")]
    CanonicalizationError(#[from] CanonicalizationError),
}

impl IJsonError {
    /// Returns true if this error is a validation error (not parsing)
    pub fn is_validation_error(&self) -> bool {
        matches!(
            self,
            Self::ValidationError(_) | Self::CanonicalizationError(_)
        )
    }

    /// Returns true if this error is a parsing error
    pub fn is_parse_error(&self) -> bool {
        matches!(self, Self::ParseError { .. })
    }

    /// Returns true if this error is an encoding error
    pub fn is_encoding_error(&self) -> bool {
        matches!(self, Self::EncodingError { .. })
    }

    /// Returns the underlying validation error if this is one
    pub fn as_validation_error(&self) -> Option<&ValidationError> {
        if let Self::ValidationError(e) = self {
            Some(e)
        } else {
            None
        }
    }
}

/// Error type for I-JSON validation failures
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// Object contains duplicate keys
    #[error("Duplicate key in JSON object: {key}")]
    DuplicateKey {
        /// The duplicate key that was found
        key: String,
        /// The path to the duplicate key (JSON Pointer format)
        path: String,
    },

    /// String contains unpaired surrogate code points (U+D800..U+DFFF)
    #[error("String contains unpaired surrogate: {location}")]
    UnpairedSurrogate {
        /// Description of where the surrogate was found
        location: String,
        /// The path to the invalid string
        path: String,
    },

    /// String contains noncharacter code points
    #[error("String contains noncharacter: {location}")]
    Noncharacter {
        /// Description of the noncharacter found
        location: String,
        /// The path to the invalid string
        path: String,
    },

    /// Number is outside the safe integer range for IEEE-754 binary64
    /// Safe range: [-(2^53)+1, (2^53)-1] = [-9007199254740991, 9007199254740991]
    #[error(
        "Number outside safe integer range: {value} (must be in [-(2^53)+1, (2^53)-1] for exact IEEE-754 binary64 representation)"
    )]
    UnsafeInteger {
        /// The value that is outside the safe range
        value: String,
        /// The path to the invalid number
        path: String,
    },

    /// Number is NaN or Infinity (not allowed in I-JSON or RFC 8259 JSON)
    #[error("Non-finite number: {value}")]
    NonFiniteNumber {
        /// The invalid number value
        value: String,
        /// The path to the invalid number
        path: String,
    },

    /// JSON contains a control character (U+0000..U+001F) outside allowed contexts
    /// Allowed control characters in I-JSON strings: U+0009 (TAB), U+000A (LF), U+000D (CR)
    #[error("Control character in string: U+{character:04X} at {location}")]
    ControlCharacter {
        /// The control character code point
        character: u32,
        /// Where it was found
        location: String,
        /// The path to the invalid string
        path: String,
    },

    /// Top-level value is not an object or array (I-JSON recommendation)
    #[error("Top-level value should be object or array (I-JSON recommendation), got {type}")]
    TopLevelNotObjectOrArray {
        /// The actual type found
        r#type: String,
    },

    /// String contains invalid Unicode escape sequence
    #[error("Invalid Unicode escape sequence: {sequence}")]
    InvalidUnicodeEscape {
        /// The invalid escape sequence
        sequence: String,
        /// The path to the invalid string
        path: String,
    },
}

impl ValidationError {
    /// Returns the JSON Pointer path to the validation error
    pub fn path(&self) -> &str {
        match self {
            Self::DuplicateKey { path, .. }
            | Self::UnpairedSurrogate { path, .. }
            | Self::Noncharacter { path, .. }
            | Self::UnsafeInteger { path, .. }
            | Self::NonFiniteNumber { path, .. }
            | Self::ControlCharacter { path, .. }
            | Self::InvalidUnicodeEscape { path, .. } => path,
            Self::TopLevelNotObjectOrArray { .. } => "",
        }
    }

    /// Returns true if this is a duplicate key error
    pub fn is_duplicate_key(&self) -> bool {
        matches!(self, Self::DuplicateKey { .. })
    }

    /// Returns true if this is a string validation error
    pub fn is_string_error(&self) -> bool {
        matches!(
            self,
            Self::UnpairedSurrogate { .. }
                | Self::Noncharacter { .. }
                | Self::ControlCharacter { .. }
                | Self::InvalidUnicodeEscape { .. }
        )
    }

    /// Returns true if this is a number validation error
    pub fn is_number_error(&self) -> bool {
        matches!(
            self,
            Self::UnsafeInteger { .. } | Self::NonFiniteNumber { .. }
        )
    }
}

/// Error type for JCS canonicalization failures
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CanonicalizationError {
    /// I-JSON validation failed during canonicalization
    #[error("Cannot canonicalize non-I-JSON data: {0}")]
    ValidationError(#[from] ValidationError),

    /// Number format error (should not occur with valid I-JSON)
    #[error("Number format error: {0}")]
    NumberFormatError(String),

    /// String encoding error during canonicalization
    #[error("String encoding error: {0}")]
    StringEncodingError(String),

    /// Cyclic reference detected (should not occur with serde_json::Value)
    #[error("Cyclic reference detected")]
    CyclicReference,

    /// Output buffer error
    #[error("Output buffer error")]
    BufferError,
}

impl CanonicalizationError {
    /// Returns true if this is a validation error
    pub fn is_validation_error(&self) -> bool {
        matches!(self, Self::ValidationError(_))
    }
}

/// Parse mode context for errors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ParseMode {
    /// The parsing mode in use
    pub mode: JsonMode,
}

impl fmt::Display for ParseMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.mode)
    }
}

/// Conversion from UTF-8 errors to I-JSON errors
impl From<Utf8Error> for IJsonError {
    fn from(error: Utf8Error) -> Self {
        Self::EncodingError {
            source: error,
            mode: JsonMode::default(),
        }
    }
}

/// Conversion from serde_json errors to I-JSON errors
impl From<SerdeJsonError> for IJsonError {
    fn from(error: SerdeJsonError) -> Self {
        Self::ParseError { source: error }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_classification() {
        let dup_key = ValidationError::DuplicateKey {
            key: "test".to_string(),
            path: "/test".to_string(),
        };
        assert!(dup_key.is_duplicate_key());
        assert!(!dup_key.is_string_error());
        assert!(!dup_key.is_number_error());

        let nonchar = ValidationError::Noncharacter {
            location: "U+FFFF".to_string(),
            path: "/string".to_string(),
        };
        assert!(!nonchar.is_duplicate_key());
        assert!(nonchar.is_string_error());
        assert!(!nonchar.is_number_error());

        let unsafe_int = ValidationError::UnsafeInteger {
            value: "123456789012345678901234567890".to_string(),
            path: "/big".to_string(),
        };
        assert!(!unsafe_int.is_duplicate_key());
        assert!(!unsafe_int.is_string_error());
        assert!(unsafe_int.is_number_error());
    }

    #[test]
    fn test_error_conversions() {
        let val_error = ValidationError::DuplicateKey {
            key: "test".to_string(),
            path: "/".to_string(),
        };

        let ijson_error: IJsonError = val_error.clone().into();
        assert!(ijson_error.is_validation_error());
        assert!(ijson_error.as_validation_error().is_some());

        let canon_error: CanonicalizationError = val_error.into();
        assert!(canon_error.is_validation_error());
    }

    #[test]
    fn test_json_mode_display() {
        assert_eq!(format!("{}", JsonMode::Loose), "Loose");
        assert_eq!(format!("{}", JsonMode::Strict), "Strict");
        assert_eq!(format!("{}", JsonMode::Canonical), "Canonical");
    }
}
