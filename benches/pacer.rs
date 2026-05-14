#![allow(missing_docs)]

use criterion::{criterion_group, criterion_main, Criterion};
use oxpulse_sfu_kit::bwe::{
    SubscriberPacer, HIGH_MIN_BPS, LOW_MIN_BPS, MEDIUM_MIN_BPS, SUSPEND_VIDEO_BPS,
};

/// NoChange path: BWE is stable well above HIGH tier — pacer should return NoChange
/// without touching any counters.
fn bench_update_steady_high_bps(c: &mut Criterion) {
    c.bench_function("pacer/update/steady_high_bps", |b| {
        b.iter_batched(
            || {
                let mut pacer = SubscriberPacer::new();
                // Drive the pacer to HIGH tier so the steady-state path is the
                // "already at highest layer, NoChange" fast path.
                for _ in 0..4 {
                    let _ = pacer.update(HIGH_MIN_BPS + 200_000);
                }
                pacer
            },
            |mut pacer| {
                let _ = pacer.update(HIGH_MIN_BPS + 200_000);
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

/// Upgrade path: BWE is consistently above MEDIUM_MIN_BPS — after UPGRADE_STREAK
/// ticks the pacer emits ChangeLayer.  Measure the cost of one tick that
/// lands on the transition tick.
fn bench_update_upgrade_transition(c: &mut Criterion) {
    c.bench_function("pacer/update/upgrade_transition", |b| {
        b.iter_batched(
            || {
                let mut pacer = SubscriberPacer::new();
                // Reach LOW tier first.
                for _ in 0..4 {
                    let _ = pacer.update(LOW_MIN_BPS + 10_000);
                }
                // Prime upgrade streak to UPGRADE_STREAK - 1 = 2 ticks above MEDIUM.
                let _ = pacer.update(MEDIUM_MIN_BPS + 10_000);
                let _ = pacer.update(MEDIUM_MIN_BPS + 10_000);
                pacer
            },
            |mut pacer| {
                // Third consecutive tick above MEDIUM — triggers ChangeLayer.
                let _ = pacer.update(MEDIUM_MIN_BPS + 10_000);
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

/// Downgrade path: BWE collapses from HIGH to below LOW — immediate downgrade.
fn bench_update_downgrade(c: &mut Criterion) {
    c.bench_function("pacer/update/downgrade", |b| {
        b.iter_batched(
            || {
                let mut pacer = SubscriberPacer::new();
                // Drive to HIGH tier.
                for _ in 0..4 {
                    let _ = pacer.update(HIGH_MIN_BPS + 200_000);
                }
                pacer
            },
            |mut pacer| {
                // Collapse to just above SUSPEND_VIDEO_BPS but below LOW.
                let _ = pacer.update(SUSPEND_VIDEO_BPS + 5_000);
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

criterion_group!(
    benches,
    bench_update_steady_high_bps,
    bench_update_upgrade_transition,
    bench_update_downgrade,
);
criterion_main!(benches);
