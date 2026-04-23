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
        registry
            .clients()
            .iter()
            .find(|c| c.id == client_id)
            .unwrap()
            .desired_layer(),
        SfuRid::LOW,
        "should start at LOW"
    );

    // 3 ticks at 400 kbps (above MEDIUM threshold 350k)
    for _ in 0..3 {
        registry.drive_pacer_for_tests(client_id, 400_000);
    }

    assert_eq!(
        registry
            .clients()
            .iter()
            .find(|c| c.id == client_id)
            .unwrap()
            .desired_layer(),
        SfuRid::MEDIUM,
        "should upgrade to MEDIUM after 3 ticks"
    );
}

#[cfg(feature = "av1-dd")]
#[test]
fn av1_dd_max_temporal_layer_accessor_exists() {
    use oxpulse_sfu_kit::client::test_seed::new_client;
    use oxpulse_sfu_kit::ClientId;
    let mut client = new_client(ClientId(999));
    assert_eq!(client.max_temporal_layer(), u8::MAX);
    client.set_max_temporal_layer(1);
    assert_eq!(client.max_temporal_layer(), 1);
}

#[test]
fn propagated_publisher_layer_hint_variant_exists() {
    use oxpulse_sfu_kit::{ClientId, Propagated, SfuRid};
    let _ = Propagated::PublisherLayerHint {
        publisher_id: ClientId(1),
        max_rid: SfuRid::MEDIUM,
    };
}

#[test]
fn propagated_audio_codec_hint_variant_exists() {
    use oxpulse_sfu_kit::{ClientId, Propagated};
    let _ = Propagated::AudioCodecHint {
        peer_id: ClientId(1),
        opus_red: true,
        opus_dred: false,
    };
}

#[test]
fn key_epoch_accessible() {
    use oxpulse_sfu_kit::KeyEpoch;
    assert_eq!(KeyEpoch::new(7).as_u64(), 7);
}

#[test]
fn layer_selector_prefers_medium_over_low_when_both_active() {
    use oxpulse_sfu_kit::{
        layer_selector::{BestFitSelector, LayerSelector},
        SfuRid,
    };
    // Subscriber wants HIGH, publisher sends [LOW, MEDIUM] → selector returns MEDIUM
    let active = [SfuRid::LOW, SfuRid::MEDIUM];
    let result = BestFitSelector.select(SfuRid::HIGH, &active);
    assert_eq!(
        result,
        SfuRid::MEDIUM,
        "when HIGH is desired but only LOW+MEDIUM available, selector must return MEDIUM"
    );
}

#[test]
fn client_is_local_by_default() {
    use oxpulse_sfu_kit::client::test_seed::new_client;
    use oxpulse_sfu_kit::{ClientId, ClientOrigin};
    let client = new_client(ClientId(200));
    assert!(
        !client.is_relay(),
        "freshly-built client must not be a relay"
    );
    assert_eq!(client.origin(), &ClientOrigin::Local);
}

#[test]
fn set_origin_marks_client_as_relay() {
    use oxpulse_sfu_kit::client::test_seed::new_client;
    use oxpulse_sfu_kit::{ClientId, ClientOrigin};
    let mut client = new_client(ClientId(201));
    client.set_origin(ClientOrigin::RelayFromSfu("sfu-eu-1".to_string()));
    assert!(client.is_relay());
    assert_eq!(
        client.origin(),
        &ClientOrigin::RelayFromSfu("sfu-eu-1".to_string())
    );
}

#[test]
fn upstream_keyframe_request_variant_exists() {
    use oxpulse_sfu_kit::{ClientId, Propagated, SfuKeyframeKind, SfuKeyframeRequest, SfuMid};
    let mid: SfuMid = "0".parse().expect("valid mid");
    let req = SfuKeyframeRequest::new_for_tests(mid, None, SfuKeyframeKind::Pli);
    let _ = Propagated::UpstreamKeyframeRequest {
        source_relay_id: ClientId(99),
        req,
        source_mid: mid,
    };
}

#[cfg(feature = "test-utils")]
#[test]
fn keyframe_request_for_relay_track_emits_upstream_variant() {
    use oxpulse_sfu_kit::client::test_seed::{
        new_client, open_track_out_for_tests, seed_track_in_relay,
    };
    use oxpulse_sfu_kit::{ClientId, ClientOrigin, Propagated};
    use str0m::media::MediaKind;

    let relay_id = ClientId(300);
    let mut relay = new_client(relay_id);
    relay.set_origin(ClientOrigin::RelayFromSfu("edge-eu".to_string()));
    let track_arc = seed_track_in_relay(&mut relay, 5, MediaKind::Video);

    let sub_id = ClientId(301);
    let mut sub = new_client(sub_id);
    sub.handle_track_open(std::sync::Arc::downgrade(&track_arc));
    open_track_out_for_tests(&mut sub, &track_arc);

    let propagated = sub.incoming_keyframe_req_for_tests(str0m::media::KeyframeRequest {
        mid: track_arc.mid,
        rid: None,
        kind: str0m::media::KeyframeRequestKind::Pli,
    });

    match propagated {
        Propagated::UpstreamKeyframeRequest {
            source_relay_id, ..
        } => {
            assert_eq!(source_relay_id, relay_id);
        }
        other => panic!("expected UpstreamKeyframeRequest, got {:?}", other),
    }
}

#[test]
fn publisher_layer_hint_for_upstream_variant_exists() {
    use oxpulse_sfu_kit::{ClientId, Propagated, SfuRid};
    let _ = Propagated::PublisherLayerHintForUpstream {
        publisher_relay_id: ClientId(500),
        max_rid: SfuRid::HIGH,
    };
}

#[cfg(feature = "test-utils")]
#[test]
fn emit_publisher_layer_hints_emits_upstream_variant_for_relay_publisher() {
    use oxpulse_sfu_kit::client::test_seed::{new_client, seed_track_in_relay};
    use oxpulse_sfu_kit::{ClientId, ClientOrigin, Propagated, Registry, SfuRid};
    use str0m::media::MediaKind;

    let mut registry = Registry::new_for_tests();

    // Relay publisher (idx 0).
    let mut relay = new_client(ClientId(501));
    relay.set_origin(ClientOrigin::RelayFromSfu("edge-eu".to_string()));
    let relay_id = relay.id;
    let _track = seed_track_in_relay(&mut relay, 7, MediaKind::Video);
    registry.insert(relay);

    // Local subscriber (idx 1) that wants HIGH.
    let sub = new_client(ClientId(502));
    registry.insert(sub);
    registry.set_desired_layer_for_tests(1, SfuRid::HIGH);

    // Wire subscriber's track_out to relay's track_in.
    registry.wire_track_for_tests(1, 0, 7);

    // Emit hints.
    registry.emit_publisher_layer_hints();

    let hints = registry.drain_propagated_for_tests();

    let found = hints.iter().any(|p| {
        matches!(p, Propagated::PublisherLayerHintForUpstream {
            publisher_relay_id,
            max_rid,
        } if *publisher_relay_id == relay_id && *max_rid == SfuRid::HIGH)
    });
    assert!(
        found,
        "expected PublisherLayerHintForUpstream; got: {:?}",
        hints
    );
}

#[cfg(feature = "test-utils")]
#[test]
fn relay_source_keyframe_is_not_delivered_to_relay_client() {
    use oxpulse_sfu_kit::client::test_seed::{
        new_client, open_track_out_for_tests, seed_track_in_relay,
    };
    use oxpulse_sfu_kit::{ClientId, ClientOrigin, Propagated, Registry};
    use str0m::media::MediaKind;

    let mut registry = Registry::new_for_tests();

    // Relay publisher (idx 0).
    let mut relay = new_client(ClientId(600));
    relay.set_origin(ClientOrigin::RelayFromSfu("sfu-eu-1".to_string()));
    let relay_id = relay.id;
    let track_arc = seed_track_in_relay(&mut relay, 9, MediaKind::Video);
    registry.insert(relay);

    // Local subscriber (idx 1).
    let mut sub = new_client(ClientId(601));
    sub.handle_track_open(std::sync::Arc::downgrade(&track_arc));
    open_track_out_for_tests(&mut sub, &track_arc);
    registry.insert(sub);

    // Simulate the subscriber emitting a keyframe request.
    let kf_prop = registry.clients_mut_for_tests()[1].incoming_keyframe_req_for_tests(
        str0m::media::KeyframeRequest {
            mid: track_arc.mid,
            rid: None,
            kind: str0m::media::KeyframeRequestKind::Pli,
        },
    );

    // Must be UpstreamKeyframeRequest, not a direct PLI/FIR.
    match &kf_prop {
        Propagated::UpstreamKeyframeRequest {
            source_relay_id, ..
        } => {
            assert_eq!(
                *source_relay_id, relay_id,
                "upstream request must reference the relay client"
            );
        }
        other => panic!("expected UpstreamKeyframeRequest, got {:?}", other),
    }

    // Fanout the event -- relay client must receive no side-effects.
    // (UpstreamKeyframeRequest is a no-op in fanout; it is for app consumption only.)
    registry.fanout_for_tests(&kf_prop);

    // Relay client (idx 0) has delivered zero media -- no PLI/FIR was sent to it.
    assert_eq!(
        registry.delivered_media_count(0),
        0,
        "relay client must not receive any media via fanout after UpstreamKeyframeRequest"
    );
}

#[test]
fn client_budget_hint_variant_exists() {
    use oxpulse_sfu_kit::{ClientId, Propagated};
    let hint = Propagated::ClientBudgetHint(ClientId(77), 500_000u64);
    assert!(matches!(hint, Propagated::ClientBudgetHint(_, 500_000)));
}

#[cfg(all(feature = "kalman-bwe", feature = "pacer", feature = "test-utils"))]
#[test]
fn kalman_bwe_drives_layer_selection_via_update_pacer_layers() {
    use oxpulse_sfu_kit::client::test_seed::new_client;
    use oxpulse_sfu_kit::{ClientId, Registry, SfuRid};

    let mut registry = Registry::new_for_tests();

    // Publisher (idx 0).
    let publisher = new_client(ClientId(800));
    let pub_id = publisher.id;
    registry.insert(publisher);

    // Subscriber (idx 1).
    let subscriber = new_client(ClientId(801));
    let sub_id = subscriber.id;
    registry.insert(subscriber);

    // Subscriber defaults to LOW.
    assert_eq!(
        registry
            .clients()
            .iter()
            .find(|c| c.id == sub_id)
            .unwrap()
            .desired_layer(),
        SfuRid::LOW,
        "should start at LOW"
    );

    // Force the Kalman delay and loss estimators to 2 Mbps directly, bypassing
    // TWCC. `record_native_estimate` acts as a ceiling, not a floor, so it would
    // leave the estimate at the 300k initial value.
    registry
        .bandwidth_mut_for_tests()
        .force_high_estimate_for_tests(sub_id, 2_000_000.0);

    // Call update_pacer_layers 3 times — pacer streak threshold is 3 ticks.
    for _ in 0..3 {
        registry.update_pacer_layers(pub_id);
    }

    let desired = registry
        .clients()
        .iter()
        .find(|c| c.id == sub_id)
        .unwrap()
        .desired_layer();
    // With 2 Mbps estimate the pacer should reach MEDIUM or HIGH.
    assert!(
        desired == SfuRid::MEDIUM || desired == SfuRid::HIGH,
        "expected MEDIUM or HIGH, got {:?}",
        desired
    );
}
