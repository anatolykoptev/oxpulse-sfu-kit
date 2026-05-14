#![allow(missing_docs)]

use criterion::{criterion_group, criterion_main, Criterion};
use oxpulse_sfu_kit::bwe::feedback::{TwccFeedback, TwccSample};
use oxpulse_sfu_kit::BandwidthEstimator;
use oxpulse_sfu_kit::ClientId;
use std::time::{Duration, Instant};

fn make_id(n: u64) -> ClientId {
    ClientId(n)
}

/// Fill the send_times map up to MAX_SEND_TIMES - 1 (511 entries) so the next
/// insert is a steady-state append with no eviction.
fn pre_fill_steady(est: &mut BandwidthEstimator, id: ClientId, count: usize, base: Instant) {
    for i in 0..count {
        est.record_send_time(id, i as u64, base + Duration::from_micros(i as u64 * 10));
    }
}

fn bench_record_send_time_steady(c: &mut Criterion) {
    let base = Instant::now();
    let id = make_id(1);

    c.bench_function("bwe_estimator/record_send_time/steady", |b| {
        b.iter_batched(
            || {
                let mut est = BandwidthEstimator::new();
                // Fill to 511 (just under cap of 512); next insert won't evict.
                pre_fill_steady(&mut est, id, 511, base);
                (est, 512u64)
            },
            |(mut est, seq)| {
                est.record_send_time(id, seq, base + Duration::from_micros(seq * 10));
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_record_send_time_at_cap(c: &mut Criterion) {
    let base = Instant::now();
    let id = make_id(2);

    c.bench_function("bwe_estimator/record_send_time/at_cap", |b| {
        b.iter_batched(
            || {
                let mut est = BandwidthEstimator::new();
                // Fill exactly to cap (512 entries); next insert triggers O(n) eviction.
                pre_fill_steady(&mut est, id, 512, base);
                (est, 512u64)
            },
            |(mut est, seq)| {
                est.record_send_time(id, seq, base + Duration::from_micros(seq * 10));
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_on_twcc_feedback(c: &mut Criterion) {
    let base = Instant::now();
    let id = make_id(3);

    // Build a 50-sample feedback batch (realistic: ~50 packets per TWCC interval at 30pps).
    let feedback = TwccFeedback {
        samples: (0u64..50)
            .map(|i| TwccSample {
                seq: i,
                arrival: Some(base + Duration::from_millis(20 + i * 33)),
            })
            .collect(),
    };

    c.bench_function("bwe_estimator/on_twcc_feedback/50_samples", |b| {
        b.iter_batched(
            || {
                let mut est = BandwidthEstimator::new();
                // Pre-populate send times so ingest_twcc has gradient data.
                for i in 0u64..50 {
                    est.record_send_time(id, i, base + Duration::from_millis(i * 33));
                }
                est
            },
            |mut est| {
                est.on_twcc_feedback(id, &feedback, Instant::now());
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_estimate_bps(c: &mut Criterion) {
    let base = Instant::now();
    let id = make_id(4);

    // Pre-warm the estimator: feed native estimate and client hint so combined_bps
    // does real work (not early-exit on None).
    let mut est = BandwidthEstimator::new();
    est.record_native_estimate(id, 1_500_000.0);
    est.record_client_hint(id, 1_000_000, base);

    c.bench_function("bwe_estimator/estimate_bps/after_feed", |b| {
        b.iter(|| est.estimate_bps(id, Instant::now()));
    });
}

criterion_group!(
    benches,
    bench_record_send_time_steady,
    bench_record_send_time_at_cap,
    bench_on_twcc_feedback,
    bench_estimate_bps,
);
criterion_main!(benches);
