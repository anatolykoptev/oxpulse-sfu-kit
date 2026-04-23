//! Keyframe-request plumbing — both directions.
//!
//! - **Upstream**: when str0m gives us non-contiguous media on an incoming
//!   track, ask the source peer for a keyframe (throttled to avoid storms).
//! - **Downstream**: when a subscriber's decoder stalls and it asks the SFU
//!   for a keyframe on an outgoing track, relay that to the origin client.

use std::time::{Duration, Instant};

use str0m::media::{KeyframeRequestKind, Mid, Rid};

use super::tracks::TrackOut;
use super::Client;
use crate::ids::SfuMid;
use crate::keyframe::SfuKeyframeRequest;
use crate::propagate::Propagated;

/// Minimum gap between PLI/FIR requests for the same track.
///
/// Matches str0m's `chat.rs` 1-second floor — fast enough to unblock receivers,
/// slow enough to avoid keyframe request storms.
const KEYFRAME_REQUEST_MIN_GAP: Duration = Duration::from_secs(1);

impl Client {
    /// Ask the source to cut a keyframe, throttled to [`KEYFRAME_REQUEST_MIN_GAP`].
    pub(super) fn request_keyframe_throttled(
        &mut self,
        mid: Mid,
        rid: Option<Rid>,
        kind: KeyframeRequestKind,
    ) {
        let Some(mut writer) = self.rtc.writer(mid) else {
            return;
        };
        let Some(entry) = self.tracks_in.iter_mut().find(|t| t.id.mid == mid) else {
            return;
        };
        if entry
            .last_keyframe_request
            .map(|t| t.elapsed() < KEYFRAME_REQUEST_MIN_GAP)
            .unwrap_or(false)
        {
            return;
        }
        let _ = writer.request_keyframe(rid, kind);
        entry.last_keyframe_request = Some(Instant::now());
    }

    /// Translate a subscriber's keyframe request to the origin client's track.
    pub(super) fn incoming_keyframe_req(
        &self,
        mut req: str0m::media::KeyframeRequest,
    ) -> Propagated {
        let Some(track_out): Option<&TrackOut> =
            self.tracks_out.iter().find(|t| t.mid() == Some(req.mid))
        else {
            return Propagated::Noop;
        };
        let Some(track_in) = track_out.track_in.upgrade() else {
            return Propagated::Noop;
        };
        req.rid = self.chosen_rid;
        if track_in.relay_source {
            // The publisher is on another SFU edge — we cannot send PLI/FIR to
            // it because it has no inbound negotiation for this direction.
            // Emit UpstreamKeyframeRequest for the application to relay upstream.
            return Propagated::UpstreamKeyframeRequest {
                source_relay_id: track_in.origin,
                req: SfuKeyframeRequest::from_str0m(req),
                source_mid: SfuMid::from_str0m(track_in.mid),
            };
        }
        Propagated::KeyframeRequest(
            self.id,
            SfuKeyframeRequest::from_str0m(req),
            track_in.origin,
            SfuMid::from_str0m(track_in.mid),
        )
    }

    /// Handle a propagated keyframe request: pass it through to str0m's writer
    /// if this client owns the matching incoming track.
    pub fn handle_keyframe_request(&mut self, req: SfuKeyframeRequest, mid_in: SfuMid) {
        let mid_in = mid_in.to_str0m();
        if !self.tracks_in.iter().any(|i| i.id.mid == mid_in) {
            return;
        }
        let Some(mut writer) = self.rtc.writer(mid_in) else {
            return;
        };
        let rid = req.rid().map(|r| r.to_str0m());
        let kind = req.kind().to_str0m();
        if let Err(e) = writer.request_keyframe(rid, kind) {
            tracing::info!(client = *self.id, error = ?e, "request_keyframe failed");
        }
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl Client {
    /// Expose `incoming_keyframe_req` for integration tests.
    pub fn incoming_keyframe_req_for_tests(
        &self,
        req: str0m::media::KeyframeRequest,
    ) -> crate::propagate::Propagated {
        self.incoming_keyframe_req(req)
    }
}
