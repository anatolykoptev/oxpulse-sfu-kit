//! 1D Kalman filter on TWCC inter-arrival delay gradient + AIMD rate control.
//!
//! Ported from oxpulse-partner-edge/crates/sfu/src/bandwidth/kalman.rs.

use std::time::{Duration, Instant};

/// Kalman process noise variance --- models uncertainty in the true gradient.
const PROCESS_NOISE_VAR: f64 = 1e-4;
/// Kalman measurement noise variance --- models jitter in observations.
const MEASUREMENT_NOISE_VAR: f64 = 5e-3;
/// Inter-arrival gradient above this threshold (us) signals overuse/congestion.
const OVERUSE_THRESHOLD_US: f64 = 12_500.0;
/// Gradient below this fraction of |estimate| signals underuse (spare capacity).
/// Multiplicative decrease factor applied on congestion (AIMD decrease).
const DECREASE_FACTOR: f64 = 0.85;
/// Additive increase per rate-control call when not overusing (bps).
const ADDITIVE_INCREASE_BPS: f64 = 8_000.0;
/// Minimum bitrate floor (bps).
pub const MIN_BITRATE_BPS: f64 = 30_000.0;
/// Maximum bitrate ceiling (bps).
pub const MAX_BITRATE_BPS: f64 = 50_000_000.0;
/// Minimum interval between consecutive multiplicative decreases.
const DECREASE_COOLDOWN: Duration = Duration::from_millis(200);

/// 1D Kalman filter on TWCC inter-arrival gradient + AIMD bitrate controller.
///
/// Feed inter-arrival gradients (us) via [`update_kalman`][Self::update_kalman],
/// then call [`apply_rate_control`][Self::apply_rate_control] to update the
/// bitrate estimate. Read the current estimate via [`bitrate_bps`][Self::bitrate_bps].
#[derive(Debug)]
pub struct DelayEstimator {
    /// Filtered gradient estimate (us).
    gradient_us: f64,
    /// Kalman error covariance.
    gradient_var: f64,
    /// Current bitrate estimate (bps).
    bitrate_bps: f64,
    /// Timestamp of the last multiplicative decrease (for cooldown).
    last_decrease: Option<Instant>,
}

impl DelayEstimator {
    /// Create with an initial bitrate estimate.
    pub fn new(initial_bitrate_bps: f64) -> Self {
        Self {
            gradient_us: 0.0,
            gradient_var: 1.0,
            bitrate_bps: initial_bitrate_bps.clamp(MIN_BITRATE_BPS, MAX_BITRATE_BPS),
            last_decrease: None,
        }
    }

    /// Feed one inter-arrival gradient sample (us) into the Kalman filter.
    ///
    /// `gradient_us = recv_delta_us - send_delta_us` (positive = growing delay = congestion).
    pub fn update_kalman(&mut self, gradient_us: f64) {
        // Predict: add process noise to covariance.
        self.gradient_var += PROCESS_NOISE_VAR;
        // Update: compute Kalman gain and apply.
        let gain = self.gradient_var / (self.gradient_var + MEASUREMENT_NOISE_VAR);
        self.gradient_us += gain * (gradient_us - self.gradient_us);
        self.gradient_var *= 1.0 - gain;
    }

    /// Apply AIMD rate control based on the current filtered gradient.
    ///
    /// Call once per batch of TWCC feedback, after all `update_kalman` calls.
    pub fn apply_rate_control(&mut self, now: Instant) {
        if self.gradient_us > OVERUSE_THRESHOLD_US {
            // Overuse: multiplicative decrease with cooldown.
            let can_decrease = self
                .last_decrease
                .map_or(true, |t| now.duration_since(t) >= DECREASE_COOLDOWN);
            if can_decrease {
                self.bitrate_bps = (self.bitrate_bps * DECREASE_FACTOR).max(MIN_BITRATE_BPS);
                self.last_decrease = Some(now);
            }
        } else {
            // No overuse: additive increase (applied every call).
            self.bitrate_bps = (self.bitrate_bps + ADDITIVE_INCREASE_BPS).min(MAX_BITRATE_BPS);
        }
    }

    /// Current bitrate estimate in bits per second.
    #[must_use]
    pub fn bitrate_bps(&self) -> f64 {
        self.bitrate_bps
    }

    /// Current filtered gradient (us). Positive = congestion, negative = spare capacity.
    #[must_use]
    pub fn filtered_gradient_us(&self) -> f64 {
        self.gradient_us
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn kalman_converges_to_injected_gradient() {
        let mut est = DelayEstimator::new(1_000_000.0);
        // Feed 100 identical gradient samples; Kalman should converge close.
        for _ in 0..100 {
            est.update_kalman(20_000.0);
        }
        let filtered = est.filtered_gradient_us();
        assert!(
            (filtered - 20_000.0).abs() < 3_000.0,
            "Kalman did not converge: got {filtered}, expected ~20000"
        );
    }

    #[test]
    fn rate_decreases_on_overuse() {
        let initial = 2_000_000.0;
        let mut est = DelayEstimator::new(initial);
        let now = Instant::now();
        // Strong overuse gradient (50ms = 50000us >> 12500us threshold)
        for _ in 0..20 {
            est.update_kalman(50_000.0);
        }
        let before = est.bitrate_bps();
        est.apply_rate_control(now);
        assert!(
            est.bitrate_bps() < before,
            "rate should decrease on overuse: {} >= {}",
            est.bitrate_bps(),
            before
        );
    }

    #[test]
    fn rate_respects_floor_and_ceiling() {
        let mut est = DelayEstimator::new(MIN_BITRATE_BPS);
        let now = Instant::now();
        // Massive overuse --- rate should not go below floor
        for _ in 0..100 {
            est.update_kalman(200_000.0);
        }
        for _ in 0..20 {
            est.apply_rate_control(now);
        }
        assert!(
            est.bitrate_bps() >= MIN_BITRATE_BPS,
            "floor violated: {}",
            est.bitrate_bps()
        );

        // Massive underuse --- rate should not exceed ceiling
        let mut est2 = DelayEstimator::new(MAX_BITRATE_BPS);
        for _ in 0..100 {
            est2.update_kalman(-1_000_000.0);
        }
        for _ in 0..1000 {
            est2.apply_rate_control(now);
        }
        assert!(
            est2.bitrate_bps() <= MAX_BITRATE_BPS,
            "ceiling violated: {}",
            est2.bitrate_bps()
        );
    }
}
