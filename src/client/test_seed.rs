//! Test-only seam that builds a `Client` without real str0m SDP negotiation.
//!
//! Used by integration tests to verify fanout semantics in isolation without
//! spinning up a full ICE/DTLS pipeline.

use std::sync::Arc;
use std::time::Instant;

use str0m::format::{Codec, CodecExtra, CodecSpec, FormatParams, PayloadParams};
use str0m::media::{Frequency, MediaData, MediaKind, MediaTime, Mid, Pt, Rid};
use str0m::rtp::{ExtensionValues, SeqNo};

use crate::ids::SfuRid;
use crate::media::SfuMediaPayload;
use crate::rtc::SfuRtc;

use super::tracks::{TrackIn, TrackInEntry};
use super::Client;
use crate::metrics::SfuMetrics;
use crate::propagate::ClientId;

impl Client {
    /// Look up the cached `Mid`→`MediaKind` mapping for the given `mid_tag`.
    ///
    /// Returns `Some(kind)` if the cache was populated (via `track_in_added`
    /// or `seed_track_in`), `None` otherwise.
    ///
    /// Used by `mid_to_kind_lookup_is_o1_after_track_open` to verify the cache
    /// without going through the full WebRTC pipeline.
    pub fn mid_to_kind_for_tests(&self, mid_tag: u8) -> Option<str0m::media::MediaKind> {
        let mid: str0m::media::Mid = str0m::media::Mid::from(&*format!("m{mid_tag}"));
        self.mid_to_kind.get(&mid).copied()
    }

    /// Inject an observed publisher RID without running the `track_in_media` path.
    ///
    /// Production code should never call this — `track_in_media` owns the
    /// canonical write. Used by screenshare-like tests that need to pin
    /// `active_rids` to a subset of the full simulcast ladder.
    pub fn seed_active_rid_for_tests(&mut self, rid: Rid) {
        self.active_rids.insert(SfuRid::from_str0m(rid));
    }

    /// Mark the underlying `Rtc` as disconnected so `is_alive` returns false.
    ///
    /// Needed for `reap_dead` tests — the real disconnect path requires an
    /// ICE/DTLS pipeline that integration tests don't set up.
    pub fn disconnect_for_tests(&mut self) {
        self.rtc.disconnect();
    }
}

/// Build a `Client` wrapping a default `Rtc` with the given `ClientId`.
///
/// The `Rtc` is unnegotiated — writer calls inside `handle_media_data_out`
/// will no-op, but the `delivered_media` counter still ticks so fanout is
/// observable from tests.
pub fn new_client(id: ClientId) -> Client {
    let rtc = SfuRtc::from_raw(str0m::Rtc::builder().build(Instant::now()));
    let metrics = Arc::new(SfuMetrics::new_default());
    let mut c = Client::new(rtc, metrics);
    c.id = id;
    c
}

/// Seed an incoming track on `client`.
///
/// Returns the `Arc<TrackIn>` so the caller can `Arc::downgrade` it into
/// other clients' `tracks_out`.
pub fn seed_track_in(client: &mut Client, mid_tag: u8, kind: MediaKind) -> Arc<TrackIn> {
    let mid: Mid = Mid::from(&*format!("m{mid_tag}"));
    // Mirror the production `track_in_added` path: populate the O(1) cache.
    client.mid_to_kind.insert(mid, kind);
    let entry = TrackInEntry {
        id: Arc::new(TrackIn {
            origin: client.id,
            mid,
            kind,
            relay_source: false,
        }),
        last_keyframe_request: None,
    };
    let arc = entry.id.clone();
    client.tracks_in.push(entry);
    arc
}

/// Build a synthetic `SfuMediaPayload` for the given mid tag and optional RID.
///
/// Used by fanout / simulcast filter tests to inject packets without running
/// RTP packetization. The layer filter runs before any writer call, so
/// tests observe filter semantics purely via the `delivered_media` counter.
pub fn make_media_data(mid_tag: u8, rid: Option<SfuRid>) -> SfuMediaPayload {
    let mid: Mid = Mid::from(&*format!("m{mid_tag}"));
    let pt = Pt::from(96u8);
    let seq: SeqNo = 0u64.into();
    let params = PayloadParams::new(
        pt,
        None,
        CodecSpec {
            codec: Codec::Vp8,
            clock_rate: Frequency::NINETY_KHZ,
            channels: None,
            format: FormatParams::default(),
        },
    );
    let raw = MediaData {
        mid,
        pt,
        rid: rid.map(|r| r.to_str0m()),
        params,
        time: MediaTime::from_90khz(0),
        network_time: Instant::now(),
        seq_range: seq..=seq,
        data: vec![0xde, 0xad, 0xbe, 0xef].into(),
        ext_vals: ExtensionValues::default(),
        codec_extra: CodecExtra::None,
        contiguous: true,
        last_sender_info: None,
        audio_start_of_talk_spurt: false,
    };
    SfuMediaPayload::from_str0m(raw)
}

/// Build a synthetic `SfuMediaPayload` carrying an SFrame key epoch (KID) in
/// its RTP header extension `user_values`, as str0m would populate it on ingest
/// when a KID extension serializer is registered.
///
/// Used by SFrame forwarding tests to verify the KID is captured off the
/// inbound packet and survives the fanout path.
pub fn make_media_data_with_key_epoch(
    mid_tag: u8,
    rid: Option<SfuRid>,
    key_epoch: crate::sframe::KeyEpoch,
) -> SfuMediaPayload {
    let mid: Mid = Mid::from(&*format!("m{mid_tag}"));
    let pt = Pt::from(96u8);
    let seq: SeqNo = 0u64.into();
    let params = PayloadParams::new(
        pt,
        None,
        CodecSpec {
            codec: Codec::Vp8,
            clock_rate: Frequency::NINETY_KHZ,
            channels: None,
            format: FormatParams::default(),
        },
    );
    let mut ext_vals = ExtensionValues::default();
    ext_vals.user_values.set(key_epoch);
    let raw = MediaData {
        mid,
        pt,
        rid: rid.map(|r| r.to_str0m()),
        params,
        time: MediaTime::from_90khz(0),
        network_time: Instant::now(),
        seq_range: seq..=seq,
        data: vec![0xde, 0xad, 0xbe, 0xef].into(),
        ext_vals,
        codec_extra: CodecExtra::None,
        contiguous: true,
        last_sender_info: None,
        audio_start_of_talk_spurt: false,
    };
    SfuMediaPayload::from_str0m(raw)
}

/// Seed an incoming track on `client` as if the client were a relay source.
///
/// Identical to [`seed_track_in`] except `relay_source = true` — so the
/// keyframe-routing path treats this track as originating from an upstream SFU.
pub fn seed_track_in_relay(client: &mut Client, mid_tag: u8, kind: MediaKind) -> Arc<TrackIn> {
    let mid: Mid = Mid::from(&*format!("m{mid_tag}"));
    // Mirror the production `track_in_added` path: populate the O(1) cache.
    client.mid_to_kind.insert(mid, kind);
    let entry = TrackInEntry {
        id: Arc::new(TrackIn {
            origin: client.id,
            mid,
            kind,
            relay_source: true,
        }),
        last_keyframe_request: None,
    };
    let arc = entry.id.clone();
    client.tracks_in.push(entry);
    arc
}

impl Client {
    /// Simulate an inbound RTP media payload arriving on this client.
    ///
    /// Calls `track_in_media` directly to trigger inbound-byte metrics
    /// without running a real WebRTC/DTLS session.
    ///
    /// `mid_tag` must match a track previously seeded with [`seed_track_in`] —
    /// the kind label is resolved from the seeded `TrackIn.kind` at the call site.
    /// `payload_bytes` sets the synthetic payload size (how many bytes to report).
    ///
    /// Triggers the `sfu_track_bytes_total{direction="in"}` metric and returns
    /// the resulting `Propagated::MediaData` exactly as the production path would.
    ///
    /// Only available with the `test-utils` feature.
    pub fn on_media_data_for_tests(
        &mut self,
        mid_tag: u8,
        payload_bytes: usize,
    ) -> crate::propagate::Propagated {
        let mid: Mid = Mid::from(&*format!("m{mid_tag}"));
        let pt = Pt::from(96u8);
        let seq: SeqNo = 0u64.into();
        let params = PayloadParams::new(
            pt,
            None,
            CodecSpec {
                codec: Codec::Vp8,
                clock_rate: Frequency::NINETY_KHZ,
                channels: None,
                format: FormatParams::default(),
            },
        );
        let raw = MediaData {
            mid,
            pt,
            rid: None,
            params,
            time: MediaTime::from_90khz(0),
            network_time: std::time::Instant::now(),
            seq_range: seq..=seq,
            data: vec![0u8; payload_bytes].into(),
            ext_vals: ExtensionValues::default(),
            codec_extra: CodecExtra::None,
            contiguous: true,
            last_sender_info: None,
            audio_start_of_talk_spurt: false,
        };
        self.track_in_media(raw)
    }
}

/// Force a track-out entry into `Open` state so `incoming_keyframe_req` can
/// find it by MID.
///
/// In production this state is set during SDP negotiation. In tests we skip
/// that pipeline and pin the MID directly so keyframe routing can be exercised.
pub fn open_track_out_for_tests(subscriber: &mut Client, track_in: &Arc<TrackIn>) {
    for track_out in subscriber.tracks_out.iter_mut() {
        if track_out.track_in.upgrade().as_deref().map(|t| t.mid) == Some(track_in.mid) {
            track_out.state = crate::client::tracks::TrackOutState::Open(track_in.mid);
            return;
        }
    }
    panic!("no track_out found for mid {:?}", track_in.mid);
}
