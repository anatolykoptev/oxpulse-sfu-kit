# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.12.3](https://github.com/anatolykoptev/oxpulse-sfu-kit/compare/v0.12.2...v0.12.3) (2026-07-17)


### Fixed

* normalize LICENSE to canonical Apache-2.0 text for GitHub license detection ([#55](https://github.com/anatolykoptev/oxpulse-sfu-kit/issues/55)) ([39fe348](https://github.com/anatolykoptev/oxpulse-sfu-kit/commit/39fe3488b8c867de411e4cce422d8b42e6788dc1))

## [0.12.2](https://github.com/anatolykoptev/oxpulse-sfu-kit/compare/v0.12.1...v0.12.2) (2026-07-13)


### Added

* add sfu_combined_bps_binding_term metric ([#53](https://github.com/anatolykoptev/oxpulse-sfu-kit/issues/53)) ([d1103d1](https://github.com/anatolykoptev/oxpulse-sfu-kit/commit/d1103d18e1d326be8884bf73d19a2ec6f1210978))

## [0.12.1](https://github.com/anatolykoptev/oxpulse-sfu-kit/compare/v0.12.0...v0.12.1) (2026-07-12)


### Added

* **bwe:** expose combined_bps binding-term for observability ([#51](https://github.com/anatolykoptev/oxpulse-sfu-kit/issues/51)) ([2398289](https://github.com/anatolykoptev/oxpulse-sfu-kit/commit/2398289fed98ab2560d73c0f30adaceb1f2f9165))

## [0.12.0](https://github.com/anatolykoptev/oxpulse-sfu-kit/compare/v0.11.9...v0.12.0) (2026-07-09)


### ⚠ BREAKING CHANGES

* SfuMediaPayload frame bytes are now Arc<[u8]> (str0m 0.21).

### Added

* migrate to str0m 0.21 with Arc&lt;[u8]&gt; zero-copy media payload (Phase -1) ([9d05414](https://github.com/anatolykoptev/oxpulse-sfu-kit/commit/9d05414c8782ed48b053522a11d44da3a61fd5f7))
* registry bandwidth accessor + googcc pass-throughs + auto-feed on receive (Phase 3) ([a8213ef](https://github.com/anatolykoptev/oxpulse-sfu-kit/commit/a8213eff50a5aa3fed0128e5c4f5d459f6cfbe6d))
* single-arbitration pacer drive with exclusivity guard + min-tick floor (Phase 2, E1 ADR-13) ([03bb6d0](https://github.com/anatolykoptev/oxpulse-sfu-kit/commit/03bb6d09eb481ee90b00132e014353b1844c78ea))


### Documentation

* rewrite API_SPEC ownership/behavior + v0.11.8→v0.12.0 migration guide (Phase 5) ([0894bff](https://github.com/anatolykoptev/oxpulse-sfu-kit/commit/0894bfff8688a0b210b1bdce30e02ea4d92c5296))


### Changed

* extract apply_pacer_action as a pure match + emit hook (Phase 0) ([c33a17a](https://github.com/anatolykoptev/oxpulse-sfu-kit/commit/c33a17aaa3a32d474899c6b1efd63ff09d9dba96))

## [0.11.9](https://github.com/anatolykoptev/oxpulse-sfu-kit/compare/v0.11.8...v0.11.9) (2026-07-09)


### Added

* forward SFrame key-epoch (KID) RTP header extension on fanout ([#42](https://github.com/anatolykoptev/oxpulse-sfu-kit/issues/42)) ([6e1a74a](https://github.com/anatolykoptev/oxpulse-sfu-kit/commit/6e1a74a4dbd53a875e99a01e12e19fd25feac431))

## [0.11.8](https://github.com/anatolykoptev/oxpulse-sfu-kit/compare/v0.11.7...v0.11.8) (2026-07-08)


### Added

* expose GoogCC per-subscriber coverage as a queryable signal (silent_downgrade) ([#35](https://github.com/anatolykoptev/oxpulse-sfu-kit/issues/35)) ([24a87bc](https://github.com/anatolykoptev/oxpulse-sfu-kit/commit/24a87bcbb353aaf9624470b94baafbbe02f46952))


### Fixed

* enforce PacerConfig::validate() in all build profiles (config_drift) ([#32](https://github.com/anatolykoptev/oxpulse-sfu-kit/issues/32)) ([946756a](https://github.com/anatolykoptev/oxpulse-sfu-kit/commit/946756aa996b55d0b1d94d756ba63772ce0547ed))
* evict BWE subscriber state on disconnect (resource_exhaustion) ([#30](https://github.com/anatolykoptev/oxpulse-sfu-kit/issues/30)) ([f319199](https://github.com/anatolykoptev/oxpulse-sfu-kit/commit/f319199d75ef5ae634908b10bbf4ecc4593b808f))
* fail-safe pacer when BWE estimator is unfed (freeze_stall) ([#29](https://github.com/anatolykoptev/oxpulse-sfu-kit/issues/29)) ([800bf33](https://github.com/anatolykoptev/oxpulse-sfu-kit/commit/800bf3349da9f1f95f76d80dcfd826fa29bb6255))
* keep detector add/remove symmetric across late set_origin (config_drift) ([#34](https://github.com/anatolykoptev/oxpulse-sfu-kit/issues/34)) ([113f2e8](https://github.com/anatolykoptev/oxpulse-sfu-kit/commit/113f2e86b192e5553fe6d0549e8d13ae6961779e))


### Performance

* de-quadratic update_pacer_layers hot path (resource_exhaustion) ([#33](https://github.com/anatolykoptev/oxpulse-sfu-kit/issues/33)) ([76cb8d8](https://github.com/anatolykoptev/oxpulse-sfu-kit/commit/76cb8d8fa394e5552b327ddbe83cf201ecffcd92))


### Documentation

* correct SFrame KeyEpoch forwarding contract; rename synthetic-green test ([#31](https://github.com/anatolykoptev/oxpulse-sfu-kit/issues/31)) ([bc58094](https://github.com/anatolykoptev/oxpulse-sfu-kit/commit/bc580949fec1e6ed942a45e95b9e4b37ff2cd634))

## [0.11.7] - 2026-05-19

### Performance
- O(1) `mid → MediaKind` lookup via `HashMap<Mid, MediaKind>` cache (was O(n) linear scan per RTP packet) (#27)

### Breaking
- `sfu_rtcp_pli_total` label `direction="rx"|"tx"` renamed to `"in"|"out"` for consistency with `sfu_track_bytes_total` (#27)
- Consumers (dashboards/alerts) referencing `direction="rx"` or `direction="tx"` on PLI counter must update. Single confirmed consumer = oxpulse-partner-edge v0.11.6; no Grafana dashboards reference old labels (verified by reviewer grep).

### Notes
- Followups not yet shipped: silent `mid_to_kind` cache-miss should log/bump metric (per project CLAUDE.md rule); `mid_to_kind_lookup_is_o1_after_track_open` test name overpromises (functional only, не perf bench).

## [0.11.6] - 2026-05-19

### Added
- `sfu_track_bytes_total{direction,kind}` counter — RTP byte flow per direction/kind (#25)
- `sfu_rtcp_pli_total{direction}` counter — PLI rate by direction (#25)

### Notes
- NACK skipped — str0m 0.18 has no public NACK hook (followup tracked)
- Jitter metric scope was deferred — `peer_jitter_ms` gauge remains placeholder

## [0.11.5] — 2026-05-14

Phase D-Lite: pragmatic perf for current workload (~8-peer rooms).
Three additive features — no API breakage, no MSRV change.

### Added

- **`forward_latency_seconds` Prometheus histogram** in `SfuMetrics`
  (feature `metrics-prometheus`). Captures end-to-end RTP forwarding
  time per packet, observed in `Client::handle_media_data_out`.
  Buckets: 100µs → 100ms (7 buckets). Gives prod visibility into call
  quality regressions before customers report them.
- **`examples/synthetic_room.rs`** — runnable load generator. CLI
  `--peers N --duration-secs S --packet-rate-pps P --bitrate-bps B`.
  Loopback transport (no real DTLS/UDP) — measures kit fanout +
  per-subscriber layer-filter + delivered_media atomics. Reports peak
  RSS, CPU%, packets forwarded, p50/p95/p99 forward latency.
- **`benches/` criterion harness** with v0.11.4 baseline numbers
  (macOS Apple Silicon, in `benches/BASELINE.md`). Regression guard for
  future refactors — bench delta MUST be committed in PR description if
  >5% movement.

### Notes

This release is about operational maturity, not raw perf. The actual
O(n) cliffs identified during the audit (send_times eviction,
Registry::route linear scan) were intentionally NOT addressed — they
don't manifest below ~50 peers/room, which is above current workload
(1:1 to ~8 peers). Will revisit when capacity data shows otherwise.

## [0.11.4] — 2026-05-14

### Added

- **`BandwidthEstimator::enable_googcc_for_subscriber(id)`** (feature
  `googcc-bwe`) — enables the per-subscriber `GoogCcEstimator` from
  outside the crate. Idempotent: calling twice preserves estimator
  state. Closes the consumer-facing gap left by v0.11.0: prior to this,
  `get_or_insert` was `pub(crate)` so consumers could not set
  `PerSubscriber.googcc = Some(...)`. Unblocks `oxpulse-partner-edge`
  PR #116, which previously had to carry a parallel `GoogCcEstimator`
  on its own `Client` struct.
- **`BandwidthEstimator::googcc_for_subscriber_mut(id) -> Option<&mut GoogCcEstimator>`**
  (feature `googcc-bwe`) — mutable accessor for feeding packet timing
  from the consumer's TWCC handler. Returns `None` if the subscriber
  doesn't exist OR GoogCC was never enabled for it.
- **Crate-root re-exports** (audit per code-quality reviewer):
  `BandwidthEstimator`, `PerSubscriber`, `ClientHint`, `DelayEstimator`,
  `LossEstimator` (all gated on `kalman-bwe`); `TrackIn`, `InvalidRid`,
  `RawRtc`, `RawRtcConfig`, `rtc_config`, `udp_loop::bind`,
  `udp_loop::serve`. Consumers no longer need to spell out
  `oxpulse_sfu_kit::bwe::estimator::*` etc. — every documented public
  API is reachable as `oxpulse_sfu_kit::X`.

### Tests

- 5 new tests covering `enable_googcc_for_subscriber`: missing-creates,
  idempotent-preserves-state, accessor-returns-none-when-disabled,
  accessor-returns-none-for-unknown-subscriber, ceiling-applies-to-estimate.
- Doctest on the two new methods showing the canonical wiring recipe.

Total: **196 tests pass**, clippy clean, `cargo fmt --check` clean.
Closes #17.

## [0.11.3] — 2026-05-14

### Changed

- **MSRV bumped: 1.86 → 1.88.** `time@0.3.47` is transitive through
  `str0m → dimpl → time`, and `time` requires rustc 1.88. The v0.11.2
  CHANGELOG claimed `time` was not a transitive dep — that was wrong
  (verified via `cargo tree -i time`). CI matrix on 1.86 broke on
  publish of v0.11.2; this release restores green CI.
- CI matrix MSRV row: `1.86` → `1.88`.
- README MSRV badge: `1.86` → `1.88`.

### Fixed

- `cargo fmt --check` failures in v0.11.2 (auto-applied in
  `examples/twcc_googcc_wiring.rs`, `src/bwe/hysteresis.rs`,
  `src/lib.rs`).

## [0.11.2] — 2026-05-14

### Added

- `PacerConfigError` enum (`UpgradeStreakZero`, `SuspendStreakZero`,
  `BitrateOrderingViolation`) with `Display`. Re-exported at crate root
  (feature `pacer`).
- `PacerConfig::validate(&self) -> Result<(), PacerConfigError>` — validates
  all ordering and streak invariants. Call before passing a custom config to
  `SubscriberPacer::with_config`.
- `TrendlineDetector::state() -> BandwidthState` getter. The `pub state` field
  is now `pub(crate)`; use the getter in consumer code.
- New example `twcc_googcc_wiring` (features: `googcc-bwe,kalman-bwe,test-utils`)
  showing the TWCC relative-delta → absolute-ms timestamp conversion recipe and
  `PerSubscriber` + GoogCC wiring.

### Changed

- `BandwidthState` is now `#[non_exhaustive]`. Consumers with exhaustive `match`
  on this enum must add a `_ =>` arm. This is intentionally a minor breaking
  change while the crate is new — future variants (`ProbationaryOveruse`,
  `Recovering`) will not require a version bump.
- `SubscriberPacer::with_config` now `debug_assert!`s that the config passes
  `validate()`. The assertion fires only in debug builds; release builds are
  unaffected.
- `upgrade_streak += 1` replaced with `saturating_add(1)` (defensive; overflow
  is unreachable in normal flow but guarded against future refactors).
- `TrendlineDetector::deltas` changed from `Vec` to `VecDeque` — `pop_front` is
  O(1) vs O(n) `Vec::remove(0)`.
- `AimdController::update_loss` now clamps `loss_fraction` to `[0.0, 1.0]` and
  `debug_assert!`s finiteness. `GoogCcEstimator::on_receive` `debug_assert!`s
  finite `arrival_ms`/`send_ms`.
- Doc comments improved: `SubscriberPacer::new` / `with_config` doc-link targets
  fixed; `PacerConfig` field docs document the ordering invariant per field;
  `PerSubscriber::googcc` expanded with integration recipe.

### MSRV

Remains **1.86**. The `time` crate is not a transitive dependency of this crate;
the 1.88 bump flagged in the code-quality review was a false alarm.

> **Correction (see v0.11.3):** this claim was wrong. `time@0.3.47` IS
> transitive via `str0m → dimpl → time` and requires rustc 1.88. The
> `cargo tree -i time` check used to derive this section returned empty
> because it was run without the active feature set. v0.11.3 bumps MSRV
> to 1.88 accordingly.

## [0.11.1] — 2026-05-14

### Added

- `From<SfuRid> for str0m::media::Rid` and `From<str0m::media::Rid> for SfuRid` --
  consumers that pattern-match on `PacerAction::ChangeLayer` and need to wire the resulting
  layer into a str0m pipeline no longer need a workaround. Both conversions are zero-cost
  lossless newtypes.
- `From<SfuMid> for str0m::media::Mid` / `From<str0m::media::Mid> for SfuMid` --
  symmetric conversion for stream identifiers.
- `From<SfuPt> for str0m::media::Pt` / `From<str0m::media::Pt> for SfuPt` --
  symmetric conversion for payload types.

### Changed

- `pub(crate)` methods `SfuRid::to_str0m()` / `SfuRid::from_str0m()` (and `SfuMid`, `SfuPt`
  equivalents) retained for internal use; the `From` traits are the stable public API.

## [0.11.0] — 2026-05-14

### Added

- **`SubscriberPacer` is now `pub`** (was `pub(crate)`). Consumers can drive
  the hysteretic layer selector directly without depending on partner-edge
  internals. `SubscriberPacer::default()` is also implemented.
- **`PacerConfig` struct** — exposes all seven BWE thresholds
  (`suspend_video_bps`, `audio_only_bps`, `low_min_bps`, `medium_min_bps`,
  `high_min_bps`, `suspend_streak`, `upgrade_streak`) as public fields with
  a `Default` impl that yields the existing production values.
  `SubscriberPacer::with_config(cfg: PacerConfig)` constructor added;
  `SubscriberPacer::new()` remains a zero-arg convenience that delegates to
  `with_config(PacerConfig::default())`.
- All BWE constants (`AUDIO_ONLY_BPS`, `LOW_MIN_BPS`, etc.) promoted from
  `pub(crate)` to `pub` so consumers can reference them as default values
  when building custom `PacerConfig` instances.
- **`googcc-bwe` feature** — opt-in GoogCC v2 per-subscriber bandwidth
  estimator:
  - `bwe::googcc::TrendlineDetector` — linear regression over a 20-packet
    rolling window; threshold 12.5 ms/s (same values as partner-edge).
  - `bwe::googcc::AimdController` — +8 % additive increase / ×0.85
    multiplicative decrease (RFC 3448); loss thresholds 0.5 %/2.0 %.
  - `bwe::googcc::GoogCcEstimator` — combines trendline + AIMD,
    per-subscriber (vs per-room in partner-edge, which is architecturally
    weaker). `with_bounds(initial, min, max)` for custom bitrate bounds.
  - `bwe::subscriber::PerSubscriber` gains an optional `googcc` field;
    `combined_bps()` includes the GoogCC estimate as an additional ceiling
    when `Some`.
  - Re-exported at crate root: `use oxpulse_sfu_kit::GoogCcEstimator`.
- New examples: `pacer_basic` (requires `pacer`) and `with_googcc`
  (requires `googcc-bwe`).

### Changed

- `bwe::subscriber::PerSubscriber::combined_bps()` now accepts the GoogCC
  ceiling as a feature-gated extension. Behaviour is unchanged when
  `googcc-bwe` is not enabled or when `googcc` is `None`.

## [0.10.0] — 2026-05-07

### Added

- `Client::with_extra_dc(label, id, cfg)` — generic builder for opening a
  third-party DataChannel against this kit's str0m peer. The new building
  block under `with_chat_dcs` and `with_voice_dc`.
- `Client::with_chat_dcs()` — convenience shim opening the two standard OxPulse
  chat DataChannels: id=4 (`"chat-data"`, reliable+ordered) and id=5
  (`"chat-ctrl"`, unreliable+`max_retransmits=0`). Did not exist before 0.10.0;
  the method was net-new (not a refactor of a prior implementation).
- `Client::with_voice_dc(max_pkt_lifetime_ms: u32)` — Phase 8 voice DC
  convenience. Opens id=6 with `Reliability::MaxPacketLifetime` for the
  Codec2-WASM voice path described in `oxpulse-chat` Phase 8 plan
  (`docs/superpowers/plans/2026-05-07-phase8-codec2-voice-dc-plan.md`).
  Parameter is `u32` (spec contract range); callers converting to str0m
  must `try_into::<u16>()` (str0m's `MaxPacketLifetime.lifetime` is `u16`).
- New `dc::ChannelConfig` wrapper with three constructors:
  `reliable_ordered()`, `unreliable_max_retransmits(n)`,
  `unreliable_max_lifetime(ms)`. Centralises the str0m `Reliability`
  variant choice. All fields are `pub(crate)`; use the accessor methods
  (`id()`, `label()`, `ordered()`, `max_packet_lifetime_ms()`,
  `max_retransmits()`) for read access.
- `Client::extra_dcs() -> &[ChannelConfig]` public accessor — returns the
  slice of pre-registered DataChannels. The `extra_dcs` field is now
  `pub(crate)` to prevent direct struct-literal construction that could
  violate the mutual-exclusion invariant on `max_packet_lifetime_ms` /
  `max_retransmits`.
- Test seams `Client::dc_config_for(label)` and `Client::dc_count()`,
  gated behind the existing `test-utils` feature.

### Changed

- (none)

## [0.9.0] — 2026-05-06

### Added (BREAKING)

- **F7-1 — `peer_id` label on `sfu_video_frames_dropped_total`**.
  - `video_frames_dropped_total` is now an `IntCounterVec` with a single
    `peer_id` label, mirroring the F2b-2 reap pattern from partner-edge PR #46.
  - `inc_video_frames_dropped(peer_id: u64)` — call-site updated in
    `client::fanout::handle_media_data_out`.
  - `SfuMetrics::reap_video_frames_dropped(peer_id: u64)` — drops the label
    series on disconnect to bound cardinality across reconnect churn.
  - `SfuMetrics::reap_dead_peer` now also reaps `video_frames_dropped_total`.
  - New integration test `video_frames_dropped_label_reaped_on_disconnect`
    verifies label presence after drops and absence after `reap_dead_peer`.
  - **Consumers must call `SfuMetrics::reap_dead_peer(peer_id)` on disconnect**
    (partner-edge already does this via the F2b-2 pattern).
  - Closes F7-1 from the oxpulse-chat 1 KB/s resilience plan.

### Changed

- Noop stub `SfuMetrics` (feature `metrics-prometheus` disabled) updated to
  match the new signature.

## [0.8.0] — 2026-05-05

### Added

- **Phase 7 — per-client video drop on suspended state**.
  - `Client.suspended: bool` field + `set_suspended` / `is_suspended` accessors (under `pacer` feature).
  - `client::fanout::handle_media_data_out` drops video frames when `suspended == true`; audio continues to flow.
  - Registry pacer-driven path (TWCC + Kalman) sets / clears the flag on `PacerAction::SuspendVideo` / `RestoreAudio`.
  - Prometheus counters: `sfu_pacer_suspend_video_total{direction="enter"|"exit"}` (transitions) and `sfu_video_frames_dropped_total` (frames dropped while suspended). Both under `metrics-prometheus` feature.
  - 5 new integration tests in `tests/pacer_suspend_drop.rs`.

### Changed

- (none)

### Removed

- (none)

## [0.7.0] — 2026-05-05

### Added

- **Phase 6 — third bandwidth tier `SuspendVideo`** below `AudioOnlyMode`
  (signal-only; per-client fanout filter lands in Phase 7).
  - `SUSPEND_VIDEO_BPS = 10_000` threshold const in `src/bwe/mod.rs`.
  - `PacerAction::SuspendVideo` and `PacerAction::RestoreAudio` variants.
  - `SubscriberPacer::suspended` sub-state with FSM cascade `Suspend →
    RestoreAudio → RestoreVideo → ChangeLayer`.
  - `Propagated::SuspendVideo { peer_id, suspended }` event, emitted from
    both the TWCC and Kalman pacer paths in the registry.
  - 9 new unit tests in `src/bwe/hysteresis.rs` and 3 integration tests in
    `tests/pacer_suspend_video.rs`.
- **`SUSPEND_STREAK = 2` debounce** on suspend-entry (F6-3). Asymmetric
  with `UPGRADE_STREAK = 3`: entry mistakes cause a visible video gap
  (high cost), so a small 2-tick debounce rejects single-tick TWCC spikes
  without delaying real-congestion response (~100–200 ms at typical TWCC
  cadence). Recovery side unchanged (single-tick aggressive — single PLI
  keyframe regen is cheap relative to a video gap). Const exposed as
  `pub` + `#[doc(hidden)]` for integration-test linkage.
- 3 new hysteresis unit tests covering the debounce contract:
  `single_tick_below_suspend_threshold_does_not_suspend`,
  `suspend_streak_consecutive_ticks_required`,
  `suspend_streak_resets_on_interruption`.

### Changed

- **BREAKING (semver-incompatible per pre-1.0 Cargo conventions)** —
  `#[non_exhaustive]` added to public enums `Propagated`, `PacerAction`,
  and `ClientOrigin`. Downstream consumers doing exhaustive `match` on
  these types must add a wildcard arm `_ => { ... }` or explicit handling
  on upgrade. Within-crate matches are unaffected (same-crate exemption).
  Phase 6+ will continue to grow these enums; this consolidates one
  inevitable downstream break.

### Removed

- Stale `#![allow(dead_code, unused_imports)]` in `src/bwe/mod.rs`
  (skeleton comment from earlier development phase). Surfaced one real
  unused import in `src/bwe/subscriber.rs` — fixed.

### Migration from 0.6 → 0.7

Consumers of `oxpulse-sfu-kit` doing exhaustive `match` on `Propagated`,
`PacerAction`, or `ClientOrigin` will see compile errors after upgrade.
Add a wildcard arm:

```rust
match p {
    Propagated::MediaData(c, data) => { /* ... */ }
    // existing arms...
    _ => { /* future variants */ }
}
```

The new `Propagated::SuspendVideo { peer_id, suspended }` event is
emitted by the registry's pacer path. To observe suspend transitions,
add an explicit arm; otherwise the wildcard handles it as a no-op
(the SFU itself does not yet drop video frames in fanout — that is
Phase 7 of the 1 KB/s resilience plan).

## [0.6.0] — 2026-04-23

### Added

- **`kalman-bwe` feature** — GoogCC-inspired bandwidth estimator combining Kalman
  delay estimation and loss-based rate control, ported from `oxpulse-partner-edge`.
  Supersedes the simple `pacer` feature hysteresis for production deployments.
  
  - `DelayEstimator` — 1D Kalman filter on TWCC inter-arrival delay gradients
    with AIMD rate control (multiplicative decrease on overuse, additive increase
    otherwise). Constants tuned to match GoogCC production values.
  - `LossEstimator` — 64-packet sliding window loss fraction with AIMD.
  - `PerSubscriber` — combines Kalman + Loss + native GCC ceiling + browser
    client hint ceiling into a single `combined_bps()`.
  - `BandwidthEstimator` — per-room state container with `record_native_estimate`,
    `record_client_hint`, `on_twcc_feedback`, `record_send_time`, `estimate_bps`.
  - `TwccFeedback` / `TwccSample` — TWCC feedback ingestion, resolves the
    `CongestionControl` trait gap from v0.4.0 (TWCC bytes now consumed internally).
  - `Registry::on_twcc_feedback` — exposes TWCC ingestion to the application.
  - `Registry::update_pacer_layers(origin)` — drives simulcast layer selection
    from BWE; called automatically on every `MediaData` event (under
    `kalman-bwe + pacer`).

- **`Propagated::ClientBudgetHint(ClientId, u64)`** — carries browser-reported
  bandwidth budget from a DataChannel `{"type":"budget","bps":N}` message.
  Always compiled. Under `kalman-bwe`, feeds `BandwidthEstimator::record_client_hint`.

- **Auto audio-level extraction** — under `active-speaker`, `poll_all` now reads
  `MediaData.ext_vals.audio_level` (str0m 0.18.1 RFC 6464 extension) and feeds it
  into the dominant-speaker detector automatically. Applications no longer need to
  call `Registry::record_audio_level` unless they parse audio levels externally.

- **`SfuMediaPayload::audio_level_raw() -> Option<i8>`** — exposes the raw RFC 6464
  audio level from the RTP header extension (negated dBov: −127 = loudest, 0 = silent).

### Dependencies

- No new external dependencies. `kalman-bwe` uses only `std`.

### Notes

- MSRV unchanged: Rust 1.86.
- `kalman-bwe` and `pacer` features are independent; enable both for full adaptive
  layer selection driven by Kalman BWE.
- `update_pacer_layers` is gated on `all(kalman-bwe, pacer)` and fires on every
  `MediaData` event — same cadence as partner-edge's production deployment.
- The `CongestionControl` trait (v0.4.0 dead seam) now has a working implementation
  path: TWCC feedback → `on_twcc_feedback` → `BandwidthEstimator`. The trait itself
  remains a plugin seam; see `docs/ROADMAP.md`.

[0.6.0]: https://github.com/anatolykoptev/oxpulse-sfu-kit/releases/tag/v0.6.0

## [0.5.0] — 2026-04-22

### Added

- **`ClientOrigin { Local, RelayFromSfu(String) }` enum** — marks a `Client` as an
  upstream SFU relay connection. Zero cost when unused (`Local` is default).
  `Client::set_origin(origin)` / `Client::origin()` / `Client::is_relay()`.

- **`TrackIn::relay_source: bool`** — propagated at track-open time from the
  publisher's `is_relay()` status. Enables per-track relay routing without
  accessing the registry.

- **`Propagated::UpstreamKeyframeRequest { source_relay_id, req, source_mid }`** —
  emitted instead of `KeyframeRequest` when a subscriber requests a keyframe
  for a relay-originated track. The application forwards this upstream via
  signalling; no PLI/FIR is sent to the relay peer.

- **`Propagated::PublisherLayerHintForUpstream { publisher_relay_id, max_rid }`** —
  Dynacast hint emitted by `emit_publisher_layer_hints()` when the publisher is
  a relay client. Application forwards upstream via inter-SFU signalling.

- **Relay clients excluded from dominant-speaker detector** — `insert()` skips
  `detector.add_peer()` for relay clients; `reap_dead()` skips `remove_peer()`.
  `record_audio_level()` also ignores relay-peer levels.

- `docs/ROADMAP.md` created.

### Notes

- No new external dependencies. No feature flag — `ClientOrigin` always compiled.
- MSRV unchanged: Rust 1.86.
- **Call-order contract:** `client.set_origin(ClientOrigin::RelayFromSfu(...))` must
  be called **before** `registry.insert(client)`.
- `serve_socket` / `run_udp_loop` drop `UpstreamKeyframeRequest` and
  `PublisherLayerHintForUpstream` silently. Drive the registry directly to consume them.

[0.5.0]: https://github.com/anatolykoptev/oxpulse-sfu-kit/releases/tag/v0.5.0

## [0.4.0] — 2026-04-22

### Added

- **`pacer` feature** — `SubscriberPacer` with LiveKit-style 3-consecutive-upgrade /
  instant-downgrade BWE hysteresis. Egress bandwidth estimates from str0m GoogCC
  automatically adjust `desired_layer` per subscriber. New `PacerAction` enum.
  `Propagated::AudioOnlyMode { peer_id, audio_only }` emitted at 80 kbps threshold.
  `Registry::emit_publisher_layer_hints()` auto-fires on the 300 ms speaker tick.
  `Registry::drive_pacer_for_tests()` available under `test-utils + pacer`.

- **`av1-dd` feature** — `av1::dependency_descriptor::parse(&[u8]) -> Option<Av1DdInfo>`
  extracts `temporal_id` / `spatial_id` from the AV1 DD RTP header extension (L3T3
  template layout, templates 0–8). `SfuMediaPayload::av1_dd()` accessor.
  `Client::set_max_temporal_layer(u8)` per-subscriber cap; packets with
  `temporal_id > cap` are dropped at fanout. Note: `av1_dd()` returns `None` on
  str0m 0.18 (DD not yet in `ExtensionValues`); the parser activates when str0m
  surfaces it.

- **`vfm` feature** — RFC 9626 Video Frame Marking RTP header extension parser for
  H.264, VP9, and HEVC. `FrameMarkingInfo { start_of_frame, end_of_frame, independent,
  discardable, base_layer_sync, temporal_id }`. `SfuMediaPayload::vfm_frame_marking()`
  accessor. `Client::set_max_vfm_temporal_layer(u8)` per-subscriber temporal-layer cap.

- **`LayerSelector` trait + `BestFitSelector`** — centralises the desired-layer +
  active-rids forwarding decision. `BestFitSelector` is now wired into
  `handle_media_data_out`: picks the highest active RID ≤ `desired_layer`, falling
  back to `desired` when `active_rids` is empty (backward-compatible).

- **`Propagated::PublisherLayerHint { publisher_id, max_rid }`** — Dynacast-style
  hint emitted by `Registry::emit_publisher_layer_hints()` when the maximum desired
  layer across all subscribers changes. Application should relay to publisher via
  RTCP or signalling.

- **`Propagated::AudioCodecHint { peer_id, opus_red, opus_dred }`** — signal that a
  subscriber supports Opus RED (RFC 2198) or DRED; relay through signalling to
  negotiate codec preferences in SDP.

- **`Propagated::ActiveSpeakerChanged`** gains `confidence: f64` — medium-window
  C2 log-ratio margin from `SpeakerChange`. `0.0` = bootstrap election; values
  above `2.0` indicate a confident, contested win. Consumers may delay UI updates
  on low-confidence switches.

- **`Registry::peer_audio_scores() -> Vec<(u64, f64, f64, f64)>`** — raw
  `(peer_id, immediate, medium, long)` activity scores from the Volfin & Cohen
  detector. Under `metrics-prometheus + active-speaker`: three new Prometheus gauges
  `sfu_speaker_{immediate,medium,long}_score{peer_id}`, cleaned up on disconnect.

- **`CongestionControl` trait** in `crate::cc` — plugin seam for alternative
  congestion-control algorithms (SCReAMv2, L4S). Default impl `DefaultGoogCC` is a
  no-op; str0m's built-in GoogCC continues to drive `BandwidthEstimate` events.
  Full integration (raw TWCC byte access) requires a future str0m API addition.

- **`KeyEpoch`** newtype in `crate::sframe` — forwarding seam for the SFrame
  key-epoch RTP header extension (RFC 9605).

- `Registry::emit_publisher_layer_hints()` — computes and enqueues
  `PublisherLayerHint` events on each tick.

- **Audio quality guidance** added to README: RNNoise / ten-vad publisher-side noise
  filtering, Opus DRED pass-through, SFrame E2E encryption architecture.

### Dependencies

- `rust-dominant-speaker` bumped `0.1.1` → `0.2` (v0.2.1). Breaking API changes
  adapted internally: `tick()` → `SpeakerChange`, `remove_peer(&)`,
  `current_dominant().copied()`. Key v0.2.x additions: `current_top_k(k)`,
  `peer_scores()`, `serde` feature, `SpeakerChange.c2_margin`.
  Two numerics bugfixes: `binomial_coefficient` and `compute_activity_score`
  underflow panic under non-default `DetectorConfig`.

### Notes

- Zero new external dependencies beyond `rust-dominant-speaker` bump.
- MSRV unchanged: Rust 1.86.
- `pacer`, `av1-dd`, `vfm` features are independent; all may be enabled simultaneously.
- All three temporal-layer drop gates (`av1-dd`, `vfm`) gate on their respective
  feature flags and default to `u8::MAX` (pass-through) when not set.

## [0.3.1] - 2026-04-22

### Polish

- `[package.metadata.docs.rs]` with `all-features = true` and `--cfg docsrs` — feature-gated public items now render with `#[doc(cfg(feature = "..."))]` badges on docs.rs.
- Stricter crate lints via `[lints]` table: `missing_docs = "deny"`, `rust_2018_idioms` and `unreachable_pub` warn, `clippy::needless_pass_by_ref_mut` deny.
- `#[must_use]` on builder chain methods and zero-cost public accessors. Ignoring a getter return is almost always a bug; the lint catches it at call site.
- Empty UDP datagrams are silently dropped with `tracing::debug!` instead of panicking via `expect("non-empty datagram")`. A zero-byte datagram is always a bug somewhere, but a hot-handler panic is worse than an early return.
- Published tarball trimmed — `docs/` and `.github/` excluded from the crate package.

### Notes

No API changes. This is a patch release focused on CI hygiene, docs.rs rendering, and lint posture.

## [0.3.0] - 2026-04-23

### Breaking

- **str0m encapsulation pass.** Public API no longer exposes `str0m::*` types directly. Motivated by str0m discussion [#944](https://github.com/algesten/str0m/discussions/944) — both Thomas Eizinger (firezone) and Martin Algesten (str0m author) recommended hiding str0m from our public surface so pre-1.0 str0m minor bumps stop propagating as breaking releases downstream.

  Signature changes:

  | Before (v0.2) | After (v0.3) |
  |---------------|---------------|
  | `Propagated::MediaData(ClientId, str0m::media::MediaData)` | `Propagated::MediaData(ClientId, SfuMediaPayload)` |
  | `Propagated::KeyframeRequest(ClientId, str0m::media::KeyframeRequest, ClientId, str0m::media::Mid)` | `Propagated::KeyframeRequest(ClientId, SfuKeyframeRequest, ClientId, SfuMid)` |
  | `Client::new(str0m::Rtc, Arc<SfuMetrics>)` | `Client::new(SfuRtc, Arc<SfuMetrics>)` |
  | `Client::handle_input(str0m::Input)` | `Client::handle_input(IncomingDatagram)` |
  | `Client::accepts(&str0m::Input) -> bool` | `Client::accepts(&IncomingDatagram) -> bool` |
  | `Client::drain_pending_out() -> Drain<'_, Transmit>` | `Client::drain_pending_out() -> impl Iterator<Item = OutgoingDatagram> + '_` |
  | `Client::desired_layer() -> str0m::media::Rid` | `Client::desired_layer() -> SfuRid` |
  | `Client::set_desired_layer(str0m::media::Rid)` | `Client::set_desired_layer(SfuRid)` |
  | `Client::active_rids() -> Vec<str0m::media::Rid>` | `Client::active_rids() -> Vec<SfuRid>` |
  | `pub type Transmit = str0m::net::Transmit;` | removed (use `OutgoingDatagram`) |

- **Escape hatch**: new `oxpulse_sfu_kit::raw` module re-exports `str0m::Rtc` as `RawRtc` and `str0m::RtcConfig` as `RawRtcConfig`. It is **explicitly semver-exempt** — minor str0m bumps may alter it without a major bump of this crate. Construct an `SfuRtc` from a raw one via `SfuRtc::from_raw(rtc)`.

### Added

- `SfuRid` / `SfuMid` / `SfuPt` — newtype wrappers for str0m identifier types. `SfuRid` has strict validation (rejects empty, non-alphanumeric, and >8-byte input) and constants `SfuRid::LOW` / `MEDIUM` / `HIGH` for the `"q"` / `"h"` / `"f"` simulcast convention.
- `SfuMediaPayload` / `SfuMediaKind` — media payload + kind wrappers with accessor-based API.
- `SfuKeyframeRequest` / `SfuKeyframeKind` — keyframe-request wrappers (`Pli` / `Fir`).
- `IncomingDatagram` / `OutgoingDatagram` / `SfuProtocol` — datagram wrappers with public fields (transparent containers).
- `SfuRtc` / `SfuRtcBuilder` — opaque Rtc handle + façade builder exposing `enable_bwe()`.
- `raw` module — semver-exempt escape hatch.
- `tests/encapsulation_surface.rs` — compile-time guard grepping public API for str0m leaks with a documented allowlist for `SfuRtc::from_raw`.

### Migration

Most downstream code changes are mechanical:

```rust
// before (v0.2)
use str0m::Rtc;
let rtc = Rtc::new(Instant::now());
let client = Client::new(rtc, metrics);

// after (v0.3)
use oxpulse_sfu_kit::SfuRtcBuilder;
let rtc = SfuRtcBuilder::new().build();
let client = Client::new(rtc, metrics);
```

```rust
// before
match propagated {
    Propagated::MediaData(id, data) => forward(data.mid, &data.data),
    _ => {}
}

// after
match propagated {
    Propagated::MediaData(id, payload) => forward(payload.mid(), payload.data()),
    _ => {}
}
```

For datagram receive paths:

```rust
// before: Input::Receive(...) passed directly to client.handle_input(...)
// after: build IncomingDatagram and pass it
let datagram = IncomingDatagram {
    received_at: Instant::now(),
    proto: SfuProtocol::Udp,
    source: remote_addr,
    destination: local_addr,
    contents: buf.to_vec(),
};
if client.accepts(&datagram) {
    client.handle_input(datagram);
}
```

## [0.2.0] - 2026-04-22

### Added

- **Bandwidth estimate surfacing** — `Propagated::BandwidthEstimate { peer_id, estimate }` emitted on every `str0m::Event::EgressBitrateEstimate`. New public `BandwidthEstimate { bps }` type. Previously str0m's internal GoogCC output was hidden.
- **Per-peer RTCP stats** — `Propagated::RtcpStats { peer_id, stats }` with `PeerRtcpStats { fraction_lost, jitter, rtt }`. New Prometheus gauges under `metrics-prometheus` feature: `sfu_peer_loss_fraction{peer_id}`, `sfu_peer_jitter_ms{peer_id}`, `sfu_peer_rtt_ms{peer_id}`, `sfu_bandwidth_estimate_bps{peer_id}`.
- **Cardinality reaping** — `SfuMetrics::reap_dead_peer(peer_id)` removes per-peer label series on disconnect. Called automatically from `Registry::reap_dead()`.
- **`serve_socket`** split out of `run_udp_loop` for multi-room deployments where the caller owns socket lifecycle. `run_udp_loop` retained as convenience.
- Integration tests: `tests/bwe_surfacing.rs`, `tests/rtcp_stats.rs`, `tests/serve_socket.rs`.

### Changed

- Dependency bump: `rust-dominant-speaker` 0.1 → 0.1.1 (adds `DetectorConfig` for tuning Volfin & Cohen constants).

### Notes

- Renamed from `str0m-sfu-kit` on 2026-04-22 per upstream guidance ([algesten/str0m#944](https://github.com/algesten/str0m/discussions/944)): coupling our name to `str0m-*` would tie our semver to str0m's pre-1.0 breaking-change cycle. Going forward, str0m is an implementation detail.
- Dropped from v0.2 scope: byte-buffer pool in the forward path. str0m owns all outbound byte buffers inside its `Rtc` state machine — there is no allocation in our code to pool. Firezone's `bufferpool` pattern applies to codepaths that allocate raw `Vec<u8>`, which we don't.

## [0.1.0] - 2026-04-21

### Added

- `Client` — per-peer state machine wrapping `str0m::Rtc`
- `Registry` — room-level UDP routing, `poll_all`, `fanout_pending`, `reap_dead`
- `Propagated` — event enum: `TrackOpen`, `MediaData`, `KeyframeRequest`, `ActiveSpeakerChanged` (feature-gated)
- `SfuConfig` — environment-driven runtime configuration
- `run_udp_loop` / `bind` / `serve` — ready-to-use async UDP loop
- Simulcast layer filtering per subscriber (`q`/`h`/`f` RID convention)
- `active-speaker` feature: dominant speaker detection via `rust-dominant-speaker`
- `metrics-prometheus` feature: Prometheus counters via `SfuMetrics`
- `test-utils` feature: test seam helpers for integration tests
- `examples/basic-sfu.rs` — complete single-node SFU with metrics endpoint
- CI: fmt, clippy, tests on stable/beta/MSRV (1.85), docs

[0.1.0]: https://github.com/anatolykoptev/oxpulse-sfu-kit/releases/tag/v0.1.0
