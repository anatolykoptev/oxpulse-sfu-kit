# oxpulse-sfu-kit — Public API Specification

**Version:** 0.11.4
**Date:** 2026-05-14
**Status:** Stable (semver-respected; MSRV 1.88)

This document specifies every consumer-facing public API in the `bwe` (Bandwidth Estimation) and `pacer` modules of `oxpulse-sfu-kit`. It complements [docs.rs](https://docs.rs/oxpulse-sfu-kit) by giving an integration-oriented view: contracts, invariants, wiring recipes, and migration notes for consumers upgrading from earlier versions.

For the broader kit overview (Client, Registry, Propagated, signaling stubs) see [README](../README.md). This document covers BWE only — that's the surface area extended in v0.11.x.

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
9. [End-to-end wiring recipe](#wiring-recipe)
10. [Migration: v0.11.0 → v0.11.4](#migration)
11. [Invariants & contracts summary](#invariants)
12. [Error model](#error-model)

---

## Feature Gates

All BWE/pacer APIs are gated. Enable in `Cargo.toml`:

```toml
[dependencies]
oxpulse-sfu-kit = { version = "0.11.4", features = ["kalman-bwe", "googcc-bwe", "pacer"] }
```

| Feature | Enables | Depends on |
|---|---|---|
| `kalman-bwe` | `BandwidthEstimator`, `PerSubscriber`, `DelayEstimator`, `LossEstimator`, `TwccFeedback`, `ClientHint` | none |
| `googcc-bwe` | `GoogCcEstimator`, `enable_googcc_for_subscriber`, `googcc_for_subscriber_mut`, `BandwidthState` | `kalman-bwe` (recommended; the GoogCc API itself works standalone but is wired into `PerSubscriber.googcc` only when both are enabled) |
| `pacer` | `SubscriberPacer`, `PacerConfig`, `PacerConfigError`, `PacerAction` | none |

All three features are independent. A consumer can wire pacer without GoogCC, or GoogCC without pacer.

**MSRV:** 1.88 (transitive `time@0.3.47` requirement via `str0m → dimpl → time`).

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

## Wiring Recipe

End-to-end, all features enabled:

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

## Migration

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
- **`#[non_exhaustive]`:** `PacerAction`, `BandwidthState`. Match with `_ =>` for forward compatibility.
- **`#[doc(hidden)]`:** `force_*_for_tests` methods. Available under `feature = "test-utils"`. Subject to change without notice.
- **`raw::*`:** explicitly semver-exempt — direct str0m re-exports for advanced consumers.

---

## See Also

- [docs.rs/oxpulse-sfu-kit](https://docs.rs/oxpulse-sfu-kit/0.11.4) — per-symbol rustdoc.
- [`examples/twcc_googcc_wiring.rs`](../examples/twcc_googcc_wiring.rs) — runnable integration recipe.
- [CHANGELOG.md](../CHANGELOG.md) — version-by-version diff.
- [README.md](../README.md) — quick-start + Client/Registry overview.
