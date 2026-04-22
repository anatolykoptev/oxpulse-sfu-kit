# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
