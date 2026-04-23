//! Registry drive loop — poll, tick, and fanout.
//!
//! Split from `registry/mod.rs` to keep the struct/insert/routing concern
//! separate from the per-iteration state machine driving concern.

use std::time::Instant;

use crate::fanout::fanout;
use crate::ids::SfuRid;
use crate::propagate::{ClientId, Propagated};

use super::Registry;

impl Registry {
    /// Poll every client until each returns a `Timeout`, queuing propagated events.
    ///
    /// Returns the earliest next wake-up deadline.
    pub fn poll_all(&mut self, now: Instant) -> Instant {
        let mut deadline = now + std::time::Duration::from_millis(100);
        for client in self.clients.iter_mut() {
            loop {
                if !client.is_alive() {
                    break;
                }
                match client.poll_output() {
                    Propagated::Timeout(t) => {
                        deadline = deadline.min(t);
                        break;
                    }
                    Propagated::Noop => continue,
                    Propagated::BandwidthEstimate {
                        peer_id,
                        ref estimate,
                    } => {
                        self.metrics.update_peer_bwe(*peer_id, estimate.bps);
                        self.to_propagate.push_back(Propagated::BandwidthEstimate {
                            peer_id,
                            estimate: *estimate,
                        });
                        #[cfg(feature = "pacer")]
                        {
                            use crate::bwe::PacerAction;
                            match client.drive_pacer(estimate.bps) {
                                PacerAction::GoAudioOnly => {
                                    self.to_propagate.push_back(Propagated::AudioOnlyMode {
                                        peer_id,
                                        audio_only: true,
                                    });
                                }
                                PacerAction::RestoreVideo => {
                                    self.to_propagate.push_back(Propagated::AudioOnlyMode {
                                        peer_id,
                                        audio_only: false,
                                    });
                                }
                                PacerAction::ChangeLayer(_) | PacerAction::NoChange => {}
                            }
                        }
                    }
                    Propagated::RtcpStats { peer_id, ref stats } => {
                        self.metrics.update_peer_rtcp(
                            *peer_id,
                            stats.fraction_lost,
                            stats.rtt.as_secs_f64() * 1000.0,
                            stats.jitter.as_secs_f64() * 1000.0,
                        );
                        self.to_propagate.push_back(Propagated::RtcpStats {
                            peer_id,
                            stats: *stats,
                        });
                    }
                    other => {
                        #[cfg(feature = "active-speaker")]
                        if let Propagated::MediaData(ref origin, ref data) = other {
                            // RFC 6464: str0m stores audio_level as negated dBov
                            // (0 = loudest, -127 = silent). The detector expects
                            // 0-127 dBov (0 = loud, 127 = silent), so we negate.
                            // MediaData originates from the current loop \, so
                            // we check client.is_relay() directly — no second borrow needed.
                            if let Some(raw) = data.audio_level_raw() {
                                if !client.is_relay() {
                                    let level = (-(raw as i16)).clamp(0, 127) as u8;
                                    let now_ms = self
                                        .detector_epoch
                                        .elapsed()
                                        .as_millis() as u64;
                                    self.detector.record_level(**origin, level, now_ms);
                                }
                            }
                        }
                        self.to_propagate.push_back(other);
                    }
                }
            }
        }
        deadline
    }

    /// Advance the dominant-speaker detector one tick.
    ///
    /// Queues a [`Propagated::ActiveSpeakerChanged`] when dominance changes.
    /// Call this on a 300ms interval (see `dominant_speaker::TICK_INTERVAL`).
    /// Only available with the `active-speaker` feature.
    #[cfg(feature = "active-speaker")]
    #[cfg_attr(docsrs, doc(cfg(feature = "active-speaker")))]
    pub fn tick_active_speaker(&mut self, now: Instant) {
        let now_ms = now
            .saturating_duration_since(self.detector_epoch)
            .as_millis() as u64;
        if let Some(change) = self.detector.tick(now_ms) {
            self.metrics.inc_dominant_speaker_changes();
            self.to_propagate
                .push_back(Propagated::ActiveSpeakerChanged {
                    peer_id: change.peer_id,
                    confidence: change.c2_margin,
                });
        }
    }

    /// Update Prometheus gauges with current per-peer speaker activity scores.
    ///
    /// Call this periodically (e.g. on the same 300ms tick as `tick_active_speaker`).
    /// Only available with both `active-speaker` and `metrics-prometheus` features.
    #[cfg(all(feature = "active-speaker", feature = "metrics-prometheus"))]
    #[cfg_attr(
        docsrs,
        doc(cfg(all(feature = "active-speaker", feature = "metrics-prometheus")))
    )]
    pub fn tick_speaker_scores(&mut self) {
        for (peer_id, imm, med, lng) in self.detector.peer_scores() {
            self.metrics
                .update_peer_speaker_scores(peer_id, imm, med, lng);
        }
    }

    /// Drive the session clock forward on every client.
    pub fn tick(&mut self, now: Instant) {
        for client in self.clients.iter_mut() {
            client.handle_timeout(now);
        }
    }

    /// Fan out every queued propagated event to the appropriate clients.
    pub fn fanout_pending(&mut self) {
        #[cfg(feature = "kalman-bwe")]
        let now = Instant::now();
        while let Some(p) = self.to_propagate.pop_front() {
            #[cfg(feature = "kalman-bwe")]
            if let Propagated::ClientBudgetHint(subscriber_id, bps) = &p {
                self.bandwidth.record_client_hint(*subscriber_id, *bps, now);
                continue;
            }
            // Update pacer-driven layer selection before forwarding the packet
            // so subscribers receive media on their freshly-chosen layer.
            #[cfg(all(feature = "kalman-bwe", feature = "pacer"))]
            if let Propagated::MediaData(origin, _) = &p {
                self.update_pacer_layers(*origin);
            }
            fanout(&p, &mut self.clients);
        }
    }
    /// Compute the maximum desired simulcast layer across all subscribers per publisher,
    /// and enqueue [`Propagated::PublisherLayerHint`] when the max changes.
    ///
    /// Call after [`fanout_pending`][Self::fanout_pending] on any tick where
    /// subscriber desired layers may have changed.
    pub fn emit_publisher_layer_hints(&mut self) {
        use crate::client::layer;
        use std::collections::HashMap;

        let mut max_per_publisher: HashMap<ClientId, SfuRid> = HashMap::new();
        for subscriber in &self.clients {
            let sub_desired = subscriber.desired_layer();
            for track_out in &subscriber.tracks_out {
                if let Some(track_in) = track_out.track_in.upgrade() {
                    let publisher_id = track_in.origin;
                    let entry = max_per_publisher.entry(publisher_id).or_insert(layer::LOW);
                    let rank = |r: SfuRid| -> u8 {
                        if r == SfuRid::LOW {
                            0
                        } else if r == SfuRid::MEDIUM {
                            1
                        } else {
                            2
                        }
                    };
                    if rank(sub_desired) > rank(*entry) {
                        *entry = sub_desired;
                    }
                }
            }
        }
        for (publisher_id, max_rid) in max_per_publisher {
            let is_relay = self
                .clients
                .iter()
                .any(|c| c.id == publisher_id && c.is_relay());

            if is_relay {
                self.to_propagate
                    .push_back(Propagated::PublisherLayerHintForUpstream {
                        publisher_relay_id: publisher_id,
                        max_rid,
                    });
            } else {
                self.to_propagate.push_back(Propagated::PublisherLayerHint {
                    publisher_id,
                    max_rid,
                });
            }
        }
    }
}

/// Default simulcast ladder used when the publisher has not yet emitted active RIDs.
#[cfg(all(feature = "kalman-bwe", feature = "pacer"))]
const DEFAULT_SIMULCAST_LADDER: &[crate::ids::SfuRid] = &[
    crate::ids::SfuRid::LOW,
    crate::ids::SfuRid::MEDIUM,
    crate::ids::SfuRid::HIGH,
];

#[cfg(all(feature = "kalman-bwe", feature = "pacer"))]
impl Registry {
    /// For every subscriber of `origin`, read the current Kalman BWE estimate
    /// and advance the subscriber's pacer to select the appropriate simulcast layer.
    ///
    /// Called on every incoming `MediaData` event from the publisher so the
    /// pacer has fresh input every 20ms (nominal video packet cadence).
    ///
    /// Only available with both `kalman-bwe` and `pacer` features.
    pub fn update_pacer_layers(&mut self, origin: crate::propagate::ClientId) {
        let now = std::time::Instant::now();

        // Snapshot publisher's active RIDs before the mutable loop (borrow checker).
        let publisher_rids: Vec<crate::ids::SfuRid> = self
            .clients
            .iter()
            .find(|c| c.id == origin)
            .map(|c| c.active_rids())
            .unwrap_or_default();

        let _available: &[crate::ids::SfuRid] = if publisher_rids.is_empty() {
            DEFAULT_SIMULCAST_LADDER
        } else {
            &publisher_rids
        };

        let subscriber_ids: Vec<crate::propagate::ClientId> = self
            .clients
            .iter()
            .filter(|c| c.id != origin)
            .map(|c| c.id)
            .collect();

        for sub_id in subscriber_ids {
            let budget = self.bandwidth.estimate_bps(sub_id, now).unwrap_or(0);

            // Mirror update_peer_bwe so Prometheus stays in sync.
            #[cfg(feature = "metrics-prometheus")]
            self.metrics.update_peer_bwe(*sub_id, budget);

            if let Some(client) = self.clients.iter_mut().find(|c| c.id == sub_id) {
                use crate::bwe::PacerAction;
                match client.drive_pacer(budget) {
                    PacerAction::GoAudioOnly => {
                        self.to_propagate.push_back(Propagated::AudioOnlyMode {
                            peer_id: sub_id,
                            audio_only: true,
                        });
                    }
                    PacerAction::RestoreVideo => {
                        self.to_propagate.push_back(Propagated::AudioOnlyMode {
                            peer_id: sub_id,
                            audio_only: false,
                        });
                    }
                    PacerAction::ChangeLayer(_) | PacerAction::NoChange => {}
                }
            }
        }
    }
}
