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

#[cfg(feature = "pacer")]
mod hysteresis;
#[cfg(feature = "pacer")]
pub mod pacer_config;
#[cfg(feature = "pacer")]
pub use pacer_config::{PacerConfig, PacerConfigError};

#[cfg(feature = "pacer")]
pub use hysteresis::PacerAction;
#[cfg(feature = "pacer")]
pub use hysteresis::SubscriberPacer;

/// Below this egress BWE, the pacer enters its `suspended` sub-state and emits
/// `PacerAction::SuspendVideo`. Audio at this BWE is below the Opus narrow-band
/// budget (~8 kbps); forwarding it burns the link without delivering speech.
/// Phase 7 wires the per-client fanout filter that drops video frames for
/// suspended subscribers.
#[cfg(feature = "pacer")]
pub const SUSPEND_VIDEO_BPS: u64 = 10_000;
/// Below this egress BWE, video is suspended (audio-only mode) --- bits/s.
#[cfg(feature = "pacer")]
pub const AUDIO_ONLY_BPS: u64 = 80_000;
/// Minimum BWE to sustain the LOW ("q") simulcast layer --- bits/s.
#[cfg(feature = "pacer")]
pub const LOW_MIN_BPS: u64 = 150_000;
/// Minimum BWE to sustain the MEDIUM ("h") simulcast layer --- bits/s.
#[cfg(feature = "pacer")]
pub const MEDIUM_MIN_BPS: u64 = 350_000;
/// Minimum BWE to sustain the HIGH ("f") simulcast layer --- bits/s.
#[cfg(feature = "pacer")]
pub const HIGH_MIN_BPS: u64 = 700_000;
/// Consecutive ticks below `SUSPEND_VIDEO_BPS` required before the pacer
/// enters its `suspended` sub-state. Asymmetric with `UPGRADE_STREAK`:
/// entry mistakes cause a visible video gap, so a small (2-tick) debounce
/// rejects single-tick TWCC spikes without delaying real-congestion response.
///
/// Effective response latency: `SUSPEND_STREAK × tick_interval`. At the
/// nominal TWCC tick rate of ~50 ms, suspend-entry takes ≈100 ms; at the
/// 100 ms BandwidthEstimate cadence, ≈200 ms. Tune in tandem with tick
/// rate when revising.
///
/// Edge case: under sustained alternating-tick loss (e.g., a 50 % periodic
/// drop pattern at the tick frequency), `SUSPEND_STREAK` never fires
/// because the streak resets on every recovered tick. The `audio_only`
/// path via `AUDIO_ONLY_BPS` then catches the BWE collapse instead. This
/// is intentional: a 2-tick debounce that does NOT clear on intervening
/// good ticks would re-introduce the spike-trigger problem this guard
/// exists to prevent.
///
/// Exposed `pub` (not `pub(crate)`) so integration tests in `tests/` can
/// import it without hardcoding the literal. Documented `#[doc(hidden)]`
/// because it is not part of the public stable API surface; behavioural
/// invariants observable through `Propagated::SuspendVideo` are the
/// stable contract, not this constant.
#[cfg(feature = "pacer")]
#[doc(hidden)]
pub const SUSPEND_STREAK: u8 = 2;
/// Ticks above next tier required before upgrading (prevents thrash).
#[cfg(feature = "pacer")]
pub const UPGRADE_STREAK: u8 = 3;

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

#[cfg(feature = "googcc-bwe")]
#[cfg_attr(docsrs, doc(cfg(feature = "googcc-bwe")))]
pub mod googcc;
#[cfg(feature = "googcc-bwe")]
#[cfg_attr(docsrs, doc(cfg(feature = "googcc-bwe")))]
pub use googcc::GoogCcEstimator;
