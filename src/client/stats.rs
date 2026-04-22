//! BWE and RTCP stats event handlers for [`Client`][super::Client].
//!
//! Translates str0m observability events (`EgressBitrateEstimate`, `PeerStats`)
//! into `Propagated` variants that the registry can forward to room-level logic.

use std::time::Duration;

use str0m::bwe::BweKind;
use str0m::stats::PeerStats;

use crate::bandwidth::BandwidthEstimate;
use crate::propagate::{ClientId, Propagated};
use crate::rtcp_stats::PeerRtcpStats;

/// Translate a str0m `BweKind` into a `Propagated::BandwidthEstimate`.
///
/// str0m 0.18 `BweKind` carries only a single `Bitrate` (either from TWCC or
/// REMB). There are no min/max bounds or uptime fields — `BandwidthEstimate.bps`
/// is the complete surface available.
pub(super) fn propagate_bwe(peer_id: ClientId, bwe: BweKind) -> Propagated {
    let bps = match bwe {
        BweKind::Twcc(bitrate) | BweKind::Remb(_, bitrate) => bitrate.as_u64(),
        // BweKind is #[non_exhaustive]; unknown future variants yield 0 rather than panic.
        _ => 0,
    };
    Propagated::BandwidthEstimate {
        peer_id,
        estimate: BandwidthEstimate { bps },
    }
}

/// Translate a str0m `PeerStats` into a `Propagated::RtcpStats`.
///
/// `PeerStats` is emitted ~1 Hz and carries the per-peer aggregate loss fraction
/// and RTT from RTCP Receiver Reports. Jitter is not available at this
/// granularity (it requires per-MID `MediaEgressStats`) and is set to
/// `Duration::ZERO`.
pub(super) fn propagate_peer_stats(peer_id: ClientId, s: PeerStats) -> Propagated {
    Propagated::RtcpStats {
        peer_id,
        stats: PeerRtcpStats {
            fraction_lost: s.egress_loss_fraction.unwrap_or(0.0),
            rtt: s.rtt.unwrap_or(Duration::ZERO),
            jitter: Duration::ZERO,
        },
    }
}
