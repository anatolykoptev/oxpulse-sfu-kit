//! Verifies that `BandwidthEstimate` type and `Propagated::BandwidthEstimate`
//! variant are correctly shaped and constructible.
//!
//! Real BWE emission requires sim_net infrastructure (planned for v0.3).
//! For v0.2 we verify type shape + variant existence + zero-value constructor.

use oxpulse_sfu_kit::{BandwidthEstimate, ClientId, Propagated};

#[test]
fn bandwidth_estimate_zero_constructor() {
    let est = BandwidthEstimate::zero();
    assert_eq!(est.bps, 0);
}

#[test]
fn bandwidth_estimate_field_assignment() {
    let est = BandwidthEstimate { bps: 1_500_000 };
    assert_eq!(est.bps, 1_500_000);
}

#[test]
fn propagated_bandwidth_estimate_variant_exists() {
    let p = Propagated::BandwidthEstimate {
        peer_id: ClientId(0),
        estimate: BandwidthEstimate::zero(),
    };
    assert!(matches!(p, Propagated::BandwidthEstimate { .. }));
}

#[test]
fn propagated_bandwidth_estimate_has_client_id() {
    let p = Propagated::BandwidthEstimate {
        peer_id: ClientId(42),
        estimate: BandwidthEstimate { bps: 500_000 },
    };
    assert_eq!(p.client_id(), Some(ClientId(42)));
}
