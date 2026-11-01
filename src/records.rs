//! SPDX-License-Identifier: MPL-2.0
//! Copyright (c) 2026 Hyperpolymath Estate
//!
//! JSON Lines / NDJSON record framing.
//!
//! JSON-POLICY J-2 rules a `.jsonl` file as "a sequence of J-1 records, one per
//! line, each followed by a line feed. No blank lines." `ijson-jcs` reads whole
//! documents only, so the estate gate has to re-split the file with shell and
//! call `canon` once per line. This module is that split, in one process, with
//! the same verdicts:
//!
//! * a blank line is invalid framing, not an empty document;
//! * a line that is not I-JSON is invalid, and the report names the line;
//! * a line whose canonical form differs from the line is not canonical;
//! * a file with no final line feed is not canonical, because every canonical
//!   record ends with one;
//! * an empty file has zero records and is canonical, which is what the pinned
//!   gate reports for it.
//!
//! Framing is *not* semantic: nothing here merges records, and a file of
//! records is never canonicalised as one JSON array.
//!
//! ```rust
//! use ijson_jcs::{Conformance, FramingError, canonicalize_records, classify_records};
//!
//! let input = b"{\"b\":1,\"a\":2}\n{\"c\":3}\n";
//! assert_eq!(canonicalize_records(input).unwrap(), b"{\"a\":2,\"b\":1}\n{\"c\":3}\n");
//! assert_eq!(classify_records(input).unwrap(), Conformance::NotCanonical);
//!
//! let blank = classify_records(b"{}\n\n{}\n").unwrap_err();
//! assert_eq!(blank.to_string(), "line 2 is blank (JSON Lines has no blank lines)");
//! assert!(matches!(blank, FramingError::BlankLine { line: 2 }));
//! ```

use std::fmt;

use crate::error::IJsonError;
use crate::profile::{Conformance, canonicalize_bytes};

/// Why a JSON Lines document is not a well-framed sequence of records.
#[derive(Debug, thiserror::Error)]
pub enum FramingError {
    /// A line was empty. JSON Lines has no blank lines (JSON-POLICY J-2).
    #[error("line {line} is blank (JSON Lines has no blank lines)")]
    BlankLine {
        /// One-based line number.
        line: usize,
    },

    /// A line did not parse as I-JSON, or would not canonicalise.
    #[error("line {line} is not valid I-JSON: {source}")]
    InvalidRecord {
        /// One-based line number.
        line: usize,
        /// The parse or validation failure for that record.
        #[source]
        source: IJsonError,
    },
}

impl FramingError {
    /// The one-based line number the failure is about, when it is about one.
    pub fn line(&self) -> usize {
        match self {
            Self::BlankLine { line } | Self::InvalidRecord { line, .. } => *line,
        }
    }
}

impl From<IJsonError> for FramingError {
    /// A whole-input failure is reported as being on line 1.
    ///
    /// Used only by [`classify_records`] when the input is not UTF-8, which is
    /// a property of the bytes rather than of one line.
    fn from(source: IJsonError) -> Self {
        Self::InvalidRecord { line: 1, source }
    }
}

/// A JSON Lines document, split into its record byte slices.
///
/// The slices keep their original bytes (no trailing `\n`, and any `\r` that a
/// CRLF file left behind), so a caller can classify them without re-reading
/// the input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Records<'a> {
    lines: Vec<&'a [u8]>,
}

impl<'a> Records<'a> {
    /// Split `input` into records, rejecting blank lines.
    ///
    /// A final line feed is not required here: a document without one is a
    /// framing *finding*, which [`classify_records`] reports as
    /// [`Conformance::NotCanonical`], not a parse error.
    pub fn parse(input: &'a [u8]) -> Result<Self, FramingError> {
        let mut lines = Vec::new();
        if !input.is_empty() {
            let mut start = 0;
            for (index, byte) in input.iter().enumerate() {
                if *byte == b'\n' {
                    lines.push(&input[start..index]);
                    start = index + 1;
                }
            }
            if start < input.len() {
                lines.push(&input[start..]);
            }
        }
        for (index, line) in lines.iter().enumerate() {
            if line.is_empty() {
                return Err(FramingError::BlankLine { line: index + 1 });
            }
        }
        Ok(Self { lines })
    }

    /// The records, in document order.
    pub fn as_slice(&self) -> &[&'a [u8]] {
        &self.lines
    }

    /// The number of records.
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// Whether the document has no records at all.
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Iterate over the record byte slices.
    pub fn iter(&self) -> impl Iterator<Item = &'a [u8]> + '_ {
        self.lines.iter().copied()
    }
}

/// Split `input` into JSON Lines records, rejecting blank lines.
pub fn records(input: &[u8]) -> Result<Records<'_>, FramingError> {
    Records::parse(input)
}

/// Rewrite a JSON Lines document as one canonical record per line.
///
/// An empty document stays empty. A document without a final line feed gains
/// one; a document with blank lines or an invalid record is refused, so `fix`
/// never silently deletes a line.
pub fn canonicalize_records(input: &[u8]) -> Result<Vec<u8>, FramingError> {
    let records = Records::parse(input)?;
    let mut out = Vec::with_capacity(input.len() + 1);
    for (index, record) in records.iter().enumerate() {
        let canonical = canonicalize_bytes(record).map_err(|source| FramingError::InvalidRecord {
            line: index + 1,
            source,
        })?;
        out.extend_from_slice(&canonical);
        out.push(b'\n');
    }
    Ok(out)
}

/// Classify a JSON Lines document: canonical records, or a finding, or an error.
///
/// The comparison is against the document's own canonical rendering, so a
/// missing final line feed, a CRLF ending, insignificant whitespace and
/// unsorted members all come out as [`Conformance::NotCanonical`], while a
/// blank line or an invalid record is an error.
pub fn classify_records(input: &[u8]) -> Result<Conformance, FramingError> {
    let canonical = canonicalize_records(input)?;
    Ok(if canonical == input {
        Conformance::Canonical
    } else {
        Conformance::NotCanonical
    })
}

/// The number of records in a well-framed JSON Lines document.
pub fn record_count(input: &[u8]) -> Result<usize, FramingError> {
    Ok(Records::parse(input)?.len())
}

impl fmt::Display for Records<'_> {
    /// Renders the record count, never the record bytes.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} record(s)", self.lines.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The framing matrix the estate gate implements, verdict for verdict.
    #[test]
    fn framing_matrix_matches_the_pinned_gate() {
        // (input, expected verdict as the gate prints it)
        let cases: &[(&[u8], &str)] = &[
            (b"", "CANONICAL"),
            (b"{\"a\":1}\n{\"b\":[1,2]}\n", "CANONICAL"),
            (b"{\"a\":1}\n{\"b\":[1,2]}", "NOT_CANONICAL"),
            (b"{\"a\":1}\n{\"z\":1,\"b\":2}\n", "NOT_CANONICAL"),
            (b"{\"a\":1}\n{\"b\": 2}\n", "NOT_CANONICAL"),
            (b"{\"a\":1}\r\n{\"b\":2}\n", "NOT_CANONICAL"),
            (b"{\"a\":1}\n\n{\"b\":2}\n", "INVALID"),
            (b"\n", "INVALID"),
            (b"{\"a\":1}\n{\"b\":\n", "INVALID"),
            (b"{\"a\":1}\n{\"a\":1,\"a\":2}\n", "INVALID"),
            (b"[18446744073709551616]\n", "INVALID"),
            (b"1\n2\n", "CANONICAL"),
        ];
        for (input, want) in cases {
            let got = match classify_records(input) {
                Ok(Conformance::Canonical) => "CANONICAL",
                Ok(Conformance::NotCanonical) => "NOT_CANONICAL",
                Err(_) => "INVALID",
            };
            assert_eq!(got, *want, "{:?}", String::from_utf8_lossy(input));
        }
    }

    /// A record's canonical form is the same bytes `canon` prints for it.
    #[test]
    fn records_are_canonicalised_individually() {
        let input = b"{\"b\":1,\"a\":2}\n[3, 1, 2]\n\"x\"\n";
        let canonical = canonicalize_records(input).unwrap();
        // `b` sorts before `a` is false: JCS sorts by key, so `a` (value 2)
        // comes first and `b` (value 1) second. The values travel with the keys.
        assert_eq!(canonical, b"{\"a\":2,\"b\":1}\n[3,1,2]\n\"x\"\n");
        assert!(classify_records(&canonical).unwrap().is_canonical());
        // `to_jcs` of the whole document would be one array, not three records.
        assert_ne!(canonicalize_bytes(b"{\"b\":1,\"a\":2}").unwrap(), canonical);
    }

    /// Splitting reports the records it found, with line-accurate errors.
    #[test]
    fn split_and_error_lines() {
        let input = b"1\n2\n3";
        let split = records(input).unwrap();
        assert_eq!(split.len(), 3);
        assert!(!split.is_empty());
        assert_eq!(split.as_slice(), &[&b"1"[..], b"2", b"3"]);
        assert_eq!(split.iter().count(), 3);
        assert_eq!(split.to_string(), "3 record(s)");
        assert_eq!(record_count(input).unwrap(), 3);
        assert_eq!(record_count(b"").unwrap(), 0);
        assert!(records(b"").unwrap().is_empty());

        // A blank line is reported at its own line number, not at line 1.
        let error = canonicalize_records(b"{}\n\n{}\n").unwrap_err();
        assert_eq!(error.line(), 2);
        assert!(matches!(error, FramingError::BlankLine { line: 2 }));

        // An invalid record names its line.
        let error = canonicalize_records(b"{}\n{\"k\":1,\"k\":2}\n").unwrap_err();
        assert_eq!(error.line(), 2);
        assert!(error.to_string().contains("line 2"));
        assert!(error.to_string().contains("duplicate object key"));
    }

    /// Every record of a canonical document is itself I-JSON text.
    #[test]
    fn canonical_records_reparse() {
        let input = b"{\"a\":[1,2.5,null,true]}\n{\"b\":\"\\u20ac\"}\n";
        let canonical = canonicalize_records(input).unwrap();
        for record in records(&canonical).unwrap().iter() {
            let value = crate::parse_json_bytes(record, crate::JsonMode::Strict).unwrap();
            assert!(crate::to_jcs(&value).is_ok());
        }
    }
}
