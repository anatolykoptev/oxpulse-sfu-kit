#![allow(missing_docs)]

use criterion::{criterion_group, criterion_main, Criterion};
use oxpulse_sfu_kit::bwe::googcc::GoogCcEstimator;

/// Simulate 30 packets/sec at steady state: uniform inter-arrival == inter-send
/// (no congestion). The estimator should stay in Normal state and slowly increase.
fn bench_on_receive_steady(c: &mut Criterion) {
    // Packet interval: ~33 ms (30 pps)
    const INTERVAL_MS: f64 = 33.0;

    c.bench_function("googcc/on_receive/steady_30pps", |b| {
        b.iter_batched(
            || {
                // Pre-warm: feed 60 packets so the trendline window is primed.
                // This avoids measuring warm-up in the bench iteration.
                let mut est = GoogCcEstimator::new();
                for i in 0u32..60 {
                    let t = INTERVAL_MS * (1.0 + i as f64);
                    est.on_receive(t, t, 0.001);
                }
                (est, 61u32)
            },
            |(mut est, start_i)| {
                // One steady-state packet per iteration.
                let t = INTERVAL_MS * start_i as f64;
                est.on_receive(t, t, 0.001);
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

/// Simulate a congestion burst: arrival grows faster than send (overuse).
/// The estimator should enter OverUse and drive AIMD decrease.
fn bench_on_receive_overuse(c: &mut Criterion) {
    const INTERVAL_MS: f64 = 33.0;

    c.bench_function("googcc/on_receive/overuse_burst", |b| {
        b.iter_batched(
            || {
                let mut est = GoogCcEstimator::new();
                // Establish baseline first.
                for i in 0u32..60 {
                    let t = INTERVAL_MS * (1.0 + i as f64);
                    est.on_receive(t, t, 0.001);
                }
                (est, 61u32)
            },
            |(mut est, i)| {
                // Growing arrival delta signals overuse.
                let send_t = INTERVAL_MS * i as f64;
                let arr_t = send_t + i as f64 * 2.0; // arrival delay grows linearly
                est.on_receive(arr_t, send_t, 0.001);
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

criterion_group!(benches, bench_on_receive_steady, bench_on_receive_overuse);
criterion_main!(benches);
