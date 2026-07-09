//! Media payload wrapper over `str0m::media::MediaData`.
//!
//! Refcounted-bytes payload for inter-peer fanout. Construction from `str0m`
//! moves the inner `Arc<[u8]>`; fanout then hands each subscriber an
//! `Arc::clone` (a refcount bump), never a byte copy of the payload.

use std::sync::Arc;
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
    data: Arc<[u8]>,
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
    /// SFrame key epoch (KID) from the RTP header extension, if a serializer is
    /// registered for it (see [`crate::sframe`]). Re-attached on fanout.
    key_epoch: Option<crate::sframe::KeyEpoch>,
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

    /// SFrame key epoch (KID) carried in the RTP header extension, if present.
    ///
    /// Populated only when an SFrame-KID extension serializer is registered on
    /// the `Rtc` (see [`crate::sframe`]); otherwise always `None`. The fanout
    /// path re-attaches this value to every forwarded packet.
    #[must_use]
    pub fn key_epoch(&self) -> Option<crate::sframe::KeyEpoch> {
        self.key_epoch
    }

    /// Whether this packet's negotiated codec is video rather than audio.
    ///
    /// Gates [`Self::rtp_send_ms`]'s use as a GoogCC delay-trend feed:
    /// audio and video RTP clocks run at different rates (e.g. 48 kHz Opus
    /// vs 90 kHz VP8/H264/AV1), so mixing them into one trendline sample
    /// would compare unrelated timings. `googcc-bwe`-only; see
    /// [`crate::bwe::googcc`].
    #[cfg(feature = "googcc-bwe")]
    #[cfg_attr(docsrs, doc(cfg(feature = "googcc-bwe")))]
    #[must_use]
    pub fn is_video(&self) -> bool {
        self.params.spec().codec.is_video()
    }

    /// RTP-timestamp-derived send time, in milliseconds.
    ///
    /// Converts this packet's raw RTP timestamp via its own negotiated clock
    /// rate (`MediaTime::as_seconds`) — no `record_send_time` tracking or
    /// TWCC round-trip required, since the timestamp is already carried on
    /// every packet. Feeds
    /// [`GoogCcEstimator::on_receive`][crate::bwe::googcc::GoogCcEstimator::on_receive]'s
    /// `send_ms` parameter directly. Meaningful only when [`Self::is_video`]
    /// is true (see that method's docs). `googcc-bwe`-only.
    #[cfg(feature = "googcc-bwe")]
    #[cfg_attr(docsrs, doc(cfg(feature = "googcc-bwe")))]
    #[must_use]
    pub fn rtp_send_ms(&self) -> f64 {
        self.time.as_seconds() * 1000.0
    }

    /// Borrow the raw parts needed by str0m's fanout write path.
    ///
    /// Returns `(pt, network_time, rtp_time, rid, data, params)` where all types
    /// are str0m-internal. The payload is an `Arc::clone` — a refcount bump, not
    /// a byte copy — so fanning one inbound frame out to N subscribers costs N
    /// refcount increments instead of N `Vec<u8>` deep copies. str0m's
    /// `Writer::write` takes `impl Into<Arc<[u8]>>`, so the returned `Arc<[u8]>`
    /// flows through as an identity `Into` with no re-allocation (ADR-S2). Used
    /// only inside `client::fanout`. Takes `&self` so the fanout loop can hold
    /// `&Propagated` across multiple clients.
    pub(crate) fn write_parts(
        &self,
    ) -> (
        str0m::media::Pt,
        Instant,
        MediaTime,
        Option<str0m::media::Rid>,
        Arc<[u8]>,
        PayloadParams,
    ) {
        (
            self.pt.to_str0m(),
            self.network_time,
            self.time,
            self.rid.map(|r| r.to_str0m()),
            Arc::clone(&self.data),
            self.params,
        )
    }

    pub(crate) fn from_str0m(data: str0m::media::MediaData) -> Self {
        Self {
            mid: SfuMid::from_str0m(data.mid),
            pt: SfuPt::from_str0m(data.pt),
            rid: data.rid.map(SfuRid::from_str0m),
            data: data.data,
            network_time: data.network_time,
            contiguous: data.contiguous,
            time: data.time,
            params: data.params,
            #[cfg(feature = "av1-dd")]
            av1_dd: None, // TODO(av1-dd): populate when str0m exposes ExtensionValues::dependency_descriptor
            #[cfg(feature = "vfm")]
            vfm_fm: None, // TODO(vfm): populate when str0m exposes ExtensionValues::frame_marking
            audio_level: data.ext_vals.audio_level,
            key_epoch: data
                .ext_vals
                .user_values
                .get::<crate::sframe::KeyEpoch>()
                .copied(),
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

    #[cfg(feature = "googcc-bwe")]
    fn payload_with_codec(
        codec: str0m::format::Codec,
        clock_rate: str0m::media::Frequency,
        rtp_ts: u64,
    ) -> SfuMediaPayload {
        use str0m::format::{CodecSpec, FormatParams, PayloadParams};
        use str0m::media::{MediaData, MediaTime, Mid, Pt};
        use str0m::rtp::{ExtensionValues, SeqNo};

        let pt = Pt::from(96u8);
        let seq: SeqNo = 0u64.into();
        let params = PayloadParams::new(
            pt,
            None,
            CodecSpec {
                codec,
                clock_rate,
                channels: None,
                format: FormatParams::default(),
            },
        );
        SfuMediaPayload::from_str0m(MediaData {
            mid: Mid::from("m0"),
            pt,
            rid: None,
            params,
            time: MediaTime::new(rtp_ts, clock_rate),
            network_time: Instant::now(),
            seq_range: seq..=seq,
            data: vec![0u8; 4].into(),
            ext_vals: ExtensionValues::default(),
            codec_extra: str0m::format::CodecExtra::None,
            contiguous: true,
            last_sender_info: None,
            audio_start_of_talk_spurt: false,
        })
    }

    #[cfg(feature = "googcc-bwe")]
    #[test]
    fn is_video_true_for_video_codec() {
        let payload = payload_with_codec(
            str0m::format::Codec::Vp8,
            str0m::media::Frequency::NINETY_KHZ,
            90_000,
        );
        assert!(payload.is_video());
    }

    #[cfg(feature = "googcc-bwe")]
    #[test]
    fn is_video_false_for_audio_codec() {
        let payload = payload_with_codec(
            str0m::format::Codec::Opus,
            str0m::media::Frequency::FORTY_EIGHT_KHZ,
            48_000,
        );
        assert!(!payload.is_video());
    }

    #[cfg(feature = "googcc-bwe")]
    #[test]
    fn rtp_send_ms_converts_via_the_packets_own_clock_rate() {
        // 90 kHz clock, RTP timestamp = 90_000 ticks -> exactly 1000 ms,
        // regardless of any hardcoded assumption about the clock rate.
        let video = payload_with_codec(
            str0m::format::Codec::Vp8,
            str0m::media::Frequency::NINETY_KHZ,
            90_000,
        );
        assert!(
            (video.rtp_send_ms() - 1000.0).abs() < 1e-6,
            "expected 1000ms at 90kHz for 90_000 ticks, got {}",
            video.rtp_send_ms()
        );

        // 48 kHz clock, same tick count -> a different ms value at the same
        // raw ticks, proving the conversion is clock-rate-aware (not a
        // hardcoded /90.0 division).
        let audio = payload_with_codec(
            str0m::format::Codec::Opus,
            str0m::media::Frequency::FORTY_EIGHT_KHZ,
            48_000,
        );
        assert!(
            (audio.rtp_send_ms() - 1000.0).abs() < 1e-6,
            "expected 1000ms at 48kHz for 48_000 ticks, got {}",
            audio.rtp_send_ms()
        );
    }

    #[cfg(feature = "av1-dd")]
    #[test]
    fn av1_dd_accessor_exists() {
        use crate::av1::Av1DdInfo;
        // Verify the accessor type signature compiles correctly.
        let _: fn(&super::SfuMediaPayload) -> Option<Av1DdInfo> = super::SfuMediaPayload::av1_dd;
    }
}
