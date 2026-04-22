//! Registry drive loop — poll, tick, and fanout.
//!
//! Split from `registry/mod.rs` to keep the struct/insert/routing concern
//! separate from the per-iteration state machine driving concern.

use std::time::Instant;

use crate::fanout::fanout;
use crate::propagate::Propagated;

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
                    other => self.to_propagate.push_back(other),
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
    pub fn tick_active_speaker(&mut self, now: Instant) {
        if let Some(peer_id) = self.detector.tick(now) {
            self.metrics.inc_dominant_speaker_changes();
            self.to_propagate
                .push_back(Propagated::ActiveSpeakerChanged { peer_id });
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
        while let Some(p) = self.to_propagate.pop_front() {
            fanout(&p, &mut self.clients);
        }
    }
}
