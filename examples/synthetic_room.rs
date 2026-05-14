//! Synthetic room load generator — D-Lite 3.
// Examples are separate compilation units — unsafe is isolated here and does
// not affect the library crate's #![forbid(unsafe_code)] invariant.
#![allow(unsafe_code)]
//!
//! Spawns N synthetic peers in one process, each publishing a fake video track,
//! all subscribing to each other. Runs for a configurable duration and reports
//! kit-level performance metrics.
//!
//! ## What is measured
//!
//! - Registry fanout dispatch (`Registry::fanout_for_tests`) for N×(N-1) routes.
//! - Per-subscriber simulcast layer-filter logic (`handle_media_data_out`).
//! - `delivered_media` counter increments (hot-path atomics).
//! - Wall-clock throughput: packets forwarded per second.
//! - p50/p95/p99 latency of a single `fanout_for_tests` call (μs).
//!
//! ## What is NOT measured
//!
//! - str0m RTP packetization, SRTP encrypt/decrypt, DTLS, ICE — all bypassed.
//! - Real UDP kernel network stack (no sockets opened).
//! - Actual wire bitrate — `--bitrate-bps` drives packet-size intent but
//!   `make_media_data` produces a 4-byte synthetic payload regardless.
//!   The flag is preserved for D-Lite 4 scripting convention; effective payload
//!   size = 4 bytes per packet.
//! - GoogCC/pacer adapting to real TWCC feedback — features are compiled in but
//!   no BWE feedback loop is driven in the synthetic path.
//!
//! ## Run
//!
//! ```not_rust
//! cargo run --release --example synthetic_room \
//!     --features active-speaker,metrics-prometheus,kalman-bwe,googcc-bwe,pacer,test-utils \
//!     -- --peers 8 --duration-secs 30 --packet-rate-pps 30 --bitrate-bps 1500000
//! ```
//!
//! ## Output
//!
//! ```text
//! SYNTHETIC_ROOM_RESULT peers=8 duration_s=30 packets_forwarded=72000 peak_rss_mb=124 cpu_percent=18.3 latency_p50_us=85 latency_p95_us=210 latency_p99_us=450
//! ```

use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::Parser;
use hdrhistogram::Histogram;
use str0m::media::MediaKind;

use oxpulse_sfu_kit::client::test_seed::{make_media_data, new_client, seed_track_in};
use oxpulse_sfu_kit::metrics::SfuMetrics;
use oxpulse_sfu_kit::{ClientId, Propagated, Registry};

/// Synthetic room load generator for oxpulse-sfu-kit.
#[derive(Parser, Debug)]
#[command(name = "synthetic_room", about = "D-Lite 3 kit fanout load generator")]
struct Args {
    /// Number of synthetic peers in the room.
    #[arg(long, default_value_t = 8)]
    peers: usize,

    /// How long to run (wall-clock seconds).
    #[arg(long, default_value_t = 30)]
    duration_secs: u64,

    /// Simulated packet rate per publisher (packets per second).
    #[arg(long, default_value_t = 30)]
    packet_rate_pps: u64,

    /// Target bitrate per publisher in bits/sec (used for documentation; actual
    /// payload is synthetic 4 bytes — see module docs).
    #[arg(long, default_value_t = 1_500_000)]
    bitrate_bps: u64,
}

fn main() {
    let args = Args::parse();
    assert!(args.peers >= 2, "--peers must be >= 2");
    assert!(args.packet_rate_pps > 0, "--packet-rate-pps must be > 0");

    // Log the effective packet size (purely informational — actual payload is 4 bytes).
    let target_bytes_per_packet = args.bitrate_bps / args.packet_rate_pps / 8;
    let effective_bytes = target_bytes_per_packet.min(1200);
    eprintln!(
        "synthetic_room: peers={} duration_secs={} pps={} target_payload_bytes={} effective_payload_bytes=4 (synthetic; actual payload is always 4 bytes)",
        args.peers, args.duration_secs, args.packet_rate_pps, effective_bytes,
    );

    // -- Build registry and insert synthetic peers ----------------------------
    let metrics = Arc::new(SfuMetrics::new_default());
    let mut registry = Registry::new(metrics);

    let peer_ids: Vec<ClientId> = (0..args.peers as u64).map(|i| ClientId(1000 + i)).collect();

    // Each peer gets one video track seeded with mid_tag = peer index.
    // seed_track_in must be called BEFORE insert so cross-advertisement fires.
    for (idx, &id) in peer_ids.iter().enumerate() {
        let mut client = new_client(id);
        seed_track_in(&mut client, idx as u8, MediaKind::Video);
        registry.insert(client);
    }
    // After all inserts, Registry::insert has cross-advertised every existing
    // peer's track to each newcomer — full N×(N-1) subscription graph is wired.

    // -- Timing setup ---------------------------------------------------------
    // hdrhistogram: track μs, 3 sig figs, range 1μs..1s.
    let mut hist: Histogram<u64> =
        Histogram::new_with_bounds(1, 1_000_000, 3).expect("hdrhistogram init");

    let interval = Duration::from_micros(1_000_000 / args.packet_rate_pps);
    let deadline = Instant::now() + Duration::from_secs(args.duration_secs);

    let mut packets_forwarded: u64 = 0;
    let wall_start = Instant::now();

    // -- Main drive loop (synchronous, no tokio) ------------------------------
    // Each iteration: every publisher sends one packet; fanout routes it to
    // all (N-1) subscribers. Sleep to maintain target PPS cadence.
    while Instant::now() < deadline {
        let iter_start = Instant::now();

        for (idx, &publisher_id) in peer_ids.iter().enumerate() {
            let data = make_media_data(idx as u8, None);
            let prop = Propagated::MediaData(publisher_id, data);

            let t0 = Instant::now();
            registry.fanout_for_tests(&prop);
            let elapsed_us = t0.elapsed().as_micros() as u64;

            // Saturating add to avoid panic on histogram overflow (very large latency).
            let _ = hist.record(elapsed_us.max(1));
            // Each fanout delivers to (peers - 1) subscribers.
            packets_forwarded += (args.peers as u64).saturating_sub(1);
        }

        // Pace to target PPS: sleep the remainder of the interval.
        let elapsed = iter_start.elapsed();
        if elapsed < interval {
            std::thread::sleep(interval - elapsed);
        }
    }

    let wall_elapsed = wall_start.elapsed();

    // -- Resource usage -------------------------------------------------------
    let (peak_rss_mb, cpu_user_s, cpu_sys_s) = read_rusage();
    let cpu_percent = (cpu_user_s + cpu_sys_s) / wall_elapsed.as_secs_f64() * 100.0;

    // -- Report ---------------------------------------------------------------
    let p50 = hist.value_at_quantile(0.50);
    let p95 = hist.value_at_quantile(0.95);
    let p99 = hist.value_at_quantile(0.99);

    println!(
        "SYNTHETIC_ROOM_RESULT peers={} duration_s={} packets_forwarded={} peak_rss_mb={:.1} cpu_percent={:.1} latency_p50_us={} latency_p95_us={} latency_p99_us={}",
        args.peers,
        wall_elapsed.as_secs_f64() as u64,
        packets_forwarded,
        peak_rss_mb,
        cpu_percent,
        p50,
        p95,
        p99,
    );
}

/// Read peak RSS (MB) and CPU user+sys time (seconds) via `getrusage(RUSAGE_SELF)`.
///
/// - macOS: `ru_maxrss` is bytes.
/// - Linux: `ru_maxrss` is kilobytes.
fn read_rusage() -> (f64, f64, f64) {
    // SAFETY: getrusage is safe when called with a zeroed rusage struct.
    // We only read the returned values, never aliasing anything live.
    #[allow(unsafe_code)]
    let ru = unsafe {
        let mut ru: libc::rusage = std::mem::zeroed();
        libc::getrusage(libc::RUSAGE_SELF, &mut ru);
        ru
    };

    #[cfg(target_os = "macos")]
    let peak_rss_mb = ru.ru_maxrss as f64 / (1024.0 * 1024.0);
    #[cfg(not(target_os = "macos"))]
    let peak_rss_mb = ru.ru_maxrss as f64 / 1024.0;

    let user_s = ru.ru_utime.tv_sec as f64 + ru.ru_utime.tv_usec as f64 / 1_000_000.0;
    let sys_s = ru.ru_stime.tv_sec as f64 + ru.ru_stime.tv_usec as f64 / 1_000_000.0;

    (peak_rss_mb, user_s, sys_s)
}
