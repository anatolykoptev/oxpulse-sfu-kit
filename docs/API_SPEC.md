# oxpulse-sfu-kit — Public API Specification

**Version:** 0.12.0
**Date:** 2026-07-08
**Status:** Code on `main`, staged in the unmerged release-please PR (`chore(main): release 0.12.0`, #45) — not yet published to crates.io (last published: 0.11.9). Semver-respected once published; MSRV 1.88 (unchanged this cycle).

> **Registry-level BWE/pacer orchestration is PRODUCTION-UNPROVEN.** oxpulse-partner-edge — the kit's most mature consumer — forked its own `Registry`/`Client` around the primitives (`bwe::estimator::BandwidthEstimator`, `bwe::googcc::GoogCcEstimator`) and does not exercise the `Registry`-level auto-feed / single-arbitration pacer-drive path documented in [§9](#registry-level-ownership--behavior) below. That path's only current exercise is the kit's own Phase-4 parametrized deterministic scenario harness (`tests/pacer_bwe_scenarios.rs`) and its unit/doc tests — an **acceptance gate, not a production-validation claim**. Treat it as new, unproven-at-scale surface until a consumer runs it live.

This document specifies every consumer-facing public API in the `bwe` (Bandwidth Estimation), `pacer`, and (as of v0.12.0) the BWE-related surface of the `registry` modules of `oxpulse-sfu-kit`. It complements [docs.rs](https://docs.rs/oxpulse-sfu-kit) by giving an integration-oriented view: contracts, invariants, wiring recipes, and migration notes for consumers upgrading from earlier versions.

For the broader kit overview (Client, Registry, Propagated, signaling stubs) see [README](../README.md). This document covers BWE/pacer — the surface area extended across v0.11.x and, in v0.12.0, given a second (Registry-driven) ownership model layered on top of the original primitives-level one.

---

## Table of Contents

1. [Feature Gates](#feature-gates)
2. [Type Reference](#type-reference)
3. [`BandwidthEstimator` — central entry point](#bandwidthestimator)
4. [`PerSubscriber` & `ClientHint` — subscriber state](#persubscriber-clienthint)
5. [`GoogCcEstimator` — congestion control](#googccestimator)
6. [`SubscriberPacer` & `PacerConfig` — layer hysteresis](#subscriberpacer-pacerconfig)
7. [`PacerAction` — action enum](#paceraction)
8. [TWCC types: `TwccFeedback`, `TwccSample`](#twcc-types)
9. [Registry-level ownership & behavior (v0.12.0)](#registry-level-ownership--behavior)
10. [End-to-end wiring recipes](#wiring-recipes)
11. [Migration: v0.11.8/v0.11.9 → v0.12.0](#migration-v0118v0119--v0120)
12. [Migration: earlier versions](#migration-earlier-versions)
13. [Invariants & contracts summary](#invariants)
14. [Error model](#error-model)

---

## Feature Gates

All BWE/pacer APIs are gated. Enable in `Cargo.toml`:

```toml
[dependencies]
oxpulse-sfu-kit = { version = "0.12", features = ["kalman-bwe", "googcc-bwe", "pacer"] }
```

| Feature | Enables | Depends on |
|---|---|---|
| `kalman-bwe` | `BandwidthEstimator`, `PerSubscriber`, `DelayEstimator`, `LossEstimator`, `TwccFeedback`, `ClientHint` | none |
| `googcc-bwe` | `GoogCcEstimator`, `enable_googcc_for_subscriber`, `googcc_for_subscriber_mut`, `BandwidthState` | `kalman-bwe` (recommended; the GoogCc API itself works standalone but is wired into `PerSubscriber.googcc` only when both are enabled) |
| `pacer` | `SubscriberPacer`, `PacerConfig`, `PacerConfigError`, `PacerAction` | none |

At the `BandwidthEstimator`/`SubscriberPacer` level, all three features are independent — a consumer can wire pacer without GoogCC, or GoogCC without pacer. **This independence does NOT extend to the `Registry`-level surface** ([§9](#registry-level-ownership--behavior)): `Registry::bandwidth()` and the GoogCC pass-throughs require `kalman-bwe` AND `googcc-bwe` together — a `googcc-bwe`-only build fails with a targeted `compile_error!` at the Registry level (ADR-9), even though `GoogCcEstimator` itself builds fine standalone.

**MSRV:** 1.88 (transitive `time@0.3.47` requirement via `str0m → dimpl → time`; unchanged in v0.12.0 — see [Migration](#migration-v0118v0119--v0120)).

---

## Type Reference

| Symbol | Crate path | Re-exported as |
|---|---|---|
| `BandwidthEstimator` | `oxpulse_sfu_kit::bwe::BandwidthEstimator` | `oxpulse_sfu_kit::BandwidthEstimator` |
| `PerSubscriber` | `oxpulse_sfu_kit::bwe::subscriber::PerSubscriber` | `oxpulse_sfu_kit::PerSubscriber` |
| `ClientHint` | `oxpulse_sfu_kit::bwe::subscriber::ClientHint` | `oxpulse_sfu_kit::ClientHint` |
| `DelayEstimator` | `oxpulse_sfu_kit::bwe::kalman::DelayEstimator` | `oxpulse_sfu_kit::DelayEstimator` |
| `LossEstimator` | `oxpulse_sfu_kit::bwe::loss::LossEstimator` | `oxpulse_sfu_kit::LossEstimator` |
| `GoogCcEstimator` | `oxpulse_sfu_kit::bwe::googcc::GoogCcEstimator` | `oxpulse_sfu_kit::GoogCcEstimator` |
| `BandwidthState` | `oxpulse_sfu_kit::bwe::googcc::trendline::BandwidthState` | (not re-exported; use full path) |
| `SubscriberPacer` | `oxpulse_sfu_kit::bwe::SubscriberPacer` | `oxpulse_sfu_kit::SubscriberPacer` |
| `PacerConfig` | `oxpulse_sfu_kit::bwe::PacerConfig` | `oxpulse_sfu_kit::PacerConfig` |
| `PacerConfigError` | `oxpulse_sfu_kit::bwe::PacerConfigError` | `oxpulse_sfu_kit::PacerConfigError` |
| `PacerAction` | `oxpulse_sfu_kit::bwe::PacerAction` | `oxpulse_sfu_kit::PacerAction` |
| `TwccFeedback`, `TwccSample` | `oxpulse_sfu_kit::bwe::feedback::*` | `oxpulse_sfu_kit::{TwccFeedback, TwccSample}` |

Every documented public symbol is reachable as `oxpulse_sfu_kit::X` (no module path required) since v0.11.4.

`Registry`'s BWE-related methods (`bandwidth()`, `enable_googcc_for_subscriber`, `googcc_ceiling_for_subscriber_mut`, `update_pacer_layers`) are covered in [§9, Registry-Level Ownership & Behavior](#registry-level-ownership--behavior) rather than this table — `Registry` itself (construction, `Client`, `Propagated`) is the broader kit overview covered by [README.md](../README.md), and this document stays scoped to its BWE/pacer-related surface.

---

## `BandwidthEstimator`

**Module:** `bwe::estimator` · **Feature:** `kalman-bwe` · **Stability:** stable

Per-room bandwidth estimator. Holds one [`PerSubscriber`](#persubscriber-clienthint) entry per connected peer. **Single source of truth** for combined BWE — Kalman delay estimate, loss estimate, native GCC ceiling, browser hint, and (optionally) per-subscriber GoogCC are all aggregated here.

### Constructor

```rust
pub fn new() -> Self
```

Creates an empty estimator. No subscribers; no allocation beyond the empty `HashMap`.

### Mutating

```rust
pub fn record_native_estimate(&mut self, subscriber: ClientId, bps: f64)
```
Update the native GCC ceiling for `subscriber`. Source: str0m `EgressBitrateEstimate` event. Creates the subscriber entry if missing. **Contract:** `bps` should be a non-negative finite value; non-finite or negative values are accepted but will produce undefined estimates downstream. Idempotent across calls — only the latest value is retained.

```rust
pub fn record_client_hint(&mut self, subscriber: ClientId, bps: u64, now: Instant)
```
Record a browser-reported budget hint (DataChannel `{"type":"budget","bps":N}`). The hint expires after **5 seconds** ([`CLIENT_HINT_MAX_AGE`](#invariants)) and is dropped from `combined_bps()` after expiry. `now` must be a monotonic instant (use `Instant::now()`).

```rust
pub fn on_twcc_feedback(&mut self, subscriber: ClientId, feedback: &TwccFeedback, now: Instant)
```
Process a TWCC feedback batch. Feeds Kalman + loss estimators inside the subscriber. **Does NOT** feed `googcc` — feeding GoogCC is the consumer's responsibility (see [`googcc_for_subscriber_mut`](#googcc-for-subscriber-mut) below). **Contract:** `record_send_time` MUST have been called for every seq listed in `feedback.samples` before this call; missing send-time entries are silently skipped.

```rust
pub fn record_send_time(&mut self, subscriber: ClientId, seq: u64, sent_at: Instant)
```
Record the send timestamp for an RTP packet. Bounded internal map (max **512 entries** per subscriber); evicts the oldest on overflow. Call exactly once per packet enqueued.

```rust
pub fn reap_dead(&mut self, subscriber: ClientId)
```
Remove subscriber state on disconnect. **Always call** when a peer leaves — without it, the HashMap grows unboundedly across the room's lifetime.

### GoogCC integration (added in v0.11.4)

```rust
#[cfg(feature = "googcc-bwe")]
pub fn enable_googcc_for_subscriber(&mut self, id: ClientId)
```
Enable the per-subscriber [`GoogCcEstimator`] for `id`. Sets `PerSubscriber.googcc = Some(GoogCcEstimator::new())` so the estimator participates in `estimate_bps()` as an additional ceiling. Creates the subscriber entry if missing.

**Contract:**
- **Idempotent.** Calling twice on the same subscriber preserves the existing estimator state — does NOT reset.
- After enabling, feed packet timing via [`googcc_for_subscriber_mut`](#googcc-for-subscriber-mut).

```rust
#[cfg(feature = "googcc-bwe")]
#[must_use]
pub fn googcc_for_subscriber_mut(&mut self, id: ClientId)
    -> Option<&mut GoogCcEstimator>
```
Mutable accessor for feeding packet arrival timing.

**Returns `None`** when EITHER:
- The subscriber doesn't exist (no prior call to any `record_*` or `enable_*` method), OR
- GoogCC was never enabled for this subscriber via `enable_googcc_for_subscriber`.

Both "not enabled" cases collapse to `None` — consumers that need to distinguish should track enablement themselves.

### Reading

```rust
#[must_use]
pub fn estimate_bps(&self, subscriber: ClientId, now: Instant) -> Option<u64>
```
Returns the combined bitrate estimate for `subscriber`, or `None` if the subscriber has no state yet. Internally calls [`PerSubscriber::combined_bps`](#persubscriber-clienthint) which applies the ceiling chain:

```
min(kalman_delay, loss) → cap by native_estimate → cap by googcc → cap by client_hint
```

Result is cast `f64 → u64` (truncating; floor of the f64 value). Values are bounded by `[0, u64::MAX]`.

### Test seam (gated)

```rust
#[cfg(any(test, feature = "test-utils"))]
#[doc(hidden)]
pub fn force_high_estimate_for_tests(&mut self, subscriber: ClientId, bps: f64)
```
Forces both Kalman and Loss to report `bps`. **Test only.** Strips `native_estimate_bps` so Kalman/Loss dominate.

---

## `PerSubscriber` & `ClientHint`

**Module:** `bwe::subscriber` · **Feature:** `kalman-bwe`

### `ClientHint`

```rust
pub struct ClientHint {
    pub bps: u64,
    pub received_at: Instant,
}
```

Browser-reported budget hint. Public fields — consumers building their own subscriber state directly construct this. Expires after `CLIENT_HINT_MAX_AGE` (5 seconds) when read via `combined_bps`.

### `PerSubscriber`

Per-subscriber BWE state. **All fields are `pub`** — this is intentional: the consumer can manipulate any sub-component for testing, custom integration, or alternative wiring patterns. Default initial bitrate: **300 000 bps** (`INITIAL_BITRATE_BPS`).

```rust
pub struct PerSubscriber {
    pub send_times: HashMap<u64, Instant>,
    pub last_arrival: Option<Instant>,
    pub last_send_for_received: Option<Instant>,
    pub delay: DelayEstimator,
    pub loss: LossEstimator,
    pub rtt: Option<Duration>,
    pub native_estimate_bps: Option<f64>,
    pub client_hint: Option<ClientHint>,
    #[cfg(feature = "googcc-bwe")]
    pub googcc: Option<GoogCcEstimator>,
}
```

#### Methods

```rust
pub fn new() -> Self            // initialised at 300_000 bps
pub fn combined_bps(&self, now: Instant) -> f64
```

`combined_bps` applies ceilings in order: `min(delay, loss)` → cap by `native_estimate_bps` → cap by `googcc.current_bps()` (if `Some`) → cap by `client_hint.bps` (if `Some` and not expired). Result clamped to `≥ 0.0`. **No min-bitrate floor** — distinguish "no estimate yet" from "floor-constrained" at call site if needed.

---

## `GoogCcEstimator`

**Module:** `bwe::googcc` · **Feature:** `googcc-bwe`

GoogCC-style congestion controller (TrendlineDetector + AimdController). One instance per subscriber. Consumers usually access this via [`BandwidthEstimator::googcc_for_subscriber_mut`](#googcc-for-subscriber-mut), but standalone construction is supported.

### Constructors

```rust
pub fn new() -> Self
pub fn with_bounds(initial_bps: u64, min_bps: u64, max_bps: u64) -> Self
```

Defaults (used by `new()`): `initial=500_000 bps`, `min=100_000 bps`, `max=10_000_000 bps`.

### Methods

```rust
pub fn on_receive(&mut self, arrival_ms: f64, send_ms: f64, loss_fraction: f32)
pub fn current_bps(&self) -> u64
```

`on_receive` parameters:
- `arrival_ms` — packet arrival time (monotonic, milliseconds).
- `send_ms` — packet send time (monotonic, milliseconds; same clock as `arrival_ms`).
- `loss_fraction` — observed packet loss in `[0.0, 1.0]`. Out-of-range values are clamped (NaN guarded).

**Contract:** `arrival_ms` and `send_ms` must be finite. Non-finite inputs trigger `debug_assert!` in debug builds; in release builds they're silently skipped.

`current_bps()` reads the most recent estimate. Cheap — no compute.

### `BandwidthState` enum

```rust
#[non_exhaustive]
pub enum BandwidthState {
    Hold,        // no rate change
    Increase,    // pump up
    Decrease,    // back off
}
```

Reachable via `trendline.state()`. **`#[non_exhaustive]`** — match with `_ =>` to remain forward-compatible.

### Test seam

```rust
pub fn force_bps_for_tests(&mut self, bps: u64)  // doc-hidden
```

---

## `SubscriberPacer` & `PacerConfig`

**Module:** `bwe::hysteresis` + `bwe::pacer_config` · **Feature:** `pacer`

Per-subscriber hysteretic layer selector. LiveKit-style: **3 consecutive upgrade ticks** required to step up; **instant downgrade** on bps drop. Audio-only and suspended sub-states are debounced by `suspend_streak` (default: 2 ticks below `suspend_video_bps`).

### `PacerConfig`

```rust
pub struct PacerConfig {
    pub suspend_video_bps: u64,   // default 10_000
    pub audio_only_bps:    u64,   // default 80_000
    pub low_min_bps:       u64,   // default 150_000
    pub medium_min_bps:    u64,   // default 350_000
    pub high_min_bps:      u64,   // default 700_000
    pub suspend_streak:    u8,    // default 2 (≥ 1)
    pub upgrade_streak:    u8,    // default 3 (≥ 1)
}
```

**Invariants** (enforced by `validate()`):
- `suspend_streak ≥ 1` AND `upgrade_streak ≥ 1`.
- `suspend_video_bps ≤ audio_only_bps ≤ low_min_bps ≤ medium_min_bps ≤ high_min_bps` (non-strict).

```rust
pub fn validate(&self) -> Result<(), PacerConfigError>
```

Returns the **first** violation found (streak checks before ordering). Equal thresholds are valid (non-strict ordering).

### `PacerConfigError`

```rust
pub enum PacerConfigError {
    UpgradeStreakZero,
    SuspendStreakZero,
    BitrateOrderingViolation,
}
```

Implements `Display`. No data payload — name is the message.

### `SubscriberPacer`

```rust
pub fn new() -> Self                        // PacerConfig::default()
pub fn with_config(config: PacerConfig) -> Self
pub fn update(&mut self, bps: u64) -> PacerAction
```

`with_config` triggers `debug_assert!` if `config.validate()` fails — release builds accept invalid configs silently and produce undefined behaviour. **Always call `validate()` upstream** before passing into the pacer.

`update(bps)` is the per-tick driver. Call from the same loop that drains TWCC. Returns the action to apply (see below). **Initial layer:** `SfuRid::LOW`. **Initial state:** not audio-only, not suspended.

---

## `PacerAction`

```rust
#[must_use]
#[non_exhaustive]
pub enum PacerAction {
    NoChange,
    ChangeLayer(SfuRid),
    GoAudioOnly,
    RestoreVideo,
    SuspendVideo,
    RestoreAudio,
}
```

`#[must_use]` — the compiler will warn if the action is dropped. Forwarding is the consumer's responsibility; the kit only computes the decision.

Semantic state diagram:

```
                                  bps ↑↑↑
                                   ┌──→  ChangeLayer(LOW/MEDIUM/HIGH)
                                   │           ↑
                                   │           │ +bps
[FULL VIDEO] ──bps↓ < audio_only─→[AUDIO-ONLY]─┘
                                   │
                                   │ bps↓ < suspend_video × suspend_streak
                                   ↓
                                [SUSPENDED]
                                   │ bps↑ ≥ audio_only
                                   ↓
                                [AUDIO-ONLY] ──bps↑ ≥ low_min_bps──→[FULL VIDEO]
                                                  (next tick)
```

| Action | When emitted |
|---|---|
| `NoChange` | bps within current layer's hysteresis band |
| `ChangeLayer(rid)` | `upgrade_streak` ticks above next tier (UP) OR instant on downgrade |
| `GoAudioOnly` | bps drops below `audio_only_bps` from full-video state |
| `RestoreVideo` | bps recovers above `low_min_bps` from audio-only state |
| `SuspendVideo` | `suspend_streak` ticks below `suspend_video_bps` (full suspend) |
| `RestoreAudio` | bps recovers above `audio_only_bps` while suspended (NOT full video) |

---

## TWCC Types

**Module:** `bwe::feedback` · **Feature:** `kalman-bwe`

```rust
pub struct TwccSample {
    pub seq: u64,                  // extended sequence number
    pub arrival: Option<Instant>,  // None = packet lost
}

pub struct TwccFeedback {
    pub samples: Vec<TwccSample>,
}
```

Construct from a str0m `Twcc` event in your TWCC handler. Pass to `BandwidthEstimator::on_twcc_feedback`.

---

## Registry-Level Ownership & Behavior

**Module:** `registry` · **Features:** `kalman-bwe` (+ `pacer`, `googcc-bwe`) · **Stability:** new in v0.12.0, **production-unproven** (see the callout at the top of this document)

Through v0.11.x, `Registry` held a `BandwidthEstimator` (`self.bandwidth`) but exposed no public surface to it — `pacer` builds drove the FSM directly from `poll_all`'s str0m-native estimate, and `kalman-bwe` consumers had no sanctioned way to reach GoogCC or read the combined estimate through `Registry`. v0.12.0 closes that gap with **one canonical ownership model**: `Registry`-driven auto-feed, a single arbitration point for the pacer FSM, and narrow intent-scoped commands for the signals that still need a manual feed.

### Two live drivers → one (ADR-3+4, Bug #7)

Before v0.12.0, `poll_all` (str0m-native cadence, ~100–500 ms) and `update_pacer_layers` (per-`MediaData`, ~20–30 ms) could **both** drive the same `SubscriberPacer` FSM in the `kalman-bwe`+`pacer` combo — two independently-valid cadences racing one hysteresis state machine. v0.12.0 makes exactly one call site the driver **per feature combo**, gated at compile time:

| Combo | Driver | Behavior |
|---|---|---|
| `pacer`, no `kalman-bwe` | `Registry::poll_all` | Drives the FSM directly from str0m's native `BandwidthEstimate` (unchanged from v0.11.x). |
| `kalman-bwe` + `pacer` | `Registry::update_pacer_layers` | Sole driver. `poll_all` only **feeds** `record_native_estimate` into the min-combiner — it no longer touches the FSM. |

This is enforced two ways, not by convention:
- **Compile-time:** `const _PACER_SINGLE_DRIVER_GUARD` in `src/registry/drive.rs` asserts the two `#[cfg]` predicates partition the `pacer` feature space (`poll_all_drives XOR update_pacer_layers_drives`) — a build fails if a future change makes both true or both false.
- **Runtime (cfg-matrix test):** `exactly_one_pacer_driver` runs the same assertion as a test, so CI across the feature matrix confirms it too.

### Bandwidth-Estimate auto-feed

`Registry::poll_all` auto-feeds the combined estimator on every pass, no consumer wiring required:
- **Native ceiling:** under `kalman-bwe` + `pacer` together, every `Propagated::BandwidthEstimate` from str0m is fed via `self.bandwidth.record_native_estimate(peer_id, estimate.bps)` instead of driving the pacer directly — this is precisely the arm that hands the FSM off to `update_pacer_layers` (see the combo table above). `kalman-bwe` without `pacer` does **not** auto-feed the native ceiling — there is no production `Registry`-level pass-through for `record_native_estimate` itself (only `bandwidth_mut_for_tests`, test-utils-gated); this combo is for consumers who only need Kalman/loss/GoogCC/hint (fed via `Registry::on_twcc_feedback` and the GoogCC pass-throughs) without str0m's native ceiling.
- **GoogCC ceiling** (`kalman-bwe` + `googcc-bwe`): `poll_all` samples RTP-timestamp-derived `(arrival_ms, send_ms)` off the **last video `MediaData` packet observed that pass** (one sample per `poll_all` call — a conservative per-room approximation, not per-packet) and feeds every subscriber with GoogCC enabled via `googcc.on_receive(arrival_ms, send_ms, 0.0)`. Audio packets are excluded (`SfuMediaPayload::is_video`) because audio/video RTP clocks run at different rates and mixing them would corrupt the trendline. This is a no-op for subscribers that never called `enable_googcc_for_subscriber`.

  **KNOWN GAP:** this live-arrival sampling path — `poll_all` pulling real `arrival_ms`/`send_ms` off a genuine str0m `MediaData` packet inside a live `client.poll_output()` loop — is **not exercised by the Phase-4 deterministic harness**. That harness (`tests/pacer_bwe_scenarios.rs`) drives the estimator/pacer surface directly (`record_native_estimate`, `on_twcc_feedback`, `googcc.on_receive` with synthetic values) and never constructs a real `str0m::Rtc` session, so it validates the *arbitration and FSM logic* but not the *RTP-timestamp extraction itself*. Validating that leg needs a live str0m `Rtc` session — deferred to Phase 6 / live validation, not covered by this release.

### Single arbitration + min-tick floor (ADR-13)

`Registry::update_pacer_layers(&mut self, origin: ClientId, now: Instant)` (breaking signature change — see [Migration](#migration-v0118v0119--v0120)) is the sole driver under `kalman-bwe`+`pacer`. Per non-origin subscriber, per call:
1. `self.bandwidth.estimate_bps(sub_id, now)` — `None` means "no estimate yet"; the pacer is **not** driven this tick (T1 cold-start fail-safe: a fresh subscriber must not be coerced to `drive_pacer(0)` → spurious `SuspendVideo`).
2. `client.pacer_tick_ready(now, PACER_MIN_TICK_INTERVAL)` (100 ms, `bwe::PACER_MIN_TICK_INTERVAL`) gates the FSM advance. Collapsing to the ~20–30 ms per-`MediaData` cadence as sole driver would shrink `SubscriberPacer`'s `SUSPEND_STREAK` debounce window from ~200–1000 ms to ~40–60 ms — inside a single lossy RTP burst's span. The floor throttles FSM advance to at most once per `PACER_MIN_TICK_INTERVAL` per subscriber; throttled ticks bump the `pacer_tick_throttled_total` metric (`metrics-prometheus`).
3. On a ready tick, `apply_pacer_action` (a pure match, extracted in Phase 0 from three previously-duplicated blocks) turns the `PacerAction` into a `Propagated` event + optional `suspended` flag change.

**Cadence-tightening subtlety:** `Registry::fanout_pending` captures `now = Instant::now()` **once** at the top of its drain loop, then calls `update_pacer_layers(origin, now)` for every queued `Propagated::MediaData` it processes in that drain. Because `pacer_tick_ready` compares against that single fixed `now`, a subscriber that receives media from **multiple publishers within one `fanout_pending` drain** only advances its FSM once — the second and later calls in the same drain see `now.saturating_duration_since(last) == 0 < PACER_MIN_TICK_INTERVAL` and are throttled. The floor therefore dedups across publishers in a burst, not just across time.

### `Registry::reap_dead` wiring (Bug #2 closure)

`Registry::reap_dead` now calls `self.bandwidth.reap_dead(client.id)` for every disconnecting client (`src/registry/lifecycle.rs`). This was a **hard prerequisite**, not an optional cleanup: auto-feeding `record_native_estimate` for every connected subscriber on every `poll_all` pass means a missing reap would leak one `BandwidthEstimator` HashMap entry per churned subscriber, unboundedly, for the lifetime of the room.

### Read-only accessor + narrow commands (ADR-8)

`Registry` deliberately exposes **no** `bandwidth_mut()`. A raw `&mut BandwidthEstimator` would let a caller build a second pacer-drive loop that bypasses the single-arbitration guarantee above — this is the exact aggregate-root leak v0.12.0 closes. Instead:

```rust
#[must_use]
pub fn bandwidth(&self) -> &crate::bwe::estimator::BandwidthEstimator
```
Read-only view for observability — metrics, debugging, `estimate_bps()`/`googcc_coverage` reads — without a `for_tests` seam. Reflects live state, not a snapshot.

```rust
#[cfg(all(feature = "kalman-bwe", feature = "googcc-bwe"))]
pub fn enable_googcc_for_subscriber(&mut self, subscriber: ClientId)

#[cfg(all(feature = "kalman-bwe", feature = "googcc-bwe"))]
#[must_use]
pub fn googcc_ceiling_for_subscriber_mut(&mut self, subscriber: ClientId)
    -> Option<&mut crate::bwe::GoogCcEstimator>
```
Thin one-line pass-throughs to the same-named `BandwidthEstimator` methods (mirroring the `Registry::on_twcc_feedback` pass-through shape). `googcc_ceiling_for_subscriber_mut` is the **only** sanctioned manual ceiling-tweak path now that `bandwidth_mut()` is gone — use it for advanced feeds outside the `poll_all` auto-feed (e.g. a TWCC-level consumer with real receiver-reported timing instead of the RTP-timestamp approximation above).

`bandwidth_mut_for_tests` remains available under `test-utils` for raw injection in tests; it is not part of the production surface.

### `googcc-bwe` ⇒ `kalman-bwe` at the Registry level (ADR-9)

`self.bandwidth` — and therefore both pass-throughs above — is gated `#[cfg(all(feature = "kalman-bwe", feature = "googcc-bwe"))]`. `googcc-bwe` alone (without `kalman-bwe`) is **not buildable at the Registry level**, despite `Cargo.toml` declaring the two features orthogonal (they are orthogonal at the `BandwidthEstimator` level — see [Feature Gates](#feature-gates) — just not through `Registry`). A `#[cfg(all(feature = "googcc-bwe", not(feature = "kalman-bwe")))] compile_error!` guard in `src/registry/mod.rs` makes this failure mode traceable instead of a confusing missing-field error. Regating `self.bandwidth` to `any(kalman-bwe, googcc-bwe)` is a named, reversible follow-up, not done in v0.12.0.

### Intentional ensemble, not duplication (ADR-14)

The hand-rolled `googcc-bwe` estimator is min-combined with str0m's own native `BandwidthEstimate` inside `combined_bps` — this is an **intentional second, independently-computed conservative ceiling** (the libwebrtc ensemble pattern), not dead duplication of str0m's built-in GoogCC. It stays.

---

## Wiring Recipes

Two models exist side by side. **Registry-driven auto-feed is the default for `Registry`/`Client` consumers.** The primitives-level recipe below it stays valid for RTP-level-only consumers (str0m `DirectApi`/`rtp_mode`, or anyone using `BandwidthEstimator`/`SubscriberPacer` standalone without `Registry`).

### Registry-driven recipe (default, `kalman-bwe` + `pacer` [+ `googcc-bwe`])

```rust
// One per room — Registry owns `self.bandwidth` and `self.clients` internally.
let mut registry = Registry::new(/* ... */);

// On subscriber join (googcc-bwe only — optional):
registry.enable_googcc_for_subscriber(sub_id);

// Per network tick:
let deadline = registry.poll_all(Instant::now());   // auto-feeds native + GoogCC ceilings
registry.fanout_pending();                            // drives the pacer FSM exactly once
                                                        // per subscriber per drain (single-
                                                        // arbitration + min-tick floor, above)

// Read the combined estimate anywhere (observability, tests):
if let Some(bps) = registry.bandwidth().estimate_bps(sub_id, Instant::now()) {
    // ...
}

// On subscriber leave: Registry::reap_dead already calls self.bandwidth.reap_dead(id).
registry.reap_dead();
```

No `record_send_time`/`on_twcc_feedback`/manual `googcc.on_receive` calls needed for the common path — `poll_all` auto-feeds both ceilings from str0m's own event stream.

### Primitives-level recipe (advanced / RTP-level consumers)

End-to-end, all features enabled, driving `BandwidthEstimator`/`SubscriberPacer` directly without `Registry`:

```rust
use std::time::Instant;
use oxpulse_sfu_kit::{
    BandwidthEstimator, ClientId, PacerAction, PacerConfig, SubscriberPacer,
    TwccFeedback, TwccSample,
};

// One per room.
let mut bwe   = BandwidthEstimator::new();
let mut pacer = SubscriberPacer::with_config(PacerConfig::default());

// On subscriber join:
let id = ClientId(42);
bwe.enable_googcc_for_subscriber(id);

// Per RTP packet enqueued (in your forwarding loop):
bwe.record_send_time(id, /* seq */ 1234, Instant::now());

// On TWCC feedback batch from str0m:
let now = Instant::now();
let fb = TwccFeedback {
    samples: vec![
        TwccSample { seq: 1234, arrival: Some(now) },
    ],
};
bwe.on_twcc_feedback(id, &fb, now);

// Feed GoogCC separately (TWCC handler doesn't auto-feed it):
if let Some(gcc) = bwe.googcc_for_subscriber_mut(id) {
    gcc.on_receive(/* arrival_ms */ 100.0, /* send_ms */ 95.0, /* loss */ 0.001);
}

// On native GCC ceiling event (str0m EgressBitrateEstimate):
bwe.record_native_estimate(id, /* bps */ 800_000.0);

// Drive pacer per tick (e.g. every 100 ms):
if let Some(bps) = bwe.estimate_bps(id, now) {
    match pacer.update(bps) {
        PacerAction::ChangeLayer(rid)  => { /* select rid for forwarding */ }
        PacerAction::GoAudioOnly       => { /* stop video, keep audio */ }
        PacerAction::RestoreVideo      => { /* resume video */ }
        PacerAction::SuspendVideo      => { /* stop ALL video */ }
        PacerAction::RestoreAudio      => { /* resume audio (not video yet) */ }
        PacerAction::NoChange          => { /* keep current */ }
    }
}

// On subscriber leave:
bwe.reap_dead(id);
```

---

## Migration: v0.11.8/v0.11.9 → v0.12.0

v0.12.0 is a breaking release folding two changes that both land on the fanout write path and the Registry drive path: a str0m 0.18.1 → 0.21.0 bump (Phase -1) and the Registry-driven BWE ownership model in [§9](#registry-level-ownership--behavior) (E1, Phases 0/2/3/4). The last published patch is v0.11.9 (SFrame KeyEpoch fix, `^0.11.8`-compatible, shipped standalone against str0m 0.18 — unrelated to this train). At the time of writing, v0.12.0 is staged on `main` behind the unmerged release-please PR #45 (crates.io still serves 0.11.9).

### str0m 0.18.1 → 0.21.0 delta

- **Cargo pin:** `str0m = "0.21"` (was `"0.18.1"`). Pulls `dimpl` 0.5→0.7 and switches the DTLS/crypto backend to `aws-lc-rs`/`aws-lc-sys` (no more plain `openssl`). `aws-lc-sys` builds from source without prebuilt bindings on some targets — verify your build toolchain has `cmake` if you don't already build `oxpulse-sfu-kit` today.
- **MSRV unchanged** at 1.88 this cycle (the 0.11.2→0.11.3 bump below was the last MSRV move).
- **`raw::*` escape hatch (`RawRtc`, `RawRtcConfig`):** these are direct str0m re-exports and are semver-exempt by design (see [Stability & SemVer](#stability--semver)). Anyone using `oxpulse_sfu_kit::raw::*` now gets str0m 0.21 types. If your own `Cargo.toml` also depends on `str0m` directly (e.g. to interop with `raw::*` values, or via `From<SfuRid> for str0m::media::Rid`), **bump your own `str0m` dependency to `0.21` in the same commit** — two different str0m minors in one dependency tree resolve as incompatible types at every interop boundary. `oxpulse-partner-edge`'s cross-repo release runbook makes this a durable CI fitness check (`cargo tree -i str0m` must resolve to exactly one version); consider the same check if you vendor `oxpulse-sfu-kit` alongside your own str0m usage.
- No change to the crate's own public function/type *names* from the str0m bump — the delta is entirely in which str0m minor the wrapped/re-exported types resolve to.

### `SfuMediaPayload.data`: `Vec<u8>` → `Arc<[u8]>`

The `data` field itself was never public — `SfuMediaPayload::data(&self) -> &[u8]` is the only public accessor, and **its signature is unchanged**: read-only consumers calling `.data()` see no source-level break. The internal `write_parts` helper (renamed from `clone_write_parts`, `pub(crate)`-only) now returns `Arc::clone(&self.data)` — a refcount bump — instead of `self.data.clone()` (a full byte copy), so fanning one inbound frame out to N subscribers costs N refcount increments, not N `Vec<u8>` allocations. This also removes a silent `Vec<u8>` → `Arc<[u8]>` double-copy that used to happen inside str0m's own `Writer::write` (str0m's `MediaData.data` is `Arc<[u8]>` as of str0m 0.20, mirrored here).

This is flagged `BREAKING CHANGE` in the release-please commit (`feat!`) because of the str0m dependency-version crossing above, not because of an observable change to `SfuMediaPayload`'s own public surface. If you construct `str0m::media::MediaData` yourself for test fixtures (e.g. driving `Client`/`Registry` in your own test harness), convert your buffer with `.into()`:

```rust
// OLD (str0m 0.18): data: Vec<u8>
// NEW (str0m 0.21): data: Arc<[u8]>
str0m::media::MediaData {
    // ...
    data: vec![0u8; 4].into(),   // Vec<u8> -> Arc<[u8]> via From
    // ...
}
```

If you maintain your own fork with a parallel fanout hot path doing the equivalent of the old `clone_write_parts` (`oxpulse-partner-edge`'s `crates/sfu/src/client/fanout.rs` does), port the same `Arc::clone` pattern; it is a strict allocation reduction, not a behavior change.

### `Registry::update_pacer_layers` gained a `now: Instant` parameter

```rust
// OLD (≤ v0.11.x):
pub fn update_pacer_layers(&mut self, origin: ClientId)

// NEW (v0.12.0):
pub fn update_pacer_layers(&mut self, origin: ClientId, now: Instant)
```

Required by the ADR-13 min-tick floor (`pacer_tick_ready(now, PACER_MIN_TICK_INTERVAL)` — see [§9](#registry-level-ownership--behavior)) and by the Phase-4 deterministic scenario harness, which needs a controllable clock rather than an internal `Instant::now()` call. `Registry::fanout_pending` is the only in-kit caller — it captures `now` once per drain and threads the same value into every `update_pacer_layers` call inside that drain. If you call `update_pacer_layers` directly, thread whatever `Instant` your own drive loop already uses for that tick; if you call it more than once per logical tick, reuse the same `Instant` across those calls, or the min-tick floor cannot dedup across them the way [§9](#registry-level-ownership--behavior)'s cadence-tightening note describes.

### Behavior changes (cadence / debounce / winning signal)

- **Winning signal:** under `kalman-bwe`+`pacer`, the pacer FSM now always advances off `combined_bps()` (min of delay/loss/native/googcc/hint) rather than sometimes off the raw str0m-native estimate (whichever driver happened to fire last, pre-v0.12.0). One coherent number replaces two racing ones.
- **Cadence:** effectively throttled to at most one FSM advance per subscriber per `PACER_MIN_TICK_INTERVAL` (100 ms), down from the previous unthrottled ~20–30 ms `MediaData` cadence in the two-driver race. This is a **debounce-latency increase by design** (ADR-13) — real lossy RTP bursts no longer spuriously trip `SuspendVideo` inside a single `SUSPEND_STREAK` window, at the cost of layer-change reaction time being bounded by the floor rather than by RTP arrival.
- **Cold start:** an unfed `estimate_bps` (`None`) no longer drives the pacer at all (T1 fail-safe) — previously a fresh subscriber's `None` case could be coerced toward a `0 bps` reading that spuriously triggered `SuspendVideo` before any real signal arrived.

### Closure notes (demoted ex-ADRs; behavior-relevant, not new APIs)

- **Open-Q1 closed:** the earlier open question — whether str0m exposes caller-assigned RTP sequence numbers so `BandwidthEstimator::record_send_time` could be auto-fed — is settled with primary str0m 0.21 source evidence. str0m's `DirectApi::stream_tx` + `RtpWrite` *does* expose caller-assigned seq, but **only** under `set_rtp_mode(true)`, which is mutually exclusive per-`Rtc` with the kit's own `Writer` path used everywhere else in this crate. `Registry::record_send_time` therefore stays consumer-only, unchanged — `DirectApi` raw-forward under `rtp_mode` remains a bounded, not-yet-built Phase-6 follow-up.
- **Receiver-side abs-capture-time introduces no new external input:** str0m 0.20+ added a receiver-side `abs_capture_time` extension value. `SfuMediaPayload::from_str0m` (`src/media.rs`) reads only `ext_vals.audio_level` from `str0m::media::MediaData` — no current `from_str0m`/BWE code path reads `abs_capture_time`, so v0.12.0 introduces no new attacker-influenced input into the BWE loop. If a future phase wires it into the estimator, input-bounds validation is required first.
- **`RtpWrite` is a no-op / no new public str0m surface:** the `RtpWrite` type surfaced by str0m's RTP-mode API (above) is not used anywhere in this release — it is mentioned only because it is the type that would carry a caller-assigned seq if/when `DirectApi` raw-forward is built. No public API in this crate exposes it today.

---

## Migration: earlier versions

### From v0.11.0 → v0.11.4

**The breaking thing that wasn't:** v0.11.0 shipped `PerSubscriber.googcc: Option<GoogCcEstimator>` as `pub`, but `BandwidthEstimator::get_or_insert` was `pub(crate)`. Result: external consumers could not enable per-subscriber GoogCC. They had to either run a registry-level shared `GoogCcEstimator` (race condition between subscribers), or carry a parallel field on their own per-subscriber struct.

**v0.11.4 closes the gap:**
```rust
// OLD (v0.11.0–v0.11.3) — workaround in oxpulse-partner-edge:
struct Client {
    googcc: GoogCcEstimator,  // duplicate state, not in BandwidthEstimator
}
// Manual feed in poll loop, manual read in pacer driver.

// NEW (v0.11.4) — canonical path:
bwe.enable_googcc_for_subscriber(id);                  // on join
bwe.googcc_for_subscriber_mut(id).unwrap().on_receive(...);  // on TWCC
let combined = bwe.estimate_bps(id, now);              // single read combines all
```

oxpulse-partner-edge migrated in PR #116 (commit a644059). Net delta: **−744 LOC**.

### From v0.11.2 → v0.11.3

- MSRV bumped 1.86 → 1.88. `time@0.3.47` is transitive via `str0m → dimpl → time`. Bump your own `rust-version` if you pin one.

### From v0.10 → v0.11.0

See [CHANGELOG.md](../CHANGELOG.md#0110--2026-05-14). Major: per-subscriber GoogCC, `SubscriberPacer` + `PacerConfig` extracted, `From<SfuRid> for str0m::media::Rid` (v0.11.1).

---

## Invariants

| Constant | Value | Where |
|---|---|---|
| `INITIAL_BITRATE_BPS` | 300 000 | `bwe::subscriber` (PerSubscriber initial) |
| `CLIENT_HINT_MAX_AGE` | 5 s | `bwe::subscriber` (hint expiry) |
| `MAX_SEND_TIMES` | 512 | `bwe::estimator` (per-subscriber send-time map cap) |
| `GOOGCC_INITIAL_BPS` | 500 000 | `bwe::googcc` |
| `GOOGCC_MIN_BPS` | 100 000 | `bwe::googcc` |
| `GOOGCC_MAX_BPS` | 10 000 000 | `bwe::googcc` |
| `PacerConfig::default().suspend_video_bps` | 10 000 | |
| `PacerConfig::default().audio_only_bps`    | 80 000 | |
| `PacerConfig::default().low_min_bps`       | 150 000 | |
| `PacerConfig::default().medium_min_bps`    | 350 000 | |
| `PacerConfig::default().high_min_bps`      | 700 000 | |
| `PacerConfig::default().suspend_streak`    | 2 | |
| `PacerConfig::default().upgrade_streak`    | 3 | |
| `PACER_MIN_TICK_INTERVAL` | 100 ms | `bwe` (v0.12.0 — min-tick floor gating `Registry::update_pacer_layers`, ADR-13) |

`oxpulse-partner-edge` overrides defaults with its production-tuned values:

```rust
PacerConfig {
    audio_only_bps:    100_000,
    low_min_bps:       150_000,
    medium_min_bps:    500_000,
    high_min_bps:    1_500_000,
    suspend_video_bps:  10_000,
    suspend_streak: 2,
    upgrade_streak: 3,
}
```

These are documented but not enforced — choose values that match your network model.

---

## Error Model

The kit minimises returned `Result` types — most APIs are infallible, with bad inputs producing degraded estimates rather than errors. Two exceptions:

- `PacerConfig::validate() -> Result<(), PacerConfigError>` — explicit invariant check.
- `SfuRid::from_str(s) -> Result<SfuRid, InvalidRid>` — string parsing (out of scope here).

`debug_assert!` is used liberally for "you broke the contract" cases (non-finite inputs, zero streaks). In release builds these are silent — the kit follows the principle "don't panic in a media pipeline." Always run `validate()` upstream.

---

## Stability & SemVer

- **Stable:** all symbols listed above. Removed methods or changed signatures will trigger a major version bump.
- **New in v0.12.0, unproven-at-scale:** the `Registry`-level surface in [§9](#registry-level-ownership--behavior) (`bandwidth()`, `enable_googcc_for_subscriber`, `googcc_ceiling_for_subscriber_mut`, `update_pacer_layers`'s new `now` parameter) is semver-stable going forward but has only in-kit test/example exercise as of this release — see the production-unproven callout at the top of this document.
- **`#[non_exhaustive]`:** `PacerAction`, `BandwidthState`. Match with `_ =>` for forward compatibility.
- **`#[doc(hidden)]`:** `force_*_for_tests` methods. Available under `feature = "test-utils"`. Subject to change without notice.
- **`raw::*`:** explicitly semver-exempt — direct str0m re-exports for advanced consumers. As of v0.12.0 these resolve to str0m 0.21 types (was 0.18.1) — see [Migration](#migration-v0118v0119--v0120).

---

## See Also

- [docs.rs/oxpulse-sfu-kit](https://docs.rs/oxpulse-sfu-kit) — per-symbol rustdoc (pending v0.12.0 publish; 0.11.9 is the currently-published version).
- [`examples/with_googcc.rs`](../examples/with_googcc.rs) — standalone `GoogCcEstimator` recipe.
- [`examples/pacer_basic.rs`](../examples/pacer_basic.rs) — standalone `SubscriberPacer` recipe.
- [`examples/twcc_googcc_wiring.rs`](../examples/twcc_googcc_wiring.rs) — advanced RTP-level-consumer-only TWCC→GoogCC wiring (primitives-level; still valid post-v0.12.0, unchanged by this release — see [ADR-5 / the primitives-level recipe](#wiring-recipes)).
- [`examples/synthetic_room.rs`](../examples/synthetic_room.rs) — multi-peer synthetic room scaffold.
- [CHANGELOG.md](../CHANGELOG.md) — version-by-version diff.
- [README.md](../README.md) — quick-start + Client/Registry overview.
