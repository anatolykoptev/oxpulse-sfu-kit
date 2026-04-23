use crate::ids::SfuRid;

/// Action returned by [].
#[must_use = "PacerAction must be applied to the subscriber's forwarding state"]
#[cfg_attr(docsrs, doc(cfg(feature = "pacer")))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacerAction {
    /// No layer change.
    NoChange,
    /// Switch to this simulcast layer immediately.
    ChangeLayer(SfuRid),
    /// BWE fell below audio-only threshold — stop forwarding video.
    GoAudioOnly,
    /// BWE recovered — resume video forwarding.
    RestoreVideo,
}

/// Per-subscriber hysteretic layer selector.
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
    #[allow(clippy::needless_pass_by_ref_mut)] // becomes mutating in Task 2
    pub(crate) fn update(&mut self, _bps: u64) -> PacerAction {
        PacerAction::NoChange
    }
}
