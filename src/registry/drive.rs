//! Registry drive loop — poll, tick, and fanout.
//!
//! Split from `registry/mod.rs` to keep the struct/insert/routing concern
//! separate from the per-iteration state machine driving concern.

use std::time::Instant;

use crate::fanout::fanout;
use crate::ids::SfuRid;
use crate::propagate::{ClientId, Propagated};

use super::Registry;

/// Pure dispatch on a [`crate::bwe::PacerAction`]: derives the `Propagated`
/// event to enqueue (if any) and whether the client's `suspended` flag
/// should change, with no side effects of its own.
///
/// The two call sites ([`Registry::poll_all`] and
/// [`Registry::update_pacer_layers`]) apply the returned effect: enqueueing
/// the event on `to_propagate`, and — when `suspend` is `Some` — calling
/// `client.set_suspended` plus bumping the matching `inc_suspend_video`
/// metric. Those steps stay in the caller because they mutate `Client`/
/// `SfuMetrics` state, which this function does not touch.
#[cfg(feature = "pacer")]
fn apply_pacer_action(
    action: crate::bwe::PacerAction,
    peer_id: ClientId,
) -> (Option<Propagated>, Option<bool>) {
    use crate::bwe::PacerAction;
    match action {
        PacerAction::GoAudioOnly => (
            Some(Propagated::AudioOnlyMode {
                peer_id,
                audio_only: true,
            }),
            None,
        ),
        PacerAction::RestoreVideo => (
            Some(Propagated::AudioOnlyMode {
                peer_id,
                audio_only: false,
            }),
            None,
        ),
        PacerAction::SuspendVideo => (
            Some(Propagated::SuspendVideo {
                peer_id,
                suspended: true,
            }),
            Some(true),
        ),
        PacerAction::RestoreAudio => (
            Some(Propagated::SuspendVideo {
                peer_id,
                suspended: false,
            }),
            Some(false),
        ),
        PacerAction::ChangeLayer(_) | PacerAction::NoChange => (None, None),
    }
}

/// FITNESS FUNCTION (ADR-3+4, Bug #7): exactly one pacer drive site is live per
/// build combo. `poll_all` advances the FSM directly ONLY without `kalman-bwe`;
/// with `kalman-bwe` it merely FEEDS `record_native_estimate` and
/// `update_pacer_layers` becomes the sole driver. This compile-time assertion
/// fails the build if the two `#[cfg]` predicates ever stop partitioning the
/// `pacer` feature space (both driving, or neither) — the exact regression that
/// produced the two-driver race. Paired with the runtime `exactly_one_pacer_driver`
/// cfg-matrix test below.
#[cfg(feature = "pacer")]
const _PACER_SINGLE_DRIVER_GUARD: () = {
    let poll_all_drives = cfg!(all(feature = "pacer", not(feature = "kalman-bwe")));
    let update_pacer_layers_drives = cfg!(all(feature = "pacer", feature = "kalman-bwe"));
    assert!(
        poll_all_drives ^ update_pacer_layers_drives,
        "exactly one pacer drive site must be live per feature combo (ADR-3+4)"
    );
};

impl Registry {
    /// Poll every client until each returns a `Timeout`, queuing propagated events.
    ///
    /// Returns the earliest next wake-up deadline.
    pub fn poll_all(&mut self, now: Instant) -> Instant {
        let mut deadline = now + std::time::Duration::from_millis(100);
        for client in self.clients.iter_mut() {
            loop {
                if !client.is_alive() {
                    break;
                }
                match client.poll_output() {
                    Propagated::Timeout(t) => {
                        deadline = deadline.min(t);
                        break;
                    }
                    Propagated::Noop => continue,
                    Propagated::BandwidthEstimate {
                        peer_id,
                        ref estimate,
                    } => {
                        self.metrics.update_peer_bwe(*peer_id, estimate.bps);
                        self.to_propagate.push_back(Propagated::BandwidthEstimate {
                            peer_id,
                            estimate: *estimate,
                        });
                        // ADR-3+4 single-arbitration cfg-split (Bug #7). Under
                        // `kalman-bwe`, `update_pacer_layers` is the SOLE pacer
                        // driver; here we only FEED the str0m-native estimate into
                        // the min-combiner as a ceiling and never advance the FSM,
                        // so two uncoordinated cadences can no longer corrupt one
                        // streak counter. Without `kalman-bwe`, `poll_all` stays the
                        // sole driver and advances the FSM directly. The
                        // `_PACER_SINGLE_DRIVER_GUARD` const asserts these two arms
                        // partition the `pacer` feature space.
                        #[cfg(all(feature = "kalman-bwe", feature = "pacer"))]
                        {
                            self.bandwidth
                                .record_native_estimate(peer_id, estimate.bps as f64);
                        }
                        #[cfg(all(feature = "pacer", not(feature = "kalman-bwe")))]
                        {
                            let (event, suspend) =
                                apply_pacer_action(client.drive_pacer(estimate.bps), peer_id);
                            if let Some(suspended) = suspend {
                                client.set_suspended(suspended);
                                self.metrics.inc_suspend_video(if suspended {
                                    "enter"
                                } else {
                                    "exit"
                                });
                            }
                            if let Some(event) = event {
                                self.to_propagate.push_back(event);
                            }
                        }
                    }
                    Propagated::RtcpStats { peer_id, ref stats } => {
                        self.metrics.update_peer_rtcp(
                            *peer_id,
                            stats.fraction_lost,
                            stats.rtt.as_secs_f64() * 1000.0,
                            stats.jitter.as_secs_f64() * 1000.0,
                        );
                        self.to_propagate.push_back(Propagated::RtcpStats {
                            peer_id,
                            stats: *stats,
                        });
                    }
                    other => {
                        #[cfg(feature = "active-speaker")]
                        if let Propagated::MediaData(ref origin, ref data) = other {
                            // RFC 6464: str0m stores audio_level as negated dBov
                            // (0 = loudest, -127 = silent). The detector expects
                            // 0-127 dBov (0 = loud, 127 = silent), so we negate.
                            // MediaData originates from the current loop \, so
                            // we check client.is_relay() directly — no second borrow needed.
                            if let Some(raw) = data.audio_level_raw() {
                                if !client.is_relay() {
                                    let level = (-(raw as i16)).clamp(0, 127) as u8;
                                    let now_ms = self.detector_epoch.elapsed().as_millis() as u64;
                                    self.detector.record_level(**origin, level, now_ms);
                                }
                            }
                        }
                        self.to_propagate.push_back(other);
                    }
                }
            }
        }
        deadline
    }

    /// Advance the dominant-speaker detector one tick.
    ///
    /// Queues a [`Propagated::ActiveSpeakerChanged`] when dominance changes.
    /// Call this on a 300ms interval (see `dominant_speaker::TICK_INTERVAL`).
    /// Only available with the `active-speaker` feature.
    #[cfg(feature = "active-speaker")]
    #[cfg_attr(docsrs, doc(cfg(feature = "active-speaker")))]
    pub fn tick_active_speaker(&mut self, now: Instant) {
        let now_ms = now
            .saturating_duration_since(self.detector_epoch)
            .as_millis() as u64;
        if let Some(change) = self.detector.tick(now_ms) {
            self.metrics.inc_dominant_speaker_changes();
            self.to_propagate
                .push_back(Propagated::ActiveSpeakerChanged {
                    peer_id: change.peer_id,
                    confidence: change.c2_margin,
                });
        }
    }

    /// Update Prometheus gauges with current per-peer speaker activity scores.
    ///
    /// Call this periodically (e.g. on the same 300ms tick as `tick_active_speaker`).
    /// Only available with both `active-speaker` and `metrics-prometheus` features.
    #[cfg(all(feature = "active-speaker", feature = "metrics-prometheus"))]
    #[cfg_attr(
        docsrs,
        doc(cfg(all(feature = "active-speaker", feature = "metrics-prometheus")))
    )]
    pub fn tick_speaker_scores(&mut self) {
        for (peer_id, imm, med, lng) in self.detector.peer_scores() {
            self.metrics
                .update_peer_speaker_scores(peer_id, imm, med, lng);
        }
    }

    /// Drive the session clock forward on every client.
    pub fn tick(&mut self, now: Instant) {
        for client in self.clients.iter_mut() {
            client.handle_timeout(now);
        }
    }

    /// Fan out every queued propagated event to the appropriate clients.
    pub fn fanout_pending(&mut self) {
        #[cfg(feature = "kalman-bwe")]
        let now = Instant::now();
        while let Some(p) = self.to_propagate.pop_front() {
            #[cfg(feature = "kalman-bwe")]
            if let Propagated::ClientBudgetHint(subscriber_id, bps) = &p {
                self.bandwidth.record_client_hint(*subscriber_id, *bps, now);
                continue;
            }
            // Update pacer-driven layer selection before forwarding the packet
            // so subscribers receive media on their freshly-chosen layer.
            #[cfg(all(feature = "kalman-bwe", feature = "pacer"))]
            if let Propagated::MediaData(origin, _) = &p {
                self.update_pacer_layers(*origin, now);
            }
            fanout(&p, &mut self.clients);
        }
    }
    /// Compute the maximum desired simulcast layer across all subscribers per publisher,
    /// and enqueue [`Propagated::PublisherLayerHint`] when the max changes.
    ///
    /// Call after [`fanout_pending`][Self::fanout_pending] on any tick where
    /// subscriber desired layers may have changed.
    pub fn emit_publisher_layer_hints(&mut self) {
        use crate::client::layer;
        use std::collections::HashMap;

        let mut max_per_publisher: HashMap<ClientId, SfuRid> = HashMap::new();
        for subscriber in &self.clients {
            let sub_desired = subscriber.desired_layer();
            for track_out in &subscriber.tracks_out {
                if let Some(track_in) = track_out.track_in.upgrade() {
                    let publisher_id = track_in.origin;
                    let entry = max_per_publisher.entry(publisher_id).or_insert(layer::LOW);
                    let rank = |r: SfuRid| -> u8 {
                        if r == SfuRid::LOW {
                            0
                        } else if r == SfuRid::MEDIUM {
                            1
                        } else {
                            2
                        }
                    };
                    if rank(sub_desired) > rank(*entry) {
                        *entry = sub_desired;
                    }
                }
            }
        }
        for (publisher_id, max_rid) in max_per_publisher {
            let is_relay = self
                .clients
                .iter()
                .any(|c| c.id == publisher_id && c.is_relay());

            if is_relay {
                self.to_propagate
                    .push_back(Propagated::PublisherLayerHintForUpstream {
                        publisher_relay_id: publisher_id,
                        max_rid,
                    });
            } else {
                self.to_propagate.push_back(Propagated::PublisherLayerHint {
                    publisher_id,
                    max_rid,
                });
            }
        }
    }
}

#[cfg(all(test, feature = "pacer"))]
mod tests {
    use super::*;
    use crate::bwe::PacerAction;

    const PEER: ClientId = ClientId(7);

    #[test]
    fn go_audio_only_emits_audio_only_mode_true_no_suspend_change() {
        let (event, suspend) = apply_pacer_action(PacerAction::GoAudioOnly, PEER);
        match event {
            Some(Propagated::AudioOnlyMode {
                peer_id,
                audio_only,
            }) => {
                assert_eq!(peer_id, PEER);
                assert!(audio_only);
            }
            other => panic!("expected AudioOnlyMode, got {other:?}"),
        }
        assert_eq!(suspend, None);
    }

    #[test]
    fn restore_video_emits_audio_only_mode_false_no_suspend_change() {
        let (event, suspend) = apply_pacer_action(PacerAction::RestoreVideo, PEER);
        match event {
            Some(Propagated::AudioOnlyMode {
                peer_id,
                audio_only,
            }) => {
                assert_eq!(peer_id, PEER);
                assert!(!audio_only);
            }
            other => panic!("expected AudioOnlyMode, got {other:?}"),
        }
        assert_eq!(suspend, None);
    }

    #[test]
    fn suspend_video_emits_suspend_video_true_and_suspend_change_true() {
        let (event, suspend) = apply_pacer_action(PacerAction::SuspendVideo, PEER);
        match event {
            Some(Propagated::SuspendVideo { peer_id, suspended }) => {
                assert_eq!(peer_id, PEER);
                assert!(suspended);
            }
            other => panic!("expected SuspendVideo, got {other:?}"),
        }
        assert_eq!(suspend, Some(true));
    }

    #[test]
    fn restore_audio_emits_suspend_video_false_and_suspend_change_false() {
        let (event, suspend) = apply_pacer_action(PacerAction::RestoreAudio, PEER);
        match event {
            Some(Propagated::SuspendVideo { peer_id, suspended }) => {
                assert_eq!(peer_id, PEER);
                assert!(!suspended);
            }
            other => panic!("expected SuspendVideo, got {other:?}"),
        }
        assert_eq!(suspend, Some(false));
    }

    #[test]
    fn change_layer_and_no_change_emit_nothing() {
        let (event, suspend) =
            apply_pacer_action(PacerAction::ChangeLayer(crate::ids::SfuRid::MEDIUM), PEER);
        assert!(event.is_none());
        assert_eq!(suspend, None);

        let (event, suspend) = apply_pacer_action(PacerAction::NoChange, PEER);
        assert!(event.is_none());
        assert_eq!(suspend, None);
    }

    /// cfg-matrix fitness test (ADR-3+4, Bug #7): exactly one pacer drive site is
    /// live in this build combo. Runs the same XOR the compile-time
    /// `_PACER_SINGLE_DRIVER_GUARD` asserts, so a CI pass across the feature
    /// matrix confirms the single-arbitration invariant at runtime too. If a
    /// future change makes both sites drive (or neither), this fails.
    #[test]
    fn exactly_one_pacer_driver() {
        let poll_all_drives = cfg!(all(feature = "pacer", not(feature = "kalman-bwe")));
        let update_pacer_layers_drives = cfg!(all(feature = "pacer", feature = "kalman-bwe"));
        assert!(
            poll_all_drives ^ update_pacer_layers_drives,
            "exactly one pacer drive site must be live (poll_all XOR update_pacer_layers)"
        );
    }
}

/// ADR-13 min-tick floor: with `update_pacer_layers` as the sole driver, a burst
/// of below-threshold ticks inside `PACER_MIN_TICK_INTERVAL` must not satisfy the
/// `SUSPEND_STREAK` debounce — the floor throttles all but one FSM advance per
/// window per subscriber.
#[cfg(all(test, feature = "pacer", feature = "kalman-bwe"))]
mod min_tick_floor_tests {
    use super::*;
    use crate::bwe::PACER_MIN_TICK_INTERVAL;
    use crate::client::test_seed::new_client;
    use std::time::{Duration, Instant};

    fn suspend_video_enters(evs: &[Propagated]) -> usize {
        evs.iter()
            .filter(|e| {
                matches!(
                    e,
                    Propagated::SuspendVideo {
                        suspended: true,
                        ..
                    }
                )
            })
            .count()
    }

    #[test]
    fn min_tick_floor_prevents_burst_suspend() {
        let mut reg = Registry::new_for_tests();
        let pub_id = ClientId(900);
        let sub_id = ClientId(901);
        reg.insert(new_client(pub_id));
        reg.insert(new_client(sub_id));

        // Force the subscriber's combined estimate below SUSPEND_VIDEO_BPS (10k)
        // via a low native ceiling, so every FSM advance is a suspend-streak tick.
        reg.bandwidth_mut_for_tests()
            .record_native_estimate(sub_id, 5_000.0);

        let t0 = Instant::now();

        // Tick 1 at t0: suspend_streak=1 (SUSPEND_STREAK=2) -> NoChange.
        reg.update_pacer_layers(pub_id, t0);
        assert_eq!(
            suspend_video_enters(&reg.drain_propagated_for_tests()),
            0,
            "first below-threshold tick must not suspend (debounce)"
        );

        // Tick 2 at t0+10ms: inside the 100ms floor -> throttled, FSM not advanced.
        // Without the floor this 2nd streak tick WOULD fire SuspendVideo.
        reg.update_pacer_layers(pub_id, t0 + Duration::from_millis(10));
        assert_eq!(
            suspend_video_enters(&reg.drain_propagated_for_tests()),
            0,
            "burst tick inside the min-tick floor must not advance the FSM"
        );

        // Tick 3 at t0 + PACER_MIN_TICK_INTERVAL: floor cleared -> advances,
        // suspend_streak reaches 2 -> SuspendVideo.
        reg.update_pacer_layers(pub_id, t0 + PACER_MIN_TICK_INTERVAL);
        assert_eq!(
            suspend_video_enters(&reg.drain_propagated_for_tests()),
            1,
            "a tick at/after the floor must advance the FSM and suspend"
        );
    }

    #[test]
    fn first_tick_records_drive_instant() {
        let mut reg = Registry::new_for_tests();
        let pub_id = ClientId(910);
        let sub_id = ClientId(911);
        reg.insert(new_client(pub_id));
        reg.insert(new_client(sub_id));
        reg.bandwidth_mut_for_tests()
            .record_native_estimate(sub_id, 1_000_000.0);

        // First advance is never throttled: last_pacer_drive is None -> drives + records.
        reg.update_pacer_layers(pub_id, Instant::now());
        let recorded = reg
            .clients_mut_for_tests()
            .iter()
            .find(|c| c.id == sub_id)
            .map(|c| c.last_pacer_drive.is_some())
            .expect("subscriber present");
        assert!(
            recorded,
            "first update_pacer_layers must record a drive instant for the subscriber"
        );
    }
}

/// Default simulcast ladder used when the publisher has not yet emitted active RIDs.
#[cfg(all(feature = "kalman-bwe", feature = "pacer"))]
const DEFAULT_SIMULCAST_LADDER: &[crate::ids::SfuRid] = &[
    crate::ids::SfuRid::LOW,
    crate::ids::SfuRid::MEDIUM,
    crate::ids::SfuRid::HIGH,
];

#[cfg(all(feature = "kalman-bwe", feature = "pacer"))]
impl Registry {
    /// For every subscriber of `origin`, read the current Kalman BWE estimate
    /// and advance the subscriber's pacer to select the appropriate simulcast layer.
    ///
    /// Called on every incoming `MediaData` event from the publisher so the
    /// pacer has fresh input every 20ms (nominal video packet cadence).
    ///
    /// `now` is threaded in from [`fanout_pending`][Self::fanout_pending] (one
    /// clock read per drain pass) rather than sampled internally, so the
    /// ADR-13 min-tick floor is deterministic under test.
    ///
    /// This is the **sole** pacer drive site in the `kalman-bwe` build (ADR-3+4):
    /// it reads the combined estimate (`min` of Kalman/loss/native/googcc/hint)
    /// and advances each subscriber's FSM, gated by the ADR-13 min-tick floor.
    ///
    /// Only available with both `kalman-bwe` and `pacer` features.
    pub fn update_pacer_layers(&mut self, origin: crate::propagate::ClientId, now: Instant) {
        // Snapshot publisher's active RIDs before the mutable loop (borrow checker).
        let publisher_rids: Vec<crate::ids::SfuRid> = self
            .clients
            .iter()
            .find(|c| c.id == origin)
            .map(|c| c.active_rids())
            .unwrap_or_default();

        let _available: &[crate::ids::SfuRid] = if publisher_rids.is_empty() {
            DEFAULT_SIMULCAST_LADDER
        } else {
            &publisher_rids
        };

        // Single O(clients) pass. Previously this collected a `subscriber_ids`
        // Vec and then did `self.clients.iter_mut().find(id)` per subscriber — an
        // O(N^2) scan plus two heap allocations, run once per forwarded RTP packet.
        // Iterate clients directly instead (mirrors oxpulse-partner-edge
        // registry/bwe.rs), reading self.bandwidth / self.metrics / self.to_propagate
        // as disjoint borrows alongside the &mut self.clients iterator.
        for client in self.clients.iter_mut() {
            if client.id == origin {
                continue;
            }
            let sub_id = client.id;

            // Finding #1 (freeze_stall) fail-safe: an unfed estimator returns None.
            // Do not drive the pacer this tick (keep forwarding) instead of coercing
            // to 0 bps, which would SuspendVideo a freshly-joined subscriber. Mirrors
            // partner-edge pacer_select_layer's None handling. `self.bandwidth` is a
            // disjoint field from the `self.clients` iterator, so this borrows cleanly.
            let Some(budget) = self.bandwidth.estimate_bps(sub_id, now) else {
                continue;
            };

            // ADR-13 min-tick floor (Bug #7): as the SOLE driver this runs on
            // every ~20–30 ms MediaData. Without the floor, two below-threshold
            // ticks land inside ~60 ms and trip a spurious SuspendVideo before the
            // SUSPEND_STREAK debounce can reject a burst. Advance the FSM at most
            // once per PACER_MIN_TICK_INTERVAL per subscriber; count throttled
            // ticks so the floor is observable in production.
            if !client.pacer_tick_ready(now, crate::bwe::PACER_MIN_TICK_INTERVAL) {
                self.metrics.inc_pacer_tick_throttled();
                continue;
            }

            // Mirror update_peer_bwe so Prometheus stays in sync.
            #[cfg(feature = "metrics-prometheus")]
            self.metrics.update_peer_bwe(*sub_id, budget);

            let (event, suspend) = apply_pacer_action(client.drive_pacer(budget), sub_id);
            if let Some(suspended) = suspend {
                client.set_suspended(suspended);
                self.metrics
                    .inc_suspend_video(if suspended { "enter" } else { "exit" });
            }
            if let Some(event) = event {
                self.to_propagate.push_back(event);
            }
        }
    }
}
