//! Bandwidth-adaptive layer selection ().
//!
//! Each subscriber gets a [] that watches per-tick BWE
//! readings and adjusts [][crate::client::Client::desired_layer]
//! with LiveKit-style hysteresis: 3 consecutive ticks above the next-tier
//! threshold to upgrade, immediate downgrade, hysteretic audio-only mode.
//!
//! Also provides advanced bandwidth estimation ().
//!
//! Implements a GoogCC-inspired congestion controller with:
//! - Kalman-filtered TWCC inter-arrival delay estimation
//! - Loss-based rate control
//! - Per-subscriber state combining both signals with native GCC + client hint ceilings
//!
//! Ported from .

#![allow(dead_code, unused_imports)] // skeleton; wired up in Task 2

#[cfg(feature = "pacer")]
mod hysteresis;

#[cfg(feature = "pacer")]
pub use hysteresis::PacerAction;
#[cfg(feature = "pacer")]
pub(crate) use hysteresis::SubscriberPacer;

/// Bandwidth thresholds, ascending. Subscriber egress BWE is compared against
/// these to drive the per-subscriber pacer FSM.
///
/// Ladder (low to high):
/// - `< SUSPEND_VIDEO_BPS` --- pacer enters `suspended` sub-state. Subscriber
///   receives no media. Audio at this BWE is below the Opus narrow-band budget
///   (~8 kbps); forwarding it burns the link without delivering speech.
/// - `[SUSPEND_VIDEO_BPS, AUDIO_ONLY_BPS)` --- pacer in `audio_only` state.
///   Video frames are dropped; audio is forwarded.
/// - `[AUDIO_ONLY_BPS, LOW_MIN_BPS)` --- recovering. Pacer remains audio-only;
///   needs `>= LOW_MIN_BPS` to lift back to video (`RestoreVideo`).
/// - `>= LOW_MIN_BPS` / `MEDIUM_MIN_BPS` / `HIGH_MIN_BPS` --- video at matching
///   simulcast layer. Upgrade requires `UPGRADE_STREAK` consecutive ticks;
///   downgrade is immediate.

/// Below this egress BWE, the pacer enters its `suspended` sub-state and emits
/// `PacerAction::SuspendVideo`. Subscriber receives no media at all.
///
/// Subsequent commits in this PR add the matching `Propagated::SuspendVideo`
/// event and (in Phase 7) the per-client fanout filter that drops frames.
#[cfg(feature = "pacer")]
pub(crate) const SUSPEND_VIDEO_BPS: u64 = 10_000;
/// Below this egress BWE, video is suspended (audio-only mode) --- bits/s.
#[cfg(feature = "pacer")]
pub(crate) const AUDIO_ONLY_BPS: u64 = 80_000;
/// Minimum BWE to sustain the LOW ("q") simulcast layer --- bits/s.
#[cfg(feature = "pacer")]
pub(crate) const LOW_MIN_BPS: u64 = 150_000;
/// Minimum BWE to sustain the MEDIUM ("h") simulcast layer --- bits/s.
#[cfg(feature = "pacer")]
pub(crate) const MEDIUM_MIN_BPS: u64 = 350_000;
/// Minimum BWE to sustain the HIGH ("f") simulcast layer --- bits/s.
#[cfg(feature = "pacer")]
pub(crate) const HIGH_MIN_BPS: u64 = 700_000;
/// Ticks above next tier required before upgrading (prevents thrash).
#[cfg(feature = "pacer")]
pub(crate) const UPGRADE_STREAK: u8 = 3;

#[cfg(feature = "kalman-bwe")]
pub mod estimator;
#[cfg(feature = "kalman-bwe")]
pub mod feedback;
#[cfg(feature = "kalman-bwe")]
pub mod kalman;
#[cfg(feature = "kalman-bwe")]
pub mod loss;
#[cfg(feature = "kalman-bwe")]
pub mod subscriber;
