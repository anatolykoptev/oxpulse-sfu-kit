//! Test-only affordances for `Registry`.
//!
//! Gated with `cfg(any(test, feature = "test-utils"))` so the release build is
//! lean and these observer APIs cannot be abused in production code.

#[cfg(feature = "active-speaker")]
use std::time::Instant;

use super::Registry;
use crate::fanout::fanout;
use crate::ids::SfuRid;
use crate::propagate::Propagated;

impl Registry {
    /// Run `fanout` against the registry's own client list.
    ///
    /// Useful when clients are already inserted (cross-advertisement in effect)
    /// and you only want to observe fanout behaviour from that point.
    #[doc(hidden)]
    pub fn fanout_for_tests(&mut self, p: &Propagated) {
        fanout(p, &mut self.clients);
    }

    /// Read a client's delivered-media counter by index.
    #[doc(hidden)]
    pub fn delivered_media_count(&self, idx: usize) -> u64 {
        self.clients[idx].delivered_media_count()
    }

    /// Read a client's delivered-active-speaker counter by index.
    ///
    /// Used to verify skip-self semantics on `ActiveSpeakerChanged` fanout.
    #[doc(hidden)]
    #[cfg(feature = "active-speaker")]
    pub fn delivered_active_speaker_count(&self, idx: usize) -> u64 {
        self.clients[idx].delivered_active_speaker_count()
    }

    /// Flip a client's desired simulcast layer by index.
    #[doc(hidden)]
    pub fn set_desired_layer_for_tests(&mut self, idx: usize, rid: SfuRid) {
        self.clients[idx].set_desired_layer(rid);
    }

    /// Inject an audio level into the dominant-speaker detector, bypassing
    /// wire-level RFC 6464 parsing. Delegates to the public `record_level` API.
    #[doc(hidden)]
    #[cfg(feature = "active-speaker")]
    pub fn inject_audio_level_for_tests(&mut self, peer_id: u64, level: u8, now: Instant) {
        // Mirror the production guard: relay clients are excluded from the detector.
        if self.clients.iter().any(|c| *c.id == peer_id && c.is_relay()) {
            return;
        }
        let now_ms = now.saturating_duration_since(self.detector_epoch).as_millis() as u64;
        self.detector.record_level(peer_id, level, now_ms);
    }

    /// Force an ASO tick and drain any fanout the detector queued.
    ///
    /// Returns the peer id if dominance changed on this tick.
    #[doc(hidden)]
    #[cfg(feature = "active-speaker")]
    pub fn force_active_speaker_tick_for_tests(&mut self, now: Instant) -> Option<u64> {
        let now_ms = now.saturating_duration_since(self.detector_epoch).as_millis() as u64;
        let changed = self.detector.tick(now_ms);
        if let Some(ref change) = changed {
            self.metrics.inc_dominant_speaker_changes();
            self.to_propagate.push_back(Propagated::ActiveSpeakerChanged {
                peer_id: change.peer_id,
                confidence: change.c2_margin,
            });
        }
        self.fanout_pending();
        changed.map(|c| c.peer_id)
    }

    /// Read the detector's current dominant peer.
    #[doc(hidden)]
    #[cfg(feature = "active-speaker")]
    pub fn current_active_speaker(&self) -> Option<u64> {
        self.detector.current_dominant().copied()
    }

    /// Force-disconnect a client by id so the next `reap_dead` pass drops it.
    #[doc(hidden)]
    pub fn disconnect_client_for_tests(&mut self, id: crate::propagate::ClientId) {
        if let Some(client) = self.clients.iter_mut().find(|c| c.id == id) {
            client.disconnect_for_tests();
        }
    }

    /// Invoke `reap_dead` out-of-band.
    #[doc(hidden)]
    pub fn reap_dead_for_tests(&mut self) {
        self.reap_dead();
    }

    /// Wire subscriber at `sub_idx` to publisher at `pub_idx` for the track tagged
    /// with `mid_tag` - forcing the track_out into Open state.
    ///
    /// Must be called after both clients are inserted.
    #[doc(hidden)]
    pub fn wire_track_for_tests(&mut self, sub_idx: usize, pub_idx: usize, mid_tag: u8) {
        use str0m::media::Mid;
        let mid = Mid::from(&*format!("m{mid_tag}"));
        let track_arc = self.clients[pub_idx]
            .tracks_in
            .iter()
            .find(|e| e.id.mid == mid)
            .map(|e| e.id.clone())
            .expect("publisher track not found");
        self.clients[sub_idx].handle_track_open(std::sync::Arc::downgrade(&track_arc));
        for track_out in self.clients[sub_idx].tracks_out.iter_mut() {
            if track_out.track_in.upgrade().as_deref().map(|t| t.mid) == Some(mid) {
                track_out.state = crate::client::tracks::TrackOutState::Open(mid);
                return;
            }
        }
    }

    /// Drain the registry's propagation queue and return all events.
    ///
    /// Used to inspect what `emit_publisher_layer_hints` produces without
    /// running `fanout_pending`.
    #[doc(hidden)]
    pub fn drain_propagated_for_tests(&mut self) -> Vec<crate::propagate::Propagated> {
        self.to_propagate.drain(..).collect()
    }


    /// Mutable access to the clients slice — for tests that need to call
    /// per-client methods (like `incoming_keyframe_req_for_tests`) without
    /// running the full poll loop.
    #[doc(hidden)]
    pub fn clients_mut_for_tests(&mut self) -> &mut [crate::client::Client] {
        &mut self.clients
    }

    /// Drive a subscriber's pacer directly --- for tests that cannot simulate TWCC.
    #[cfg(all(any(test, feature = "test-utils"), feature = "pacer"))]
    #[doc(hidden)]
    pub fn drive_pacer_for_tests(
        &mut self,
        peer_id: crate::propagate::ClientId,
        bps: u64,
    ) {
        use crate::bwe::PacerAction;
        if let Some(client) = self.clients.iter_mut().find(|c| c.id == peer_id) {
            match client.drive_pacer(bps) {
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
                _ => {}
            }
        }
    }
}
