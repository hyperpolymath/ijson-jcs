//! SPDX-License-Identifier: MPL-2.0
//! Copyright (c) 2026 Hyperpolymath Estate
//!
//! Library canonical bytes versus the estate file profile.
//!
//! Two different byte strings are both "canonical" depending on what they are
//! for, and conflating them is a real defect class: a hash taken over a file
//! that still carries its trailing line feed is not the hash of the JCS bytes,
//! and a file with no trailing line feed is not estate-canonical even though
//! [`check`](crate::parse_json) would accept it.
//!
//! | Profile | Bytes | Used for |
//! |---|---|---|
//! | [`CanonicalProfile::LibraryBytes`] | `to_jcs(value)` | hashing, signing, the wire |
//! | [`CanonicalProfile::EstateFile`] | `to_jcs(value)` + exactly one `\n` | files on disk (JSON-POLICY J-1) |
//! | [`CanonicalProfile::Tolerant`] | either | the historical `ijson-jcs check` rule |
//!
//! The historical rule is kept as the default so that consumers which already
//! accept `b` or `b\n` ([metadatastician/standards] `scripts/check-ijson-jcs.sh`
//! and the pinned CLI contract in kyq) keep working unchanged. A gate that
//! claims to enforce JSON-POLICY J-1 must ask for
//! [`CanonicalProfile::EstateFile`]; `check`'s exit 0 alone is not that rule.
//! That script currently re-implements the missing-line-feed probe with
//! `tail -c 1`; `check --profile estate-file` is the same test in one process.
//!
//! [metadatastician/standards]: https://github.com/metadatastician/standards
//!
//! ```rust
//! use ijson_jcs::{CanonicalProfile, classify, estate_file_bytes, to_jcs};
//! use serde_json::json;
//!
//! let value = json!({"b": 1, "a": 2});
//! let library = to_jcs(&value).unwrap();
//! assert_eq!(library, b"{\"a\":2,\"b\":1}");
//! assert_eq!(estate_file_bytes(&value).unwrap(), b"{\"a\":2,\"b\":1}\n");
//!
//! // The same bytes are canonical for one profile and not for the other.
//! let verdict = |bytes: &[u8], profile| classify(bytes, profile).unwrap().is_canonical();
//! assert!(verdict(b"{\"a\":2,\"b\":1}", CanonicalProfile::LibraryBytes));
//! assert!(!verdict(b"{\"a\":2,\"b\":1}", CanonicalProfile::EstateFile));
//! assert!(verdict(b"{\"a\":2,\"b\":1}\n", CanonicalProfile::EstateFile));
//! assert!(!verdict(b"{\"a\":2,\"b\":1}\n", CanonicalProfile::LibraryBytes));
//! // Both byte strings are canonical for the historical tolerant rule.
//! assert!(verdict(b"{\"a\":2,\"b\":1}", CanonicalProfile::Tolerant));
//! assert!(verdict(b"{\"a\":2,\"b\":1}\n", CanonicalProfile::Tolerant));
//! ```

use std::fmt;

use serde_json::Value;

use crate::error::{CanonicalizationError, IJsonError};
use crate::{JsonMode, parse_json_bytes, to_jcs};

/// The single byte the estate file profile appends to canonical bytes.
///
/// JSON-POLICY J-1: "followed by exactly one line feed". Two of them, `\r\n`,
/// or a trailing space are not this byte string.
pub const FINAL_LF: u8 = b'\n';

/// Which canonical form a caller is asking about.
///
/// See the [module documentation](self) for the difference between the
/// library bytes and the estate file profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CanonicalProfile {
    /// `to_jcs(value)` exactly: no trailing line feed. This is what you hash
    /// or sign.
    LibraryBytes,

    /// `to_jcs(value)` followed by exactly one `\n`: the estate file profile
    /// (JSON-POLICY J-1), which is what a file on disk must contain.
    EstateFile,

    /// Either of the two forms above: the rule `ijson-jcs check` has always
    /// applied, and the only rule that may be assumed of existing consumers.
    #[default]
    Tolerant,
}

impl CanonicalProfile {
    /// The stable name of this profile, as accepted by the CLI's `--profile`
    /// and written into receipts and vectors.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LibraryBytes => "library",
            Self::EstateFile => "estate-file",
            Self::Tolerant => "tolerant",
        }
    }

    /// Parse a profile name; `None` for anything that is not one.
    ///
    /// The aliases exist because both spellings appear in the estate
    /// documentation ("library bytes", "file profile").
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "library" | "library-bytes" => Some(Self::LibraryBytes),
            "estate-file" | "file" => Some(Self::EstateFile),
            "tolerant" | "check" => Some(Self::Tolerant),
            _ => None,
        }
    }

    /// The canonical bytes of `value` under this profile.
    ///
    /// [`Self::Tolerant`] has no single byte string; it returns the library
    /// bytes, and [`classify`] is the only correct way to test that profile.
    pub fn canonical_bytes(self, value: &Value) -> Result<Vec<u8>, CanonicalizationError> {
        let mut bytes = to_jcs(value)?;
        if self == Self::EstateFile {
            bytes.push(FINAL_LF);
        }
        Ok(bytes)
    }
}

impl fmt::Display for CanonicalProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The verdict for bytes that parsed as I-JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Conformance {
    /// The bytes are exactly canonical for the profile asked about.
    Canonical,

    /// The bytes are valid I-JSON but not canonical for that profile: extra
    /// whitespace, unsorted members, a missing or doubled final line feed.
    NotCanonical,
}

impl Conformance {
    /// The stable name, for CLI output, receipts and differential vectors.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Canonical => "canonical",
            Self::NotCanonical => "not-canonical",
        }
    }

    /// Whether the verdict is [`Self::Canonical`].
    pub const fn is_canonical(self) -> bool {
        matches!(self, Self::Canonical)
    }
}

impl fmt::Display for Conformance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Classify `input` against `profile`.
///
/// Invalid I-JSON is an error, never [`Conformance::NotCanonical`]: the caller
/// must be able to tell "rewrite this" from "refuse this".
pub fn classify(input: &[u8], profile: CanonicalProfile) -> Result<Conformance, IJsonError> {
    let value = parse_json_bytes(input, JsonMode::Canonical)?;
    let canonical = profile.canonical_bytes(&value)?;
    Ok(if is_canonical_form(input, &canonical, profile) {
        Conformance::Canonical
    } else {
        Conformance::NotCanonical
    })
}

/// Whether `input` is the canonical byte string, in a form `profile` allows.
fn is_canonical_form(input: &[u8], canonical: &[u8], profile: CanonicalProfile) -> bool {
    if input == canonical {
        return true;
    }
    profile == CanonicalProfile::Tolerant
        && input
            .strip_suffix(&[FINAL_LF])
            .is_some_and(|stripped| stripped == canonical)
}

/// Library canonical bytes for JSON text: no trailing line feed.
///
/// This is `canon` and `to_jcs` output, and the bytes to hash.
pub fn canonicalize_bytes(input: &[u8]) -> Result<Vec<u8>, IJsonError> {
    let value = parse_json_bytes(input, JsonMode::Canonical)?;
    Ok(to_jcs(&value)?)
}

/// Estate file bytes for JSON text: canonical bytes plus exactly one `\n`.
///
/// This is what `fix` writes and what JSON-POLICY J-1 requires on disk.
pub fn estate_file_from_bytes(input: &[u8]) -> Result<Vec<u8>, IJsonError> {
    let mut bytes = canonicalize_bytes(input)?;
    bytes.push(FINAL_LF);
    Ok(bytes)
}

/// Estate file bytes for a value you built: `to_jcs` plus exactly one `\n`.
pub fn estate_file_bytes(value: &Value) -> Result<Vec<u8>, CanonicalizationError> {
    CanonicalProfile::EstateFile.canonical_bytes(value)
}

/// The library bytes inside an estate-canonical file: its one final `\n` removed.
///
/// `None` when the input does not end with exactly one line feed, i.e. when it
/// is not an estate file at all. This does not verify the bytes; check them
/// with [`classify`] and [`CanonicalProfile::EstateFile`] first.
pub fn strip_estate_final_lf(input: &[u8]) -> Option<&[u8]> {
    input.strip_suffix(&[FINAL_LF])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The three profiles disagree exactly where the policy says they do.
    #[test]
    fn profiles_differ_on_the_final_line_feed() {
        for (bytes, library, estate_file, tolerant) in [
            (
                &b"{\"a\":1}"[..],
                Conformance::Canonical,
                Conformance::NotCanonical,
                Conformance::Canonical,
            ),
            (
                &b"{\"a\":1}\n"[..],
                Conformance::NotCanonical,
                Conformance::Canonical,
                Conformance::Canonical,
            ),
            (
                &b"{\"a\":1}\n\n"[..],
                Conformance::NotCanonical,
                Conformance::NotCanonical,
                Conformance::NotCanonical,
            ),
            (
                &b"{\"a\":1}\r\n"[..],
                Conformance::NotCanonical,
                Conformance::NotCanonical,
                Conformance::NotCanonical,
            ),
            (
                &b"{\"a\": 1}\n"[..],
                Conformance::NotCanonical,
                Conformance::NotCanonical,
                Conformance::NotCanonical,
            ),
            (
                &b"{\"b\":1,\"a\":2}\n"[..],
                Conformance::NotCanonical,
                Conformance::NotCanonical,
                Conformance::NotCanonical,
            ),
        ] {
            let verdict = |profile| {
                classify(bytes, profile).unwrap_or_else(|e| panic!("{profile} {bytes:?}: {e}"))
            };
            assert_eq!(
                verdict(CanonicalProfile::LibraryBytes),
                library,
                "library {bytes:?}"
            );
            assert_eq!(
                verdict(CanonicalProfile::EstateFile),
                estate_file,
                "estate-file {bytes:?}"
            );
            assert_eq!(
                verdict(CanonicalProfile::Tolerant),
                tolerant,
                "tolerant {bytes:?}"
            );
        }
    }

    /// Invalid I-JSON is an error under every profile, never `NotCanonical`.
    #[test]
    fn invalid_is_an_error_for_every_profile() {
        for bad in [
            &b""[..],
            b"{\"a\":1,\"a\":2}\n",
            b"{\"a\":1}\n trailing",
            b"[18446744073709551616]\n",
            b"[1e400]\n",
        ] {
            for profile in [
                CanonicalProfile::LibraryBytes,
                CanonicalProfile::EstateFile,
                CanonicalProfile::Tolerant,
            ] {
                assert!(
                    classify(bad, profile).is_err(),
                    "{profile} accepted {bad:?}"
                );
            }
        }
    }

    /// Round-tripping each profile's own output classifies as canonical.
    #[test]
    fn each_profile_round_trips() {
        let value = json!({"b": [1, 2.5, null], "a": "x"});
        for profile in [CanonicalProfile::LibraryBytes, CanonicalProfile::EstateFile] {
            let bytes = profile.canonical_bytes(&value).unwrap();
            assert!(
                classify(&bytes, profile).unwrap().is_canonical(),
                "{profile}"
            );
            assert_eq!(canonicalize_bytes(&bytes).unwrap(), to_jcs(&value).unwrap());
        }
        let text = b"{ \"a\" : 1 }";
        assert_eq!(estate_file_from_bytes(text).unwrap(), b"{\"a\":1}\n");
        assert_eq!(canonicalize_bytes(text).unwrap(), b"{\"a\":1}");
        assert_eq!(
            strip_estate_final_lf(b"{\"a\":1}\n"),
            Some(&b"{\"a\":1}"[..])
        );
        assert_eq!(strip_estate_final_lf(b"{\"a\":1}"), None);
    }

    /// The profile names are stable, and only known names parse.
    #[test]
    fn profile_names_are_stable() {
        for profile in [
            CanonicalProfile::LibraryBytes,
            CanonicalProfile::EstateFile,
            CanonicalProfile::Tolerant,
        ] {
            assert_eq!(CanonicalProfile::from_name(profile.as_str()), Some(profile));
            assert_eq!(profile.to_string(), profile.as_str());
        }
        assert_eq!(
            CanonicalProfile::from_name("library-bytes"),
            Some(CanonicalProfile::LibraryBytes)
        );
        assert_eq!(
            CanonicalProfile::from_name("file"),
            Some(CanonicalProfile::EstateFile)
        );
        assert_eq!(CanonicalProfile::from_name(""), None);
        assert_eq!(CanonicalProfile::from_name("strict"), None);
        assert_eq!(CanonicalProfile::default(), CanonicalProfile::Tolerant);
        assert!(Conformance::Canonical.is_canonical());
        assert!(!Conformance::NotCanonical.is_canonical());
        assert_eq!(Conformance::NotCanonical.to_string(), "not-canonical");
    }
}
