//! Phase 7 — per-client `Client.suspended` video-drop end-to-end.
//!
//! Drives `Registry::drive_pacer_for_tests` with synthetic bps readings,
//! fanout-injects video and audio packets, asserts:
//!   1. Suspended subscriber receives 0 video frames.
//!   2. Suspended subscriber continues receiving audio frames.
//!   3. Counter `sfu_video_frames_dropped_total` increments per dropped video.
//!   4. Counter `sfu_pacer_suspend_video_total{direction="enter"|"exit"}`
//!      increments correctly across transitions.
//!   5. After RestoreAudio, video frames are forwarded again.
//!
//! NOTE on index vs ClientId:
//!   `wire_track_for_tests(sub_idx, pub_idx, mid_tag)` and
//!   `delivered_media_count(idx)` take insertion-order indices.
//!   All fanout helpers take `ClientId` via `Propagated`.

#![cfg(all(feature = "pacer", feature = "test-utils", feature = "metrics-prometheus"))]

use oxpulse_sfu_kit::bwe::SUSPEND_STREAK;
use oxpulse_sfu_kit::client::test_seed::{make_media_data, new_client, seed_track_in};
use oxpulse_sfu_kit::propagate::Propagated;
use oxpulse_sfu_kit::{ClientId, Registry};
use str0m::media::MediaKind;

/// Drive `sub_id`'s pacer into suspended state via `SUSPEND_STREAK` consecutive
/// ticks below `SUSPEND_VIDEO_BPS`.
fn pump_into_suspended(reg: &mut Registry, sub_id: ClientId) {
    for _ in 0..SUSPEND_STREAK {
        reg.drive_pacer_for_tests(sub_id, 5_000);
    }
}

// ── Scenario 1+2: suspended drops video, keeps audio ────────────────────────

#[test]
fn suspended_subscriber_drops_video_keeps_audio() {
    let mut reg = Registry::new_for_tests();

    // Publisher inserted at idx 0, subscriber at idx 1.
    let pub_id = ClientId(800);
    let sub_id = ClientId(801);
    let mut publisher = new_client(pub_id);
    let _video_track = seed_track_in(&mut publisher, 1, MediaKind::Video);
    let _audio_track = seed_track_in(&mut publisher, 2, MediaKind::Audio);
    reg.insert(publisher);
    reg.insert(new_client(sub_id));

    // Wire subscriber (idx 1) to publisher (idx 0) for both tracks.
    reg.wire_track_for_tests(1, 0, 1); // video mid_tag=1
    reg.wire_track_for_tests(1, 0, 2); // audio mid_tag=2

    // Enter suspended state.
    pump_into_suspended(&mut reg, sub_id);
    let _ = reg.drain_propagated_for_tests();

    // Fanout one video frame and one audio frame.
    let video_data = make_media_data(1, None);
    let audio_data = make_media_data(2, None);
    reg.fanout_for_tests(&Propagated::MediaData(pub_id, video_data));
    reg.fanout_for_tests(&Propagated::MediaData(pub_id, audio_data));

    // Subscriber (idx 1): audio delivered (1), video dropped (0).
    let delivered = reg.delivered_media_count(1);
    assert_eq!(
        delivered, 1,
        "suspended subscriber must receive exactly 1 frame (audio only); \
        video must be dropped. got total delivered={delivered}"
    );
}

// ── Scenario 4: enter/exit counter increments exactly once ──────────────────

#[test]
fn suspend_enter_exit_counter_increments() {
    let mut reg = Registry::new_for_tests();
    let sub_id = ClientId(811);
    reg.insert(new_client(sub_id));

    // Enter suspended.
    pump_into_suspended(&mut reg, sub_id);
    // Exit suspended (RestoreAudio at AUDIO_ONLY_BPS = 80_000).
    reg.drive_pacer_for_tests(sub_id, 80_000);

    let after = reg.scrape_metrics_for_tests();
    // Exact-value assertions: trailing \n prevents prefix-collision (e.g.
    // "10" matching "1"). The pacer emits each transition exactly once due
    // to hysteresis; counter must read exactly 1 each.
    assert!(
        after.contains("sfu_pacer_suspend_video_total{direction=\"enter\"} 1\n"),
        "enter counter must equal exactly 1:\n{after}"
    );
    assert!(
        after.contains("sfu_pacer_suspend_video_total{direction=\"exit\"} 1\n"),
        "exit counter must equal exactly 1:\n{after}"
    );
}

// ── Scenario 5: post-RestoreAudio subscriber resumes video ──────────────────

#[test]
fn restored_subscriber_resumes_video() {
    let mut reg = Registry::new_for_tests();

    let pub_id = ClientId(820);
    let sub_id = ClientId(821);
    let mut publisher = new_client(pub_id);
    let _video_track = seed_track_in(&mut publisher, 1, MediaKind::Video);
    reg.insert(publisher);
    reg.insert(new_client(sub_id));
    reg.wire_track_for_tests(1, 0, 1);

    // Enter suspended, then exit (RestoreAudio), then climb to video (RestoreVideo).
    pump_into_suspended(&mut reg, sub_id);
    reg.drive_pacer_for_tests(sub_id, 80_000);  // RestoreAudio: suspended=false
    reg.drive_pacer_for_tests(sub_id, 200_000); // RestoreVideo: audio_only=false
    let _ = reg.drain_propagated_for_tests();

    // Now fanout a video frame — must be delivered.
    let video_data = make_media_data(1, None);
    reg.fanout_for_tests(&Propagated::MediaData(pub_id, video_data));

    let delivered = reg.delivered_media_count(1);
    assert_eq!(
        delivered, 1,
        "post-restore subscriber must receive the video frame; got delivered={delivered}"
    );
}

// ── Scenario 3: frames_dropped counter increments per dropped video ─────────

#[test]
fn frames_dropped_counter_increments_per_dropped_video() {
    let mut reg = Registry::new_for_tests();

    let pub_id = ClientId(830);
    let sub_id = ClientId(831);
    let mut publisher = new_client(pub_id);
    let _video_track = seed_track_in(&mut publisher, 1, MediaKind::Video);
    reg.insert(publisher);
    reg.insert(new_client(sub_id));
    reg.wire_track_for_tests(1, 0, 1);

    // Enter suspended.
    pump_into_suspended(&mut reg, sub_id);
    let _ = reg.drain_propagated_for_tests();

    // Fanout 3 video frames — all must be dropped and counted.
    for _ in 0..3 {
        let data = make_media_data(1, None);
        reg.fanout_for_tests(&Propagated::MediaData(pub_id, data));
    }

    let scrape = reg.scrape_metrics_for_tests();
    // Trailing \n: prevents "30"/"31"/etc. matching the "3" needle.
    assert!(
        scrape.contains("sfu_video_frames_dropped_total 3\n"),
        "video frames dropped counter must equal exactly 3:\n{scrape}"
    );
}

// ── Bonus scenario: GoAudioOnly path must not set Client.suspended ──────────

#[test]
fn audio_only_path_unchanged_by_suspended_filter() {
    let mut reg = Registry::new_for_tests();
    let sub_id = ClientId(841);
    reg.insert(new_client(sub_id));

    // 50_000 bps is between SUSPEND_VIDEO_BPS (10_000) and AUDIO_ONLY_BPS (80_000).
    // This is the audio-only range — no SuspendVideo event should fire AND
    // Client.suspended must remain false (Phase 6's GoAudioOnly path must not
    // accidentally activate Phase 7's drop guard).
    reg.drive_pacer_for_tests(sub_id, 50_000);

    let events = reg.drain_propagated_for_tests();
    let has_suspend = events
        .iter()
        .any(|p| matches!(p, Propagated::SuspendVideo { .. }));
    assert!(
        !has_suspend,
        "no SuspendVideo must be emitted in audio-only range (50_000 bps), got: {events:?}"
    );

    // Direct assertion against the client field — defends against a future
    // refactor that sets Client.suspended in the GoAudioOnly arm without
    // emitting SuspendVideo (which would silently leak the drop guard).
    let client = reg
        .clients_mut_for_tests()
        .iter()
        .find(|c| *c.id == sub_id.0)
        .expect("subscriber present");
    assert!(
        !client.is_suspended(),
        "GoAudioOnly path must not set Client.suspended"
    );
}
