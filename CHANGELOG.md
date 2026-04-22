# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
