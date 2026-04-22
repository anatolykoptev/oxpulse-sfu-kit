//! Bandwidth estimation types for the kit.
//!
//! str0m 0.18 internally runs a libWebRTC GoogCC (`src/bwe/`) — trendline delay
//! estimator + MLE loss controller + leaky-bucket pacer — and surfaces its output
//! via `str0m::Event::EgressBitrateEstimate(BweKind)`. The kit translates that
//! event into `Propagated::BandwidthEstimate` so downstream room logic or a
//! `LayerSelector` implementation can adapt forwarding without importing str0m types.
//!
//! # str0m 0.18 divergence
//!
//! str0m's `BweKind` carries only a single `Bitrate` value (either from TWCC or
//! REMB). There are no min/max bounds or estimator uptime fields on the event.
//! `BandwidthEstimate` therefore exposes only `bps`; callers should not assume
//! lower/upper bounds are available from this event type.

/// Aggregated egress bandwidth estimate for a peer's outgoing stream.
///
/// Units: bits per second. `0` is legal — it means the estimator has observed
/// network failure or has no data yet.
///
/// Emitted from str0m's internal GoogCC each time the estimator produces a new
/// value (typically every 100–500 ms, driven by TWCC or REMB feedback).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BandwidthEstimate {
    /// Current estimate in bits per second.
    pub bps: u64,
}

impl BandwidthEstimate {
    /// Zero-bandwidth fallback useful in tests and as a safe initializer.
    pub fn zero() -> Self {
        Self { bps: 0 }
    }
}
