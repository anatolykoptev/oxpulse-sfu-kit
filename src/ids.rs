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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SfuMid(str0m::media::Mid);

/// RTP payload type (codec binding within a session).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SfuPt(str0m::media::Pt);

impl FromStr for SfuRid {
    type Err = InvalidRid;

    /// Parse from a `str` slice (RID is a short ASCII identifier).
    ///
    /// Returns [`Err(InvalidRid)`][InvalidRid] if `s` is empty.
    fn from_str(s: &str) -> Result<Self, InvalidRid> {
        if s.is_empty() {
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
}
