//! Downstream fanout: apply a forwarded `MediaData` or speaker-change event
//! to *this* peer.
//!
//! Split from `client/mod.rs` because it owns a distinct concern: per-subscriber
//! simulcast layer filtering and the writer-stage early-returns that tolerate
//! unnegotiated sessions in tests.

use std::sync::atomic::Ordering;

use str0m::media::{MediaKind, Rid};

use super::{layer, Client};
use crate::media::SfuMediaPayload;
use crate::propagate::ClientId;

impl Client {
    /// Forward a `SfuMediaPayload` from `origin` out to this peer.
    ///
    /// Applies the simulcast layer filter (drops packets not matching
    /// [`desired_layer`][Client::desired_layer]) and increments Prometheus
    /// counters for forwarded packets and layer selections.
    pub fn handle_media_data_out(&mut self, origin: ClientId, data: &SfuMediaPayload) {
        // Suspended-state filter: drop video frames when the per-subscriber
        // pacer is in the `suspended` sub-state. Audio frames continue to flow.
        // Phase 7 of the 1 KB/s resilience plan.
        #[cfg(feature = "pacer")]
        if self.suspended {
            // Detect media kind by matching the inbound track on origin + mid.
            // Defensive default = `true` (treat unknown as video → drop): a
            // suspended subscriber's whole point is bandwidth conservation, so
            // when track metadata is unavailable (Weak::upgrade fails on a
            // disconnected publisher, or no matching tracks_out entry) we bias
            // toward dropping rather than forwarding. This inverts the polarity
            // of the simulcast layer filter below — that one biases toward
            // forward because uncertainty there means "no simulcast" not "no
            // video"; here uncertainty must not leak bandwidth.
            let is_video = self
                .tracks_out
                .iter()
                .find_map(|o| {
                    let i = o.track_in.upgrade()?;
                    if i.origin == origin && i.mid == data.mid().to_str0m() {
                        Some(matches!(i.kind, MediaKind::Video))
                    } else {
                        None
                    }
                })
                .unwrap_or(true);
            if is_video {
                // F7-1: use cached handle — single atomic add, no per-frame alloc.
                #[cfg(feature = "metrics-prometheus")]
                self.video_frames_dropped.inc();
                #[cfg(not(feature = "metrics-prometheus"))]
                self.metrics.inc_video_frames_dropped(*self.id);
                return;
            }
        }

        // Use LayerSelector to pick the best available RID for this subscriber.
        // active_rids() is empty until the first video packet arrives — fall back
        // to the old RID-exact match in that case (BestFitSelector handles empty correctly).
        {
            use crate::layer_selector::{BestFitSelector, LayerSelector as _};
            let active: Vec<crate::ids::SfuRid> = self.active_rids();
            let target = BestFitSelector.select(self.desired_layer, &active);
            match data.rid() {
                None => {}                       // non-simulcast: always forward
                Some(rid) if rid == target => {} // correct layer
                Some(_) => return,               // wrong layer — drop
            }
        }

        // Drop AV1 packets whose temporal layer exceeds this subscriber's cap.
        #[cfg(feature = "av1-dd")]
        if let Some(dd) = data.av1_dd() {
            if dd.temporal_id > self.max_temporal_layer {
                return;
            }
        }

        // Drop H.264/VP9/HEVC packets whose temporal layer exceeds this subscriber's cap.
        #[cfg(feature = "vfm")]
        if let Some(fm) = data.vfm_frame_marking() {
            if fm.temporal_id > self.max_vfm_temporal_layer {
                return;
            }
        }

        let data_mid = data.mid().to_str0m();

        // Find the matching outbound track entry.
        let matched = self.tracks_out.iter().find(|o| {
            o.track_in
                .upgrade()
                .filter(|i| i.origin == origin && i.mid == data_mid)
                .is_some()
        });

        // Prometheus: forwarded_packets_total{kind}.
        let kind_label = matched
            .and_then(|o| o.track_in.upgrade())
            .map(|t| match t.kind {
                MediaKind::Audio => "audio",
                MediaKind::Video => "video",
            })
            .unwrap_or("other");
        self.metrics.inc_forwarded_packets(kind_label);

        // Metric: outbound bytes forwarded to this subscriber (direction=out).
        // Skip "other" kind to keep label cardinality to two known values.
        if kind_label != "other" {
            self.metrics
                .add_track_bytes_out(kind_label, data.data().len() as u64);
        }

        // Prometheus: layer_selection_total{layer} — simulcast packets only.
        if let Some(rid) = data.rid() {
            let layer_label = rid_label(rid.to_str0m());
            self.metrics.inc_layer_selection(layer_label);
        }

        // Count *after* the filter, *before* writer early-returns.
        self.delivered_media.fetch_add(1, Ordering::Relaxed);

        let Some(mid) = self
            .tracks_out
            .iter()
            .find(|o| {
                o.track_in
                    .upgrade()
                    .filter(|i| i.origin == origin && i.mid == data_mid)
                    .is_some()
            })
            .and_then(|o| o.mid())
        else {
            return;
        };

        // Track the last forwarded RID so keyframe requests target the same layer.
        let data_rid = data.rid().map(|r| r.to_str0m());
        if data_rid.is_some() && self.chosen_rid != data_rid {
            self.chosen_rid = data_rid;
        }

        let Some(writer) = self.rtc.writer(mid) else {
            return;
        };
        let (_pt_raw, network_time, rtp_time, _rid, payload, params) = data.clone_write_parts();
        let Some(pt) = writer.match_params(params) else {
            return;
        };
        // Re-attach the SFrame key epoch (KID) RTP header extension so a
        // subscriber can select the right decryption key. str0m only emits it
        // when the extension is registered + negotiated (see crate::sframe);
        // otherwise `key_epoch` is None and this is a no-op. The SFU never
        // inspects the encrypted payload.
        let writer = match data.key_epoch() {
            Some(kid) => writer.user_extension_value(kid),
            None => writer,
        };
        // Observe end-to-end forwarding latency: time from publisher receipt to
        // subscriber str0m handoff. Uses the packet's own network_time so the
        // measurement is independent of when handle_media_data_out was called.
        // observe() is a single atomic bucket increment — no allocation.
        self.metrics
            .observe_forward_latency(kind_label, network_time.elapsed().as_secs_f64());
        if let Err(e) = writer.write(pt, network_time, rtp_time, payload) {
            tracing::warn!(client = *self.id, error = ?e, "writer.write failed");
            self.rtc.disconnect();
        }
    }

    /// Handle a dominant-speaker election change.
    ///
    /// The registry skips the speaker themselves (skip-self rule), so this
    /// method is only called on *other* clients. In `test-utils` builds a
    /// counter is bumped to let tests verify skip-self semantics.
    #[cfg(feature = "active-speaker")]
    pub fn handle_active_speaker_changed(&mut self, _peer_id: u64) {
        #[cfg(any(test, feature = "test-utils"))]
        {
            self.delivered_active_speaker
                .fetch_add(1, Ordering::Relaxed);
        }
    }
}

fn rid_label(rid: Rid) -> &'static str {
    if rid == layer::LOW.to_str0m() {
        "q"
    } else if rid == layer::MEDIUM.to_str0m() {
        "h"
    } else if rid == layer::HIGH.to_str0m() {
        "f"
    } else {
        "other"
    }
}
