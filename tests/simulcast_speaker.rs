//! Dominant-speaker integration tests (requires `active-speaker` + `test-utils`).
//!
//! Verifies Registry → detector → fanout wiring including hysteresis and
//! skip-self. Algorithm unit tests live in `rust-dominant-speaker`.

use std::time::{Duration, Instant};

use oxpulse_sfu_kit::client::test_seed::new_client;
use oxpulse_sfu_kit::{ClientId, Propagated, Registry};

#[test]
fn active_speaker_dominance_and_hysteresis_and_skip_self() {
    let mut registry = Registry::new_for_tests();
    let mut a = new_client(ClientId(1));
    a.id = ClientId(1);
    let mut b = new_client(ClientId(2));
    b.id = ClientId(2);
    let mut c = new_client(ClientId(3));
    c.id = ClientId(3);
    registry.insert(a);
    registry.insert(b);
    registry.insert(c);

    // Bootstrap: first tick elects some peer (HashMap internals — order not deterministic).
    let t0 = Instant::now();
    registry.force_active_speaker_tick_for_tests(t0);
    let winner = registry
        .current_active_speaker()
        .expect("bootstrap elected someone");
    assert!(
        [1u64, 2, 3].contains(&winner),
        "bootstrap picked a valid peer"
    );
    let winner_idx = (winner - 1) as usize;
    assert_eq!(
        registry.delivered_active_speaker_count(winner_idx),
        0,
        "winner skip-self"
    );
    for idx in 0..3 {
        if idx != winner_idx {
            assert!(
                registry.delivered_active_speaker_count(idx) >= 1,
                "non-winner notified"
            );
        }
    }

    // Hysteresis: 3 more ticks without audio → incumbent persists.
    for step in 1..=3 {
        registry.force_active_speaker_tick_for_tests(t0 + Duration::from_millis(300 * step));
    }
    assert_eq!(
        registry.current_active_speaker(),
        Some(winner),
        "incumbent holds"
    );

    // Skip-self on flip to peer 2: B must not receive its own dominance event.
    let [a0, b0, c0] = [
        registry.delivered_active_speaker_count(0),
        registry.delivered_active_speaker_count(1),
        registry.delivered_active_speaker_count(2),
    ];
    registry.fanout_for_tests(&Propagated::ActiveSpeakerChanged {
        peer_id: 2,
        confidence: 0.0,
    });
    assert_eq!(
        registry.delivered_active_speaker_count(1),
        b0,
        "B skip-self on flip"
    );
    assert_eq!(
        registry.delivered_active_speaker_count(0),
        a0 + 1,
        "A notified"
    );
    assert_eq!(
        registry.delivered_active_speaker_count(2),
        c0 + 1,
        "C notified"
    );
}

#[test]
fn reap_dead_removes_peer_from_detector() {
    let mut registry = Registry::new_for_tests();
    let a = new_client(ClientId(10));
    let b = new_client(ClientId(11));
    registry.insert(a);
    registry.insert(b);

    assert_eq!(registry.len(), 2);

    // Kill A and reap.
    registry.disconnect_client_for_tests(ClientId(10));
    registry.reap_dead_for_tests();

    assert_eq!(registry.len(), 1, "dead client removed");
    // No panic / unwrap on the detector after removal.
    let t0 = Instant::now();
    registry.force_active_speaker_tick_for_tests(t0);
}

#[cfg(all(feature = "test-utils", feature = "active-speaker"))]
#[test]
fn relay_client_is_not_elected_dominant_speaker() {
    use oxpulse_sfu_kit::client::test_seed::new_client;
    use oxpulse_sfu_kit::{ClientId, ClientOrigin, Registry};
    use std::time::{Duration, Instant};

    let mut registry = Registry::new_for_tests();

    let local = new_client(ClientId(400));
    let local_id = *local.id;
    registry.insert(local);

    let mut relay = new_client(ClientId(401));
    relay.set_origin(ClientOrigin::RelayFromSfu("edge-eu".to_string()));
    let relay_id = *relay.id;
    registry.insert(relay);

    let now = Instant::now();
    // Relay gets the loudest possible audio (0 = max volume).
    for i in 0..20u64 {
        registry.inject_audio_level_for_tests(relay_id, 0, now + Duration::from_millis(i * 30));
    }
    // Local gets silence.
    for i in 0..20u64 {
        registry.inject_audio_level_for_tests(local_id, 127, now + Duration::from_millis(i * 30));
    }

    let winner = registry.force_active_speaker_tick_for_tests(now + Duration::from_millis(600));

    if let Some(w) = winner {
        assert_ne!(
            w, relay_id,
            "relay client must never be elected dominant speaker"
        );
    }
    // None is also acceptable (no winner when only silent peers exist after relay is excluded).
}
