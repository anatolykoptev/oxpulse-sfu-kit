# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
