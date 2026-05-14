//! GoogCC per-subscriber estimator example.
//!
//! Shows how to feed packet timing and loss into `GoogCcEstimator` and how the
//! trendline overuse detector drives the AIMD bitrate controller.
//!
//! Run with:
//! ```not_rust
//! cargo run --example with_googcc --features googcc-bwe
//! ```

use oxpulse_sfu_kit::bwe::googcc::{BandwidthState, GoogCcEstimator};

fn main() {
    println!("=== GoogCcEstimator example ===\n");

    // --- Stable link (no congestion) ---
    println!("1. Stable link — bitrate should increase:");
    let mut est = GoogCcEstimator::new();
    let initial = est.current_bps();
    for i in 0u32..30 {
        let t = 20.0 * (1.0 + i as f64);
        est.on_receive(t, t, 0.001); // stable timing, 0.1% loss
    }
    println!(
        "   initial={initial} bps  ->  final={} bps  (delta=+{})",
        est.current_bps(),
        est.current_bps().saturating_sub(initial)
    );

    // --- Congested link ---
    println!("\n2. Congested link — growing delay => overuse => bitrate drops:");
    let mut est2 = GoogCcEstimator::new();
    // First get to a high bitrate
    for i in 0u32..30 {
        let t = 20.0 * (1.0 + i as f64);
        est2.on_receive(t, t, 0.001);
    }
    let high = est2.current_bps();
    // Now introduce delay buildup
    for i in 0u32..30 {
        let base = 20.0 * (31.0 + i as f64);
        est2.on_receive(base + i as f64 * 50.0, base, 0.001);
    }
    println!(
        "   peak={high} bps  ->  after overuse={} bps",
        est2.current_bps()
    );

    // --- High-loss link ---
    println!("\n3. High-loss link — AIMD multiplicative decrease:");
    let mut est3 = GoogCcEstimator::new();
    for i in 0u32..10 {
        let t = 20.0 * (1.0 + i as f64);
        est3.on_receive(t, t, 0.05); // 5% loss
    }
    println!(
        "   initial=500000 bps  ->  final={} bps",
        est3.current_bps()
    );

    // --- Custom bounds ---
    println!("\n4. Custom bounds (min=50k, max=1M, initial=200k):");
    let mut est4 = GoogCcEstimator::with_bounds(200_000, 50_000, 1_000_000);
    for i in 0u32..50 {
        let t = 20.0 * (1.0 + i as f64);
        est4.on_receive(t, t, 0.0);
    }
    println!("   final={} bps (capped at 1M)", est4.current_bps());

    println!("\nTrendline state detection example:");
    let mut est5 = GoogCcEstimator::new();
    // Feed stable, then congested
    for i in 0u32..5 {
        let t = 20.0 * (1.0 + i as f64);
        est5.on_receive(t, t, 0.0);
    }
    // Access trendline state via the googcc module
    use oxpulse_sfu_kit::bwe::googcc::TrendlineDetector;
    let mut tl = TrendlineDetector::new();
    for i in 0..25 {
        tl.update(20.0 + i as f64 * 2.0, 20.0);
    }
    println!("   Growing delay trendline state: {:?}", tl.state());
    assert_eq!(tl.state(), BandwidthState::Overuse);

    println!("\nDone. See src/bwe/googcc/ for algorithm details.");
}
