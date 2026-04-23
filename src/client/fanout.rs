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
        // Use LayerSelector to pick the best available RID for this subscriber.
        // active_rids() is empty until the first video packet arrives — fall back
        // to the old RID-exact match in that case (BestFitSelector handles empty correctly).
        {
            use crate::layer_selector::{BestFitSelector, LayerSelector as _};
            let active: Vec<crate::ids::SfuRid> = self.active_rids();
            let target = BestFitSelector.select(self.desired_layer, &active);
            match data.rid() {
                None => {}            // non-simulcast: always forward
                Some(rid) if rid == target => {}  // correct layer
                Some(_) => return,    // wrong layer — drop
            }
        }

        // Drop AV1 packets whose temporal layer exceeds this subscriber's cap.
        #[cfg(feature = "av1-dd")]
        if let Some(dd) = data.av1_dd() {
            if dd.temporal_id > self.max_temporal_layer {
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
