//! Verifies that `PeerRtcpStats` type and `Propagated::RtcpStats` variant are
//! correctly shaped and constructible.
//!
//! Real RTCP stat emission requires a live str0m session (planned for v0.3).
//! For v0.2 we verify type shape + variant existence + zero-value constructor.

use std::time::Duration;

use oxpulse_sfu_kit::{ClientId, PeerRtcpStats, Propagated};

#[test]
fn peer_rtcp_stats_zero_constructor() {
    let s = PeerRtcpStats::zero();
    assert_eq!(s.fraction_lost, 0.0);
    assert_eq!(s.rtt, Duration::ZERO);
    assert_eq!(s.jitter, Duration::ZERO);
}

#[test]
fn peer_rtcp_stats_field_assignment() {
    let s = PeerRtcpStats {
        fraction_lost: 0.05,
        rtt: Duration::from_millis(42),
        jitter: Duration::ZERO,
    };
    assert!((s.fraction_lost - 0.05).abs() < f32::EPSILON);
    assert_eq!(s.rtt, Duration::from_millis(42));
}

#[test]
fn propagated_rtcp_stats_variant_exists() {
    let p = Propagated::RtcpStats {
        peer_id: ClientId(0),
        stats: PeerRtcpStats::zero(),
    };
    assert!(matches!(p, Propagated::RtcpStats { .. }));
}

#[test]
fn propagated_rtcp_stats_has_client_id() {
    let p = Propagated::RtcpStats {
        peer_id: ClientId(7),
        stats: PeerRtcpStats {
            fraction_lost: 0.1,
            rtt: Duration::from_millis(20),
            jitter: Duration::ZERO,
        },
    };
    assert_eq!(p.client_id(), Some(ClientId(7)));
}

#[test]
fn reap_dead_peer_unknown_id_does_not_panic() {
    // reap_dead_peer must be safe to call with any peer_id, including unknown ones.
    let metrics = oxpulse_sfu_kit::SfuMetrics::new_default();
    metrics.reap_dead_peer(u64::MAX); // must not panic
    metrics.reap_dead_peer(0); // double-reap must not panic
    metrics.reap_dead_peer(0);
}
