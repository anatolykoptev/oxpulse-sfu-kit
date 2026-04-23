//! Bandwidth-adaptive layer selection ().
//!
//! Each subscriber gets a [] that watches per-tick BWE
//! readings and adjusts [][crate::client::Client::desired_layer]
//! with LiveKit-style hysteresis: 3 consecutive ticks above the next-tier
//! threshold to upgrade, immediate downgrade, hysteretic audio-only mode.

#![allow(dead_code, unused_imports)] // skeleton; wired up in Task 2

mod hysteresis;

pub(crate) use hysteresis::SubscriberPacer;
pub use hysteresis::PacerAction;

/// Below this egress BWE, video is suspended (audio-only mode) — bits/s.
pub(crate) const AUDIO_ONLY_BPS: u64 = 80_000;
/// Minimum BWE to sustain the LOW ("q") simulcast layer — bits/s.
pub(crate) const LOW_MIN_BPS: u64 = 150_000;
/// Minimum BWE to sustain the MEDIUM ("h") simulcast layer — bits/s.
pub(crate) const MEDIUM_MIN_BPS: u64 = 350_000;
/// Minimum BWE to sustain the HIGH ("f") simulcast layer — bits/s.
pub(crate) const HIGH_MIN_BPS: u64 = 700_000;
/// Ticks above next tier required before upgrading (prevents thrash).
pub(crate) const UPGRADE_STREAK: u8 = 3;
