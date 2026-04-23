//! Per-subscriber state combining Kalman delay, loss, native GCC, and client hint.
//!
//! Ported from `oxpulse-partner-edge/crates/sfu/src/bandwidth/subscriber.rs`.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::kalman::{DelayEstimator, MAX_BITRATE_BPS, MIN_BITRATE_BPS};
use super::loss::LossEstimator;

/// Initial bitrate assigned to a new subscriber (bps).
const INITIAL_BITRATE_BPS: f64 = 300_000.0;

/// Maximum age of a client-reported budget hint before it is discarded.
const CLIENT_HINT_MAX_AGE: Duration = Duration::from_secs(5);

/// Browser-reported bandwidth budget from DataChannel `{"type":"budget","bps":N}`.
#[derive(Debug, Clone, Copy)]
pub struct ClientHint {
    /// Budget ceiling in bits per second.
    pub bps: u64,
    /// Monotonic timestamp when the hint was received.
    pub received_at: Instant,
}

/// Per-subscriber BWE state: delay estimate, loss estimate, native GCC ceiling,
/// browser hint ceiling, and send-time map for TWCC gradient computation.
#[derive(Debug)]
pub struct PerSubscriber {
    /// Map from extended RTP seq number -> send Instant, used to compute
    /// inter-send deltas for the TWCC gradient.
    pub send_times: HashMap<u64, Instant>,
    /// Arrival time of the last successfully received packet for gradient delta.
    pub last_arrival: Option<Instant>,
    /// Kalman-filtered delay-based rate estimator.
    pub delay: DelayEstimator,
    /// Loss-window-based rate estimator.
    pub loss: LossEstimator,
    /// Estimated round-trip time (from RTCP SR/RR, if available).
    pub rtt: Option<Duration>,
    /// Native GCC estimate from str0m EgressBitrateEstimate event (ceiling).
    pub native_estimate_bps: Option<f64>,
    /// Browser-reported budget hint (additional ceiling, expires after 5s).
    pub client_hint: Option<ClientHint>,
}

impl PerSubscriber {
    /// Create new subscriber state at INITIAL_BITRATE_BPS.
    pub fn new() -> Self {
        Self {
            send_times: HashMap::new(),
            last_arrival: None,
            delay: DelayEstimator::new(INITIAL_BITRATE_BPS),
            loss: LossEstimator::new(INITIAL_BITRATE_BPS),
            rtt: None,
            native_estimate_bps: None,
            client_hint: None,
        }
    }

    /// Combined bitrate estimate: min(kalman, loss) then apply GCC and hint ceilings.
    ///
    /// Returns at least 0; the result is not further clamped to MIN_BITRATE_BPS
    /// here so callers can distinguish "no estimate yet" from a floor-constrained value.
    #[must_use]
    pub fn combined_bps(&self, now: Instant) -> f64 {
        let base = self.delay.bitrate_bps().min(self.loss.bitrate_bps());

        let after_native = match self.native_estimate_bps {
            Some(native) => base.min(native),
            None => base,
        };

        let after_hint = match self.client_hint {
            Some(h) if now.duration_since(h.received_at) < CLIENT_HINT_MAX_AGE => {
                after_native.min(h.bps as f64)
            }
            _ => after_native,
        };

        after_hint.max(0.0)
    }
}

impl Default for PerSubscriber {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn combined_bps_takes_minimum_of_delay_and_loss() {
        let now = Instant::now();
        let mut sub = PerSubscriber::new();
        sub.delay = DelayEstimator::new(2_000_000.0);
        sub.loss = LossEstimator::new(1_000_000.0);
        let combined = sub.combined_bps(now);
        // Should take the minimum ~1_000_000
        assert!(
            combined <= 1_100_000.0,
            "expected approx 1Mbps (min of 2M and 1M), got {combined}"
        );
    }

    #[test]
    fn native_estimate_acts_as_ceiling() {
        let now = Instant::now();
        let mut sub = PerSubscriber::new();
        sub.delay = DelayEstimator::new(2_000_000.0);
        sub.loss = LossEstimator::new(2_000_000.0);
        sub.native_estimate_bps = Some(500_000.0);
        let combined = sub.combined_bps(now);
        assert!(
            combined <= 500_100.0,
            "native GCC ceiling not applied: {combined}"
        );
    }

    #[test]
    fn client_hint_acts_as_ceiling_when_fresh() {
        let now = Instant::now();
        let mut sub = PerSubscriber::new();
        sub.delay = DelayEstimator::new(2_000_000.0);
        sub.loss = LossEstimator::new(2_000_000.0);
        sub.client_hint = Some(ClientHint { bps: 400_000, received_at: now });
        let combined = sub.combined_bps(now);
        assert!(
            combined <= 400_100.0,
            "client hint ceiling not applied: {combined}"
        );
    }

    #[test]
    fn stale_client_hint_is_ignored() {
        let past = Instant::now() - Duration::from_secs(10); // older than CLIENT_HINT_MAX_AGE
        let now = Instant::now();
        let mut sub = PerSubscriber::new();
        sub.delay = DelayEstimator::new(2_000_000.0);
        sub.loss = LossEstimator::new(2_000_000.0);
        sub.client_hint = Some(ClientHint { bps: 100, received_at: past }); // absurdly small
        let combined = sub.combined_bps(now);
        // Hint is stale -> should not cap at 100 bps
        assert!(
            combined > 1_000.0,
            "stale client hint should be ignored, got {combined}"
        );
    }
}
