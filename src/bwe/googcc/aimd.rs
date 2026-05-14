//! Loss-based AIMD bandwidth controller --- GoogCC component 2.
//!
//! Implements additive increase / multiplicative decrease (AIMD):
//! - Increase: +8% when loss fraction < 0.5 %
//! - Decrease: ×0.85 when loss fraction > 2 % or trendline reports overuse
//!
//! Ported from `oxpulse-partner-edge`. **Per-subscriber** in this crate.

/// Loss fraction below which the controller applies additive increase.
const LOSS_LOW_THRESHOLD: f32 = 0.005; // 0.5 %
/// Loss fraction above which the controller applies multiplicative decrease.
const LOSS_HIGH_THRESHOLD: f32 = 0.02; // 2.0 %
/// Additive increase factor per update interval.
const AI_FRACTION: f64 = 0.08; // 8 %
/// Multiplicative decrease factor (RFC 3448 recommendation).
const MD_FACTOR: f64 = 0.85;

/// Loss-based AIMD congestion controller.
///
/// Call [`Self::update_loss`] each feedback interval with the observed loss
/// fraction. Call [`Self::on_overuse`] when [`super::trendline::TrendlineDetector`]
/// reports overuse.
///
/// # Examples
///
/// ```rust
/// # #[cfg(feature = "googcc-bwe")]
/// # {
/// use oxpulse_sfu_kit::bwe::googcc::AimdController;
///
/// let mut ctrl = AimdController::new(500_000, 50_000, 2_500_000);
/// let new_bps = ctrl.update_loss(0.001); // 0.1% loss -> increase
/// assert!(new_bps > 500_000);
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct AimdController {
    bitrate_bps: u64,
    min_bps: u64,
    max_bps: u64,
}

impl AimdController {
    /// Create with initial, minimum, and maximum bitrates (bps).
    pub fn new(initial_bps: u64, min_bps: u64, max_bps: u64) -> Self {
        Self {
            bitrate_bps: initial_bps,
            min_bps,
            max_bps,
        }
    }

    /// Apply AIMD based on the observed loss fraction `[0.0, 1.0]`.
    ///
    /// `loss_fraction` must be finite and in `[0.0, 1.0]`.
    /// Values outside that range are clamped rather than rejected so that a
    /// misbehaving RTCP parser does not permanently lock the bitrate.
    ///
    /// Returns the new target bitrate.
    pub fn update_loss(&mut self, loss_fraction: f32) -> u64 {
        debug_assert!(
            loss_fraction.is_finite(),
            "loss_fraction must be finite, got {loss_fraction}"
        );
        let loss_fraction = loss_fraction.clamp(0.0, 1.0);
        if loss_fraction < LOSS_LOW_THRESHOLD {
            let increase = (self.bitrate_bps as f64 * AI_FRACTION) as u64;
            self.bitrate_bps = (self.bitrate_bps + increase.max(8_000)).min(self.max_bps);
        } else if loss_fraction > LOSS_HIGH_THRESHOLD {
            self.bitrate_bps = ((self.bitrate_bps as f64 * MD_FACTOR) as u64).max(self.min_bps);
        }
        // In the neutral zone [LOW, HIGH]: hold current bitrate.
        self.bitrate_bps
    }

    /// Apply multiplicative decrease immediately (called on trendline overuse).
    ///
    /// Returns the new target bitrate.
    pub fn on_overuse(&mut self) -> u64 {
        self.bitrate_bps = ((self.bitrate_bps as f64 * MD_FACTOR) as u64).max(self.min_bps);
        self.bitrate_bps
    }

    /// Current target bitrate (bps).
    pub fn current(&self) -> u64 {
        self.bitrate_bps
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_loss_increases_bitrate() {
        let mut c = AimdController::new(500_000, 100_000, 2_000_000);
        let before = c.current();
        c.update_loss(0.001);
        assert!(c.current() > before);
    }

    #[test]
    fn high_loss_decreases_bitrate() {
        let mut c = AimdController::new(500_000, 100_000, 2_000_000);
        let before = c.current();
        c.update_loss(0.05);
        assert!(c.current() < before);
    }

    #[test]
    fn neutral_loss_holds_bitrate() {
        let mut c = AimdController::new(500_000, 100_000, 2_000_000);
        let before = c.current();
        // Loss between thresholds => hold
        c.update_loss(0.01);
        assert_eq!(c.current(), before);
    }

    #[test]
    fn overuse_decreases_bitrate() {
        let mut c = AimdController::new(1_000_000, 100_000, 2_000_000);
        let before = c.current();
        c.on_overuse();
        assert!(c.current() < before);
    }

    #[test]
    fn bitrate_never_exceeds_max() {
        let mut c = AimdController::new(1_900_000, 100_000, 2_000_000);
        for _ in 0..100 {
            c.update_loss(0.0);
        }
        assert!(c.current() <= 2_000_000);
    }

    #[test]
    fn bitrate_never_goes_below_min() {
        let mut c = AimdController::new(200_000, 100_000, 2_000_000);
        for _ in 0..100 {
            c.on_overuse();
        }
        assert!(c.current() >= 100_000);
    }
}
