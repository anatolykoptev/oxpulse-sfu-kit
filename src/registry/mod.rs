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

use crate::client::Client;
use crate::metrics::SfuMetrics;
use crate::net::{IncomingDatagram, SfuProtocol};
use crate::propagate::Propagated;

mod drive;
mod lifecycle;
#[cfg(any(test, feature = "test-utils"))]
mod test_seams;

/// ADR-9 (Phase 3): `googcc-bwe`'s Registry-level pass-throughs
/// (`Registry::enable_googcc_for_subscriber`,
/// `Registry::googcc_ceiling_for_subscriber_mut`) live behind
/// `self.bandwidth`, which is gated on `kalman-bwe` alone (see the
/// `bandwidth` field a few lines below). `Cargo.toml` declares `kalman-bwe`
/// and `googcc-bwe` orthogonal, so a `googcc-bwe`-only build would otherwise
/// fail deep inside this module with a cryptic "no field `bandwidth`" error.
/// Fail loudly and traceably instead: enable `kalman-bwe` alongside
/// `googcc-bwe`, or use `BandwidthEstimator` directly without `Registry`.
/// Regating `bandwidth` to `any(kalman-bwe, googcc-bwe)` is a named,
/// out-of-scope follow-up (Phase 6c).
#[cfg(all(feature = "googcc-bwe", not(feature = "kalman-bwe")))]
compile_error!(
    "googcc-bwe requires kalman-bwe at the Registry level: the `bandwidth` field on \
     `Registry` that hosts GoogCC's per-subscriber ceiling is gated on `kalman-bwe` alone \
     (see the `Registry` struct definition in this file). Enable both features, or drive \
     `oxpulse_sfu_kit::BandwidthEstimator` directly without `Registry`."
);

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
    /// Fixed epoch for converting  to u64 ms for the speaker detector.
    /// u64 ms with start at registry creation time (monotonic, not wall-clock).
    #[cfg(feature = "active-speaker")]
    detector_epoch: std::time::Instant,
    /// Per-subscriber Kalman/loss bandwidth estimator.
    #[cfg(feature = "kalman-bwe")]
    pub(crate) bandwidth: crate::bwe::estimator::BandwidthEstimator,
    /// Fixed epoch for converting `Instant` to `f64` ms for the RTP-derived
    /// GoogCC feed in `poll_all` (ADR-9/Phase 3). Kept separate from
    /// `detector_epoch` above: `active-speaker` and `googcc-bwe` are
    /// orthogonal features, so the GoogCC feed cannot depend on the
    /// speaker-detector epoch existing.
    #[cfg(all(feature = "kalman-bwe", feature = "googcc-bwe"))]
    bwe_epoch: std::time::Instant,
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
            #[cfg(feature = "active-speaker")]
            detector_epoch: std::time::Instant::now(),
            #[cfg(feature = "kalman-bwe")]
            bandwidth: crate::bwe::estimator::BandwidthEstimator::new(),
            #[cfg(all(feature = "kalman-bwe", feature = "googcc-bwe"))]
            bwe_epoch: std::time::Instant::now(),
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
        // F7-1: re-resolve the cached drop counter against the registry's
        // metrics arc (which just replaced the client's initial one). Without
        // this the cached handle points to the throwaway metrics from new_client()
        // and increments are invisible to the registry's scrape endpoint.
        #[cfg(feature = "metrics-prometheus")]
        {
            client.video_frames_dropped = self.metrics.peer_drop_counter(*client.id);
        }
        for entry in self.clients.iter().flat_map(|c| c.tracks_in.iter()) {
            client.handle_track_open(std::sync::Arc::downgrade(&entry.id));
        }
        #[cfg(feature = "active-speaker")]
        {
            // Relay clients relay another room's audio — their levels are not
            // meaningful for this room's dominant-speaker election. Capture the
            // registration decision ONCE, here, and store it on the client so
            // reap_dead removes exactly what we added — even if `set_origin` is
            // (incorrectly) called after insert.
            let register = !client.is_relay();
            client.in_speaker_detector = register;
            if register {
                let now_ms = self.now_ms();
                self.detector.add_peer(*client.id, now_ms);
            }
        }
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
        let datagram = IncomingDatagram {
            received_at: Instant::now(),
            proto: SfuProtocol::Udp,
            source,
            destination,
            contents: payload.to_vec(),
        };
        if let Some(client) = self.clients.iter_mut().find(|c| c.accepts(&datagram)) {
            client.handle_input(datagram);
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
    ///
    /// Levels for peers not registered with the detector at insert
    /// (`Client::in_speaker_detector` — relays, or clients inserted before their
    /// origin was set) are silently ignored — that audio does not belong to this
    /// room's election.
    #[cfg(feature = "active-speaker")]
    #[cfg_attr(docsrs, doc(cfg(feature = "active-speaker")))]
    pub fn record_audio_level(&mut self, peer_id: u64, level_raw: u8, now: Instant) {
        // Gate on the SAME captured decision as insert/reap (`in_speaker_detector`),
        // NOT a live `is_relay()`. `detector.record_level` implicitly registers an
        // unknown peer, so recording a level for a peer that insert never added would
        // leak it in the detector map (reap only removes what insert registered) — the
        // mirror of the set_origin-after-insert bug this field fixes. Record only for
        // peers actually registered at insert.
        if !self
            .clients
            .iter()
            .any(|c| *c.id == peer_id && c.in_speaker_detector)
        {
            return;
        }
        let now_ms = now
            .saturating_duration_since(self.detector_epoch)
            .as_millis() as u64;
        self.detector.record_level(peer_id, level_raw, now_ms);
    }

    /// Monotonic millisecond timestamp relative to the registry epoch.
    ///
    /// Used internally to convert  values to the u64 ms the
    /// dominant-speaker detector requires (v0.3 API).
    #[cfg(feature = "active-speaker")]
    fn now_ms(&self) -> u64 {
        self.detector_epoch.elapsed().as_millis() as u64
    }

    /// Return raw activity scores for all non-paused peers in the room.
    ///
    /// Each tuple is `(peer_id, immediate_score, medium_score, long_score)`.
    /// Scores are the raw log-domain values from the Volfin & Cohen algorithm.
    /// Useful for debugging speaker detection or building custom UIs.
    ///
    /// Only available with the `active-speaker` feature.
    #[cfg(feature = "active-speaker")]
    #[cfg_attr(docsrs, doc(cfg(feature = "active-speaker")))]
    #[must_use]
    pub fn peer_audio_scores(&self) -> Vec<(u64, f64, f64, f64)> {
        self.detector.peer_scores()
    }
}

#[cfg(feature = "kalman-bwe")]
#[cfg_attr(docsrs, doc(cfg(feature = "kalman-bwe")))]
impl Registry {
    /// Process a TWCC feedback batch for a subscriber.
    ///
    /// Call this when str0m emits TWCC feedback for the subscriber's egress path.
    /// `subscriber` is the peer whose outgoing stream the feedback describes.
    ///
    /// Only available with the `kalman-bwe` feature.
    pub fn on_twcc_feedback(
        &mut self,
        subscriber: crate::propagate::ClientId,
        feedback: &crate::bwe::feedback::TwccFeedback,
        now: std::time::Instant,
    ) {
        self.bandwidth.on_twcc_feedback(subscriber, feedback, now);
    }

    /// Read-only view of the per-room bandwidth estimator, for observability
    /// (metrics, debugging, tests that want to read `estimate_bps` /
    /// `googcc_coverage` without a `for_tests` seam).
    ///
    /// ADR-8: this is the *only* way to reach `BandwidthEstimator` from
    /// outside `Registry` in production code. There is deliberately no public
    /// `bandwidth_mut()` — a raw `&mut` would let a caller construct a second
    /// pacer-drive loop that bypasses the single-arbitration guarantee
    /// ([`Registry::update_pacer_layers`]). Mutation instead goes through the
    /// narrow intent-scoped commands: [`Self::enable_googcc_for_subscriber`],
    /// [`Self::googcc_ceiling_for_subscriber_mut`], and the
    /// [`Propagated::ClientBudgetHint`] auto-consumption in `fanout_pending`.
    /// Tests that need raw injection keep using `bandwidth_mut_for_tests`
    /// (test-utils-gated).
    #[must_use]
    pub fn bandwidth(&self) -> &crate::bwe::estimator::BandwidthEstimator {
        &self.bandwidth
    }
}

/// GoogCC pass-throughs (ADR-8, ADR-9, Bug #6). Both `kalman-bwe` (hosts
/// `self.bandwidth`) and `googcc-bwe` (hosts the estimator these delegate to)
/// are required — see the `compile_error!` guard above this module's
/// `Registry` struct definition for the googcc-bwe-alone failure mode.
#[cfg(all(feature = "kalman-bwe", feature = "googcc-bwe"))]
#[cfg_attr(docsrs, doc(cfg(all(feature = "kalman-bwe", feature = "googcc-bwe"))))]
impl Registry {
    /// Enable the per-subscriber [`GoogCcEstimator`][crate::bwe::GoogCcEstimator]
    /// for `subscriber`, so it participates in [`Self::bandwidth`]'s combined
    /// estimate as an additional, independently-computed ceiling (ADR-14 —
    /// intentional ensemble with the native/Kalman signal, not duplication).
    ///
    /// Idempotent (delegates to
    /// [`BandwidthEstimator::enable_googcc_for_subscriber`][crate::BandwidthEstimator::enable_googcc_for_subscriber]).
    /// After enabling, [`Registry::poll_all`] auto-feeds the estimator from
    /// RTP-timestamp-derived video packet timing (ADR-1); use
    /// [`Self::googcc_ceiling_for_subscriber_mut`] only for advanced manual
    /// feeds (e.g. a TWCC-level consumer).
    pub fn enable_googcc_for_subscriber(&mut self, subscriber: crate::propagate::ClientId) {
        self.bandwidth.enable_googcc_for_subscriber(subscriber);
    }

    /// Mutable access to the per-subscriber
    /// [`GoogCcEstimator`][crate::bwe::GoogCcEstimator] for `subscriber`, for
    /// advanced manual ceiling feeds outside the [`Registry::poll_all`]
    /// auto-feed (e.g. a TWCC-level consumer feeding real receiver-reported
    /// timing instead of the RTP-timestamp approximation).
    ///
    /// Returns `None` if `subscriber` is unknown to the estimator or
    /// [`Self::enable_googcc_for_subscriber`] was never called for it.
    ///
    /// ADR-8: this is the *only* sanctioned ceiling-tweak path now that the
    /// raw `bandwidth_mut()` accessor has been dropped — narrow and
    /// intent-scoped, mirroring [`Self::on_twcc_feedback`]'s pass-through shape.
    #[must_use]
    pub fn googcc_ceiling_for_subscriber_mut(
        &mut self,
        subscriber: crate::propagate::ClientId,
    ) -> Option<&mut crate::bwe::GoogCcEstimator> {
        self.bandwidth.googcc_for_subscriber_mut(subscriber)
    }
}

#[cfg(all(test, feature = "kalman-bwe"))]
mod bandwidth_accessor_tests {
    use super::Registry;
    use crate::propagate::ClientId;

    #[test]
    fn bandwidth_reads_native_estimate_fed_via_on_twcc_feedback_seam() {
        // bandwidth() must reflect state written through the estimator's own
        // API (record_native_estimate via bandwidth_mut_for_tests here), not
        // just a fresh empty estimator — i.e. it's a live view, not a copy.
        let mut reg = Registry::new_for_tests();
        let id = ClientId(1);
        reg.bandwidth_mut_for_tests()
            .record_native_estimate(id, 1_000_000.0);
        assert!(
            reg.bandwidth()
                .estimate_bps(id, std::time::Instant::now())
                .is_some(),
            "bandwidth() must observe state written via the estimator seam"
        );
    }

    #[test]
    fn bandwidth_returns_none_for_unknown_subscriber() {
        let reg = Registry::new_for_tests();
        assert!(reg
            .bandwidth()
            .estimate_bps(ClientId(99), std::time::Instant::now())
            .is_none());
    }
}

#[cfg(all(test, feature = "kalman-bwe", feature = "googcc-bwe"))]
mod googcc_pass_through_tests {
    use super::Registry;
    use crate::propagate::ClientId;

    #[test]
    fn enable_googcc_for_subscriber_is_idempotent_and_reachable_via_ceiling_mut() {
        let mut reg = Registry::new_for_tests();
        let id = ClientId(7);

        // Not enabled yet: the ceiling accessor is None.
        assert!(reg.googcc_ceiling_for_subscriber_mut(id).is_none());

        reg.enable_googcc_for_subscriber(id);
        assert!(reg.googcc_ceiling_for_subscriber_mut(id).is_some());

        // Feed one packet, then re-enable: state must survive (idempotent).
        reg.googcc_ceiling_for_subscriber_mut(id)
            .unwrap()
            .on_receive(100.0, 95.0, 0.0);
        let bps_before = reg
            .googcc_ceiling_for_subscriber_mut(id)
            .unwrap()
            .current_bps();
        reg.enable_googcc_for_subscriber(id);
        let bps_after = reg
            .googcc_ceiling_for_subscriber_mut(id)
            .unwrap()
            .current_bps();
        assert_eq!(
            bps_before, bps_after,
            "re-enabling must not reset existing GoogCC state"
        );
    }

    #[test]
    fn googcc_ceiling_applies_through_bandwidth_accessor() {
        let mut reg = Registry::new_for_tests();
        let id = ClientId(8);
        let now = std::time::Instant::now();

        reg.bandwidth_mut_for_tests()
            .force_high_estimate_for_tests(id, 5_000_000.0);
        let bps_before = reg.bandwidth().estimate_bps(id, now).unwrap();
        assert!(
            bps_before > 1_000_000,
            "expected a high pre-ceiling estimate"
        );

        reg.enable_googcc_for_subscriber(id);
        let gcc = reg.googcc_ceiling_for_subscriber_mut(id).unwrap();
        for i in 0..20 {
            gcc.on_receive(i as f64 * 10.0, i as f64 * 10.0, 0.5);
        }

        let bps_after = reg.bandwidth().estimate_bps(id, now).unwrap();
        assert!(
            bps_after < bps_before,
            "GoogCC ceiling fed via the Registry pass-through did not apply: \
             before={bps_before}, after={bps_after}"
        );
    }
}
