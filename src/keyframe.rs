//! Keyframe request wrapper.
//!
//! Public surface over `str0m::media::KeyframeRequest` and
//! `str0m::media::KeyframeRequestKind` so downstream consumers don't depend
//! on str0m's type semver.

use crate::ids::{SfuMid, SfuRid};

/// The two keyframe-request mechanisms RTCP supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SfuKeyframeKind {
    /// Picture Loss Indication — lightweight, most common.
    Pli,
    /// Full Intra Request — heavier, used when PLI is unsupported.
    Fir,
}

impl SfuKeyframeKind {
    #[allow(dead_code)]
    pub(crate) fn from_str0m(k: str0m::media::KeyframeRequestKind) -> Self {
        match k {
            str0m::media::KeyframeRequestKind::Pli => Self::Pli,
            str0m::media::KeyframeRequestKind::Fir => Self::Fir,
        }
    }
    #[allow(dead_code)]
    pub(crate) fn to_str0m(self) -> str0m::media::KeyframeRequestKind {
        match self {
            Self::Pli => str0m::media::KeyframeRequestKind::Pli,
            Self::Fir => str0m::media::KeyframeRequestKind::Fir,
        }
    }
}

/// A keyframe request arriving from a subscriber, destined for a publisher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SfuKeyframeRequest {
    mid: SfuMid,
    rid: Option<SfuRid>,
    kind: SfuKeyframeKind,
}

impl SfuKeyframeRequest {
    /// Media stream this request targets on the publisher.
    pub fn mid(&self) -> SfuMid {
        self.mid
    }
    /// Simulcast layer this request targets, if simulcast is in use.
    pub fn rid(&self) -> Option<SfuRid> {
        self.rid
    }
    /// The RTCP mechanism (PLI or FIR).
    pub fn kind(&self) -> SfuKeyframeKind {
        self.kind
    }

    #[allow(dead_code)]
    pub(crate) fn from_str0m(r: str0m::media::KeyframeRequest) -> Self {
        Self {
            mid: SfuMid::from_str0m(r.mid),
            rid: r.rid.map(SfuRid::from_str0m),
            kind: SfuKeyframeKind::from_str0m(r.kind),
        }
    }
}


#[cfg(any(test, feature = "test-utils"))]
impl SfuKeyframeRequest {
    /// Construct a keyframe request for tests.
    ///
    /// Bypasses the str0m conversion path — use only in unit/integration tests.
    pub fn new_for_tests(mid: SfuMid, rid: Option<SfuRid>, kind: SfuKeyframeKind) -> Self {
        Self { mid, rid, kind }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_roundtrip() {
        for k in [
            str0m::media::KeyframeRequestKind::Pli,
            str0m::media::KeyframeRequestKind::Fir,
        ] {
            let wrapped = SfuKeyframeKind::from_str0m(k);
            assert_eq!(wrapped.to_str0m(), k);
        }
    }
}
