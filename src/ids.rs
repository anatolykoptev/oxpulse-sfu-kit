//! Opaque newtype wrappers for str0m identifier types.
//!
//! These exist to prevent str0m semver churn from propagating to downstream
//! consumers. Internal modules keep using str0m types directly.

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

// Conversion helpers are used by Tasks 6-7 (migration of internal modules).
#[allow(dead_code)]
impl SfuRid {
    pub(crate) fn from_str0m(r: str0m::media::Rid) -> Self {
        Self(r)
    }
    pub(crate) fn to_str0m(self) -> str0m::media::Rid {
        self.0
    }

    /// LiveKit low-resolution simulcast layer (`q`).
    pub const LOW: Self = Self(str0m::media::Rid::from_array(*b"q       "));
    /// LiveKit mid-resolution simulcast layer (`h`).
    pub const MEDIUM: Self = Self(str0m::media::Rid::from_array(*b"h       "));
    /// LiveKit full-resolution simulcast layer (`f`).
    pub const HIGH: Self = Self(str0m::media::Rid::from_array(*b"f       "));
}

#[allow(dead_code)]
impl SfuMid {
    pub(crate) fn from_str0m(m: str0m::media::Mid) -> Self {
        Self(m)
    }
    pub(crate) fn to_str0m(self) -> str0m::media::Mid {
        self.0
    }
}

#[allow(dead_code)]
impl SfuPt {
    pub(crate) fn from_str0m(p: str0m::media::Pt) -> Self {
        Self(p)
    }
    pub(crate) fn to_str0m(self) -> str0m::media::Pt {
        self.0
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
