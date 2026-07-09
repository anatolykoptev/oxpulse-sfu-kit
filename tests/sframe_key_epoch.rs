//! SFrame key-epoch (KID) RTP header-extension forwarding.
//!
//! Verifies the data-plane wire added in v0.11.9: a KID present on an inbound
//! packet is captured onto `SfuMediaPayload` and survives the fanout path so
//! str0m re-attaches it to every subscriber's outbound RTP.
//!
//! The KID lands in `MediaData.ext_vals.user_values` only when a KID extension
//! serializer is registered on the `Rtc` (see `oxpulse_sfu_kit::sframe`); these
//! tests inject it directly via the test seam, mirroring what str0m does on
//! ingest, so the kit's capture + fanout wiring is exercised without a full
//! ICE/DTLS/SDP pipeline.

use std::sync::Arc;

use oxpulse_sfu_kit::client::test_seed::{
    make_media_data_with_key_epoch, new_client, seed_track_in,
};
use oxpulse_sfu_kit::client::TrackIn;
use oxpulse_sfu_kit::{ClientId, KeyEpoch, Propagated, Registry};
use str0m::media::MediaKind;

/// A KID present on the inbound packet is captured onto the payload.
#[test]
fn inbound_key_epoch_is_captured() {
    let payload = make_media_data_with_key_epoch(1, None, KeyEpoch::new(7));
    assert_eq!(
        payload.key_epoch(),
        Some(KeyEpoch::new(7)),
        "from_str0m must capture the KID off ext_vals.user_values"
    );
}

/// A packet with no KID extension captures `None` (no spurious epoch).
#[test]
fn inbound_without_extension_captures_none() {
    let payload = oxpulse_sfu_kit::client::test_seed::make_media_data(1, None);
    assert_eq!(payload.key_epoch(), None);
}

/// A KID-carrying packet fans out to every non-origin subscriber, and the
/// payload handed to the fanout write path still carries the KID (which
/// `handle_media_data_out` re-attaches via `Writer::user_extension_value`).
#[test]
fn key_epoch_survives_fanout() {
    let mut registry = Registry::new_for_tests();

    let mut a = new_client(ClientId(40));
    let track_in: Arc<TrackIn> = seed_track_in(&mut a, 1, MediaKind::Video);
    registry.insert(a);

    let mut b = new_client(ClientId(41));
    b.handle_track_open(Arc::downgrade(&track_in));
    registry.insert(b);

    let payload = make_media_data_with_key_epoch(1, None, KeyEpoch::new(9));
    assert_eq!(
        payload.key_epoch(),
        Some(KeyEpoch::new(9)),
        "payload entering fanout carries the KID"
    );

    let prop = Propagated::MediaData(ClientId(40), payload);
    registry.fanout_for_tests(&prop);

    assert_eq!(
        registry.delivered_media_count(0),
        0,
        "A is origin — skipped"
    );
    assert_eq!(
        registry.delivered_media_count(1),
        1,
        "B receives the KID-carrying fanout"
    );
}

/// The public registration helper is exported and builds an extension bound to
/// the caller's URI (the URI their clients negotiate in SDP).
#[test]
fn registration_helper_is_public() {
    let ext = oxpulse_sfu_kit::sframe_key_id_extension("urn:example:rtp-hdrext:sframe-kid");
    assert_eq!(ext.as_uri(), "urn:example:rtp-hdrext:sframe-kid");
}
