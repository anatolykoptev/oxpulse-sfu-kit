//! Per-room `BandwidthEstimator` aggregating per-subscriber state.

use std::collections::HashMap;
use std::time::Instant;

use super::subscriber::{BindingTerm, ClientHint, PerSubscriber};
use crate::propagate::ClientId;

/// Per-room bandwidth estimator: one `PerSubscriber` entry per connected peer.
#[derive(Debug, Default)]
pub struct BandwidthEstimator {
    subscribers: HashMap<ClientId, PerSubscriber>,
}

impl BandwidthEstimator {
    /// Create an empty estimator.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get or create subscriber state for `id`.
    pub(crate) fn get_or_insert(&mut self, id: ClientId) -> &mut PerSubscriber {
        self.subscribers.entry(id).or_default()
    }

    /// Update the native GCC ceiling for a subscriber (from str0m EgressBitrateEstimate).
    pub fn record_native_estimate(&mut self, subscriber: ClientId, bps: f64) {
        self.get_or_insert(subscriber).native_estimate_bps = Some(bps);
    }

    /// Record a browser-reported budget hint (from DataChannel {"type":"budget","bps":N}).
    pub fn record_client_hint(&mut self, subscriber: ClientId, bps: u64, now: Instant) {
        self.get_or_insert(subscriber).client_hint = Some(ClientHint {
            bps,
            received_at: now,
        });
    }

    /// Combined bitrate estimate for `subscriber`, or `None` if no state exists yet.
    #[must_use]
    pub fn estimate_bps(&self, subscriber: ClientId, now: Instant) -> Option<u64> {
        self.subscribers
            .get(&subscriber)
            .map(|s| s.combined_bps(now) as u64)
    }

    /// Combined bitrate estimate *and* the [`BindingTerm`] that bound the min()
    /// chain, or `None` if no state exists yet.
    ///
    /// Sibling of [`Self::estimate_bps`]; the `u64` component is identical to
    /// that method's result. Lets a consumer label an observability metric with
    /// *which* ceiling term is binding the estimate without reaching into the
    /// private per-subscriber state or re-deriving the min-chain (issue #2310 V0).
    #[must_use]
    pub fn estimate_with_term(
        &self,
        subscriber: ClientId,
        now: Instant,
    ) -> Option<(u64, BindingTerm)> {
        self.subscribers.get(&subscriber).map(|s| {
            let (bps, term) = s.combined_bps_with_term(now);
            (bps as u64, term)
        })
    }

    /// Remove subscriber state on disconnect.
    pub fn reap_dead(&mut self, subscriber: ClientId) {
        self.subscribers.remove(&subscriber);
    }

    /// GoogCC-tier coverage as `(active, total)`: how many tracked subscribers have
    /// a GoogCC estimator wired in (`PerSubscriber::googcc_active`) versus the total.
    ///
    /// Exposes the presence signal that `estimate_bps` alone hides: a subscriber
    /// without GoogCC silently omits the GoogCC ceiling, which is indistinguishable
    /// from "GoogCC wired but not currently constraining". Emit as a gauge / alert
    /// when `active < total` in a room built with `googcc-bwe`.
    #[cfg(feature = "googcc-bwe")]
    #[must_use]
    pub fn googcc_coverage(&self) -> (usize, usize) {
        let active = self
            .subscribers
            .values()
            .filter(|s| s.googcc_active())
            .count();
        (active, self.subscribers.len())
    }

    /// Force both the Kalman delay and loss estimators for `subscriber` to report
    /// `bps`, bypassing TWCC.  Use in tests that need a known estimate without
    /// simulating real network feedback.
    #[cfg(any(test, feature = "test-utils"))]
    #[doc(hidden)]
    pub fn force_high_estimate_for_tests(&mut self, subscriber: ClientId, bps: f64) {
        let sub = self.get_or_insert(subscriber);
        sub.delay = super::kalman::DelayEstimator::new(bps);
        sub.loss = super::loss::LossEstimator::new(bps);
        sub.native_estimate_bps = None; // remove ceiling so Kalman/loss dominate
    }

    /// Enable the per-subscriber [`GoogCcEstimator`] for `id`.
    ///
    /// Sets `PerSubscriber.googcc = Some(GoogCcEstimator::new())` so the
    /// estimator participates in [`Self::estimate_bps`] as an additional
    /// ceiling (delegated to [`PerSubscriber::combined_bps`]).
    ///
    /// Idempotent — calling twice on the same subscriber preserves the
    /// existing estimator state (does NOT reset).
    ///
    /// After enabling, feed packet timing via
    /// [`Self::googcc_for_subscriber_mut`].
    ///
    /// # Example
    /// ```no_run
    /// # #[cfg(feature = "googcc-bwe")]
    /// # {
    /// use oxpulse_sfu_kit::BandwidthEstimator;
    /// use oxpulse_sfu_kit::ClientId;
    ///
    /// let mut est = BandwidthEstimator::new();
    /// est.enable_googcc_for_subscriber(ClientId(42));
    /// // Now est.googcc_for_subscriber_mut(ClientId(42)) returns Some(&mut _).
    /// # }
    /// ```
    ///
    /// [`GoogCcEstimator`]: super::googcc::GoogCcEstimator
    #[cfg(feature = "googcc-bwe")]
    #[cfg_attr(docsrs, doc(cfg(feature = "googcc-bwe")))]
    pub fn enable_googcc_for_subscriber(&mut self, id: ClientId) {
        let sub = self.get_or_insert(id);
        if sub.googcc.is_none() {
            sub.googcc = Some(super::googcc::GoogCcEstimator::new());
        }
    }

    /// Mutable accessor to the per-subscriber [`GoogCcEstimator`] for feeding
    /// packet arrival timing from the TWCC handler.
    ///
    /// Returns `None` if either the subscriber doesn't exist or GoogCC was
    /// never enabled for it via [`Self::enable_googcc_for_subscriber`].
    ///
    /// # Example
    /// ```no_run
    /// # #[cfg(feature = "googcc-bwe")]
    /// # {
    /// use oxpulse_sfu_kit::BandwidthEstimator;
    /// use oxpulse_sfu_kit::ClientId;
    ///
    /// let mut est = BandwidthEstimator::new();
    /// let id = ClientId(7);
    /// est.enable_googcc_for_subscriber(id);
    ///
    /// // From the TWCC handler:
    /// if let Some(gcc) = est.googcc_for_subscriber_mut(id) {
    ///     gcc.on_receive(/* arrival_ms */ 100.0, /* send_ms */ 95.0, /* loss */ 0.0);
    /// }
    /// # }
    /// ```
    ///
    /// [`GoogCcEstimator`]: super::googcc::GoogCcEstimator
    #[cfg(feature = "googcc-bwe")]
    #[cfg_attr(docsrs, doc(cfg(feature = "googcc-bwe")))]
    #[must_use]
    pub fn googcc_for_subscriber_mut(
        &mut self,
        id: ClientId,
    ) -> Option<&mut super::googcc::GoogCcEstimator> {
        self.subscribers.get_mut(&id)?.googcc.as_mut()
    }
}

use super::feedback::{ingest_twcc, TwccFeedback};

impl BandwidthEstimator {
    /// Process a TWCC feedback batch for a subscriber.
    ///
    /// Feeds the feedback into the Kalman delay estimator and loss estimator.
    /// Must be called after [][Self::record_send_time] has
    /// been called for each RTP packet that was sent to this subscriber.
    pub fn on_twcc_feedback(
        &mut self,
        subscriber: ClientId,
        feedback: &TwccFeedback,
        now: Instant,
    ) {
        let sub = self.get_or_insert(subscriber);
        ingest_twcc(sub, feedback, now);
    }

    /// Record the send timestamp for an RTP packet destined for .
    ///
    /// Call this when each RTP packet is enqueued. The send time is used to
    /// compute inter-send deltas when TWCC feedback arrives.
    pub fn record_send_time(&mut self, subscriber: ClientId, seq: u64, sent_at: Instant) {
        let sub = self.get_or_insert(subscriber);
        // Bound the map: evict the oldest entry when it grows too large.
        const MAX_SEND_TIMES: usize = 512;
        if sub.send_times.len() >= MAX_SEND_TIMES {
            if let Some(&oldest_seq) = sub.send_times.keys().min() {
                sub.send_times.remove(&oldest_seq);
            }
        }
        sub.send_times.insert(seq, sent_at);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::propagate::ClientId;
    use std::time::Instant;

    fn id(n: u64) -> ClientId {
        ClientId(n)
    }

    #[test]
    fn estimate_returns_none_for_unknown_subscriber() {
        let est = BandwidthEstimator::new();
        assert!(est.estimate_bps(id(99), Instant::now()).is_none());
    }

    #[test]
    fn native_estimate_acts_as_ceiling_via_estimator() {
        let mut est = BandwidthEstimator::new();
        let now = Instant::now();
        // First call to record_native_estimate creates the entry.
        est.record_native_estimate(id(1), 600_000.0);
        // The PerSubscriber is initialised at INITIAL_BITRATE_BPS (300k) < 600k,
        // so the native ceiling should not reduce it. Just verify we get a value.
        let bps = est.estimate_bps(id(1), now).unwrap();
        assert!(bps > 0, "expected non-zero estimate");
    }

    #[test]
    fn client_hint_caps_estimate() {
        let mut est = BandwidthEstimator::new();
        let now = Instant::now();
        // Force high internal estimates by creating subscriber and overriding directly.
        {
            let sub = est.get_or_insert(id(2));
            sub.delay = super::super::kalman::DelayEstimator::new(5_000_000.0);
            sub.loss = super::super::loss::LossEstimator::new(5_000_000.0);
        }
        est.record_client_hint(id(2), 400_000, now);
        let bps = est.estimate_bps(id(2), now).unwrap();
        assert!(bps <= 400_100, "hint ceiling not applied: {bps}");
    }

    #[test]
    fn estimate_with_term_none_for_unknown_subscriber() {
        let est = BandwidthEstimator::new();
        assert!(est.estimate_with_term(id(99), Instant::now()).is_none());
    }

    #[test]
    fn estimate_with_term_reports_client_hint_and_matching_value() {
        let mut est = BandwidthEstimator::new();
        let now = Instant::now();
        {
            let sub = est.get_or_insert(id(5));
            sub.delay = super::super::kalman::DelayEstimator::new(5_000_000.0);
            sub.loss = super::super::loss::LossEstimator::new(5_000_000.0);
        }
        est.record_client_hint(id(5), 400_000, now);
        let (bps, term) = est.estimate_with_term(id(5), now).unwrap();
        assert_eq!(term, BindingTerm::ClientHint);
        // Value component must equal estimate_bps for the same instant.
        assert_eq!(Some(bps), est.estimate_bps(id(5), now));
    }

    #[cfg(feature = "googcc-bwe")]
    #[test]
    fn enable_googcc_creates_estimator_when_missing() {
        let mut est = BandwidthEstimator::new();
        assert!(est.googcc_for_subscriber_mut(id(10)).is_none());
        est.enable_googcc_for_subscriber(id(10));
        assert!(est.googcc_for_subscriber_mut(id(10)).is_some());
    }

    #[cfg(feature = "googcc-bwe")]
    #[test]
    fn enable_googcc_is_idempotent_preserves_state() {
        let mut est = BandwidthEstimator::new();
        est.enable_googcc_for_subscriber(id(11));
        // Feed one packet to mutate internal state.
        est.googcc_for_subscriber_mut(id(11))
            .unwrap()
            .on_receive(100.0, 95.0, 0.0);
        let bps_after_feed = est.googcc_for_subscriber_mut(id(11)).unwrap().current_bps();

        // Second enable must NOT reset the estimator.
        est.enable_googcc_for_subscriber(id(11));
        let bps_after_reenable = est.googcc_for_subscriber_mut(id(11)).unwrap().current_bps();
        assert_eq!(
            bps_after_feed, bps_after_reenable,
            "enable_googcc_for_subscriber must be idempotent"
        );
    }

    #[cfg(feature = "googcc-bwe")]
    #[test]
    fn googcc_for_subscriber_mut_returns_none_when_disabled() {
        let mut est = BandwidthEstimator::new();
        // Subscriber exists (via record_native_estimate) but googcc never enabled.
        est.record_native_estimate(id(12), 1_000_000.0);
        assert!(est.googcc_for_subscriber_mut(id(12)).is_none());
    }

    #[cfg(feature = "googcc-bwe")]
    #[test]
    fn googcc_for_subscriber_mut_returns_none_for_unknown_subscriber() {
        let mut est = BandwidthEstimator::new();
        assert!(est.googcc_for_subscriber_mut(id(99)).is_none());
    }

    #[cfg(feature = "googcc-bwe")]
    #[test]
    fn googcc_ceiling_applies_to_estimate_bps() {
        let mut est = BandwidthEstimator::new();
        let now = Instant::now();
        est.force_high_estimate_for_tests(id(13), 5_000_000.0);
        // Without GoogCC, estimate is high.
        let bps_before = est.estimate_bps(id(13), now).unwrap();
        assert!(
            bps_before > 1_000_000,
            "expected high estimate: {bps_before}"
        );

        // Enable GoogCC and feed loss to drive it down.
        est.enable_googcc_for_subscriber(id(13));
        let gcc = est.googcc_for_subscriber_mut(id(13)).unwrap();
        // GoogCC starts at INITIAL_BPS=500_000 and decays under high loss.
        for i in 0..20 {
            gcc.on_receive(i as f64 * 10.0, i as f64 * 10.0, 0.5);
        }
        let bps_after = est.estimate_bps(id(13), now).unwrap();
        assert!(
            bps_after < bps_before,
            "GoogCC ceiling did not apply: before={bps_before}, after={bps_after}"
        );
    }

    #[test]
    fn reap_dead_removes_subscriber() {
        let mut est = BandwidthEstimator::new();
        let now = Instant::now();
        est.record_native_estimate(id(3), 1_000_000.0);
        assert!(est.estimate_bps(id(3), now).is_some());
        est.reap_dead(id(3));
        assert!(est.estimate_bps(id(3), now).is_none());
    }

    #[cfg(feature = "googcc-bwe")]
    #[test]
    fn googcc_coverage_counts_enabled_subscribers() {
        let mut est = BandwidthEstimator::new();
        est.record_native_estimate(id(1), 1_000_000.0);
        est.record_native_estimate(id(2), 1_000_000.0);
        assert_eq!(
            est.googcc_coverage(),
            (0, 2),
            "no subscriber has googcc yet"
        );
        est.enable_googcc_for_subscriber(id(1));
        assert_eq!(
            est.googcc_coverage(),
            (1, 2),
            "one of two subscribers now has the googcc ceiling wired"
        );
    }
}
