//! Per-subscriber GoogCC v2 bandwidth estimator.
//!
//! Combines [`TrendlineDetector`] (delay-based overuse detection) and
//! [`AimdController`] (loss-based rate control) into a single per-subscriber
//! estimator. The trendline detector feeds overuse signals directly into the
//! AIMD controller for multiplicative decrease.
//!
//! **Per-subscriber design.** Unlike the original `oxpulse-partner-edge`
//! implementation (one `GoogCcEstimator` per room), each subscriber gets its
//! own instance. This ensures that congestion on one link does not affect the
//! estimated capacity for other subscribers in the same room.
//!
//! # Integration with `PerSubscriber`
//!
//! Enable via the `googcc-bwe` feature.  When the feature is active,
//! [`crate::bwe::subscriber::PerSubscriber`] gains a `googcc` field that is
//! included in the [`combined_bps`][crate::bwe::subscriber::PerSubscriber::combined_bps]
//! calculation as an additional ceiling.

pub use self::aimd::AimdController;
pub use self::trendline::{BandwidthState, TrendlineDetector};

mod aimd;
mod trendline;

/// Default initial bitrate assigned to a new GoogCC estimator (bps).
pub const GOOGCC_INITIAL_BPS: u64 = 500_000;
/// Default minimum bitrate floor (bps).
pub const GOOGCC_MIN_BPS: u64 = 100_000;
/// Default maximum bitrate ceiling (bps).
pub const GOOGCC_MAX_BPS: u64 = 2_500_000;

/// Per-subscriber GoogCC v2 bandwidth estimator.
///
/// Feed packet timing information via [`Self::on_receive`]; the estimator
/// returns a target bitrate. Call [`Self::current_bps`] at any time to read
/// the latest estimate.
///
/// # Examples
///
/// ```rust
/// # #[cfg(feature = "googcc-bwe")]
/// # {
/// use oxpulse_sfu_kit::bwe::googcc::GoogCcEstimator;
///
/// let mut est = GoogCcEstimator::new();
/// // Stable link: 30 packets with equal inter-arrival and send deltas
/// for i in 0u32..30 {
///     let t = 20.0 * (1.0 + i as f64);
///     est.on_receive(t, t, 0.001); // 0.1% loss
/// }
/// assert!(est.current_bps() > 500_000, "low-loss link should have increased bitrate");
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct GoogCcEstimator {
    trendline: TrendlineDetector,
    aimd: AimdController,
    last_arrival_ms: Option<f64>,
    last_send_ms: Option<f64>,
}

impl GoogCcEstimator {
    /// Create a new estimator with default bitrate bounds.
    pub fn new() -> Self {
        Self::with_bounds(GOOGCC_INITIAL_BPS, GOOGCC_MIN_BPS, GOOGCC_MAX_BPS)
    }

    /// Create with explicit initial, minimum, and maximum bitrates (bps).
    pub fn with_bounds(initial_bps: u64, min_bps: u64, max_bps: u64) -> Self {
        Self {
            trendline: TrendlineDetector::new(),
            aimd: AimdController::new(initial_bps, min_bps, max_bps),
            last_arrival_ms: None,
            last_send_ms: None,
        }
    }

    /// Feed a received packet's timing and loss information.
    ///
    /// - `arrival_ms`: absolute arrival timestamp (ms, monotonic).
    /// - `send_ms`: the packet's send timestamp (ms, from RTP extension or TWCC).
    /// - `loss_fraction`: fraction of packets lost in this feedback interval `[0.0, 1.0]`.
    ///
    /// Returns the updated target bitrate (bps).
    pub fn on_receive(&mut self, arrival_ms: f64, send_ms: f64, loss_fraction: f32) -> u64 {
        debug_assert!(
            arrival_ms.is_finite() && send_ms.is_finite(),
            "arrival_ms and send_ms must be finite; NaN/inf silently locks state to Normal"
        );
        debug_assert!(
            loss_fraction.is_finite() && (0.0..=1.0).contains(&loss_fraction),
            "loss_fraction must be finite and in [0.0, 1.0], got {loss_fraction}"
        );
        if let (Some(la), Some(ls)) = (self.last_arrival_ms, self.last_send_ms) {
            let arr_delta = arrival_ms - la;
            let snd_delta = send_ms - ls;
            self.trendline.update(arr_delta, snd_delta);

            if self.trendline.overuse() {
                self.last_arrival_ms = Some(arrival_ms);
                self.last_send_ms = Some(send_ms);
                return self.aimd.on_overuse();
            }
        }
        self.last_arrival_ms = Some(arrival_ms);
        self.last_send_ms = Some(send_ms);
        self.aimd.update_loss(loss_fraction)
    }

    /// Current target bitrate estimate (bps).
    pub fn current_bps(&self) -> u64 {
        self.aimd.current()
    }

    /// Force the AIMD bitrate to a specific value.
    ///
    /// Use in tests to set up a known estimate without simulating real TWCC
    /// feedback. Does **not** modify trendline state.
    #[cfg(any(test, feature = "test-utils"))]
    #[doc(hidden)]
    pub fn force_bps_for_tests(&mut self, bps: u64) {
        self.aimd = AimdController::new(bps, GOOGCC_MIN_BPS, GOOGCC_MAX_BPS);
    }
}

impl Default for GoogCcEstimator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_bitrate_is_default() {
        let est = GoogCcEstimator::new();
        assert_eq!(est.current_bps(), GOOGCC_INITIAL_BPS);
    }

    #[test]
    fn low_loss_raises_bitrate() {
        let mut est = GoogCcEstimator::new();
        for i in 0u32..30 {
            let t = 20.0 * (1.0 + i as f64);
            est.on_receive(t, t, 0.001);
        }
        assert!(
            est.current_bps() > GOOGCC_INITIAL_BPS,
            "low-loss stable link should increase bitrate"
        );
    }

    #[test]
    fn overuse_signal_decreases_bitrate() {
        let mut est = GoogCcEstimator::new();
        // Establish high bitrate
        for i in 0u32..30 {
            let t = 20.0 * (1.0 + i as f64);
            est.on_receive(t, t, 0.001);
        }
        let high_bps = est.current_bps();

        // Simulate growing delay (overuse)
        for i in 0u32..30 {
            let base = 20.0 * (31.0 + i as f64);
            est.on_receive(base + i as f64 * 50.0, base, 0.001);
        }
        assert!(
            est.current_bps() < high_bps,
            "overuse should reduce bitrate"
        );
    }

    #[test]
    fn high_loss_keeps_bitrate_low() {
        let mut est = GoogCcEstimator::new();
        for i in 0u32..10 {
            let t = 20.0 * (1.0 + i as f64);
            est.on_receive(t, t, 0.05); // 5% loss
        }
        assert!(
            est.current_bps() <= GOOGCC_INITIAL_BPS,
            "high loss should not increase bitrate"
        );
    }

    #[test]
    fn with_bounds_respects_min_max() {
        let mut est = GoogCcEstimator::with_bounds(200_000, 100_000, 300_000);
        // Drive bitrate up
        for i in 0u32..100 {
            let t = 20.0 * (1.0 + i as f64);
            est.on_receive(t, t, 0.0);
        }
        assert!(est.current_bps() <= 300_000, "must not exceed max_bps");
    }
}
