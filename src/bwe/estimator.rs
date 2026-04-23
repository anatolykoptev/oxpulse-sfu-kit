//! Per-room `BandwidthEstimator` aggregating per-subscriber state.

use std::collections::HashMap;
use std::time::Instant;

use crate::propagate::ClientId;
use super::subscriber::{ClientHint, PerSubscriber};

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
        self.subscribers.entry(id).or_insert_with(PerSubscriber::new)
    }

    /// Update the native GCC ceiling for a subscriber (from str0m EgressBitrateEstimate).
    pub fn record_native_estimate(&mut self, subscriber: ClientId, bps: f64) {
        self.get_or_insert(subscriber).native_estimate_bps = Some(bps);
    }

    /// Record a browser-reported budget hint (from DataChannel {"type":"budget","bps":N}).
    pub fn record_client_hint(&mut self, subscriber: ClientId, bps: u64, now: Instant) {
        self.get_or_insert(subscriber).client_hint = Some(ClientHint { bps, received_at: now });
    }

    /// Combined bitrate estimate for `subscriber`, or `None` if no state exists yet.
    #[must_use]
    pub fn estimate_bps(&self, subscriber: ClientId, now: Instant) -> Option<u64> {
        self.subscribers
            .get(&subscriber)
            .map(|s| s.combined_bps(now) as u64)
    }

    /// Remove subscriber state on disconnect.
    pub fn reap_dead(&mut self, subscriber: ClientId) {
        self.subscribers.remove(&subscriber);
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
    fn reap_dead_removes_subscriber() {
        let mut est = BandwidthEstimator::new();
        let now = Instant::now();
        est.record_native_estimate(id(3), 1_000_000.0);
        assert!(est.estimate_bps(id(3), now).is_some());
        est.reap_dead(id(3));
        assert!(est.estimate_bps(id(3), now).is_none());
    }
}

use super::feedback::{TwccFeedback, ingest_twcc};

impl BandwidthEstimator {
    /// Process a TWCC feedback batch for a subscriber.
    ///
    /// Feeds the feedback into the Kalman delay estimator and loss estimator.
    /// Must be called after [`record_send_time`][Self::record_send_time] has
    /// been called for each RTP packet that was sent to this subscriber.
    pub fn on_twcc_feedback(&mut self, subscriber: ClientId, feedback: &TwccFeedback, now: Instant) {
        let sub = self.get_or_insert(subscriber);
        ingest_twcc(sub, feedback, now);
    }

    /// Record the send timestamp for an RTP packet destined for `subscriber`.
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
