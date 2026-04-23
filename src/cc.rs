//! Congestion control plugin seam.
//!
//! The default congestion control in this kit is Google Congestion Control (GoogCC),
//! implemented inside str0m and surfaced via [`BandwidthEstimate`][crate::BandwidthEstimate]
//! events. This trait provides a plugin point for alternative algorithms (SCReAMv2, L4S)
//! when Rust implementations become available.
//!
//! # Current state
//!
//! - **GoogCC (default):** fully functional via str0m 0.18 trendline estimator.
//!   No code required; use [`BandwidthEstimate`][crate::BandwidthEstimate] events.
//! - **SCReAMv2 (RFC 8298 + IETF 125 updates):** no Rust implementation exists yet.
//!   Reference: `EricssonResearch/scream` (Apache-2.0, C++).
//! - **L4S (RFC 9330):** requires kernel + network-path cooperation (ECT(1) / DualPI2).
//!   Chromium rollout in progress; not deployable in general-purpose SFUs today.
//!
//! When a Rust SCReAMv2 or L4S crate is available, implement this trait and pass it
//! to [`SfuConfig`][crate::SfuConfig].

use std::time::Instant;

/// Plugin interface for replacing the default GoogCC congestion controller.
///
/// Implement this trait to inject a custom algorithm (SCReAM, L4S, etc.).
/// The kit feeds incoming TWCC feedback packets; the impl returns a bitrate estimate.
///
/// # Stability note
///
/// This trait is `#[non_exhaustive]`-equivalent for now: it will gain methods as
/// integration deepens. Implement with a `#[allow(unused)]` if forward-compatibility
/// is needed.
pub trait CongestionControl: Send + Sync + 'static {
    /// Feed a raw TWCC feedback packet payload for processing.
    ///
    /// Called for every incoming RTCP TWCC packet received from a subscriber.
    /// `peer_id` identifies which peer's feedback this is.
    fn on_twcc_feedback(&mut self, peer_id: u64, payload: &[u8], now: Instant);

    /// Current egress bandwidth estimate for a peer, in bits per second.
    ///
    /// Called by the kit after `on_twcc_feedback` to retrieve the updated estimate.
    /// Return `None` if the estimator has not yet converged.
    fn egress_estimate_bps(&self, peer_id: u64) -> Option<u64>;
}

/// Default congestion control — delegates to str0m's built-in GoogCC.
///
/// This is a no-op implementation: str0m already runs GoogCC internally and
/// surfaces the result via `Event::EgressBitrateEstimate`. Using this default
/// means no additional TWCC processing is done at the kit level.
#[derive(Debug, Default)]
pub struct DefaultGoogCC;

impl CongestionControl for DefaultGoogCC {
    fn on_twcc_feedback(&mut self, _peer_id: u64, _payload: &[u8], _now: Instant) {
        // Handled inside str0m; no additional processing needed.
    }

    fn egress_estimate_bps(&self, _peer_id: u64) -> Option<u64> {
        None // Kit reads from BandwidthEstimate events instead.
    }
}
