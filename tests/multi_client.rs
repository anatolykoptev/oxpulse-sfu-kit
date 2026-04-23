//! Multi-client fanout and registry cross-advertisement tests.
//!
//! Fanout semantics: `Propagated::MediaData` from A reaches B and C, not A
//! (origin skip). Cross-advertisement: a late-joiner's `tracks_out` is
//! pre-populated with every already-open track.

use std::sync::Arc;

use oxpulse_sfu_kit::client::layer;
use oxpulse_sfu_kit::client::test_seed::{make_media_data, new_client, seed_track_in};
use oxpulse_sfu_kit::client::TrackIn;
use oxpulse_sfu_kit::{ClientId, Propagated, Registry};
use str0m::media::MediaKind;

#[test]
fn fanout_every_to_every_excludes_origin() {
    let (a_id, b_id, c_id) = (ClientId(10), ClientId(11), ClientId(12));

    let mut a = new_client(a_id);
    let mut b = new_client(b_id);
    let mut c = new_client(c_id);

    let track_in: Arc<TrackIn> = seed_track_in(&mut a, 1, MediaKind::Video);
    b.handle_track_open(Arc::downgrade(&track_in));
    c.handle_track_open(Arc::downgrade(&track_in));

    let data = make_media_data(1, None);
    let prop = Propagated::MediaData(a_id, data);

    let mut peers = vec![a, b, c];
    oxpulse_sfu_kit::fanout::fanout_for_tests(&prop, &mut peers);

    assert_eq!(peers[0].delivered_media_count(), 0, "A is origin — skipped");
    assert_eq!(peers[1].delivered_media_count(), 1, "B receives fanout");
    assert_eq!(peers[2].delivered_media_count(), 1, "C receives fanout");
}

#[test]
fn registry_insert_cross_advertises_existing_tracks() {
    let mut registry = Registry::new_for_tests();

    let mut a = new_client(ClientId(20));
    let _arc_a = seed_track_in(&mut a, 1, MediaKind::Video);
    registry.insert(a);

    let mut b = new_client(ClientId(21));
    let _arc_b = seed_track_in(&mut b, 2, MediaKind::Audio);
    registry.insert(b);

    let c = new_client(ClientId(22));
    registry.insert(c);

    assert_eq!(registry.len(), 3);

    let prop = Propagated::MediaData(ClientId(20), make_media_data(1, None));
    registry.fanout_for_tests(&prop);
    assert_eq!(registry.delivered_media_count(0), 0, "A origin");
    assert_eq!(registry.delivered_media_count(1), 1, "B saw A's media");
    assert_eq!(registry.delivered_media_count(2), 1, "C saw A's media");

    let prop = Propagated::MediaData(ClientId(21), make_media_data(2, None));
    registry.fanout_for_tests(&prop);
    assert_eq!(registry.delivered_media_count(0), 1, "A saw B's media");
    assert_eq!(registry.delivered_media_count(1), 1, "B origin (unchanged)");
    assert_eq!(registry.delivered_media_count(2), 2, "C saw A+B media");
}

#[test]
fn simulcast_rid_filter_drops_mismatched_layers() {
    let mut registry = Registry::new_for_tests();

    let mut a = new_client(ClientId(30));
    let _arc = seed_track_in(&mut a, 1, MediaKind::Video);
    registry.insert(a);

    let b = new_client(ClientId(31));
    registry.insert(b);
    let c = new_client(ClientId(32));
    registry.insert(c);

    assert_eq!(
        registry.clients()[1].desired_layer(),
        layer::LOW,
        "B default LOW"
    );
    assert_eq!(
        registry.clients()[2].desired_layer(),
        layer::LOW,
        "C default LOW"
    );

    // RID=q — both match at LOW.
    let prop_q = Propagated::MediaData(ClientId(30), make_media_data(1, Some(layer::LOW)));
    registry.fanout_for_tests(&prop_q);
    assert_eq!(registry.delivered_media_count(1), 1, "B got q");
    assert_eq!(registry.delivered_media_count(2), 1, "C got q");

    // RID=f — neither matches.
    let prop_f = Propagated::MediaData(ClientId(30), make_media_data(1, Some(layer::HIGH)));
    registry.fanout_for_tests(&prop_f);
    assert_eq!(registry.delivered_media_count(1), 1, "B filters out f");
    assert_eq!(registry.delivered_media_count(2), 1, "C filters out f");

    // Flip C to HIGH.
    registry.set_desired_layer_for_tests(2, layer::HIGH);
    assert_eq!(
        registry.clients()[2].desired_layer(),
        layer::HIGH,
        "C now HIGH"
    );

    // RID=f — C matches, B doesn't.
    let prop_f = Propagated::MediaData(ClientId(30), make_media_data(1, Some(layer::HIGH)));
    registry.fanout_for_tests(&prop_f);
    assert_eq!(registry.delivered_media_count(1), 1, "B still filters f");
    assert_eq!(registry.delivered_media_count(2), 2, "C got f");

    // rid=None bypasses the filter.
    let prop_none = Propagated::MediaData(ClientId(30), make_media_data(1, None));
    registry.fanout_for_tests(&prop_none);
    assert_eq!(registry.delivered_media_count(1), 2, "B got non-simulcast");
    assert_eq!(registry.delivered_media_count(2), 3, "C got non-simulcast");
}

#[cfg(feature = "pacer")]
#[test]
fn pacer_upgrades_layer_after_three_bwe_ticks() {
    use oxpulse_sfu_kit::client::test_seed::new_client;
    use oxpulse_sfu_kit::{ClientId, Registry, SfuRid};

    let mut registry = Registry::new_for_tests();
    let client = new_client(ClientId(100));
    let client_id = client.id;
    registry.insert(client);

    assert_eq!(
        registry.clients().iter().find(|c| c.id == client_id).unwrap().desired_layer(),
        SfuRid::LOW,
        "should start at LOW"
    );

    // 3 ticks at 400 kbps (above MEDIUM threshold 350k)
    for _ in 0..3 {
        registry.drive_pacer_for_tests(client_id, 400_000);
    }

    assert_eq!(
        registry.clients().iter().find(|c| c.id == client_id).unwrap().desired_layer(),
        SfuRid::MEDIUM,
        "should upgrade to MEDIUM after 3 ticks"
    );
}
