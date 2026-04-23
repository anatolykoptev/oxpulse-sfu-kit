# oxpulse-sfu-kit

[![CI](https://github.com/anatolykoptev/oxpulse-sfu-kit/actions/workflows/ci.yml/badge.svg)](https://github.com/anatolykoptev/oxpulse-sfu-kit/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/oxpulse-sfu-kit.svg)](https://crates.io/crates/oxpulse-sfu-kit)
[![docs.rs](https://docs.rs/oxpulse-sfu-kit/badge.svg)](https://docs.rs/oxpulse-sfu-kit)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![MSRV](https://img.shields.io/badge/MSRV-1.86-blue.svg)](#status)

Reusable multi-client SFU primitives built on top of [str0m](https://github.com/algesten/str0m).

str0m is a sans-I/O Rust WebRTC library — you plug in your own networking.
This crate adds the multi-client glue that str0m's `examples/chat.rs` leaves as
an exercise: per-peer state machines, UDP packet routing, event fanout, and
simulcast layer forwarding.

## What this gives you

- **`Client`** — per-peer state machine wrapping `str0m::Rtc`
- **`Registry`** — room-level UDP routing and event fanout
- **`Propagated`** — the event enum flowing between registry and clients
- **Simulcast layer forwarding** — per-subscriber RID filter (`q`/`h`/`f`)
- **Optional**: dominant speaker detection (`active-speaker` feature)
- **Optional**: Prometheus metrics (`metrics-prometheus` feature)

## Audio quality guidance

### Publisher-side noise filtering

The dominant-speaker detector reads RFC 6464 audio-level RTP header extensions.
For cleaner speaker elections, publishers should filter audio through a noise
suppressor before computing the level:

- **RNNoise** (`xiph/rnnoise`, BSD-3-Clause) — DSP/DNN hybrid, runs on mobile,
  produces clean levels even in ambient noise.
- **ten-vad** (`TEN-framework/ten-vad`, MIT) — small CPU-friendly VAD alternative.

Neither is a dependency of this kit. Wire them at the publisher (browser/native app)
before the RFC 6464 level is computed.

### Opus DRED (Deep REDundancy)

Opus DRED (`opus-dred` in libopus ≥ 1.4, shipping in recent Chromium) embeds a
neural-decoded redundant stream at ≈1 kbps overhead. The SFU forwards it transparently
— no kit changes required. Receivers with libopus ≥ 1.4 automatically decode DRED
for packet-loss concealment up to ≈1 second lookback.

To signal DRED capability between peers, emit
[`Propagated::AudioCodecHint`][crate::propagate::Propagated::AudioCodecHint] with
`opus_dred: true` and relay it through your signalling layer.

### End-to-end encryption (SFrame)

The kit forwards RTP payloads opaquely — SFrame (RFC 9605) encrypted frames pass
through unchanged. Use [`KeyEpoch`][crate::sframe::KeyEpoch] to forward the key-epoch
RTP header extension so subscribers can identify the correct decryption key.

Key distribution (MLS RFC 9420 or Double Ratchet) is your signalling layer's
responsibility. Reference implementations:
- `livekit/client-sdk-js` `src/e2ee/` (Apache-2.0)
- `element-hq/element-call` `matrixKeyProvider.ts` (AGPL-3.0)

## Not included (by design)

- Signaling (bring your own — WebSocket, HTTP, gRPC)
- TURN server (run coturn or similar)
- Bandwidth estimation beyond str0m's `Event::EgressBitrateEstimate`
- End-to-end encryption (use SFrame)

## Usage

Add to `Cargo.toml`:

```toml
[dependencies]
oxpulse-sfu-kit = "0.1"
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
use oxpulse_sfu_kit::{Client, Registry, SfuRtcBuilder};
use oxpulse_sfu_kit::metrics::SfuMetrics;
use std::sync::Arc;

let mut registry = Registry::new(Arc::new(SfuMetrics::default()));
let rtc = SfuRtcBuilder::new().build(); // or SfuRtc::from_raw(raw_rtc) for advanced use
let client = Client::new(rtc, Arc::new(SfuMetrics::default()));
registry.insert(client);
```

## Feature flags

| Flag | What it does |
|------|--------------|
| `active-speaker` | Dominant speaker tracking via [`rust-dominant-speaker`](https://crates.io/crates/rust-dominant-speaker). Adds `Propagated::ActiveSpeakerChanged` and `Registry::tick_active_speaker` / `Registry::record_audio_level`. |
| `metrics-prometheus` | Prometheus counters on `SfuMetrics`. You choose how to expose them (axum, warp, etc.). |
| `test-utils` | Test seam helpers (`test_seed` module, `Registry::*_for_tests` methods). Gate your own tests on this. |

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

## License

Dual MIT / Apache-2.0. See [LICENSE-MIT](LICENSE-MIT) and
[LICENSE-APACHE](LICENSE-APACHE).

## Status

v0.1 — Initial release. API may shift during v0.x; we commit to stability from v1.
