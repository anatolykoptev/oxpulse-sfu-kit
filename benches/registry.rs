#![allow(missing_docs)]
// Registry routing benchmark.
//
// `Registry::handle_incoming` is the hot path: it scans clients via
// `Client::accepts()` until one claims the datagram.  We bench with 1, 8, and
// 50 peers to expose the O(n) linear scan cost.
//
// Note: because `accepts()` calls into str0m's Rtc which requires real
// ICE/DTLS state to positively claim a datagram, a freshly-seeded test client
// will NOT accept any packet (Rtc in initial state returns false for all
// datagrams).  The bench therefore measures the "no client accepts" scan — the
// worst-case path that traverses every client before returning false.  This is
// the exact scenario that fires for every stale/STUN datagram and for any
// packet arriving before signalling completes.  It is the load-bearing path for
// capacity planning.
//
// To measure the "match on first client" fast path, a real ICE handshake would
// be required; that is out of scope for a micro-benchmark and is better covered
// by the synthetic_room example (D-Lite 3).

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use oxpulse_sfu_kit::client::test_seed::new_client;
use oxpulse_sfu_kit::propagate::ClientId;
use oxpulse_sfu_kit::Registry;
use std::net::SocketAddr;

fn build_registry(n: usize) -> Registry {
    let mut reg = Registry::new_for_tests();
    for i in 0..n {
        let client = new_client(ClientId(i as u64 + 1));
        reg.insert(client);
    }
    reg
}

fn bench_handle_incoming(c: &mut Criterion) {
    let src: SocketAddr = "127.0.0.1:10000".parse().unwrap();
    let dst: SocketAddr = "127.0.0.1:3478".parse().unwrap();
    // Minimal 4-byte payload — content doesn't matter since no client accepts it
    // (all Rtc instances are unnegotiated).
    let payload = [0xde, 0xad, 0xbe, 0xef];

    let mut group = c.benchmark_group("registry/handle_incoming");
    for peers in [1usize, 8, 50] {
        group.bench_with_input(BenchmarkId::new("peers", peers), &peers, |b, &n| {
            b.iter_batched(
                || build_registry(n),
                |mut reg| {
                    // Returns false (no match) — full linear scan across all n clients.
                    reg.handle_incoming(src, dst, &payload);
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

criterion_group!(benches, bench_handle_incoming);
criterion_main!(benches);
