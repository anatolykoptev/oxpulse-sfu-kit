# oxpulse-sfu-kit

[![CI](https://github.com/anatolykoptev/oxpulse-sfu-kit/actions/workflows/ci.yml/badge.svg)](https://github.com/anatolykoptev/oxpulse-sfu-kit/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/oxpulse-sfu-kit.svg)](https://crates.io/crates/oxpulse-sfu-kit)
[![docs.rs](https://docs.rs/oxpulse-sfu-kit/badge.svg)](https://docs.rs/oxpulse-sfu-kit)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![MSRV](https://img.shields.io/badge/MSRV-1.86-blue.svg)](#status)

Reusable multi-client SFU primitives built on top of [str0m](https://github.com/algesten/str0m).

str0m is a sans-I/O Rust WebRTC library — you plug in your own networking.
This crate adds the multi-client glue: per-peer state machines, UDP packet routing,
event fanout, simulcast layer forwarding, and bandwidth-adaptive layer selection.

## What this gives you

- **`Client`** — per-peer state machine wrapping `str0m::Rtc`
- **`Registry`** — room-level UDP routing and event fanout
- **`Propagated`** — event enum flowing between registry and clients
- **`LayerSelector` + `BestFitSelector`** — per-subscriber simulcast layer selection using desired layer + publisher's active RIDs
- **`ClientOrigin::RelayFromSfu`** — cascade SFU support: mark a client as an upstream relay, reroute keyframe requests and Dynacast hints upstream
- **Optional `pacer`** — BWE-adaptive layer switching (3-up/instant-down hysteresis, audio-only mode below 80 kbps)
- **Optional `av1-dd`** — AV1 Dependency Descriptor parser, per-subscriber temporal-layer drop gate
- **Optional `vfm`** — RFC 9626 Video Frame Marking for H.264/VP9/HEVC temporal-layer drop
- **Optional `active-speaker`** — dominant speaker detection with confidence margin
- **Optional `metrics-prometheus`** — Prometheus gauges including per-peer speaker activity scores

## Usage

```toml
[dependencies]
oxpulse-sfu-kit = "0.5"
```

Minimal run loop:

```rust,no_run
use oxpulse_sfu_kit::{SfuConfig, udp_loop};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = SfuConfig { udp_port: 3478, ..SfuConfig::default() };
    let shutdown = async { tokio::signal::ctrl_c().await.unwrap() };
    udp_loop::run_udp_loop(config, shutdown).await
}
```

Insert a peer after completing ICE/DTLS signaling:

```rust,no_run
use oxpulse_sfu_kit::{Client, Registry, SfuRtcBuilder, SfuMetrics};
use std::sync::Arc;

let mut registry = Registry::new(Arc::new(SfuMetrics::new_default()));
let rtc = SfuRtcBuilder::new().build();
let client = Client::new(rtc, Arc::new(SfuMetrics::new_default()));
registry.insert(client);
```

Mark a client as a relay from another SFU edge (call **before** `registry.insert`):

```rust,no_run
use oxpulse_sfu_kit::{Client, ClientOrigin};

let mut relay_client = Client::new(rtc, metrics);
relay_client.set_origin(ClientOrigin::RelayFromSfu("edge-eu-1".to_string()));
registry.insert(relay_client);
// Keyframe requests for relay-originated tracks now emit
// Propagated::UpstreamKeyframeRequest instead of sending PLI/FIR to the relay.
```

## Feature flags

| Flag | What it does |
|------|--------------|
| `kalman-bwe` | GoogCC-inspired Kalman delay + loss-based BWE. `BandwidthEstimator` with TWCC ingestion. `Registry::update_pacer_layers` for automatic layer selection. Enable with `pacer` for full adaptive forwarding. |
| `pacer` | BWE-adaptive layer switching via `SubscriberPacer` (LiveKit-style 3-up/instant-down hysteresis). Adds `Propagated::AudioOnlyMode` at 80 kbps threshold. |
| `av1-dd` | AV1 Dependency Descriptor parser (`av1::dependency_descriptor`). `SfuMediaPayload::av1_dd()` accessor. `Client::set_max_temporal_layer(u8)` per-subscriber drop gate. |
| `vfm` | RFC 9626 Video Frame Marking parser for H.264/VP9/HEVC. `SfuMediaPayload::vfm_frame_marking()`. `Client::set_max_vfm_temporal_layer(u8)`. |
| `active-speaker` | Dominant speaker tracking via [`rust-dominant-speaker`](https://crates.io/crates/rust-dominant-speaker). `Propagated::ActiveSpeakerChanged { peer_id, confidence }`. `Registry::tick_active_speaker` / `record_audio_level` / `peer_audio_scores`. |
| `metrics-prometheus` | Prometheus counters on `SfuMetrics`, including per-peer BWE, loss, RTT, and speaker activity gauges. |
| `test-utils` | Test seam helpers (`test_seed` module, `Registry::*_for_tests` methods). |

## Audio quality guidance

### Publisher-side noise filtering

For cleaner dominant-speaker elections, publishers should filter audio through a
noise suppressor before computing the RFC 6464 level:

- **RNNoise** (`xiph/rnnoise`, BSD-3-Clause) — DSP/DNN hybrid, runs on mobile.
- **ten-vad** (`TEN-framework/ten-vad`, MIT) — small CPU-friendly VAD alternative.

### Opus DRED (Deep REDundancy)

Opus DRED (libopus ≥ 1.4, shipping in recent Chromium) embeds a neural-decoded
redundant stream at ≈1 kbps overhead. The SFU forwards it transparently — no kit
changes required. Signal DRED capability with `Propagated::AudioCodecHint`.

### End-to-end encryption (SFrame)

The kit forwards RTP payloads opaquely — SFrame (RFC 9605) frames pass through
unchanged. Use `KeyEpoch` from `crate::sframe` to forward the key-epoch RTP
header extension. Key distribution (MLS RFC 9420) is your signalling layer's
responsibility.

## Not included (by design)

- Signaling (bring your own — WebSocket, HTTP, gRPC)
- TURN server (run coturn or similar)
- End-to-end encryption payload processing (use SFrame; see `sframe::KeyEpoch`)
- Server-side audio/video mixing (MCU mode)
- WHIP / WHEP ingestion endpoints

## Examples

```sh
cargo run --example basic-sfu --features active-speaker,metrics-prometheus
```

See `examples/basic-sfu.rs` for a complete single-node SFU with a Prometheus
`/metrics` endpoint.

## Relationship to str0m

We build on str0m's `Rtc` state machine. We do not replace it — we connect
multiple instances together for multi-party rooms. All credit for the underlying
protocol work goes to [Martin Algesten](https://github.com/algesten) and the
str0m contributors.

## Extracted from

Originally built as part of [OxPulse Chat](https://oxpulse.chat). Published
standalone for the broader Rust WebRTC ecosystem.

## Status

API is stabilising through v0.x. Minor breaking changes may occur between minors;
check `CHANGELOG.md` before upgrading. Stability commitment from v1.0.

## License

Dual MIT / Apache-2.0. See [LICENSE-MIT](LICENSE-MIT) and
[LICENSE-APACHE](LICENSE-APACHE).
