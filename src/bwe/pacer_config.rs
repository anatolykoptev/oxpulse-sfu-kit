//! Configurable thresholds for [`SubscriberPacer`].
use super::{
    AUDIO_ONLY_BPS, HIGH_MIN_BPS, LOW_MIN_BPS, MEDIUM_MIN_BPS, SUSPEND_STREAK, SUSPEND_VIDEO_BPS,
    UPGRADE_STREAK,
};

/// Configurable thresholds for [`SubscriberPacer`].
///
/// All bitrate values are in bits per second. Streak counters are in ticks
/// (one tick = one call to `SubscriberPacer::update`).
///
/// Construct with `Default::default()` or override individual fields:
///
/// ```rust
/// # #[cfg(feature = "pacer")]
/// # {
/// use oxpulse_sfu_kit::bwe::PacerConfig;
///
/// let config = PacerConfig { upgrade_streak: 5, ..PacerConfig::default() };
/// # }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(docsrs, doc(cfg(feature = "pacer")))]
pub struct PacerConfig {
    /// BWE below which all video forwarding is suspended (bits/s).
    ///
    /// Default: 10 000 bps.
    pub suspend_video_bps: u64,
    /// BWE below which the subscriber enters audio-only mode (bits/s).
    ///
    /// Default: 80 000 bps.
    pub audio_only_bps: u64,
    /// Minimum BWE to sustain the LOW simulcast layer (bits/s).
    ///
    /// Default: 150 000 bps.
    pub low_min_bps: u64,
    /// Minimum BWE to sustain the MEDIUM simulcast layer (bits/s).
    ///
    /// Default: 350 000 bps.
    pub medium_min_bps: u64,
    /// Minimum BWE to sustain the HIGH simulcast layer (bits/s).
    ///
    /// Default: 700 000 bps.
    pub high_min_bps: u64,
    /// Consecutive ticks below `suspend_video_bps` before entering suspended sub-state.
    ///
    /// Default: 2.
    pub suspend_streak: u8,
    /// Consecutive ticks above the next tier threshold required before upgrading.
    ///
    /// Default: 3.
    pub upgrade_streak: u8,
}

impl Default for PacerConfig {
    fn default() -> Self {
        Self {
            suspend_video_bps: SUSPEND_VIDEO_BPS,
            audio_only_bps: AUDIO_ONLY_BPS,
            low_min_bps: LOW_MIN_BPS,
            medium_min_bps: MEDIUM_MIN_BPS,
            high_min_bps: HIGH_MIN_BPS,
            suspend_streak: SUSPEND_STREAK,
            upgrade_streak: UPGRADE_STREAK,
        }
    }
}
