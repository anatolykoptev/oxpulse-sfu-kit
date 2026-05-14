//! Trendline delay detector --- GoogCC component 1.
//!
//! Fits a linear regression over the last [`WINDOW`] inter-arrival time deltas.
//! Positive slope signals delay build-up (congestion); negative or zero means
//! the link is recovering or stable.
//!
//! Ported from the WebRTC GoogCC specification and originally extracted from
//! `oxpulse-partner-edge`. **Per-subscriber** in this crate (vs per-room in the
//! original): each subscriber runs its own detector so per-link congestion does
//! not affect unrelated subscribers sharing the same room.

use std::collections::VecDeque;

/// Window size for trendline regression (number of packet-pair samples).
const WINDOW: usize = 20;
/// Slope threshold above which the detector declares overuse (ms/s).
const OVERUSE_THRESHOLD: f64 = 12.5;

/// Signal emitted by [`TrendlineDetector`].
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BandwidthState {
    /// Delay is decreasing --- link has spare capacity.
    Underuse,
    /// Delay is stable --- bitrate roughly matches capacity.
    Normal,
    /// Delay is building --- reduce bitrate.
    Overuse,
}

/// Linear-regression trendline congestion detector.
///
/// Feed inter-arrival/inter-send time pairs via [`Self::update`]; read the
/// current [`BandwidthState`] via [`Self::state`] or use [`Self::overuse`].
///
/// # Examples
///
/// ```rust
/// # #[cfg(feature = "googcc-bwe")]
/// # {
/// use oxpulse_sfu_kit::bwe::googcc::{TrendlineDetector, BandwidthState};
///
/// let mut detector = TrendlineDetector::new();
/// for _ in 0..25 {
///     detector.update(20.0, 20.0); // stable timing
/// }
/// assert_eq!(detector.state(), BandwidthState::Normal);
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct TrendlineDetector {
    /// Rolling window of (arrival_delta_ms, send_delta_ms) pairs.
    deltas: VecDeque<(f64, f64)>,
    /// Current overuse state. Read via [].
    pub(crate) state: BandwidthState,
}

impl TrendlineDetector {
    /// Create a new detector in [`BandwidthState::Normal`].
    pub fn new() -> Self {
        Self {
            deltas: VecDeque::with_capacity(WINDOW + 1),
            state: BandwidthState::Normal,
        }
    }

    /// Feed a new packet timing pair.
    ///
    /// - `arrival_delta_ms`: arrival time difference from previous packet (ms).
    /// - `send_delta_ms`: send time difference from previous packet (ms).
    ///
    /// Updates [`Self::state`] after at least 3 samples have been collected.
    pub fn update(&mut self, arrival_delta_ms: f64, send_delta_ms: f64) {
        if self.deltas.len() >= WINDOW {
            self.deltas.pop_front();
        }
        self.deltas.push_back((arrival_delta_ms, send_delta_ms));
        if self.deltas.len() < 3 {
            return;
        }

        let slope = self.trendline_slope();
        self.state = if slope > OVERUSE_THRESHOLD {
            BandwidthState::Overuse
        } else if slope < -OVERUSE_THRESHOLD {
            BandwidthState::Underuse
        } else {
            BandwidthState::Normal
        };
    }

    /// Returns `true` when the detector has declared overuse.
    pub fn overuse(&self) -> bool {
        self.state == BandwidthState::Overuse
    }

    /// Current congestion state.
    pub fn state(&self) -> BandwidthState {
        self.state
    }

    fn trendline_slope(&self) -> f64 {
        let n = self.deltas.len() as f64;
        let accumulated: Vec<f64> = self
            .deltas
            .iter()
            .scan(0.0f64, |acc, (a, s)| {
                *acc += a - s;
                Some(*acc)
            })
            .collect();

        let x_bar = (n - 1.0) / 2.0;
        let y_bar: f64 = accumulated.iter().sum::<f64>() / n;
        let mut num = 0.0f64;
        let mut den = 0.0f64;
        for (i, y) in accumulated.iter().enumerate() {
            let x = i as f64;
            num += (x - x_bar) * (y - y_bar);
            den += (x - x_bar).powi(2);
        }
        if den.abs() < 1e-10 {
            0.0
        } else {
            num / den
        }
    }
}

impl Default for TrendlineDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_arrival_times_stay_normal() {
        let mut d = TrendlineDetector::new();
        for _ in 0..25 {
            d.update(20.0, 20.0);
        }
        assert_eq!(d.state, BandwidthState::Normal);
    }

    #[test]
    fn increasing_delay_triggers_overuse() {
        let mut d = TrendlineDetector::new();
        for i in 0..25 {
            d.update(20.0 + i as f64 * 2.0, 20.0);
        }
        assert_eq!(d.state, BandwidthState::Overuse);
    }

    #[test]
    fn insufficient_samples_stay_normal() {
        let mut d = TrendlineDetector::new();
        d.update(100.0, 0.0);
        d.update(200.0, 0.0);
        assert_eq!(d.state, BandwidthState::Normal, "need at least 3 samples");
    }

    #[test]
    fn decreasing_delay_triggers_underuse() {
        let mut d = TrendlineDetector::new();
        // Push increasingly negative slopes
        for i in 1..=25u32 {
            d.update(1.0, (i as f64) * 3.0);
        }
        assert_eq!(d.state, BandwidthState::Underuse);
    }

    #[test]
    fn window_limits_sample_count() {
        let mut d = TrendlineDetector::new();
        for _ in 0..(WINDOW + 10) {
            d.update(20.0, 20.0);
        }
        assert!(d.deltas.len() <= WINDOW);
    }
}
