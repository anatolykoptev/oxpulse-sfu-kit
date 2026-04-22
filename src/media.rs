//! Media payload wrapper over `str0m::media::MediaData`.
//!
//! Owned-bytes payload for inter-peer fanout. Construction from `str0m`
//! is zero-alloc via `mem::take` on the inner `Vec<u8>`.

use std::time::Instant;

use crate::ids::{SfuMid, SfuPt, SfuRid};

/// Kind of a media stream (audio vs video).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SfuMediaKind {
    /// Audio track.
    Audio,
    /// Video track.
    Video,
}

impl SfuMediaKind {
    #[allow(dead_code)]
    pub(crate) fn from_str0m(k: str0m::media::MediaKind) -> Self {
        match k {
            str0m::media::MediaKind::Audio => Self::Audio,
            str0m::media::MediaKind::Video => Self::Video,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn to_str0m(self) -> str0m::media::MediaKind {
        match self {
            Self::Audio => str0m::media::MediaKind::Audio,
            Self::Video => str0m::media::MediaKind::Video,
        }
    }
}

/// An inbound RTP media payload received from a peer, ready for fanout.
///
/// Held by the [`Propagated::MediaData`][crate::propagate::Propagated::MediaData]
/// variant (migration lands in Task 6).
#[derive(Debug)]
pub struct SfuMediaPayload {
    mid: SfuMid,
    pt: SfuPt,
    rid: Option<SfuRid>,
    data: Vec<u8>,
    network_time: Instant,
    contiguous: bool,
}

impl SfuMediaPayload {
    /// Media stream id within the sending peer's session.
    pub fn mid(&self) -> SfuMid {
        self.mid
    }

    /// Payload type (codec identifier).
    pub fn pt(&self) -> SfuPt {
        self.pt
    }

    /// Simulcast layer identifier, if this stream uses simulcast.
    pub fn rid(&self) -> Option<SfuRid> {
        self.rid
    }

    /// Raw RTP payload bytes (already depacketized by str0m).
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Wall-clock instant at which the datagram was received.
    pub fn network_time(&self) -> Instant {
        self.network_time
    }

    /// Whether this payload is contiguous with the previous one (no gap).
    pub fn contiguous(&self) -> bool {
        self.contiguous
    }

    #[allow(dead_code)]
    pub(crate) fn from_str0m(mut data: str0m::media::MediaData) -> Self {
        Self {
            mid: SfuMid::from_str0m(data.mid),
            pt: SfuPt::from_str0m(data.pt),
            rid: data.rid.map(SfuRid::from_str0m),
            data: std::mem::take(&mut data.data),
            network_time: data.network_time,
            contiguous: data.contiguous,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_kind_roundtrip() {
        for k in [
            str0m::media::MediaKind::Audio,
            str0m::media::MediaKind::Video,
        ] {
            let wrapped = SfuMediaKind::from_str0m(k);
            assert_eq!(wrapped.to_str0m(), k);
        }
    }
}
