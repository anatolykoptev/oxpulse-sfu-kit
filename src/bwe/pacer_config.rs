//! Configurable thresholds for [`SubscriberPacer`][super::SubscriberPacer].
use super::{
    AUDIO_ONLY_BPS, HIGH_MIN_BPS, LOW_MIN_BPS, MEDIUM_MIN_BPS, SUSPEND_STREAK, SUSPEND_VIDEO_BPS,
    UPGRADE_STREAK,
};

/// Validation error returned by [`PacerConfig::validate`].
///
/// Each variant names the invariant that was violated.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(docsrs, doc(cfg(feature = "pacer")))]
#[non_exhaustive]
pub enum PacerConfigError {
    /// `upgrade_streak` must be ≥ 1.  Value 0 causes instant upgrade on every tick.
    UpgradeStreakZero,
    /// `suspend_streak` must be ≥ 1.  Value 0 causes instant suspend on first below-threshold tick.
    SuspendStreakZero,
    /// Bitrate ordering invariant violated: must hold
    /// `suspend_video_bps ≤ audio_only_bps ≤ low_min_bps ≤ medium_min_bps ≤ high_min_bps`.
    BitrateOrderingViolation,
}

impl core::fmt::Display for PacerConfigError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UpgradeStreakZero => write!(f, "upgrade_streak must be >= 1"),
            Self::SuspendStreakZero => write!(f, "suspend_streak must be >= 1"),
            Self::BitrateOrderingViolation => write!(
                f,
                "bitrate fields must satisfy: suspend_video_bps <= audio_only_bps <= low_min_bps <= medium_min_bps <= high_min_bps"
            ),
        }
    }
}

/// Configurable thresholds for [`SubscriberPacer`][super::SubscriberPacer].
///
/// All bitrate values are in bits per second. Streak counters are in ticks
/// (one tick = one call to `SubscriberPacer::update`).
///
/// ## Ordering invariant
///
/// Bitrate fields **must** satisfy:
/// ```text
/// suspend_video_bps <= audio_only_bps <= low_min_bps <= medium_min_bps <= high_min_bps
/// ```
///
/// Streak fields must each be ≥ 1 (a value of 0 causes instant state transition).
///
/// Use [`PacerConfig::validate`] to check a custom config before passing it to
/// [`SubscriberPacer::with_config`].
///
/// Construct with `Default::default()` or override individual fields:
///
/// ```rust
/// # #[cfg(feature = "pacer")]
/// # {
/// use oxpulse_sfu_kit::bwe::PacerConfig;
///
/// let config = PacerConfig { upgrade_streak: 5, ..PacerConfig::default() };
/// assert!(config.validate().is_ok());
/// # }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(docsrs, doc(cfg(feature = "pacer")))]
pub struct PacerConfig {
    /// BWE below which all video forwarding is suspended (bits/s).
    ///
    /// Must be ≤ [`Self::audio_only_bps`]. Default: 10 000 bps.
    pub suspend_video_bps: u64,
    /// BWE below which the subscriber enters audio-only mode (bits/s).
    ///
    /// Must be ≥ [`Self::suspend_video_bps`] and ≤ [`Self::low_min_bps`].
    /// Default: 80 000 bps.
    pub audio_only_bps: u64,
    /// Minimum BWE to sustain the LOW simulcast layer (bits/s).
    ///
    /// Must be ≥ [`Self::audio_only_bps`] and ≤ [`Self::medium_min_bps`].
    /// Default: 150 000 bps.
    pub low_min_bps: u64,
    /// Minimum BWE to sustain the MEDIUM simulcast layer (bits/s).
    ///
    /// Must be ≥ [`Self::low_min_bps`] and ≤ [`Self::high_min_bps`].
    /// Default: 350 000 bps.
    pub medium_min_bps: u64,
    /// Minimum BWE to sustain the HIGH simulcast layer (bits/s).
    ///
    /// Must be ≥ [`Self::medium_min_bps`]. Default: 700 000 bps.
    pub high_min_bps: u64,
    /// Consecutive ticks below `suspend_video_bps` before entering suspended sub-state.
    ///
    /// Must be ≥ 1. Default: 2.
    pub suspend_streak: u8,
    /// Consecutive ticks above the next tier threshold required before upgrading.
    ///
    /// Must be ≥ 1. Default: 3.
    pub upgrade_streak: u8,
}

impl PacerConfig {
    /// Validate all invariants.
    ///
    /// Returns the first violation found (streak checks before ordering).
    ///
    /// ```rust
    /// # #[cfg(feature = "pacer")]
    /// # {
    /// use oxpulse_sfu_kit::bwe::{PacerConfig, PacerConfigError};
    ///
    /// let bad = PacerConfig { upgrade_streak: 0, ..PacerConfig::default() };
    /// assert_eq!(bad.validate(), Err(PacerConfigError::UpgradeStreakZero));
    ///
    /// let good = PacerConfig::default();
    /// assert!(good.validate().is_ok());
    /// # }
    /// ```
    pub fn validate(&self) -> Result<(), PacerConfigError> {
        if self.upgrade_streak == 0 {
            return Err(PacerConfigError::UpgradeStreakZero);
        }
        if self.suspend_streak == 0 {
            return Err(PacerConfigError::SuspendStreakZero);
        }
        if !(self.suspend_video_bps <= self.audio_only_bps
            && self.audio_only_bps <= self.low_min_bps
            && self.low_min_bps <= self.medium_min_bps
            && self.medium_min_bps <= self.high_min_bps)
        {
            return Err(PacerConfigError::BitrateOrderingViolation);
        }
        Ok(())
    }
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
    fn default_config_validates_ok() {
        assert!(PacerConfig::default().validate().is_ok());
    }

    #[test]
    fn validate_upgrade_streak_zero() {
        let cfg = PacerConfig {
            upgrade_streak: 0,
            ..PacerConfig::default()
        };
        assert_eq!(cfg.validate(), Err(PacerConfigError::UpgradeStreakZero));
    }

    #[test]
    fn validate_suspend_streak_zero() {
        let cfg = PacerConfig {
            suspend_streak: 0,
            ..PacerConfig::default()
        };
        assert_eq!(cfg.validate(), Err(PacerConfigError::SuspendStreakZero));
    }

    #[test]
    fn validate_audio_only_below_suspend_video() {
        let cfg = PacerConfig {
            audio_only_bps: 5_000, // < SUSPEND_VIDEO_BPS (10_000)
            ..PacerConfig::default()
        };
        assert_eq!(
            cfg.validate(),
            Err(PacerConfigError::BitrateOrderingViolation)
        );
    }

    #[test]
    fn validate_low_min_below_audio_only() {
        let cfg = PacerConfig {
            low_min_bps: 50_000, // < AUDIO_ONLY_BPS (80_000)
            ..PacerConfig::default()
        };
        assert_eq!(
            cfg.validate(),
            Err(PacerConfigError::BitrateOrderingViolation)
        );
    }

    #[test]
    fn validate_medium_min_below_low_min() {
        let cfg = PacerConfig {
            medium_min_bps: 100_000, // < LOW_MIN_BPS (150_000)
            ..PacerConfig::default()
        };
        assert_eq!(
            cfg.validate(),
            Err(PacerConfigError::BitrateOrderingViolation)
        );
    }

    #[test]
    fn validate_high_min_below_medium_min() {
        let cfg = PacerConfig {
            high_min_bps: 200_000, // < MEDIUM_MIN_BPS (350_000)
            ..PacerConfig::default()
        };
        assert_eq!(
            cfg.validate(),
            Err(PacerConfigError::BitrateOrderingViolation)
        );
    }

    #[test]
    fn validate_equal_thresholds_ok() {
        // Edge case: equal adjacent thresholds are valid (<=, not <)
        let cfg = PacerConfig {
            suspend_video_bps: 10_000,
            audio_only_bps: 10_000,
            low_min_bps: 10_000,
            medium_min_bps: 10_000,
            high_min_bps: 10_000,
            suspend_streak: 1,
            upgrade_streak: 1,
        };
        assert!(cfg.validate().is_ok());
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
