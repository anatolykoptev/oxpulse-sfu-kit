//! Client lifecycle management and outbound datagram draining.
//!
//! Split from `registry/mod.rs` to keep the routing/polling concern separate
//! from the reap/drain concern.

use crate::client::Transmit;

use super::Registry;

impl Registry {
    /// Drain every client's outbound queue into `sink`.
    ///
    /// The caller (usually the UDP loop) writes the bytes to the socket.
    pub fn drain_transmits<F: FnMut(Transmit)>(&mut self, mut sink: F) {
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
        self.clients.retain(|c| {
            let alive = c.is_alive();
            if !alive {
                #[cfg(feature = "active-speaker")]
                detector.remove_peer(*c.id);
                metrics.inc_client_disconnect();
                metrics.dec_active_participants();
            }
            alive
        });
    }
}
