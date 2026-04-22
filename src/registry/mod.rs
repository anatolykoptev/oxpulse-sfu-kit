//! Multi-client registry — routes UDP datagrams to the owning client and fans
//! out propagated events. Single-task ownership model (no `Arc<RwLock>`).
//!
//! Ported from the str0m `chat.rs` example with multi-client fanout, simulcast
//! layer management, and optional dominant-speaker detection added.
//!
//! Submodules: `lifecycle` (reap/drain), `test_seams` (test-only).

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use str0m::net::{Protocol, Receive};
use str0m::Input;

use crate::client::Client;
use crate::fanout::fanout;
use crate::metrics::SfuMetrics;
use crate::propagate::Propagated;

mod lifecycle;
#[cfg(any(test, feature = "test-utils"))]
mod test_seams;

/// Single-owner registry of connected peers in a room.
///
/// Drive it by calling [`insert`][Registry::insert] when a peer completes
/// signaling, then in a loop: feed datagrams via
/// [`handle_incoming`][Registry::handle_incoming], call
/// [`poll_all`][Registry::poll_all] + [`fanout_pending`][Registry::fanout_pending],
/// flush transmits via [`drain_transmits`][Registry::drain_transmits].
///
/// For the simple case, use [`run_udp_loop`][crate::run_udp_loop] which does
/// all of this for you.
#[derive(Debug)]
pub struct Registry {
    pub(super) clients: Vec<Client>,
    pub(super) to_propagate: VecDeque<Propagated>,
    pub(super) metrics: Arc<SfuMetrics>,
    #[cfg(feature = "active-speaker")]
    pub(super) detector: dominant_speaker::ActiveSpeakerDetector,
}

impl Registry {
    /// Create a new registry wired to the given metrics instance.
    pub fn new(metrics: Arc<SfuMetrics>) -> Self {
        Self {
            clients: Vec::new(),
            to_propagate: VecDeque::new(),
            metrics,
            #[cfg(feature = "active-speaker")]
            detector: dominant_speaker::ActiveSpeakerDetector::new(),
        }
    }

    /// Create a registry with a throwaway metrics instance.
    ///
    /// Intended only for tests that don't care about metrics values.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn new_for_tests() -> Self {
        Self::new(Arc::new(SfuMetrics::new_default()))
    }

    /// Whether the registry has no connected peers.
    pub fn is_empty(&self) -> bool {
        self.clients.is_empty()
    }

    /// Number of connected peers.
    pub fn len(&self) -> usize {
        self.clients.len()
    }

    /// Read-only view of all clients.
    ///
    /// Intended for metrics inspection and tests; not for hot-path use.
    pub fn clients(&self) -> &[Client] {
        &self.clients
    }

    /// Insert a freshly-built client into the room.
    ///
    /// Announces every existing client's tracks to the newcomer
    /// (cross-advertisement pattern from str0m `chat.rs`). The client's
    /// metrics handle is replaced with the registry's own so all counters
    /// flow to one Prometheus registry.
    pub fn insert(&mut self, mut client: Client) {
        client.metrics = self.metrics.clone();
        for entry in self.clients.iter().flat_map(|c| c.tracks_in.iter()) {
            client.handle_track_open(std::sync::Arc::downgrade(&entry.id));
        }
        #[cfg(feature = "active-speaker")]
        self.detector.add_peer(*client.id, Instant::now());
        self.metrics.inc_client_connect();
        self.metrics.inc_active_participants();
        self.clients.push(client);
    }

    /// Feed an incoming UDP datagram to whichever client claims it.
    ///
    /// Returns `true` if a client accepted the datagram, `false` when no
    /// client matched (common early in a connection — STUN arrives before
    /// the `Rtc` is registered).
    pub fn handle_incoming(
        &mut self,
        source: SocketAddr,
        destination: SocketAddr,
        payload: &[u8],
    ) -> bool {
        let Ok(contents) = payload.try_into() else {
            tracing::debug!(?source, bytes = payload.len(), "undecodable udp datagram");
            return false;
        };
        let input = Input::Receive(
            Instant::now(),
            Receive {
                proto: Protocol::Udp,
                source,
                destination,
                contents,
            },
        );
        if let Some(client) = self.clients.iter_mut().find(|c| c.accepts(&input)) {
            client.handle_input(input);
            true
        } else {
            tracing::debug!(?source, "no client accepts udp datagram");
            false
        }
    }

    /// Feed an RFC 6464 audio-level observation into the dominant-speaker detector.
    ///
    /// `level_raw` is 0–127 dBov (0 = loud, 127 = silent). Call this for every
    /// audio RTP packet received from `peer_id` after parsing the audio-level
    /// RTP header extension. Only available with the `active-speaker` feature.
    #[cfg(feature = "active-speaker")]
    pub fn record_audio_level(&mut self, peer_id: u64, level_raw: u8, now: Instant) {
        self.detector.record_level(peer_id, level_raw, now);
    }

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
            client.handle_input(Input::Timeout(now));
        }
    }

    /// Fan out every queued propagated event to the appropriate clients.
    pub fn fanout_pending(&mut self) {
        while let Some(p) = self.to_propagate.pop_front() {
            fanout(&p, &mut self.clients);
        }
    }
}
