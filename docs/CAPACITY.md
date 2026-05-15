# Capacity — krolik (Linux ARM, 24 GB) — 2026-05-14

**Hardware:** Oracle Cloud arm-max-1768977332, ARM 24 GB RAM, 4 vCPUs.
**OS:** Ubuntu Linux (aarch64-unknown-linux-gnu).
**Rust:** 1.95.0 stable.
**Kit version:** v0.11.5 (commit 329990d).

**Workload:** `synthetic_room` example (loopback transport, no DTLS/SRTP/UDP).
Simulated: 30 fps per publisher, 1.5 Mbps target bitrate intent (effective payload is
4 bytes/packet — see example docs). All publishers and subscribers share one process
and one Tokio-free synchronous loop. Latency = wall-clock duration of one
`Registry::fanout_for_tests` call.

**Methodology:** 30-second single-room runs, varying peer count. Serial (one run at a
time). CPU% computed from `getrusage(RUSAGE_SELF)` (user+sys) / wall-clock.

## Results

| Peers | Packets fwd | Peak RSS (MB) | CPU % | p50 (µs) | p95 (µs) | p99 (µs) |
|------:|------------:|--------------:|------:|---------:|---------:|---------:|
|     2 |       1,770 |           4.1 |   0.3 |        2 |        3 |        3 |
|     4 |      10,596 |           4.1 |   0.3 |        1 |        3 |        4 |
|     8 |      49,560 |           4.1 |   0.4 |        1 |        5 |        5 |
|    16 |     207,840 |           4.4 |   0.6 |        3 |        7 |        9 |
|    32 |     855,104 |           4.6 |   1.6 |       12 |       14 |       30 |
|    50 |   2,067,800 |           5.0 |   4.3 |       27 |       34 |       84 |

## Observations

**Memory:** RSS is flat at 4.1 MB from 2 to 8 peers and grows to just 5.0 MB at 50.
Memory is not a capacity cliff at this scale — `Registry` holds a `Vec<Client>` with
seeded tracks; the 4-byte synthetic payload means no real buffer pressure.

**CPU:** Stays below 1% up to 16 peers, then climbs to 4.3% at 50. On a 4-vCPU ARM
instance one core = ~25%; at 50 peers we're at 17% of one core. Not a cliff.

**Latency cliff (p99):** No sharp cliff visible in the 2–50 peer range.

- 2–16 peers: p99 ≤ 9 µs — flat, dominated by function-call overhead and HashMap
  lookups.
- 32 peers: p99 jumps to 30 µs — first sign of the O(n) `Registry::route` scan
  becoming visible. Still well within any practical SLO.
- 50 peers: p99 = 84 µs — 3× increase over 32 peers. Linear scan across 50 clients
  on ARM L1/L2 is starting to show. Not a hard cliff yet, but the slope suggests
  100+ peers would push p99 into hundreds of µs.

**Practical cliff estimate:** p99 latency growth rate suggests ~80 peers as the point
where p99 crosses 200 µs. That is not measured — extrapolated from the 32→50 slope.

## Recommendations

**Max safe peers per room (single-room, measured):** 50 peers, p99 = 84 µs.
Comfortable ceiling for current 1:1–8-peer oxpulse-chat workload. Headroom is large.

**Estimated max simultaneous rooms (extrapolation, NOT measured):** At 8 peers per
room (current typical workload) and 4.1 MB RSS per room, krolik (24 GB) could in
theory hold ~5,800 rooms before RAM is exhausted. CPU is the real bound: 4.3% per
core at 50 peers scales to ~0.4% per core at 8 peers, so at ~600 simultaneous 8-peer
rooms the process would saturate one core. The real limit for a 4-core ARM node is
probably ~2,000 rooms before scheduling jitter becomes visible — but this is
back-of-envelope, not measured.

**Caveats:**

1. `synthetic_room` bypasses DTLS/SRTP. Real prod load will have ~30% more CPU per
   packet from str0m crypto.
2. No real network stack (no UDP I/O, no kernel packet processing).
3. Synthetic 4-byte payloads avoid any serialization or buffer overhead. Real 1200-byte
   RTP payloads will increase RSS and cache pressure.
4. Single-threaded fanout loop: no tokio overhead measured. Real SFU uses async I/O.
5. Multi-room concurrency not measured — `synthetic_room` is single-room only.

## Reproduce

```bash
ssh krolik
cd ~/tmp/oxpulse-sfu-kit-d-lite-4-runs

# Build once (needs --features all)
CARGO_BUILD_JOBS=2 cargo build --release --example synthetic_room \
  --features "active-speaker,metrics-prometheus,kalman-bwe,googcc-bwe,pacer,test-utils"

# Run sweep
for N in 2 4 8 16 32 50; do
  ./target/release/examples/synthetic_room \
    --peers $N --duration-secs 30 --packet-rate-pps 30 --bitrate-bps 1500000 \
    2>&1 | grep SYNTHETIC_ROOM_RESULT
done
```
