use crate::ids::SfuRid;

/// Action returned by [`SubscriberPacer::update`].
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
    pub(crate) fn update(&mut self, _bps: u64) -> PacerAction { PacerAction::NoChange }
}
