//! A reusable multi-client SFU (Selective Forwarding Unit) kit built on top of
//! [str0m](https://github.com/algesten/str0m).
//!
//! str0m is a sans-I/O Rust WebRTC library — you plug in your own networking.
//! This crate adds the multi-client glue: per-peer state machines, UDP packet
//! routing, event fanout, and simulcast layer forwarding. It does **not** replace
//! str0m; it connects multiple str0m [`Rtc`][str0m::Rtc] instances together into
//! a functioning room.
//!
//! # What this gives you
//!
//! - **[`Client`]** — per-peer state wrapping a str0m `Rtc` instance.
//!   Handles `poll_output`, incoming datagrams, keyframe requests, and simulcast
//!   layer filtering.
//! - **[`Registry`]** — room-level packet router. Routes UDP datagrams to the
//!   correct peer via `rtc.accepts()`, drives `poll_all`, and fans out events to
//!   every non-origin peer.
//! - **[`Propagated`]** — the event enum flowing between the registry and clients.
//! - **[`SfuConfig`]** — runtime configuration (UDP port, bind address).
//! - **[`run_udp_loop`]** — a ready-to-use async UDP
//!   receive loop for the simple single-room case.
//!
//! # Quick start
//!
//! ```no_run
//! use std::sync::Arc;
//! use str0m::Rtc;
//! use str0m_sfu_kit::{Client, Registry, SfuConfig, udp_loop};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let config = SfuConfig {
//!         udp_port: 3478,
//!         ..SfuConfig::default()
//!     };
//!
//!     // Shutdown on Ctrl-C.
//!     let shutdown = async { tokio::signal::ctrl_c().await.unwrap() };
//!     udp_loop::run_udp_loop(config, shutdown).await
//! }
//! ```
//!
//! Clients are inserted into the registry via [`Registry::insert`] after
//! completing ICE/DTLS signaling (your responsibility — bring your own
//! WebSocket/HTTP signaling layer).
//!
//! # Feature flags
//!
//! | Feature | What it enables |
//! |---------|----------------|
//! | `active-speaker` | Dominant speaker detection via [`rust-dominant-speaker`](https://crates.io/crates/rust-dominant-speaker). Adds [`Propagated::ActiveSpeakerChanged`] and [`Registry::tick_active_speaker`] / [`Registry::record_audio_level`]. |
//! | `metrics-prometheus` | Prometheus counters exposed via [`SfuMetrics`]. The library carries the handles; you choose how to expose them (e.g. via axum). |
//! | `test-utils` | Exposes test seam helpers (`test_seed` module, `Registry::*_for_tests` methods). Gate your own tests on this. |
//!
//! # Not included (by design)
//!
//! - Signaling (bring your own — WebSocket, HTTP, gRPC)
//! - TURN server (run coturn, rfc5766-turn-server, or similar)
//! - Bandwidth estimation beyond what str0m exposes via `Event::EgressBitrateEstimate`
//! - End-to-end encryption (use SFrame; see the OxPulse Chat reference implementation)
//!
//! # Examples
//!
//! - `examples/basic-sfu.rs` — a complete single-node SFU that binds a UDP port
//!   and handles signaling stubs. Run with `cargo run --example basic-sfu --features active-speaker,metrics-prometheus`.
//!
//! # Relationship to str0m
//!
//! We build on str0m's `Rtc` state machine. We do not replace it — we connect
//! multiple instances together for multi-party rooms. All credit for the
//! underlying protocol work goes to [Martin Algesten](https://github.com/algesten)
//! and the str0m contributors.
//!
//! # Extracted from
//!
//! Originally built as part of [OxPulse Chat](https://oxpulse.chat).
//! Published standalone for the broader Rust WebRTC ecosystem.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod client;
pub mod config;
pub mod fanout;
pub mod metrics;
pub mod propagate;
pub mod registry;
pub mod udp_loop;

pub use client::Client;
pub use config::SfuConfig;
pub use metrics::SfuMetrics;
pub use propagate::{ClientId, Propagated};
pub use registry::Registry;
pub use udp_loop::run_udp_loop;
