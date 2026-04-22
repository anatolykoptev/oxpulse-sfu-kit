//! Per-peer RTCP-derived stats.
//!
//! Populated from str0m's `Event::PeerStats` (emitted approximately every second).
//! The `PeerStats` struct carries `egress_loss_fraction` and `rtt` from RTCP
//! Receiver Reports; it is the coarsest-granularity, per-peer aggregate available
//! in str0m 0.18.
//!
//! # Jitter
//!
//! Jitter is available only via `Event::MediaEgressStats { remote: RemoteIngressStats }`,
//! which is per-MID and carries the raw RTCP `jitter` u32 in RTP timestamp units.
//! Converting to wall-clock requires a codec clock rate, which is MID-dependent.
//! For v0.2 `jitter` is always `Duration::ZERO`; a per-MID extension is deferred
//! to v0.3.

use std::time::Duration;

/// Rolling statistics for one peer derived from RTCP Receiver Reports.
///
/// Values are snapshots from the most recent `Event::PeerStats`; they are
/// replaced (not averaged) on each update.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PeerRtcpStats {
    /// Fraction of packets lost in the last reporting interval, in the range
    /// `[0.0, 1.0]`. `0.0` when no data is available yet.
    pub fraction_lost: f32,
    /// Round-trip time derived from RTCP SR/RR exchange. `Duration::ZERO` when
    /// no measurement is available yet.
    pub rtt: Duration,
    /// Per-stream jitter. Always `Duration::ZERO` in this release — see module
    /// documentation for rationale.
    pub jitter: Duration,
}

impl PeerRtcpStats {
    /// Zero-value fallback useful in tests and as a safe initializer.
    pub fn zero() -> Self {
        Self {
            fraction_lost: 0.0,
            rtt: Duration::ZERO,
            jitter: Duration::ZERO,
        }
    }
}
