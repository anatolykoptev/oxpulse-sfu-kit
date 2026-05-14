//! Opaque newtype wrappers for str0m identifier types.
//!
//! These exist to prevent str0m semver churn from propagating to downstream
//! consumers. Internal modules keep using str0m types directly.
//!
//! # Interoperability with str0m
//!
//! Consumers that use str0m types directly (e.g. when wiring
//! [`PacerAction::ChangeLayer`][crate::bwe::PacerAction::ChangeLayer] into a
//! str0m-backed pipeline) can convert between kit types and str0m types via
//! the [`From`] impls on each wrapper:
//!
//! ```
//! use oxpulse_sfu_kit::{SfuRid, SfuMid, SfuPt};
//!
//! // SfuRid -> str0m
//! let rid: str0m::media::Rid = SfuRid::LOW.into();
//! // str0m -> SfuRid
//! let back: SfuRid = rid.into();
//! assert_eq!(back, SfuRid::LOW);
//! ```

use std::fmt;
use std::str::FromStr;

/// Simulcast layer identifier (RFC 8852 RID).
///
/// Constructors accept the conventional `"q"` / `"h"` / `"f"` quality tags
/// plus any other short ASCII string str0m accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SfuRid(str0m::media::Rid);

/// Media stream identifier within a single peer connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SfuMid(str0m::media::Mid);

/// RTP payload type (codec binding within a session).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SfuPt(str0m::media::Pt);

impl FromStr for SfuRid {
    type Err = InvalidRid;

    /// Parse a simulcast layer identifier from a string.
    ///
    /// Accepts ASCII alphanumeric strings of length 1..=8 bytes.
    /// Rejects:
    /// - empty input
    /// - any character outside `[A-Za-z0-9]` (RFC 8852 restricts RID to alphanumeric)
    /// - input longer than 8 bytes (str0m's internal limit)
    ///
    /// This is stricter than str0m's own `Rid::from(&str)` which silently
    /// mangles non-alphanumeric characters and truncates overlong input.
    /// The wrapper enforces the contract explicitly so roundtrips are faithful.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err(InvalidRid);
        }
        if s.len() > 8 {
            return Err(InvalidRid);
        }
        if !s.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return Err(InvalidRid);
        }
        Ok(SfuRid(str0m::media::Rid::from(s)))
    }
}

impl fmt::Display for SfuRid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// Error returned when a string cannot be parsed as a [`SfuRid`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidRid;

impl fmt::Display for InvalidRid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid RID: must be short ASCII")
    }
}

impl std::error::Error for InvalidRid {}

impl SfuRid {
    /// LiveKit low-resolution simulcast layer (`q`).
    pub const LOW: Self = Self(str0m::media::Rid::from_array(*b"q       "));
    /// LiveKit mid-resolution simulcast layer (`h`).
    pub const MEDIUM: Self = Self(str0m::media::Rid::from_array(*b"h       "));
    /// LiveKit full-resolution simulcast layer (`f`).
    pub const HIGH: Self = Self(str0m::media::Rid::from_array(*b"f       "));

    // Internal helpers kept for existing crate-internal callers.
    pub(crate) fn from_str0m(r: str0m::media::Rid) -> Self {
        Self(r)
    }

    pub(crate) fn to_str0m(self) -> str0m::media::Rid {
        self.0
    }
}

/// Convert a [`SfuRid`] to the underlying `str0m` [`Rid`][str0m::media::Rid].
///
/// This is the primary interop path for consumers that pattern-match on
/// [`PacerAction::ChangeLayer`][crate::bwe::PacerAction::ChangeLayer] and
/// need to pass the resulting layer identifier back into a `str0m`-backed
/// pipeline (e.g. as a key into `MediaData.rid` or `Rtc` track maps).
///
/// The conversion is lossless and zero-cost.
impl From<SfuRid> for str0m::media::Rid {
    fn from(rid: SfuRid) -> Self {
        rid.0
    }
}

/// Convert a `str0m` [`Rid`][str0m::media::Rid] to a [`SfuRid`].
///
/// Accepts any `Rid` str0m produces, including non-standard values. For
/// consumer code that receives a `Rid` from str0m and needs to compare it
/// against [`SfuRid::LOW`] / [`SfuRid::MEDIUM`] / [`SfuRid::HIGH`].
///
/// No validation is performed — the `Rid` is wrapped as-is. If you are
/// constructing a `SfuRid` from user-supplied strings, use
/// [`SfuRid::from_str`] instead so validation fires.
impl From<str0m::media::Rid> for SfuRid {
    fn from(rid: str0m::media::Rid) -> Self {
        Self(rid)
    }
}

impl SfuMid {
    // Internal helpers.
    pub(crate) fn from_str0m(m: str0m::media::Mid) -> Self {
        Self(m)
    }

    pub(crate) fn to_str0m(self) -> str0m::media::Mid {
        self.0
    }
}

impl std::str::FromStr for SfuMid {
    type Err = std::convert::Infallible;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(str0m::media::Mid::from(s)))
    }
}

/// Convert a [`SfuMid`] to the underlying `str0m` [`Mid`][str0m::media::Mid].
impl From<SfuMid> for str0m::media::Mid {
    fn from(mid: SfuMid) -> Self {
        mid.0
    }
}

/// Convert a `str0m` [`Mid`][str0m::media::Mid] to a [`SfuMid`].
impl From<str0m::media::Mid> for SfuMid {
    fn from(mid: str0m::media::Mid) -> Self {
        Self(mid)
    }
}

impl SfuPt {
    // Internal helpers.
    pub(crate) fn from_str0m(p: str0m::media::Pt) -> Self {
        Self(p)
    }

    pub(crate) fn to_str0m(self) -> str0m::media::Pt {
        self.0
    }
}

/// Convert a [`SfuPt`] to the underlying `str0m` [`Pt`][str0m::media::Pt].
impl From<SfuPt> for str0m::media::Pt {
    fn from(pt: SfuPt) -> Self {
        pt.0
    }
}

/// Convert a `str0m` [`Pt`][str0m::media::Pt] to a [`SfuPt`].
impl From<str0m::media::Pt> for SfuPt {
    fn from(pt: str0m::media::Pt) -> Self {
        Self(pt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rid_roundtrip() {
        let rid = "h".parse::<SfuRid>().expect("parse h");
        assert_eq!(rid.to_string(), "h");
        let raw = rid.to_str0m();
        let back = SfuRid::from_str0m(raw);
        assert_eq!(rid, back);
    }

    #[test]
    fn rid_from_trait_roundtrip() {
        let rid = SfuRid::MEDIUM;
        let raw: str0m::media::Rid = rid.into();
        let back: SfuRid = raw.into();
        assert_eq!(back, SfuRid::MEDIUM);
    }

    #[test]
    fn mid_from_trait_roundtrip() {
        let raw = str0m::media::Mid::from("0");
        let mid: SfuMid = raw.into();
        let back: str0m::media::Mid = mid.into();
        assert_eq!(back, raw);
    }

    #[test]
    fn pt_from_trait_roundtrip() {
        let raw = str0m::media::Pt::from(96u8);
        let pt: SfuPt = raw.into();
        let back: str0m::media::Pt = pt.into();
        assert_eq!(back, raw);
    }

    #[test]
    fn rid_rejects_empty() {
        assert!("".parse::<SfuRid>().is_err());
    }

    #[test]
    fn rid_rejects_non_alphanumeric() {
        assert!(SfuRid::from_str("low-res").is_err());
        assert!(SfuRid::from_str("a b").is_err());
        assert!(SfuRid::from_str("x!").is_err());
    }

    #[test]
    fn rid_rejects_overlong() {
        // 9 bytes > 8-byte str0m limit
        assert!(SfuRid::from_str("123456789").is_err());
    }

    #[test]
    fn rid_accepts_all_alphanumeric() {
        for s in &["q", "h", "f", "a1", "LAYER1", "12345678"] {
            assert!(SfuRid::from_str(s).is_ok(), "expected {s} to parse");
        }
    }

    #[test]
    fn rid_roundtrip_fidelity() {
        // With strict validation, display MUST match input for all accepted values.
        for s in &["q", "h", "f", "hi1080"] {
            let rid: SfuRid = s.parse().expect("parse");
            assert_eq!(rid.to_string(), *s);
        }
    }

    #[test]
    fn mid_roundtrip() {
        let raw = str0m::media::Mid::from("0");
        let mid = SfuMid::from_str0m(raw);
        assert_eq!(mid.to_str0m(), raw);
    }

    #[test]
    fn pt_roundtrip() {
        // Pt implements From<u8> via str0m's num_id! macro.
        let raw = str0m::media::Pt::from(96u8);
        let pt = SfuPt::from_str0m(raw);
        assert_eq!(pt.to_str0m(), raw);
    }
}
