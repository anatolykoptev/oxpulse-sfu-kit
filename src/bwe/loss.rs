//! Loss-based rate controller with sliding window.
//!
//! Ported from `oxpulse-partner-edge/crates/sfu/src/bandwidth/loss.rs`.

use std::collections::VecDeque;
use std::time::{Duration, Instant};
use super::kalman::{MAX_BITRATE_BPS, MIN_BITRATE_BPS};

/// Sliding window size in packets.
const LOSS_WINDOW: usize = 64;
/// Loss fraction above which the rate is decreased (10%).
const LOSS_HIGH_THRESHOLD: f64 = 0.10;
/// Loss fraction below which rate increase is allowed (2%).
const LOSS_LOW_THRESHOLD: f64 = 0.02;
/// Multiplicative decrease factor on high loss.
const LOSS_DECREASE_FACTOR: f64 = 0.85;
/// Cooldown between consecutive multiplicative decreases.
const LOSS_DECREASE_COOLDOWN: Duration = Duration::from_millis(250);
/// Additive increase per call when loss is low (bps).
const LOSS_INCREASE_BPS: f64 = 5_000.0;

/// Loss-based bitrate controller using a fixed-size sliding packet window.
///
/// Records per-packet received/lost status and applies AIMD:
/// - Loss >= 10%: multiplicative decrease (x0.85), with 250ms cooldown.
/// - Loss < 2%: additive increase (+5 kbps).
/// - 2%-10%: hold current rate.
#[derive(Debug)]
pub struct LossEstimator {
    window: VecDeque<bool>,   // true = received, false = lost
    bitrate_bps: f64,
    last_decrease: Option<Instant>,
}

impl LossEstimator {
    /// Create with an initial bitrate estimate.
    pub fn new(initial_bitrate_bps: f64) -> Self {
        Self {
            window: VecDeque::with_capacity(LOSS_WINDOW + 1),
            bitrate_bps: initial_bitrate_bps.clamp(MIN_BITRATE_BPS, MAX_BITRATE_BPS),
            last_decrease: None,
        }
    }

    /// Record whether a packet was received (`true`) or lost (`false`).
    pub fn record(&mut self, received: bool) {
        if self.window.len() >= LOSS_WINDOW {
            self.window.pop_front();
        }
        self.window.push_back(received);
    }

    /// Fraction of lost packets in the current window (0.0-1.0).
    #[must_use]
    pub fn loss_fraction(&self) -> f64 {
        if self.window.is_empty() {
            return 0.0;
        }
        let lost = self.window.iter().filter(|&&r| !r).count();
        lost as f64 / self.window.len() as f64
    }

    /// Apply AIMD rate control based on current window loss fraction.
    ///
    /// Call after recording a batch of packets.
    pub fn apply_rate_control(&mut self, now: Instant) {
        let loss = self.loss_fraction();
        if loss >= LOSS_HIGH_THRESHOLD {
            // High loss: multiplicative decrease with cooldown.
            let can_decrease = self
                .last_decrease
                .map_or(true, |t| now.duration_since(t) >= LOSS_DECREASE_COOLDOWN);
            if can_decrease {
                self.bitrate_bps = (self.bitrate_bps * LOSS_DECREASE_FACTOR).max(MIN_BITRATE_BPS);
                self.last_decrease = Some(now);
            }
        } else if loss < LOSS_LOW_THRESHOLD {
            // Low loss: additive increase.
            self.bitrate_bps = (self.bitrate_bps + LOSS_INCREASE_BPS).min(MAX_BITRATE_BPS);
        }
        // In between: hold current rate.
    }

    /// Current bitrate estimate (bps).
    #[must_use]
    pub fn bitrate_bps(&self) -> f64 {
        self.bitrate_bps
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn loss_fraction_computed_correctly() {
        let mut est = LossEstimator::new(1_000_000.0);
        // 20% loss: 4 out of every 5 received
        for i in 0..LOSS_WINDOW {
            est.record(i % 5 != 0); // every 5th is lost
        }
        let f = est.loss_fraction();
        assert!(
            (f - 0.20).abs() < 0.05,
            "expected ~20% loss fraction, got {f}"
        );
    }

    #[test]
    fn rate_decreases_on_high_loss() {
        let initial = 2_000_000.0;
        let mut est = LossEstimator::new(initial);
        let now = Instant::now();
        // 50% loss
        for i in 0..LOSS_WINDOW {
            est.record(i % 2 == 0);
        }
        let before = est.bitrate_bps();
        est.apply_rate_control(now);
        assert!(
            est.bitrate_bps() < before,
            "rate should decrease on 50% loss: {} >= {}",
            est.bitrate_bps(),
            before
        );
    }

    #[test]
    fn window_slides_old_entries_out() {
        let mut est = LossEstimator::new(1_000_000.0);
        let _now = Instant::now();
        // Fill with losses
        for _ in 0..LOSS_WINDOW {
            est.record(false);
        }
        assert!(est.loss_fraction() > 0.99, "should be ~100% loss");
        // Slide in all received
        for _ in 0..LOSS_WINDOW {
            est.record(true);
        }
        assert!(
            est.loss_fraction() < 0.01,
            "old losses should be evicted: {}",
            est.loss_fraction()
        );
    }

    #[test]
    fn rate_increases_on_low_loss() {
        let initial = 500_000.0;
        let mut est = LossEstimator::new(initial);
        let now = Instant::now();
        // 0% loss -- fill window with received
        for _ in 0..LOSS_WINDOW {
            est.record(true);
        }
        let before = est.bitrate_bps();
        est.apply_rate_control(now);
        assert!(
            est.bitrate_bps() > before,
            "rate should increase on 0% loss: {} <= {}",
            est.bitrate_bps(),
            before
        );
    }
}
