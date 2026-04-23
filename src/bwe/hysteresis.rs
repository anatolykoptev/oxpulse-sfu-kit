use crate::ids::SfuRid;
use super::{AUDIO_ONLY_BPS, HIGH_MIN_BPS, LOW_MIN_BPS, MEDIUM_MIN_BPS, UPGRADE_STREAK};

/// Action returned by [`SubscriberPacer::update`].
#[must_use = "PacerAction must be applied to the subscriber's forwarding state"]
#[cfg_attr(docsrs, doc(cfg(feature = "pacer")))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacerAction {
    /// No layer change.
    NoChange,
    /// Switch to this simulcast layer immediately.
    ChangeLayer(SfuRid),
    /// BWE fell below audio-only threshold --- stop forwarding video.
    GoAudioOnly,
    /// BWE recovered --- resume video forwarding.
    RestoreVideo,
}

/// Per-subscriber hysteretic layer selector.
///
/// Implements LiveKit-style 3-consecutive-upgrade / instant-downgrade.
/// Feed each BWE reading via [`Self::update`]; act on the returned [`PacerAction`].
#[derive(Debug)]
pub(crate) struct SubscriberPacer {
    current_layer: SfuRid,
    audio_only: bool,
    upgrade_streak: u8,
}

impl SubscriberPacer {
    pub(crate) fn new() -> Self {
        Self { current_layer: SfuRid::LOW, audio_only: false, upgrade_streak: 0 }
    }

    /// Feed a new egress BWE reading. Returns the action to take (if any).
    pub(crate) fn update(&mut self, bps: u64) -> PacerAction {
        // Audio-only mode: enter below AUDIO_ONLY_BPS, exit only above LOW_MIN_BPS.
        if self.audio_only {
            if bps >= LOW_MIN_BPS {
                self.audio_only = false;
                self.current_layer = SfuRid::LOW;
                self.upgrade_streak = 0;
                return PacerAction::RestoreVideo;
            }
            return PacerAction::NoChange;
        }
        if bps < AUDIO_ONLY_BPS {
            self.audio_only = true;
            self.upgrade_streak = 0;
            return PacerAction::GoAudioOnly;
        }

        let target = layer_for_bps(bps);

        // Downgrade: immediate + reset streak.
        if rank(target) < rank(self.current_layer) {
            self.current_layer = target;
            self.upgrade_streak = 0;
            return PacerAction::ChangeLayer(target);
        }

        // Upgrade: require UPGRADE_STREAK consecutive ticks above next tier.
        if rank(target) > rank(self.current_layer) {
            self.upgrade_streak += 1;
            if self.upgrade_streak >= UPGRADE_STREAK {
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

    #[cfg(test)]
    fn layer(&self) -> SfuRid { self.current_layer }
    #[cfg(test)]
    fn audio_only(&self) -> bool { self.audio_only }
}

fn rank(r: SfuRid) -> u8 {
    if r == SfuRid::LOW { 0 }
    else if r == SfuRid::MEDIUM { 1 }
    else if r == SfuRid::HIGH { 2 }
    else { unreachable!("unhandled SfuRid in pacer rank") }
}

fn layer_for_bps(bps: u64) -> SfuRid {
    if bps >= HIGH_MIN_BPS { SfuRid::HIGH }
    else if bps >= MEDIUM_MIN_BPS { SfuRid::MEDIUM }
    else { SfuRid::LOW }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pump(p: &mut SubscriberPacer, bps: u64, n: u8) -> PacerAction {
        let mut last = PacerAction::NoChange;
        for _ in 0..n { last = p.update(bps); }
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
        pump(&mut p, bps, 2);
        assert_eq!(p.layer(), SfuRid::LOW, "should not upgrade after 2 ticks");
        let a = p.update(bps);
        assert_eq!(a, PacerAction::ChangeLayer(SfuRid::MEDIUM));
        assert_eq!(p.layer(), SfuRid::MEDIUM);
    }

    #[test]
    fn downgrade_is_immediate() {
        let mut p = SubscriberPacer::new();
        // Reach HIGH: 3 ticks to MEDIUM, 3 more to HIGH
        pump(&mut p, HIGH_MIN_BPS + 100_000, 6);
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
        assert_eq!(p.layer(), SfuRid::LOW, "should NOT have upgraded --- streak reset");
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
        assert_ne!(action, PacerAction::GoAudioOnly,
            "exactly AUDIO_ONLY_BPS should remain in video mode (condition is strictly <)");
    }

    #[test]
    fn no_double_go_audio_only() {
        // Second call while already audio-only must return NoChange, not GoAudioOnly again
        let mut p = SubscriberPacer::new();
        let first = p.update(AUDIO_ONLY_BPS - 1);
        assert_eq!(first, PacerAction::GoAudioOnly);
        let second = p.update(1_000); // even lower --- still audio-only, must NOT emit again
        assert_eq!(second, PacerAction::NoChange,
            "GoAudioOnly must not be emitted twice; second call while audio-only must return NoChange");
    }

    #[test]
    fn restore_video_resets_streak_for_upgrade() {
        // After RestoreVideo, subscriber is at LOW. Upgrading to MEDIUM still needs 3 ticks.
        let mut p = SubscriberPacer::new();
        p.update(AUDIO_ONLY_BPS - 1); // GoAudioOnly
        p.update(LOW_MIN_BPS + 1);    // RestoreVideo, now at LOW, streak=0
        // 2 ticks above MEDIUM threshold --- not enough
        p.update(MEDIUM_MIN_BPS + 1);
        p.update(MEDIUM_MIN_BPS + 1);
        assert_eq!(p.layer(), SfuRid::LOW, "after RestoreVideo, still need 3 ticks to upgrade");
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
        assert_eq!(action, PacerAction::RestoreVideo,
            "exactly LOW_MIN_BPS while audio-only should trigger RestoreVideo (condition is >=)");
    }

    #[test]
    fn grey_zone_while_audio_only_is_no_change() {
        // bps in (AUDIO_ONLY_BPS, LOW_MIN_BPS) while audio-only: no action
        let mut p = SubscriberPacer::new();
        p.update(AUDIO_ONLY_BPS - 1); // enter audio-only
        for bps in [AUDIO_ONLY_BPS, AUDIO_ONLY_BPS + 1, LOW_MIN_BPS - 1] {
            assert_eq!(p.update(bps), PacerAction::NoChange,
                "bps={bps} in grey zone should be NoChange while audio-only");
        }
    }

    #[test]
    fn downgrade_from_medium_resets_streak_so_re_upgrade_needs_3_ticks() {
        let mut p = SubscriberPacer::new();
        // Get to MEDIUM
        for _ in 0..3 { p.update(MEDIUM_MIN_BPS + 1); }
        assert_eq!(p.layer(), SfuRid::MEDIUM);
        // Downgrade
        p.update(LOW_MIN_BPS + 1);
        assert_eq!(p.layer(), SfuRid::LOW);
        // 2 ticks up --- not enough (streak was reset by downgrade)
        p.update(MEDIUM_MIN_BPS + 1);
        p.update(MEDIUM_MIN_BPS + 1);
        assert_eq!(p.layer(), SfuRid::LOW, "streak must have reset on downgrade");
        // 3rd tick --- upgrades again
        p.update(MEDIUM_MIN_BPS + 1);
        assert_eq!(p.layer(), SfuRid::MEDIUM);
    }
}
