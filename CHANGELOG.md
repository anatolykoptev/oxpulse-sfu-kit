# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
