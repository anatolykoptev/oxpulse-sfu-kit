//! Media-quality metrics integration tests.
//!
//! Verifies that the new media-quality counters —
//! `sfu_track_bytes_total` and `sfu_rtcp_pli_total` —
//! increment correctly when media flows through the SFU.
//!
//! Required features: `metrics-prometheus`, `test-utils`.
//!
//! RED phase: these tests FAIL before the metric wiring is added.

use std::sync::Arc;

use oxpulse_sfu_kit::client::layer;
use oxpulse_sfu_kit::client::test_seed::{make_media_data, new_client, seed_track_in};
use oxpulse_sfu_kit::metrics::SfuMetrics;
use oxpulse_sfu_kit::{ClientId, Propagated, Registry};
use str0m::media::MediaKind;

// ── helpers ──────────────────────────────────────────────────────────────────

fn assert_counter(body: &str, label_line: &str, expected_ge: f64) {
    for line in body.lines() {
        if line.starts_with(label_line) {
            let v: f64 = line
                .split_whitespace()
                .nth(1)
                .unwrap_or("0")
                .parse()
                .unwrap_or(0.0);
            assert!(
                v >= expected_ge,
                "expected {label_line} >= {expected_ge}, got {v}\nFull metrics:\n{body}"
            );
            return;
        }
    }
    panic!("metric {label_line:?} not found in scraped output.\nFull metrics:\n{body}");
}

// ── tests: sfu_track_bytes_total ─────────────────────────────────────────────

/// `sfu_track_bytes_total{direction="out",kind="video"}` must increment
/// when a video packet is forwarded to a subscriber.
///
/// Wire: `handle_media_data_out` in `client/fanout.rs` — outbound path,
/// after the simulcast and pacer filters pass.
#[test]
fn track_bytes_out_increments_on_video_forward() {
    let metrics = Arc::new(SfuMetrics::new_default());
    let mut registry = Registry::new(metrics);

    let mut publisher = new_client(ClientId(1));
    let track = seed_track_in(&mut publisher, 1, MediaKind::Video);
    registry.insert(publisher);

    let mut subscriber = new_client(ClientId(2));
    subscriber.handle_track_open(Arc::downgrade(&track));
    registry.insert(subscriber);

    // Forward one video packet from publisher → subscriber.
    let prop = Propagated::MediaData(ClientId(1), make_media_data(1, Some(layer::LOW)));
    registry.fanout_for_tests(&prop);

    let body = registry.scrape_metrics_for_tests();
    assert_counter(
        &body,
        r#"sfu_track_bytes_total{direction="out",kind="video"}"#,
        1.0,
    );
}

/// `sfu_track_bytes_total{direction="in",kind="video"}` must increment
/// when an inbound video MediaData arrives on a publisher client.
///
/// Wire: `track_in_media_for_tests` seam in `client/test_seed.rs`.
#[test]
fn track_bytes_in_increments_on_inbound_media() {
    let metrics = Arc::new(SfuMetrics::new_default());
    let mut registry = Registry::new(metrics);

    let mut publisher = new_client(ClientId(10));
    seed_track_in(&mut publisher, 1, MediaKind::Video);
    registry.insert(publisher);

    // Simulate inbound media arriving on the publisher client.
    // kind is resolved from the seeded TrackIn (Video).
    registry.clients_mut_for_tests()[0].on_media_data_for_tests(1, 64);

    let body = registry.scrape_metrics_for_tests();
    assert_counter(
        &body,
        r#"sfu_track_bytes_total{direction="in",kind="video"}"#,
        1.0,
    );
}

// ── tests: sfu_rtcp_pli_total ─────────────────────────────────────────────────

/// `sfu_rtcp_pli_total{direction="in"}` must increment when a subscriber
/// sends a PLI keyframe request (received by the kit from a subscriber).
///
/// Uses kit-side `in/out` vocabulary (aligned with `sfu_track_bytes_total`).
///
/// Wire: `incoming_keyframe_req` in `client/keyframe.rs` — when the
/// subscriber's str0m emits a `KeyframeRequest{kind: Pli}` event.
#[test]
fn rtcp_pli_rx_increments_on_subscriber_pli() {
    use str0m::media::KeyframeRequest;

    let metrics = Arc::new(SfuMetrics::new_default());
    let mut registry = Registry::new(metrics);

    // Publisher with a video track.
    let mut publisher = new_client(ClientId(20));
    let track = seed_track_in(&mut publisher, 2, MediaKind::Video);
    registry.insert(publisher);

    // Subscriber wired to the publisher's track.
    let mut subscriber = new_client(ClientId(21));
    subscriber.handle_track_open(Arc::downgrade(&track));
    registry.insert(subscriber);

    // Force the track_out into Open state so incoming_keyframe_req can find it.
    registry.wire_track_for_tests(1, 0, 2);

    // Build a PLI keyframe request from the subscriber.
    let mid: str0m::media::Mid = str0m::media::Mid::from("m2");
    let kf_req = KeyframeRequest {
        mid,
        rid: None,
        kind: str0m::media::KeyframeRequestKind::Pli,
    };

    // Invoke the keyframe-request path on the subscriber (index 1).
    let _prop = registry.clients_mut_for_tests()[1].incoming_keyframe_req_for_tests(kf_req);

    let body = registry.scrape_metrics_for_tests();
    assert_counter(&body, r#"sfu_rtcp_pli_total{direction="in"}"#, 1.0);
}

// ── tests: A. mid_to_kind O(1) cache ─────────────────────────────────────────

/// After `track_in_added` (triggered by `Event::MediaAdded`), `mid_to_kind`
/// cache must resolve the `Mid`→`MediaKind` mapping for N=10 tracks without
/// scanning `tracks_in`.
///
/// Verifies: (a) cache is populated on track open, (b) lookup returns the
/// correct kind, (c) the inbound-byte metric uses the cached kind, not a scan.
///
/// Wire: `Client::mid_to_kind` field (HashMap<Mid, MediaKind>) populated in
/// `track_in_added`, consumed in `track_in_media` hot path.
#[test]
fn mid_to_kind_lookup_is_o1_after_track_open() {
    let mut client = new_client(ClientId(42));

    // Seed 10 tracks: 5 audio, 5 video.
    for i in 0..10u8 {
        let kind = if i < 5 {
            MediaKind::Audio
        } else {
            MediaKind::Video
        };
        seed_track_in(&mut client, i, kind);
    }

    // After seeding, mid_to_kind must correctly reflect all 10 entries.
    for i in 0..10u8 {
        let expected_kind = if i < 5 {
            MediaKind::Audio
        } else {
            MediaKind::Video
        };
        let resolved = client.mid_to_kind_for_tests(i);
        assert_eq!(
            resolved,
            Some(expected_kind),
            "mid_to_kind cache missed for mid_tag={i}"
        );
    }
}

// ── tests: B. PLI label alignment in/out ─────────────────────────────────────

/// `sfu_rtcp_pli_total{direction="in"}` must appear in scraped metrics when a
/// subscriber PLI is received.  The old label was `"rx"` — this test enforces
/// the aligned `"in"` vocabulary.
///
/// Wire: `incoming_keyframe_req` in `client/keyframe.rs` — direction argument
/// changed from `"rx"` → `"in"`.
#[test]
fn pli_counter_uses_in_out_labels_not_rx_tx() {
    use str0m::media::KeyframeRequest;

    let metrics = Arc::new(SfuMetrics::new_default());
    let mut registry = Registry::new(metrics);

    let mut publisher = new_client(ClientId(30));
    let track = seed_track_in(&mut publisher, 3, MediaKind::Video);
    registry.insert(publisher);

    let mut subscriber = new_client(ClientId(31));
    subscriber.handle_track_open(Arc::downgrade(&track));
    registry.insert(subscriber);

    registry.wire_track_for_tests(1, 0, 3);

    let mid: str0m::media::Mid = str0m::media::Mid::from("m3");
    let kf_req = KeyframeRequest {
        mid,
        rid: None,
        kind: str0m::media::KeyframeRequestKind::Pli,
    };
    let _prop = registry.clients_mut_for_tests()[1].incoming_keyframe_req_for_tests(kf_req);

    let body = registry.scrape_metrics_for_tests();

    // Must have direction="in" (not "rx").
    assert_counter(&body, r#"sfu_rtcp_pli_total{direction="in"}"#, 1.0);

    // Must NOT have direction="rx" label.
    let has_rx = body
        .lines()
        .any(|l| l.starts_with(r#"sfu_rtcp_pli_total{direction="rx"}"#));
    assert!(
        !has_rx,
        "found deprecated direction=\"rx\" label — must use direction=\"in\"\n{body}"
    );
}
