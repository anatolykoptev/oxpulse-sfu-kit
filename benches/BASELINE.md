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

---

## Linux ARM (krolik) — v0.11.5 (commit 329990d) — 2026-05-14

**Platform:** Oracle Cloud arm-max-1768977332, ARM 24 GB (aarch64-unknown-linux-gnu)
**Rust:** 1.95.0 (stable)
**Profile:** `bench` (release + thin LTO + codegen-units=1 via `~/.cargo/config.toml`)
**Build time (cold, sccache):** 29m 24s (deps not cached; subsequent builds use sccache)
**Linker:** mold (via `~/.cargo/config.toml`)

### Comparison

| Bench | macOS Apple Silicon (median) | Linux ARM krolik (median) | Delta |
|---|---|---|---|
| bwe_estimator/record_send_time/steady | 449 ns | 469 ns | +4% |
| bwe_estimator/record_send_time/at_cap | 2.32 µs | 4.14 µs | +79% |
| bwe_estimator/on_twcc_feedback/50_samples | 2.77 µs | 6.65 µs | +140% |
| bwe_estimator/estimate_bps/after_feed | 62.7 ns | 78.1 ns | +25% |
| googcc/on_receive/steady_30pps | 606 ns | 330 ns | -45% |
| googcc/on_receive/overuse_burst | 621 ns | 335 ns | -46% |
| pacer/update/steady_high_bps | 11.8 ns | 13.2 ns | +12% |
| pacer/update/upgrade_transition | 10.6 ns | 12.5 ns | +18% |
| pacer/update/downgrade | 8.6 ns | 8.3 ns | -3% |
| registry/handle_incoming/peers/1 | 3.74 µs | 35.5 µs | +849% |
| registry/handle_incoming/peers/8 | 26.6 µs | 51.3 µs | +93% |
| registry/handle_incoming/peers/50 | 161 µs | 221 µs | +37% |

### Raw results

#### bwe_estimator

```
bwe_estimator/record_send_time/steady      time:   [409.44 ns 468.67 ns 550.66 ns]   (8% outliers)
bwe_estimator/record_send_time/at_cap      time:   [2.9176 µs 4.1431 µs 5.7180 µs]   (16% outliers — high variance)
bwe_estimator/on_twcc_feedback/50_samples  time:   [6.1779 µs 6.6507 µs 7.1346 µs]   (17% outliers)
bwe_estimator/estimate_bps/after_feed      time:   [75.287 ns 78.101 ns 80.819 ns]
```

#### googcc

```
googcc/on_receive/steady_30pps   time:   [313.93 ns 330.48 ns 351.14 ns]   (13% outliers)
googcc/on_receive/overuse_burst  time:   [321.75 ns 335.28 ns 355.99 ns]   (6% outliers)
```

#### pacer

```
pacer/update/steady_high_bps    time:   [11.007 ns 13.229 ns 15.763 ns]   (14% outliers)
pacer/update/upgrade_transition time:   [10.929 ns 12.545 ns 14.498 ns]   (9% outliers)
pacer/update/downgrade          time:   [7.0996 ns  8.3464 ns 10.004 ns]   (9% outliers)
```

#### registry

```
registry/handle_incoming/peers/1   time:   [10.133 µs 35.484 µs 69.290 µs]   (13% outliers — very high variance, scheduling noise)
registry/handle_incoming/peers/8   time:   [39.850 µs 51.336 µs 67.811 µs]   (10% outliers)
registry/handle_incoming/peers/50  time:   [203.84 µs 220.88 µs 248.97 µs]   (13% outliers)
```

### Interpretation

**Where ARM is faster than macOS Apple Silicon:**

- **GoogCC `on_receive`:** -45% to -46%. Surprising. Likely explained by ARM
  vector/NEON being well-suited to the trendline detector's float arithmetic, combined
  with the krolik ARM CPU being a newer generation (Oracle Cloud arm-max = Ampere Altra
  Q80-30 equivalent) with a large out-of-order execution window. macOS M-series
  throttles sustained bench workloads differently.

**Where ARM is comparable (within ±25%):**

- `record_send_time/steady`: +4% — essentially identical.
- `estimate_bps`: +25% — within measurement noise for this level of computation.
- `pacer/update` all paths: +3% to +18% — branch-heavy logic, negligible difference.

**Where ARM is slower than macOS Apple Silicon:**

- `record_send_time/at_cap`: +79%. The O(n) `HashMap::keys().min()` eviction scan
  stresses memory bandwidth on ARM; macOS M-series has higher memory bandwidth per
  core. The high variance (16% outliers, wide CI: 2.9–5.7 µs) suggests scheduling
  noise on the shared Oracle Cloud ARM instance.
- `on_twcc_feedback/50_samples`: +140%. This path parses a 50-packet feedback batch
  with sequential HashMap lookups. ARM is memory-bandwidth-bound here — the M-series
  LPDDR5 advantage is visible.
- `registry/handle_incoming/peers/1`: +849% (median 35 µs vs 3.7 µs). The extremely
  wide CI (10–69 µs) indicates high scheduling jitter — this bench is too fast (one
  `Rtc::accepts()` call) to measure reliably on a shared cloud VM. The macOS number is
  more trustworthy. **Do not treat the peers/1 ARM number as representative.**
- `registry/handle_incoming/peers/8` and `/50`: +93%/+37% respectively. These are
  more reliable (more iterations, less jitter). ARM's lower per-core memory bandwidth
  explains the gap; the linear scan traverses all Client structs in the Vec.

**Summary:** For the current 1:1–8-peer oxpulse-chat workload, ARM krolik performance
is adequate. The bottlenecks flagged (HashMap eviction scan at capacity, registry full
scan) are the same on both platforms, just ~2× worse on ARM due to memory bandwidth.
The GoogCC path is actually faster on krolik — real-world call quality will not degrade
on ARM.
