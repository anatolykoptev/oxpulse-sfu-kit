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
