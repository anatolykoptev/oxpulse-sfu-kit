use super::{PacerConfig, PacerConfigError};
use crate::ids::SfuRid;

/// Action returned by `SubscriberPacer::update`.
#[must_use = "PacerAction must be applied to the subscriber's forwarding state"]
#[cfg_attr(docsrs, doc(cfg(feature = "pacer")))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PacerAction {
    /// No layer change.
    NoChange,
    /// Switch to this simulcast layer immediately.
    ChangeLayer(SfuRid),
    /// BWE fell below audio-only threshold — stop forwarding video.
    GoAudioOnly,
    /// BWE recovered — resume video forwarding.
    RestoreVideo,
    /// BWE fell below the suspend-video threshold — stop forwarding ALL video,
    /// including the audio-only continuation. Audio remains forwarded.
    SuspendVideo,
    /// BWE recovered above the audio-only threshold while suspended — resume
    /// audio-only forwarding (NOT full video; that requires `RestoreVideo` later).
    RestoreAudio,
}

/// Per-subscriber hysteretic layer selector.
///
/// Implements LiveKit-style 3-consecutive-upgrade / instant-downgrade.
/// Feed each BWE reading via [`Self::update`]; act on the returned [`PacerAction`].
#[derive(Debug)]
pub struct SubscriberPacer {
    current_layer: SfuRid,
    audio_only: bool,
    /// `true` once BWE dropped below `SUSPEND_VIDEO_BPS`. Implies `audio_only=true`
    /// (suspended is a stricter sub-state). Cleared on `RestoreAudio`.
    suspended: bool,
    upgrade_streak: u8,
    /// Consecutive ticks below `SUSPEND_VIDEO_BPS` while not yet suspended.
    /// Must reach `SUSPEND_STREAK` before transition to suspended state.
    suspend_streak: u8,
    /// Runtime-configurable thresholds.
    config: PacerConfig,
}

impl SubscriberPacer {
    /// Create a new [`SubscriberPacer`] with default thresholds.
    ///
    /// Equivalent to [`Self::with_config`] with [`PacerConfig::default`].
    pub fn new() -> Self {
        Self::with_config(PacerConfig::default())
    }

    /// Create a new [`SubscriberPacer`] with custom thresholds.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # #[cfg(feature = "pacer")]
    /// # {
    /// use oxpulse_sfu_kit::bwe::{PacerConfig, SubscriberPacer};
    ///
    /// let config = PacerConfig { upgrade_streak: 5, ..PacerConfig::default() };
    /// let pacer = SubscriberPacer::with_config(config);
    /// # }
    /// ```
    pub fn with_config(config: PacerConfig) -> Self {
        // Fail fast in EVERY build profile. The previous `debug_assert!` was a
        // no-op under `cargo build --release` (the production build), so an
        // invalid config silently reached the hot bps->layer decision path.
        // Callers that want to handle the error instead of panicking should use
        // [`Self::try_with_config`].
        Self::try_with_config(config)
            .expect("invalid PacerConfig; use SubscriberPacer::try_with_config to handle it")
    }

    /// Create a [`SubscriberPacer`] with custom thresholds, returning a
    /// [`PacerConfigError`] instead of panicking when `config` is invalid.
    ///
    /// The non-panicking counterpart to [`Self::with_config`]. Validation is
    /// enforced in every build profile (unlike the old debug-only assertion).
    ///
    /// # Examples
    ///
    /// ```rust
    /// # #[cfg(feature = "pacer")]
    /// # {
    /// use oxpulse_sfu_kit::bwe::{PacerConfig, PacerConfigError, SubscriberPacer};
    ///
    /// let bad = PacerConfig { upgrade_streak: 0, ..PacerConfig::default() };
    /// assert_eq!(
    ///     SubscriberPacer::try_with_config(bad).err(),
    ///     Some(PacerConfigError::UpgradeStreakZero)
    /// );
    /// assert!(SubscriberPacer::try_with_config(PacerConfig::default()).is_ok());
    /// # }
    /// ```
    pub fn try_with_config(config: PacerConfig) -> Result<Self, PacerConfigError> {
        config.validate()?;
        Ok(Self {
            current_layer: SfuRid::LOW,
            audio_only: false,
            suspended: false,
            upgrade_streak: 0,
            suspend_streak: 0,
            config,
        })
    }

    /// Feed a new egress BWE reading. Returns the action to take (if any).
    pub fn update(&mut self, bps: u64) -> PacerAction {
        // Suspended: stay suspended until we cross AUDIO_ONLY_BPS upward.
        if self.suspended {
            if bps >= self.config.audio_only_bps {
                self.suspended = false;
                // RestoreAudio lands us in audio_only=true. RestoreVideo will
                // be emitted on the NEXT tick if bps also clears LOW_MIN_BPS.
                self.audio_only = true;
                self.upgrade_streak = 0;
                self.suspend_streak = 0;
                return PacerAction::RestoreAudio;
            }
            return PacerAction::NoChange;
        }

        // Debounced suspend-entry: require SUSPEND_STREAK consecutive ticks below
        // SUSPEND_VIDEO_BPS before entering suspended state. A single anomalous TWCC
        // packet must not kill all video for the subscriber.
        if bps < self.config.suspend_video_bps {
            self.suspend_streak = self.suspend_streak.saturating_add(1);
            if self.suspend_streak >= self.config.suspend_streak {
                self.suspended = true;
                self.audio_only = true;
                self.upgrade_streak = 0;
                self.suspend_streak = 0;
                return PacerAction::SuspendVideo;
            }
            return PacerAction::NoChange;
        }
        // tick is at or above SUSPEND_VIDEO_BPS; reset streak (single-tick spike rejected).
        //
        // INVARIANT — branch ordering: every code path below this point is reachable only
        // when `bps >= SUSPEND_VIDEO_BPS`, so `suspend_streak` has been zeroed for this
        // tick. Subsequent emissions (`GoAudioOnly`, `RestoreVideo`, `ChangeLayer`) do
        // NOT need to reset `suspend_streak` again — the reset is implicit via this
        // ordering. If the suspend-entry branch is ever moved below the audio_only or
        // layer branches, every emission below would need an explicit `suspend_streak = 0`
        // to preserve correctness. Do not reorder without updating those resets.
        self.suspend_streak = 0;

        // Audio-only mode: enter below AUDIO_ONLY_BPS, exit only above LOW_MIN_BPS.
        if self.audio_only {
            if bps >= self.config.low_min_bps {
                self.audio_only = false;
                self.current_layer = SfuRid::LOW;
                self.upgrade_streak = 0;
                return PacerAction::RestoreVideo;
            }
            return PacerAction::NoChange;
        }
        if bps < self.config.audio_only_bps {
            self.audio_only = true;
            self.upgrade_streak = 0;
            return PacerAction::GoAudioOnly;
        }

        let target = self.layer_for_bps(bps);

        // Downgrade: immediate + reset streak.
        if rank(target) < rank(self.current_layer) {
            self.current_layer = target;
            self.upgrade_streak = 0;
            return PacerAction::ChangeLayer(target);
        }

        // Upgrade: require UPGRADE_STREAK consecutive ticks above next tier.
        if rank(target) > rank(self.current_layer) {
            // Defensive: use saturating_add to guard against future refactors that
            // might allow streak to grow past cfg.upgrade_streak (currently impossible
            // because upgrade fires at == cfg.upgrade_streak and resets to 0).
            self.upgrade_streak = self.upgrade_streak.saturating_add(1);
            if self.upgrade_streak >= self.config.upgrade_streak {
                self.current_layer = target;
                self.upgrade_streak = 0;
                return PacerAction::ChangeLayer(target);
            }
        } else {
            // At target layer already --- reset streak.
            self.upgrade_streak = 0;
        }

        PacerAction::NoChange
    }

    fn layer_for_bps(&self, bps: u64) -> SfuRid {
        if bps >= self.config.high_min_bps {
            SfuRid::HIGH
        } else if bps >= self.config.medium_min_bps {
            SfuRid::MEDIUM
        } else {
            SfuRid::LOW
        }
    }

    #[cfg(test)]
    fn layer(&self) -> SfuRid {
        self.current_layer
    }
    #[cfg(test)]
    fn audio_only(&self) -> bool {
        self.audio_only
    }
    #[cfg(test)]
    fn suspended(&self) -> bool {
        self.suspended
    }
}

impl Default for SubscriberPacer {
    fn default() -> Self {
        Self::new()
    }
}

fn rank(r: SfuRid) -> u8 {
    if r == SfuRid::LOW {
        0
    } else if r == SfuRid::MEDIUM {
        1
    } else if r == SfuRid::HIGH {
        2
    } else {
        unreachable!("unhandled SfuRid in pacer rank")
    }
}

#[cfg(test)]
#[allow(unused_must_use)] // test calls check side-effects, not return value
mod tests {
    use super::*;
    use crate::bwe::{
        AUDIO_ONLY_BPS, HIGH_MIN_BPS, LOW_MIN_BPS, MEDIUM_MIN_BPS, SUSPEND_STREAK,
        SUSPEND_VIDEO_BPS,
    };

    fn pump(p: &mut SubscriberPacer, bps: u64, n: u8) -> PacerAction {
        let mut last = PacerAction::NoChange;
        for _ in 0..n {
            last = p.update(bps);
        }
        last
    }

    #[test]
    fn starts_at_low() {
        let p = SubscriberPacer::new();
        assert_eq!(p.layer(), SfuRid::LOW);
        assert!(!p.audio_only());
    }

    #[test]
    fn upgrade_requires_3_consecutive_ticks() {
        let mut p = SubscriberPacer::new();
        let bps = MEDIUM_MIN_BPS + 1_000;
        let _ = pump(&mut p, bps, 2);
        assert_eq!(p.layer(), SfuRid::LOW, "should not upgrade after 2 ticks");
        let a = p.update(bps);
        assert_eq!(a, PacerAction::ChangeLayer(SfuRid::MEDIUM));
        assert_eq!(p.layer(), SfuRid::MEDIUM);
    }

    #[test]
    fn downgrade_is_immediate() {
        let mut p = SubscriberPacer::new();
        // Reach HIGH: 3 ticks to MEDIUM, 3 more to HIGH
        let _ = pump(&mut p, HIGH_MIN_BPS + 100_000, 6);
        assert_eq!(p.layer(), SfuRid::HIGH);
        let a = p.update(MEDIUM_MIN_BPS - 10_000);
        assert_eq!(a, PacerAction::ChangeLayer(SfuRid::LOW));
        assert_eq!(p.layer(), SfuRid::LOW);
    }

    #[test]
    fn streak_resets_on_interruption() {
        let mut p = SubscriberPacer::new();
        let hi = MEDIUM_MIN_BPS + 1_000;
        let lo = LOW_MIN_BPS + 1_000;
        p.update(hi); // streak=1
        p.update(hi); // streak=2
        p.update(lo); // drops --- streak resets
        p.update(hi); // streak=1 again
        p.update(hi); // streak=2
        assert_eq!(
            p.layer(),
            SfuRid::LOW,
            "should NOT have upgraded --- streak reset"
        );
    }

    #[test]
    fn audio_only_below_threshold() {
        let mut p = SubscriberPacer::new();
        let a = p.update(AUDIO_ONLY_BPS - 1_000);
        assert_eq!(a, PacerAction::GoAudioOnly);
        assert!(p.audio_only());
        // While audio-only, BWE in grey zone --> no action
        assert_eq!(p.update(100_000), PacerAction::NoChange);
        // Above LOW_MIN_BPS --> restore
        let a = p.update(LOW_MIN_BPS + 1_000);
        assert_eq!(a, PacerAction::RestoreVideo);
        assert!(!p.audio_only());
    }

    #[test]
    fn no_change_at_correct_layer() {
        let mut p = SubscriberPacer::new();
        for _ in 0..10 {
            assert_eq!(p.update(LOW_MIN_BPS + 50_000), PacerAction::NoChange);
        }
    }

    #[test]
    fn exact_audio_only_boundary_is_video_mode() {
        // bps == AUDIO_ONLY_BPS is NOT audio-only (the condition is `bps < AUDIO_ONLY_BPS`)
        let mut p = SubscriberPacer::new();
        let action = p.update(AUDIO_ONLY_BPS); // exactly 80_000
        assert_ne!(
            action,
            PacerAction::GoAudioOnly,
            "exactly AUDIO_ONLY_BPS should remain in video mode (condition is strictly <)"
        );
    }

    #[test]
    fn no_double_go_audio_only() {
        // Second call while already audio-only must return NoChange, not GoAudioOnly again
        let mut p = SubscriberPacer::new();
        let first = p.update(AUDIO_ONLY_BPS - 1);
        assert_eq!(first, PacerAction::GoAudioOnly);
        let second = p.update(SUSPEND_VIDEO_BPS + 1); // grey zone (above suspend, below LOW_MIN_BPS) --- still audio-only, must NOT emit again
        assert_eq!(second, PacerAction::NoChange,
            "GoAudioOnly must not be emitted twice; second call while audio-only must return NoChange");
    }

    #[test]
    fn restore_video_resets_streak_for_upgrade() {
        // After RestoreVideo, subscriber is at LOW. Upgrading to MEDIUM still needs 3 ticks.
        let mut p = SubscriberPacer::new();
        p.update(AUDIO_ONLY_BPS - 1); // GoAudioOnly
        p.update(LOW_MIN_BPS + 1); // RestoreVideo, now at LOW, streak=0
                                   // 2 ticks above MEDIUM threshold --- not enough
        p.update(MEDIUM_MIN_BPS + 1);
        p.update(MEDIUM_MIN_BPS + 1);
        assert_eq!(
            p.layer(),
            SfuRid::LOW,
            "after RestoreVideo, still need 3 ticks to upgrade"
        );
        // 3rd tick upgrades
        let action = p.update(MEDIUM_MIN_BPS + 1);
        assert_eq!(action, PacerAction::ChangeLayer(SfuRid::MEDIUM));
    }

    #[test]
    fn exact_low_min_boundary_triggers_restore_video() {
        // bps == LOW_MIN_BPS while audio-only should trigger RestoreVideo
        let mut p = SubscriberPacer::new();
        p.update(AUDIO_ONLY_BPS - 1); // enter audio-only
        let action = p.update(LOW_MIN_BPS); // exactly LOW_MIN_BPS
        assert_eq!(
            action,
            PacerAction::RestoreVideo,
            "exactly LOW_MIN_BPS while audio-only should trigger RestoreVideo (condition is >=)"
        );
    }

    #[test]
    fn grey_zone_while_audio_only_is_no_change() {
        // bps in (AUDIO_ONLY_BPS, LOW_MIN_BPS) while audio-only: no action
        let mut p = SubscriberPacer::new();
        p.update(AUDIO_ONLY_BPS - 1); // enter audio-only
        for bps in [AUDIO_ONLY_BPS, AUDIO_ONLY_BPS + 1, LOW_MIN_BPS - 1] {
            assert_eq!(
                p.update(bps),
                PacerAction::NoChange,
                "bps={bps} in grey zone should be NoChange while audio-only"
            );
        }
    }

    #[test]
    fn downgrade_from_medium_resets_streak_so_re_upgrade_needs_3_ticks() {
        let mut p = SubscriberPacer::new();
        // Get to MEDIUM
        for _ in 0..3 {
            p.update(MEDIUM_MIN_BPS + 1);
        }
        assert_eq!(p.layer(), SfuRid::MEDIUM);
        // Downgrade
        p.update(LOW_MIN_BPS + 1);
        assert_eq!(p.layer(), SfuRid::LOW);
        // 2 ticks up --- not enough (streak was reset by downgrade)
        p.update(MEDIUM_MIN_BPS + 1);
        p.update(MEDIUM_MIN_BPS + 1);
        assert_eq!(
            p.layer(),
            SfuRid::LOW,
            "streak must have reset on downgrade"
        );
        // 3rd tick --- upgrades again
        p.update(MEDIUM_MIN_BPS + 1);
        assert_eq!(p.layer(), SfuRid::MEDIUM);
    }

    #[test]
    fn enters_suspended_below_suspend_threshold() {
        // updated for SUSPEND_STREAK debounce
        let mut p = SubscriberPacer::new();
        let a = pump(&mut p, SUSPEND_VIDEO_BPS - 1, SUSPEND_STREAK);
        assert_eq!(a, PacerAction::SuspendVideo);
        assert!(p.suspended());
        assert!(p.audio_only(), "suspended implies audio_only=true");
    }

    #[test]
    fn double_suspend_is_no_change() {
        // updated for SUSPEND_STREAK debounce: pump SUSPEND_STREAK ticks to enter, then verify no re-emit
        let mut p = SubscriberPacer::new();
        assert_eq!(
            pump(&mut p, SUSPEND_VIDEO_BPS - 1, SUSPEND_STREAK),
            PacerAction::SuspendVideo
        );
        assert_eq!(
            p.update(1),
            PacerAction::NoChange,
            "second tick while suspended must NOT re-emit SuspendVideo"
        );
    }

    #[test]
    fn grey_zone_while_suspended_is_no_change() {
        // (SUSPEND_VIDEO_BPS, AUDIO_ONLY_BPS) while suspended --- no action
        // updated for SUSPEND_STREAK debounce
        let mut p = SubscriberPacer::new();
        pump(&mut p, SUSPEND_VIDEO_BPS - 1, SUSPEND_STREAK); // enter suspended
        for bps in [SUSPEND_VIDEO_BPS, SUSPEND_VIDEO_BPS + 1, AUDIO_ONLY_BPS - 1] {
            assert_eq!(
                p.update(bps),
                PacerAction::NoChange,
                "bps={bps} grey zone while suspended"
            );
        }
    }

    #[test]
    fn restore_audio_lands_in_audio_only_state() {
        // From suspended, crossing AUDIO_ONLY_BPS upward emits RestoreAudio
        // and lands in audio_only=true, suspended=false (NOT directly to LOW).
        // updated for SUSPEND_STREAK debounce
        let mut p = SubscriberPacer::new();
        pump(&mut p, SUSPEND_VIDEO_BPS - 1, SUSPEND_STREAK);
        let a = p.update(AUDIO_ONLY_BPS);
        assert_eq!(a, PacerAction::RestoreAudio);
        assert!(!p.suspended());
        assert!(
            p.audio_only(),
            "RestoreAudio must put pacer in audio_only state, not full video"
        );
    }

    #[test]
    fn full_recovery_cascade_suspend_to_video() {
        // updated for SUSPEND_STREAK debounce
        let mut p = SubscriberPacer::new();
        assert_eq!(
            pump(&mut p, SUSPEND_VIDEO_BPS - 1, SUSPEND_STREAK),
            PacerAction::SuspendVideo
        );
        assert_eq!(p.update(AUDIO_ONLY_BPS), PacerAction::RestoreAudio);
        assert_eq!(p.update(LOW_MIN_BPS), PacerAction::RestoreVideo);
        // Upgrade still requires 3 streaks.
        p.update(MEDIUM_MIN_BPS + 1);
        p.update(MEDIUM_MIN_BPS + 1);
        let a = p.update(MEDIUM_MIN_BPS + 1);
        assert_eq!(a, PacerAction::ChangeLayer(SfuRid::MEDIUM));
    }

    #[test]
    fn exact_suspend_boundary_is_audio_only_mode() {
        // bps == SUSPEND_VIDEO_BPS is NOT suspended (condition is strictly <)
        let mut p = SubscriberPacer::new();
        let a = p.update(SUSPEND_VIDEO_BPS); // exactly 10_000
        assert_eq!(
            a,
            PacerAction::GoAudioOnly,
            "exactly SUSPEND_VIDEO_BPS should be GoAudioOnly, not SuspendVideo"
        );
        assert!(!p.suspended());
    }

    #[test]
    fn exact_audio_only_boundary_while_suspended_triggers_restore_audio() {
        // bps == AUDIO_ONLY_BPS while suspended should trigger RestoreAudio (>=).
        // updated for SUSPEND_STREAK debounce
        let mut p = SubscriberPacer::new();
        pump(&mut p, SUSPEND_VIDEO_BPS - 1, SUSPEND_STREAK);
        let a = p.update(AUDIO_ONLY_BPS);
        assert_eq!(a, PacerAction::RestoreAudio);
    }

    #[test]
    fn suspend_supersedes_pending_layer_change() {
        // With a high upgrade streak, a sudden drop to suspended must NOT emit
        // ChangeLayer first --- Suspend wins. With debounce, requires SUSPEND_STREAK
        // consecutive ticks below threshold; ChangeLayer must not appear in the meantime.
        // updated for SUSPEND_STREAK debounce
        let mut p = SubscriberPacer::new();
        p.update(MEDIUM_MIN_BPS + 1); // upgrade streak=1
        p.update(MEDIUM_MIN_BPS + 1); // upgrade streak=2
                                      // Pump SUSPEND_STREAK - 1 ticks below threshold: must be NoChange (debouncing)
        for _ in 0..(SUSPEND_STREAK - 1) {
            let a = p.update(SUSPEND_VIDEO_BPS - 1);
            assert_eq!(
                a,
                PacerAction::NoChange,
                "during suspend debounce, NO action may emit (not ChangeLayer, not GoAudioOnly, not SuspendVideo)"
            );
            assert!(!p.suspended());
        }
        // Final tick triggers SuspendVideo
        let a = p.update(SUSPEND_VIDEO_BPS - 1);
        assert_eq!(a, PacerAction::SuspendVideo);
        assert!(p.suspended());
    }

    #[test]
    fn drop_from_video_directly_to_suspend_is_one_event() {
        // From layer=LOW (no audio_only state), SUSPEND_STREAK ticks to <10k must
        // emit exactly one SuspendVideo (at the Nth tick), skipping GoAudioOnly.
        // updated for SUSPEND_STREAK debounce
        let mut p = SubscriberPacer::new();
        assert_eq!(p.update(LOW_MIN_BPS + 1), PacerAction::NoChange);
        // First SUSPEND_STREAK - 1 ticks: NoChange (debouncing)
        for _ in 0..(SUSPEND_STREAK - 1) {
            let a = p.update(SUSPEND_VIDEO_BPS - 1);
            assert_ne!(
                a,
                PacerAction::GoAudioOnly,
                "must not emit GoAudioOnly while debouncing suspend"
            );
            assert_ne!(a, PacerAction::SuspendVideo);
        }
        // Final tick: SuspendVideo
        let a = p.update(SUSPEND_VIDEO_BPS - 1);
        assert_eq!(a, PacerAction::SuspendVideo);
        assert!(p.suspended());
        assert!(p.audio_only());
    }

    // --- NEW tests for SUSPEND_STREAK debounce (F6-3) ---

    #[test]
    fn single_tick_below_suspend_threshold_does_not_suspend() {
        let mut p = SubscriberPacer::new();
        let action = p.update(SUSPEND_VIDEO_BPS - 1);
        assert_eq!(
            action,
            PacerAction::NoChange,
            "single-tick spike below SUSPEND_VIDEO_BPS must not enter suspended (debounce)"
        );
        assert!(!p.suspended());
    }

    #[test]
    fn suspend_streak_consecutive_ticks_required() {
        let mut p = SubscriberPacer::new();
        // SUSPEND_STREAK - 1 ticks: still NoChange
        for _ in 0..(SUSPEND_STREAK - 1) {
            assert_eq!(p.update(SUSPEND_VIDEO_BPS - 1), PacerAction::NoChange);
        }
        assert!(!p.suspended());
        // Final tick: emits SuspendVideo
        let action = p.update(SUSPEND_VIDEO_BPS - 1);
        assert_eq!(action, PacerAction::SuspendVideo);
        assert!(p.suspended());
    }

    #[test]
    fn suspend_streak_resets_on_interruption() {
        let mut p = SubscriberPacer::new();
        // SUSPEND_STREAK - 1 ticks below threshold
        for _ in 0..(SUSPEND_STREAK - 1) {
            p.update(SUSPEND_VIDEO_BPS - 1);
        }
        // One tick at or above threshold — streak resets
        p.update(SUSPEND_VIDEO_BPS);
        assert!(!p.suspended());
        // Now another SUSPEND_STREAK - 1 ticks below should NOT trigger suspend
        for _ in 0..(SUSPEND_STREAK - 1) {
            assert_eq!(p.update(SUSPEND_VIDEO_BPS - 1), PacerAction::NoChange);
        }
        assert!(!p.suspended());
        // SUSPEND_STREAKth tick triggers
        assert_eq!(p.update(SUSPEND_VIDEO_BPS - 1), PacerAction::SuspendVideo);
    }
    #[test]
    #[should_panic(expected = "invalid PacerConfig")]
    fn upgrade_streak_zero_panics() {
        use crate::bwe::PacerConfig;
        // upgrade_streak=0 means streak(1) >= 0 is always true -> single-tick upgrade = thrash.
        // with_config must panic in EVERY profile (was debug-only), so an invalid
        // config can never silently reach the hot path in a --release build.
        let cfg = PacerConfig {
            upgrade_streak: 0,
            ..PacerConfig::default()
        };
        let _ = SubscriberPacer::with_config(cfg);
    }

    #[test]
    #[should_panic(expected = "invalid PacerConfig")]
    fn suspend_streak_zero_panics() {
        use crate::bwe::PacerConfig;
        // suspend_streak=0 means streak(1) >= 0 is always true -> instant suspend on first tick.
        let cfg = PacerConfig {
            suspend_streak: 0,
            ..PacerConfig::default()
        };
        let _ = SubscriberPacer::with_config(cfg);
    }

    #[test]
    fn try_with_config_rejects_invalid_in_all_profiles() {
        use crate::bwe::{PacerConfig, PacerConfigError};
        // Profile-independent: unlike the old debug-only assertion, validation is
        // enforced here in release too. This is the non-panicking path the fix adds.
        assert_eq!(
            SubscriberPacer::try_with_config(PacerConfig {
                upgrade_streak: 0,
                ..PacerConfig::default()
            })
            .err(),
            Some(PacerConfigError::UpgradeStreakZero)
        );
        assert_eq!(
            SubscriberPacer::try_with_config(PacerConfig {
                suspend_streak: 0,
                ..PacerConfig::default()
            })
            .err(),
            Some(PacerConfigError::SuspendStreakZero)
        );
        assert!(SubscriberPacer::try_with_config(PacerConfig::default()).is_ok());
    }
}
