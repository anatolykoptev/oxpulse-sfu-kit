//! Media payload wrapper over `str0m::media::MediaData`.
//!
//! Owned-bytes payload for inter-peer fanout. Construction from `str0m`
//! is zero-alloc via `mem::take` on the inner `Vec<u8>`.

use std::time::Instant;

use str0m::format::PayloadParams;
use str0m::media::MediaTime;

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
/// variant.
#[derive(Debug)]
pub struct SfuMediaPayload {
    mid: SfuMid,
    pt: SfuPt,
    rid: Option<SfuRid>,
    data: Vec<u8>,
    network_time: Instant,
    contiguous: bool,
    /// RTP timestamp — required by str0m's writer at the fanout write site.
    time: MediaTime,
    /// Negotiated codec parameters — required for `writer.match_params` at fanout.
    params: PayloadParams,
    /// Parsed AV1 Dependency Descriptor info, if the `av1-dd` feature is enabled.
    #[cfg(feature = "av1-dd")]
    av1_dd: Option<crate::av1::Av1DdInfo>,
    /// Parsed RFC 9626 Video Frame Marking header extension, if present.
    #[cfg(feature = "vfm")]
    vfm_fm: Option<crate::vfm::FrameMarkingInfo>,
    /// RFC 6464 audio level from the RTP header extension (str0m negated-dBov).
    ///
    /// Stored as "0 = loudest, -127 = silent". None if the extension is absent.
    audio_level: Option<i8>,
}

impl SfuMediaPayload {
    /// Media stream id within the sending peer's session.
    #[must_use]
    pub fn mid(&self) -> SfuMid {
        self.mid
    }

    /// Payload type (codec identifier).
    #[must_use]
    pub fn pt(&self) -> SfuPt {
        self.pt
    }

    /// Simulcast layer identifier, if this stream uses simulcast.
    #[must_use]
    pub fn rid(&self) -> Option<SfuRid> {
        self.rid
    }

    /// Raw RTP payload bytes (already depacketized by str0m).
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Wall-clock instant at which the datagram was received.
    #[must_use]
    pub fn network_time(&self) -> Instant {
        self.network_time
    }

    /// Whether this payload is contiguous with the previous one (no gap).
    #[must_use]
    pub fn contiguous(&self) -> bool {
        self.contiguous
    }

    /// Parsed AV1 Dependency Descriptor, if present and the `av1-dd` feature is enabled.
    ///
    /// Returns `None` if the packet carries no DD extension, the codec is not AV1,
    /// or str0m 0.18 does not yet surface the extension in `ExtensionValues`.
    #[cfg(feature = "av1-dd")]
    #[cfg_attr(docsrs, doc(cfg(feature = "av1-dd")))]
    #[must_use]
    pub fn av1_dd(&self) -> Option<crate::av1::Av1DdInfo> {
        self.av1_dd
    }

    /// Parsed RFC 9626 Video Frame Marking header extension, if present.
    ///
    /// Returns `None` if the packet carries no frame marking extension,
    /// or str0m 0.18 does not surface it in `ExtensionValues`.
    #[cfg(feature = "vfm")]
    #[cfg_attr(docsrs, doc(cfg(feature = "vfm")))]
    #[must_use]
    pub fn vfm_frame_marking(&self) -> Option<crate::vfm::FrameMarkingInfo> {
        self.vfm_fm
    }

    /// RFC 6464 audio level from the RTP header extension.
    ///
    /// Returns the raw negated-dBov value as stored by str0m:
    ///  = loudest,  = silent. Returns  if the extension was
    /// absent or this is a video packet.
    ///
    /// To convert to the detector's 0-127 dBov scale (0=loud, 127=silent):
    /// 
    #[must_use]
    pub fn audio_level_raw(&self) -> Option<i8> {
        self.audio_level
    }

    /// Clone the raw parts needed by str0m's fanout write path.
    ///
    /// Returns `(pt, network_time, rtp_time, rid, data, params)` where all types
    /// are str0m-internal. Used only inside `client::fanout`. Takes `&self` so
    /// the fanout loop can hold `&Propagated` across multiple clients.
    pub(crate) fn clone_write_parts(
        &self,
    ) -> (
        str0m::media::Pt,
        Instant,
        MediaTime,
        Option<str0m::media::Rid>,
        Vec<u8>,
        PayloadParams,
    ) {
        (
            self.pt.to_str0m(),
            self.network_time,
            self.time,
            self.rid.map(|r| r.to_str0m()),
            self.data.clone(),
            self.params,
        )
    }

    pub(crate) fn from_str0m(mut data: str0m::media::MediaData) -> Self {
        Self {
            mid: SfuMid::from_str0m(data.mid),
            pt: SfuPt::from_str0m(data.pt),
            rid: data.rid.map(SfuRid::from_str0m),
            data: std::mem::take(&mut data.data),
            network_time: data.network_time,
            contiguous: data.contiguous,
            time: data.time,
            params: data.params,
            #[cfg(feature = "av1-dd")]
            av1_dd: None, // TODO(av1-dd): populate when str0m exposes ExtensionValues::dependency_descriptor
            #[cfg(feature = "vfm")]
            vfm_fm: None, // TODO(vfm): populate when str0m exposes ExtensionValues::frame_marking
            audio_level: data.ext_vals.audio_level,
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

    #[cfg(feature = "av1-dd")]
    #[test]
    fn av1_dd_accessor_exists() {
        use crate::av1::Av1DdInfo;
        // Verify the accessor type signature compiles correctly.
        let _: fn(&super::SfuMediaPayload) -> Option<Av1DdInfo> = super::SfuMediaPayload::av1_dd;
    }
}
