//! Configurable thresholds for [`SubscriberPacer`][super::SubscriberPacer].
use super::{
    AUDIO_ONLY_BPS, HIGH_MIN_BPS, LOW_MIN_BPS, MEDIUM_MIN_BPS, SUSPEND_STREAK, SUSPEND_VIDEO_BPS,
    UPGRADE_STREAK,
};

/// Configurable thresholds for [`SubscriberPacer`][super::SubscriberPacer].
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_values_match_constants() {
        let cfg = PacerConfig::default();
        assert_eq!(cfg.suspend_video_bps, SUSPEND_VIDEO_BPS);
        assert_eq!(cfg.audio_only_bps, AUDIO_ONLY_BPS);
        assert_eq!(cfg.low_min_bps, LOW_MIN_BPS);
        assert_eq!(cfg.medium_min_bps, MEDIUM_MIN_BPS);
        assert_eq!(cfg.high_min_bps, HIGH_MIN_BPS);
        assert_eq!(cfg.suspend_streak, SUSPEND_STREAK);
        assert_eq!(cfg.upgrade_streak, UPGRADE_STREAK);
    }

    #[test]
    fn struct_update_syntax_works() {
        let cfg = PacerConfig {
            upgrade_streak: 5,
            ..PacerConfig::default()
        };
        // Override applied
        assert_eq!(cfg.upgrade_streak, 5);
        // Other fields unchanged
        assert_eq!(cfg.audio_only_bps, AUDIO_ONLY_BPS);
        assert_eq!(cfg.suspend_streak, SUSPEND_STREAK);
    }

    #[test]
    fn custom_config_drives_pacer_behavior() {
        use crate::bwe::{PacerAction, SubscriberPacer};

        // With upgrade_streak=1, a single high-BWE tick should upgrade.
        let cfg = PacerConfig {
            upgrade_streak: 1,
            ..PacerConfig::default()
        };
        let mut pacer = SubscriberPacer::with_config(cfg);
        // MEDIUM_MIN_BPS + 1 should trigger upgrade in 1 tick (not 3)
        let action = pacer.update(MEDIUM_MIN_BPS + 1);
        assert_eq!(
            action,
            PacerAction::ChangeLayer(crate::ids::SfuRid::MEDIUM),
            "upgrade_streak=1 must upgrade in first tick"
        );
    }

    #[test]
    fn custom_audio_only_threshold() {
        use crate::bwe::{PacerAction, SubscriberPacer};

        // Raise audio_only threshold so pacer enters audio-only at higher BWE.
        let custom_audio_only = 200_000u64;
        let cfg = PacerConfig {
            audio_only_bps: custom_audio_only,
            low_min_bps: custom_audio_only + 50_000,
            ..PacerConfig::default()
        };
        let mut pacer = SubscriberPacer::with_config(cfg);
        // 100kbps is below custom threshold -> GoAudioOnly
        let action = pacer.update(100_000);
        assert_eq!(action, PacerAction::GoAudioOnly);
    }
}
