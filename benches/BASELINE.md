# Baseline — v0.11.4 (commit 4095709) on macOS Apple Silicon

**Platform:** Apple M-series (macOS 15.x, arm64)
**Rust:** 1.88 (stable)
**Profile:** `bench` (release + LTO)
**Date:** 2026-05-14
**Note:** krolik-ARM numbers will land in D-Lite 4 after the synthetic_room load
generator (D-Lite 3) is wired. Expect ARM latencies to be ~1.5–2× higher for
compute-heavy paths (Kalman filter) and comparable for memory-bound paths.

---

## bwe_estimator

```
bwe_estimator/record_send_time/steady      time:   [398.39 ns 449.23 ns 517.02 ns]
bwe_estimator/record_send_time/at_cap      time:   [2.1979 µs 2.3217 µs 2.4482 µs]   ← O(n) cliff: HashMap::keys().min() on 512 entries
bwe_estimator/on_twcc_feedback/50_samples  time:   [2.6914 µs 2.7715 µs 2.8481 µs]
bwe_estimator/estimate_bps/after_feed      time:   [62.140 ns 62.737 ns 63.384 ns]
```

### Interpretation

- `steady` vs `at_cap`: ~5× overhead at cap. The `HashMap::keys().min()` eviction
  scan is O(n) over 512 keys — this is the cliff flagged in Phase D plan. At 8 peers
  × 30pps each the map stays well under cap (8 × 30 × ~1.67s = ~400 entries); at 50
  peers × 30pps it would cycle the cap every ~340ms, making the eviction visible in
  production. D-Lite 2 / future work: consider `BTreeMap` or ring-buffer for O(log n)
  eviction.
- `on_twcc_feedback/50_samples`: ~2.77 µs for a full 50-packet feedback batch.
  At 50ms TWCC intervals this is 55 µs/s per subscriber — negligible.
- `estimate_bps`: ~63 ns — read-only HashMap lookup + float arithmetic. Hot-path safe.

---

## googcc

```
googcc/on_receive/steady_30pps   time:   [589.97 ns 606.20 ns 625.99 ns]
googcc/on_receive/overuse_burst  time:   [605.38 ns 620.60 ns 635.92 ns]
```

### Interpretation

- Overuse path (~1% slower than steady): trendline detector does the same work;
  AIMD multiplicative decrease is a single float multiply — cost is identical.
- ~610 ns/packet. At 30 pps per subscriber, 8 subscribers: ~146 µs/s total — cheap.

---

## pacer

```
pacer/update/steady_high_bps      time:   [11.193 ns 11.833 ns 12.395 ns]
pacer/update/upgrade_transition   time:   [9.8185 ns 10.576 ns 11.277 ns]
pacer/update/downgrade            time:   [8.2410 ns 8.6443 ns 9.0404 ns]
```

### Interpretation

- All paths <12 ns — pure branch logic, no allocation. The hysteresis state machine
  is branch-heavy but the branch predictor handles it well at steady state.
- Downgrade is slightly faster (fewer fields to update — no streak counter reset
  needed for immediate-downgrade path).

---

## registry

```
registry/handle_incoming/peers/1   time:   [3.5866 µs 3.7431 µs 3.9033 µs]
registry/handle_incoming/peers/8   time:   [25.464 µs 26.635 µs 27.871 µs]   ← current realistic workload
registry/handle_incoming/peers/50  time:   [153.31 µs 161.06 µs 169.67 µs]
```

### Interpretation

- **What is measured:** the "no client accepts" scan — every client's `Rtc::accepts()`
  is called before returning `false`. This is the worst-case path and the dominant
  cost for STUN/early packets. In production every datagram either matches immediately
  (first client wins) or traverses all N clients before giving up.
- **Linear scaling confirmed:** 1→8→50 peers scales roughly 1× → 7.1× → 43×. Nearly
  linear O(n) as expected from the `Vec::find` implementation.
- **At 8 peers:** ~27 µs per unmatched datagram. At 100 packets/sec this is 2.7 ms/s —
  well within budget for the 1:1–8-peer workload.
- **At 50 peers:** ~161 µs per unmatched datagram. At 100 pps: 16 ms/s — still OK but
  approaching meaningful overhead. A socket-addr index (HashMap<SocketAddr, ClientIdx>)
  would reduce this to O(1); deferred to full Phase D.
- **Bench caveat:** test-seeded `Client` instances have unnegotiated `Rtc` — they never
  accept a datagram. Real production packets match on the FIRST client (the one that
  owns the source address), so the realistic hot path is O(1). The bench measures the
  worst-case full-scan, which is what matters for capacity planning.

---

## Skipped benches

None — all four planned harnesses are implemented and run.

## Future baseline (krolik ARM)

To be added in D-Lite 4 after D-Lite 3 (`synthetic_room`) lands on main. Run:

```bash
CARGO_BUILD_JOBS=2 cargo bench --bench bwe_estimator --features kalman-bwe
CARGO_BUILD_JOBS=2 cargo bench --bench googcc --features googcc-bwe
CARGO_BUILD_JOBS=2 cargo bench --bench pacer --features pacer
CARGO_BUILD_JOBS=2 cargo bench --bench registry --features test-utils
```

Append results here under `# Baseline — v0.11.4 on krolik (Linux ARM, Oracle Cloud)`.
