//! Phase 4 — parametrized deterministic pacer/BWE scenario harness (ADR-10).
//!
//! **Acceptance gate** for the E1 v0.12.0 Registry-integrated BWE-feed +
//! single-arbitration pacer-drive orchestration
//! (`docs/../2026-07-08-registry-bwe-feed-pacer-drive.md`), which the plan
//! explicitly marks PRODUCTION-UNPROVEN: oxpulse-partner-edge forked around
//! the kit's `Registry`/`Client` abstraction and only validates the
//! primitives-layer wiring pattern. This file is the in-kit confidence gate
//! for the abstraction itself.
//!
//! Three feature-combo modules mirror `.github/workflows/ci.yml`'s clippy
//! matrix naming (`pacer-only`, `kalman-bwe+pacer`, `all-features`). Each
//! drives the SAME entry point production wiring uses in that combo — not a
//! bypass seam — so a regression that breaks the *real* call site is caught:
//!
//! - `pacer_only`   — `pacer` alone (`kalman-bwe` off). `poll_all`'s
//!   `#[cfg(all(feature = "pacer", not(feature = "kalman-bwe")))]` arm
//!   (`registry/drive.rs`) drives `Client::drive_pacer` directly with no
//!   intermediate combiner. `Registry::drive_pacer_for_tests` calls the exact
//!   same `Client::drive_pacer` method with the exact same effect, so it
//!   stands in for that arm deterministically (str0m's own
//!   `Propagated::BandwidthEstimate` production is not synthesizable without
//!   a live `Rtc` — see the module doc below for that gap).
//!
//! - `kalman_pacer` — `pacer` + `kalman-bwe` (`googcc-bwe` off).
//!   `Registry::update_pacer_layers` is the SOLE driver (ADR-3+4); driven
//!   directly with an explicit `Instant`, mirroring the deterministic-clock
//!   seam `registry/drive.rs`'s own `min_tick_floor_tests` module already
//!   uses — NOT via `fanout_pending`, which samples wall-clock
//!   `Instant::now()` internally and is therefore unusable for a
//!   no-sleep/no-wall-clock harness.
//!
//! - `all_features` — adds `googcc-bwe`. Same sole driver; the GoogCC
//!   ceiling is fed via `Registry::googcc_ceiling_for_subscriber_mut` — the
//!   exact call (`gcc.on_receive(...)` / `force_bps_for_tests` for exact
//!   values) `poll_all`'s RTP-timing auto-feed makes downstream of
//!   `enable_googcc_for_subscriber`. The RTP-timestamp *extraction* itself
//!   (arrival/send-ms sampled from a real video `MediaData` inside
//!   `poll_all`) is NOT exercised here — that requires a live str0m session.
//!   Flagged as the harness's one open gap (Phase 6 / live validation).
//!
//! Deterministic: every tick's clock is an explicit `Instant` derived from a
//! fixed `t0`; every estimate is injected via `record_native_estimate` /
//! `force_bps_for_tests` / `record_send_time` + `on_twcc_feedback` /
//! `drive_pacer_for_tests`. No `sleep`, no wall-clock read inside a
//! scenario body, no randomness.
//!
//! Run across the matrix (mirrors `.github/workflows/ci.yml`'s clippy
//! feature sets; at least one leg below runs `--release`):
//!
//! ```text
//! cargo nextest run -p oxpulse-sfu-kit --no-default-features \
//!   --features pacer,test-utils --test pacer_bwe_scenarios
//! cargo nextest run -p oxpulse-sfu-kit --no-default-features \
//!   --features pacer,kalman-bwe,test-utils --test pacer_bwe_scenarios
//! cargo nextest run -p oxpulse-sfu-kit --release --all-features \
//!   --test pacer_bwe_scenarios
//! ```

#![cfg(all(feature = "pacer", feature = "test-utils"))]

use oxpulse_sfu_kit::client::test_seed::new_client;
use oxpulse_sfu_kit::{ClientId, Propagated, Registry};

/// Insert a bare publisher/subscriber pair with no negotiated tracks.
///
/// `update_pacer_layers`/`drive_pacer_for_tests` only need the subscriber to
/// be present; `pub_id` stands in for the fanout origin the real call sites
/// thread through (`update_pacer_layers(origin, now)` / `poll_all`'s
/// per-client loop).
fn insert_pair(reg: &mut Registry, pub_id: ClientId, sub_id: ClientId) {
    reg.insert(new_client(pub_id));
    reg.insert(new_client(sub_id));
}

/// Extract `(peer_id, suspended)` pairs from a drained event batch.
fn suspend_events(evs: &[Propagated]) -> Vec<(ClientId, bool)> {
    evs.iter()
        .filter_map(|p| match p {
            Propagated::SuspendVideo { peer_id, suspended } => Some((*peer_id, *suspended)),
            _ => None,
        })
        .collect()
}

/// Extract `(peer_id, audio_only)` pairs from a drained event batch.
fn audio_only_events(evs: &[Propagated]) -> Vec<(ClientId, bool)> {
    evs.iter()
        .filter_map(|p| match p {
            Propagated::AudioOnlyMode {
                peer_id,
                audio_only,
            } => Some((*peer_id, *audio_only)),
            _ => None,
        })
        .collect()
}

// ── pacer-only: poll_all's direct-drive arm (kalman-bwe off) ───────────────

#[cfg(not(feature = "kalman-bwe"))]
mod pacer_only {
    use super::{audio_only_events, insert_pair, suspend_events};
    use oxpulse_sfu_kit::bwe::{AUDIO_ONLY_BPS, LOW_MIN_BPS, SUSPEND_STREAK, SUSPEND_VIDEO_BPS};
    use oxpulse_sfu_kit::{ClientId, Registry};

    /// Documents, at the integration-test boundary, which arm is authoritative
    /// in THIS build — the black-box counterpart to `drive.rs`'s
    /// `_PACER_SINGLE_DRIVER_GUARD` compile-time assert and
    /// `exactly_one_pacer_driver` unit test (ADR-3+4).
    #[test]
    fn single_arbitration_poll_all_is_sole_driver_in_this_combo() {
        // `let`-bound (not an inline `cfg!(...)` in `assert!`) so clippy's
        // `assertions_on_constants` doesn't flag this build-matrix sanity
        // check -- mirrors `drive.rs`'s own `exactly_one_pacer_driver` test.
        let poll_all_is_sole_driver = cfg!(all(feature = "pacer", not(feature = "kalman-bwe")));
        assert!(
            poll_all_is_sole_driver,
            "pacer-only combo must have poll_all's direct-drive arm as the \
             sole pacer drive site"
        );
    }

    /// Sustained low bandwidth (SUSPEND_STREAK consecutive below-threshold
    /// ticks) suspends video exactly once.
    #[test]
    fn sustained_low_bandwidth_suspends_after_streak() {
        let mut reg = Registry::new_for_tests();
        let (pub_id, sub_id) = (ClientId(1_000), ClientId(1_001));
        insert_pair(&mut reg, pub_id, sub_id);

        for _ in 0..SUSPEND_STREAK - 1 {
            reg.drive_pacer_for_tests(sub_id, SUSPEND_VIDEO_BPS - 1);
            assert!(
                suspend_events(&reg.drain_propagated_for_tests()).is_empty(),
                "must not suspend before SUSPEND_STREAK is reached"
            );
        }
        reg.drive_pacer_for_tests(sub_id, SUSPEND_VIDEO_BPS - 1);
        let evs = suspend_events(&reg.drain_propagated_for_tests());
        assert_eq!(
            evs,
            vec![(sub_id, true)],
            "expected exactly one SuspendVideo(true) at the SUSPEND_STREAK-th tick"
        );
    }

    /// A transient single below-threshold tick, recovered before the streak
    /// completes, must NOT suspend. In this combo the SUSPEND_STREAK debounce
    /// alone is the transient-burst guard — the ADR-13 min-tick floor is
    /// `kalman-bwe`-only (poll_all's ~100ms native `BandwidthEstimate`
    /// cadence is already spaced wider than the floor would enforce).
    #[test]
    fn transient_single_tick_burst_does_not_suspend() {
        let mut reg = Registry::new_for_tests();
        let (pub_id, sub_id) = (ClientId(1_010), ClientId(1_011));
        insert_pair(&mut reg, pub_id, sub_id);

        reg.drive_pacer_for_tests(sub_id, SUSPEND_VIDEO_BPS - 1);
        let _ = reg.drain_propagated_for_tests();
        reg.drive_pacer_for_tests(sub_id, LOW_MIN_BPS + 1);
        let evs = suspend_events(&reg.drain_propagated_for_tests());
        assert!(
            evs.is_empty(),
            "a single transient below-threshold tick must not suspend: {evs:?}"
        );
    }

    /// Recovery path: RestoreAudio then RestoreVideo fire in order, and
    /// `Client.suspended` clears.
    #[test]
    fn recovery_restores_audio_then_video() {
        let mut reg = Registry::new_for_tests();
        let (pub_id, sub_id) = (ClientId(1_020), ClientId(1_021));
        insert_pair(&mut reg, pub_id, sub_id);

        for _ in 0..SUSPEND_STREAK {
            reg.drive_pacer_for_tests(sub_id, SUSPEND_VIDEO_BPS - 1);
        }
        let evs = suspend_events(&reg.drain_propagated_for_tests());
        assert_eq!(
            evs,
            vec![(sub_id, true)],
            "must be suspended before recovery"
        );

        reg.drive_pacer_for_tests(sub_id, AUDIO_ONLY_BPS);
        let evs = suspend_events(&reg.drain_propagated_for_tests());
        assert_eq!(evs, vec![(sub_id, false)], "RestoreAudio must fire");

        reg.drive_pacer_for_tests(sub_id, LOW_MIN_BPS + 1);
        let evs = audio_only_events(&reg.drain_propagated_for_tests());
        assert_eq!(
            evs,
            vec![(sub_id, false)],
            "RestoreVideo (AudioOnlyMode false) must fire"
        );

        let client = reg
            .clients_mut_for_tests()
            .iter()
            .find(|c| c.id == sub_id)
            .expect("subscriber present");
        assert!(
            !client.is_suspended(),
            "suspended flag must clear after full recovery"
        );
    }
}

// ── kalman-bwe+pacer: update_pacer_layers is the sole driver ───────────────

#[cfg(all(feature = "kalman-bwe", not(feature = "googcc-bwe")))]
mod kalman_pacer {
    use super::{audio_only_events, insert_pair, suspend_events};
    use oxpulse_sfu_kit::bwe::{
        AUDIO_ONLY_BPS, LOW_MIN_BPS, PACER_MIN_TICK_INTERVAL, SUSPEND_VIDEO_BPS,
    };
    use oxpulse_sfu_kit::client::test_seed::new_client;
    use oxpulse_sfu_kit::{ClientId, Registry, TwccFeedback, TwccSample};
    use std::time::{Duration, Instant};

    #[test]
    fn single_arbitration_update_pacer_layers_is_sole_driver_in_this_combo() {
        let update_pacer_layers_is_sole_driver =
            cfg!(all(feature = "pacer", feature = "kalman-bwe"));
        assert!(
            update_pacer_layers_is_sole_driver,
            "kalman-bwe+pacer combo must have update_pacer_layers as the sole \
             pacer drive site"
        );
    }

    /// T1 cold-start fail-safe (ADR-7, Bug #1 freeze_stall): before any
    /// estimate is fed, `estimate_bps` returns `None` and
    /// `update_pacer_layers` must skip driving entirely — NOT coerce to 0
    /// bps, which would `SuspendVideo` a freshly-joined subscriber.
    #[test]
    fn cold_start_without_estimate_does_not_drive_or_suspend() {
        let mut reg = Registry::new_for_tests();
        let (pub_id, sub_id) = (ClientId(2_000), ClientId(2_001));
        insert_pair(&mut reg, pub_id, sub_id);

        reg.update_pacer_layers(pub_id, Instant::now());
        let evs = reg.drain_propagated_for_tests();
        assert!(
            evs.is_empty(),
            "cold start (no fed estimate) must not emit any pacer event: {evs:?}"
        );

        let client = reg
            .clients_mut_for_tests()
            .iter()
            .find(|c| c.id == sub_id)
            .expect("subscriber present");
        assert!(!client.is_suspended(), "cold start must not set suspended");
    }

    /// ADR-13: the min-tick floor suppresses a transient below-threshold
    /// burst (two ticks inside `PACER_MIN_TICK_INTERVAL`), but a SUSTAINED
    /// low-bandwidth condition (ticks spaced at/after the floor) still
    /// suspends. Both halves of the invariant in one scenario.
    #[test]
    fn min_tick_floor_suppresses_burst_but_sustained_low_bw_still_suspends() {
        let mut reg = Registry::new_for_tests();
        let (pub_id, sub_id) = (ClientId(2_010), ClientId(2_011));
        insert_pair(&mut reg, pub_id, sub_id);
        reg.bandwidth_mut_for_tests()
            .record_native_estimate(sub_id, (SUSPEND_VIDEO_BPS - 1) as f64);

        let t0 = Instant::now();
        reg.update_pacer_layers(pub_id, t0);
        assert!(
            suspend_events(&reg.drain_propagated_for_tests()).is_empty(),
            "1st tick: suspend_streak=1, must not suspend yet"
        );

        // Burst: a 2nd tick 10ms later is inside the 100ms floor — throttled,
        // FSM not advanced. Without the floor this WOULD complete the
        // SUSPEND_STREAK=2 debounce and fire a spurious suspend.
        reg.update_pacer_layers(pub_id, t0 + Duration::from_millis(10));
        assert!(
            suspend_events(&reg.drain_propagated_for_tests()).is_empty(),
            "burst tick inside the min-tick floor must not advance the FSM"
        );

        // Sustained: bandwidth is STILL low, and the next tick clears the
        // floor — the FSM advances and the debounce completes.
        reg.update_pacer_layers(pub_id, t0 + PACER_MIN_TICK_INTERVAL);
        let evs = suspend_events(&reg.drain_propagated_for_tests());
        assert_eq!(
            evs,
            vec![(sub_id, true)],
            "a floor-cleared tick under sustained low bandwidth must suspend"
        );
    }

    /// Recovery: RestoreAudio then RestoreVideo fire in order through the
    /// real sole-driver call site, each floor-spaced tick apart.
    #[test]
    fn recovery_restores_audio_then_video_through_sole_driver() {
        let mut reg = Registry::new_for_tests();
        let (pub_id, sub_id) = (ClientId(2_020), ClientId(2_021));
        insert_pair(&mut reg, pub_id, sub_id);
        reg.bandwidth_mut_for_tests()
            .record_native_estimate(sub_id, (SUSPEND_VIDEO_BPS - 1) as f64);

        let t0 = Instant::now();
        reg.update_pacer_layers(pub_id, t0);
        reg.update_pacer_layers(pub_id, t0 + PACER_MIN_TICK_INTERVAL);
        let evs = suspend_events(&reg.drain_propagated_for_tests());
        assert_eq!(
            evs,
            vec![(sub_id, true)],
            "must be suspended before recovery"
        );

        reg.bandwidth_mut_for_tests()
            .record_native_estimate(sub_id, AUDIO_ONLY_BPS as f64);
        reg.update_pacer_layers(pub_id, t0 + PACER_MIN_TICK_INTERVAL * 2);
        let evs = suspend_events(&reg.drain_propagated_for_tests());
        assert_eq!(evs, vec![(sub_id, false)], "RestoreAudio must fire");

        reg.bandwidth_mut_for_tests()
            .record_native_estimate(sub_id, (LOW_MIN_BPS + 1) as f64);
        reg.update_pacer_layers(pub_id, t0 + PACER_MIN_TICK_INTERVAL * 3);
        let evs = audio_only_events(&reg.drain_propagated_for_tests());
        assert_eq!(
            evs,
            vec![(sub_id, false)],
            "RestoreVideo (AudioOnlyMode false) must fire"
        );

        let client = reg
            .clients_mut_for_tests()
            .iter()
            .find(|c| c.id == sub_id)
            .expect("subscriber present");
        assert!(
            !client.is_suspended(),
            "suspended flag must clear after full recovery"
        );
    }

    /// Phase-3 auto-feed pattern: a native estimate written via
    /// `record_native_estimate` becomes the ceiling `Registry::bandwidth()`
    /// reports AND the value the sole driver reads on its next tick.
    #[test]
    fn native_estimate_feed_becomes_the_ceiling_the_driver_reads() {
        let mut reg = Registry::new_for_tests();
        let (pub_id, sub_id) = (ClientId(2_030), ClientId(2_031));
        insert_pair(&mut reg, pub_id, sub_id);

        reg.bandwidth_mut_for_tests()
            .record_native_estimate(sub_id, 1_000_000.0);
        let high = reg
            .bandwidth()
            .estimate_bps(sub_id, Instant::now())
            .expect("entry created by record_native_estimate");
        assert!(
            high > LOW_MIN_BPS,
            "a high native estimate must not be pre-capped: {high}"
        );

        reg.bandwidth_mut_for_tests()
            .record_native_estimate(sub_id, 50_000.0);
        let low = reg
            .bandwidth()
            .estimate_bps(sub_id, Instant::now())
            .expect("entry still present");
        assert!(
            low <= 50_100,
            "a lowered native estimate must re-ceiling the combined estimate: {low}"
        );

        let evs = audio_only_events(&{
            reg.update_pacer_layers(pub_id, Instant::now());
            reg.drain_propagated_for_tests()
        });
        assert_eq!(
            evs,
            vec![(sub_id, true)],
            "the lowered ceiling must drive GoAudioOnly on the sole driver's next tick"
        );
    }

    /// `on_twcc combined_bps trajectory` (Phase 4 acceptance bullet): feeding
    /// growing inter-arrival delay through `Registry::on_twcc_feedback` must
    /// actually pull the combined estimate DOWN — not merely avoid a panic.
    #[test]
    fn on_twcc_feedback_trajectory_reflects_congestion_not_just_no_panic() {
        let mut reg = Registry::new_for_tests();
        let sub_id = ClientId(2_040);
        reg.insert(new_client(sub_id));

        let base = Instant::now();
        for (i, seq) in (1u64..=5).enumerate() {
            reg.bandwidth_mut_for_tests().record_send_time(
                sub_id,
                seq,
                base + Duration::from_millis(i as u64 * 10),
            );
        }
        let before = reg
            .bandwidth()
            .estimate_bps(sub_id, base)
            .expect("entry created by record_send_time");

        // 10ms uniform send spacing vs 40ms arrival spacing -> ~30ms gradient
        // per step, well above the 12.5ms overuse threshold, over 4 pairwise
        // deltas -- the Kalman filter's gain is high enough (small initial
        // measurement noise) to converge above threshold within the batch.
        let feedback = TwccFeedback {
            samples: (1u64..=5)
                .enumerate()
                .map(|(i, seq)| TwccSample {
                    seq,
                    arrival: Some(base + Duration::from_millis(50 + i as u64 * 40)),
                })
                .collect(),
        };
        let after_time = base + Duration::from_millis(300);
        reg.on_twcc_feedback(sub_id, &feedback, after_time);
        let after = reg
            .bandwidth()
            .estimate_bps(sub_id, after_time)
            .expect("entry still present");

        assert!(
            after < before,
            "growing inter-arrival delay must decrease the combined estimate \
             (a real trajectory), not merely avoid a panic: before={before}, after={after}"
        );
    }
}

// ── all-features: + GoogCC ceiling auto-feed ────────────────────────────────

#[cfg(all(feature = "kalman-bwe", feature = "googcc-bwe"))]
mod all_features {
    use super::{audio_only_events, insert_pair, suspend_events};
    use oxpulse_sfu_kit::bwe::{AUDIO_ONLY_BPS, PACER_MIN_TICK_INTERVAL, SUSPEND_VIDEO_BPS};
    use oxpulse_sfu_kit::{ClientId, Registry};
    use std::time::Instant;

    #[test]
    fn single_arbitration_survives_googcc_layered_on() {
        let update_pacer_layers_is_sole_driver =
            cfg!(all(feature = "pacer", feature = "kalman-bwe"));
        assert!(
            update_pacer_layers_is_sole_driver,
            "all-features combo must retain update_pacer_layers as the sole \
             drive site even with googcc-bwe (and active-speaker etc.) layered on"
        );
    }

    /// GoogCC auto-feed influences the subscriber's combined estimate and,
    /// through it, the pacer decision (ADR-1/ADR-14 downstream half).
    ///
    /// Feeds the ceiling via `googcc_ceiling_for_subscriber_mut` — the exact
    /// call `Registry::poll_all`'s RTP-timing auto-feed makes internally
    /// after `enable_googcc_for_subscriber`. The RTP-timestamp *extraction*
    /// itself (from a live str0m video `MediaData`) is the one link this
    /// harness cannot exercise deterministically — see the file doc comment.
    #[test]
    fn googcc_ceiling_drives_go_audio_only_through_sole_driver() {
        let mut reg = Registry::new_for_tests();
        let (pub_id, sub_id) = (ClientId(3_000), ClientId(3_001));
        insert_pair(&mut reg, pub_id, sub_id);

        // Native ceiling stays high so it never constrains -- isolates the
        // GoogCC ceiling as the variable under test.
        reg.bandwidth_mut_for_tests()
            .record_native_estimate(sub_id, 1_000_000.0);
        reg.enable_googcc_for_subscriber(sub_id);
        reg.googcc_ceiling_for_subscriber_mut(sub_id)
            .expect("enabled above")
            .force_bps_for_tests(50_000); // between SUSPEND_VIDEO_BPS and AUDIO_ONLY_BPS

        let now = Instant::now();
        let combined = reg
            .bandwidth()
            .estimate_bps(sub_id, now)
            .expect("entry present");
        assert!(
            combined <= 50_100,
            "GoogCC ceiling must cap the combined estimate below the high \
             native ceiling: {combined}"
        );

        reg.update_pacer_layers(pub_id, now);
        let evs = audio_only_events(&reg.drain_propagated_for_tests());
        assert_eq!(
            evs,
            vec![(sub_id, true)],
            "GoogCC-ceilinged estimate must drive GoAudioOnly"
        );
    }

    /// A sustained GoogCC-fed low ceiling still suspends video end-to-end
    /// through the sole driver, and recovers via RestoreAudio once the
    /// ceiling is relaxed.
    #[test]
    fn sustained_googcc_ceiling_suspends_then_recovers() {
        let mut reg = Registry::new_for_tests();
        let (pub_id, sub_id) = (ClientId(3_010), ClientId(3_011));
        insert_pair(&mut reg, pub_id, sub_id);

        reg.bandwidth_mut_for_tests()
            .record_native_estimate(sub_id, 1_000_000.0);
        reg.enable_googcc_for_subscriber(sub_id);
        reg.googcc_ceiling_for_subscriber_mut(sub_id)
            .expect("enabled above")
            .force_bps_for_tests(SUSPEND_VIDEO_BPS - 1);

        let t0 = Instant::now();
        reg.update_pacer_layers(pub_id, t0);
        assert!(
            suspend_events(&reg.drain_propagated_for_tests()).is_empty(),
            "1st tick: suspend_streak=1, must not suspend yet"
        );
        reg.update_pacer_layers(pub_id, t0 + PACER_MIN_TICK_INTERVAL);
        let evs = suspend_events(&reg.drain_propagated_for_tests());
        assert_eq!(
            evs,
            vec![(sub_id, true)],
            "floor-cleared 2nd tick must suspend"
        );

        // Recover: relax the GoogCC ceiling above AUDIO_ONLY_BPS.
        reg.googcc_ceiling_for_subscriber_mut(sub_id)
            .expect("still enabled")
            .force_bps_for_tests(AUDIO_ONLY_BPS);
        reg.update_pacer_layers(pub_id, t0 + PACER_MIN_TICK_INTERVAL * 2);
        let evs = suspend_events(&reg.drain_propagated_for_tests());
        assert_eq!(
            evs,
            vec![(sub_id, false)],
            "relaxed ceiling must RestoreAudio"
        );
    }
}
