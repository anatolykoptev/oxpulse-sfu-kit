//! Client lifecycle management and outbound datagram draining.
//!
//! Split from `registry/mod.rs` to keep the routing/polling concern separate
//! from the reap/drain concern.

use crate::net::OutgoingDatagram;

use super::Registry;

impl Registry {
    /// Drain every client's outbound queue into `sink`.
    ///
    /// The caller (usually the UDP loop) writes the bytes to the socket.
    pub fn drain_transmits<F: FnMut(OutgoingDatagram)>(&mut self, mut sink: F) {
        for client in self.clients.iter_mut() {
            for t in client.drain_pending_out() {
                sink(t);
            }
        }
    }

    /// Remove dead clients and update metrics.
    ///
    /// Call this at the top of each loop iteration so dead peers are evicted
    /// before polling. Clients are considered dead when `is_alive()` returns
    /// false (str0m disconnected them via ICE failure or an explicit call to
    /// `rtc.disconnect()`).
    pub fn reap_dead(&mut self) {
        #[cfg(feature = "active-speaker")]
        let detector = &mut self.detector;
        let metrics = &self.metrics;
        // Finding #2 (resource_exhaustion): pre-borrow the estimator so the
        // retain closure can evict its per-subscriber state in the SAME sweep
        // as the client + detector cleanup. Without this, BandwidthEstimator's
        // `subscribers` map grows unbounded across reconnect churn (per-room,
        // in-memory, no persistence) toward OOM. Mirrors oxpulse-partner-edge
        // crates/sfu/src/registry/bwe.rs, which reaps bandwidth in-sweep too.
        #[cfg(feature = "kalman-bwe")]
        let bandwidth = &mut self.bandwidth;
        self.clients.retain(|c| {
            let alive = c.is_alive();
            if !alive {
                #[cfg(feature = "active-speaker")]
                {
                    // Remove exactly what insert registered. Use the stored flag,
                    // NOT a live is_relay() re-check: if set_origin(Relay) was called
                    // after insert, is_relay() would now read true and we'd skip a
                    // peer that WAS added as local — leaking it in the detector map.
                    if c.in_speaker_detector {
                        detector.remove_peer(&*c.id);
                    }
                }
                // Evict the per-subscriber BWE state (Kalman/loss/send_times),
                // mirroring the detector cleanup above — the already-written
                // BandwidthEstimator::reap_dead was never called on disconnect.
                #[cfg(feature = "kalman-bwe")]
                bandwidth.reap_dead(c.id);
                metrics.inc_client_disconnect();
                metrics.dec_active_participants();
                metrics.reap_dead_peer(*c.id);
            }
            alive
        });
    }
}
