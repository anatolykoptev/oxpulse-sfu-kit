//! Phase 6 — `Propagated::SuspendVideo` end-to-end through the registry.
//!
//! Drives `Registry::drive_pacer_for_tests` with synthetic bps readings and
//! asserts the propagation queue gets the expected events in the expected
//! order. The fanout side (real video-frame drop) is Phase 7 and is NOT
//! covered here.

#![cfg(all(feature = "pacer", feature = "test-utils"))]

use oxpulse_sfu_kit::bwe::SUSPEND_STREAK;
use oxpulse_sfu_kit::client::test_seed::new_client;
use oxpulse_sfu_kit::propagate::Propagated;
use oxpulse_sfu_kit::{ClientId, Registry};

/// Returns `(sub_id, pub_id)` — two clients inserted into the registry.
/// Publisher is present to match realistic registry usage; pacer is driven
/// on the subscriber only.
fn insert_two_clients(reg: &mut Registry) -> (ClientId, ClientId) {
    let pub_id = ClientId(700);
    let sub_id = ClientId(701);
    reg.insert(new_client(pub_id));
    reg.insert(new_client(sub_id));
    (sub_id, pub_id)
}

fn drain_suspend_events(reg: &mut Registry) -> Vec<(ClientId, bool)> {
    reg.drain_propagated_for_tests()
        .into_iter()
        .filter_map(|p| match p {
            Propagated::SuspendVideo { peer_id, suspended } => Some((peer_id, suspended)),
            _ => None,
        })
        .collect()
}

#[test]
fn suspend_then_restore_audio_then_restore_video_emits_events() {
    let mut reg = Registry::new_for_tests();
    let (sub_id, _pub_id) = insert_two_clients(&mut reg);

    // Drop into suspended (< SUSPEND_VIDEO_BPS = 10_000).
    // Requires SUSPEND_STREAK consecutive ticks (F6-3 debounce). After the
    // (SUSPEND_STREAK - 1)th tick, the queue MUST still be empty — only the
    // SUSPEND_STREAKth tick emits SuspendVideo. Verified end-to-end here.
    for _ in 0..(SUSPEND_STREAK - 1) {
        reg.drive_pacer_for_tests(sub_id, 5_000);
        assert!(
            drain_suspend_events(&mut reg).is_empty(),
            "debounce violated: SuspendVideo emitted before SUSPEND_STREAK reached"
        );
    }
    reg.drive_pacer_for_tests(sub_id, 5_000);
    let evs = drain_suspend_events(&mut reg);
    assert_eq!(
        evs,
        vec![(sub_id, true)],
        "expected single SuspendVideo(true)"
    );

    // Climb back to audio-only (>= AUDIO_ONLY_BPS = 80_000, while suspended).
    reg.drive_pacer_for_tests(sub_id, 80_000);
    let evs = drain_suspend_events(&mut reg);
    assert_eq!(
        evs,
        vec![(sub_id, false)],
        "expected single SuspendVideo(false)"
    );

    // Climb above LOW_MIN_BPS (= 150_000) — produces AudioOnlyMode, NOT SuspendVideo.
    reg.drive_pacer_for_tests(sub_id, 200_000);
    let evs = drain_suspend_events(&mut reg);
    assert!(
        evs.is_empty(),
        "no SuspendVideo events expected after RestoreVideo, got {:?}",
        evs
    );
}

#[test]
fn double_suspend_emits_only_once() {
    let mut reg = Registry::new_for_tests();
    let (sub_id, _pub_id) = insert_two_clients(&mut reg);

    // SUSPEND_STREAK ticks below threshold to enter suspended (F6-3 debounce).
    for _ in 0..SUSPEND_STREAK {
        reg.drive_pacer_for_tests(sub_id, 5_000);
    }
    // Additional ticks while already suspended — must not re-emit.
    reg.drive_pacer_for_tests(sub_id, 1_000);
    reg.drive_pacer_for_tests(sub_id, 500);

    let evs = drain_suspend_events(&mut reg);
    assert_eq!(
        evs,
        vec![(sub_id, true)],
        "SuspendVideo must NOT re-emit while already suspended"
    );
}

#[test]
fn drop_video_to_suspend_in_one_tick_skips_audio_only_event() {
    // From a healthy LOW state, a single tick to <SUSPEND_VIDEO_BPS must
    // produce exactly one SuspendVideo event — no AudioOnlyMode in between.
    let mut reg = Registry::new_for_tests();
    let (sub_id, _pub_id) = insert_two_clients(&mut reg);

    // Warm up at LOW (200_000 > LOW_MIN_BPS = 150_000) — clears the queue.
    reg.drive_pacer_for_tests(sub_id, 200_000);
    let _ = reg.drain_propagated_for_tests();

    // SUSPEND_STREAK ticks straight to suspended (F6-3 debounce).
    for _ in 0..SUSPEND_STREAK {
        reg.drive_pacer_for_tests(sub_id, 5_000);
    }

    let all = reg.drain_propagated_for_tests();
    let audio_only_events: Vec<_> = all
        .iter()
        .filter(|p| matches!(p, Propagated::AudioOnlyMode { .. }))
        .collect();
    assert!(
        audio_only_events.is_empty(),
        "must not emit AudioOnlyMode when dropping straight to suspend, got {:?}",
        audio_only_events
    );
    let suspend_events: Vec<_> = all
        .iter()
        .filter_map(|p| match p {
            Propagated::SuspendVideo { peer_id, suspended } => Some((*peer_id, *suspended)),
            _ => None,
        })
        .collect();
    assert_eq!(
        suspend_events,
        vec![(sub_id, true)],
        "expected exactly one SuspendVideo(true)"
    );
}
