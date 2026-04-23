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
        data: vec![0xde, 0xad, 0xbe, 0xef],
        ext_vals: ExtensionValues::default(),
        codec_extra: CodecExtra::None,
        contiguous: true,
        last_sender_info: None,
        audio_start_of_talk_spurt: false,
    };
    SfuMediaPayload::from_str0m(raw)
}
