# Research — Potential Improvements for oxpulse-sfu-kit

**Date:** 2026-04-22 (updated; originally 2026-04-21)
**Scope:** `oxpulse-sfu-kit` v0.3.1 (and companion `rust-dominant-speaker` v0.1.1) — grounded in
academic + industry research, read-only research pass.
**Anchor constraints:**
- Must be actionable in the Rust ecosystem.
- Must not propose rewriting / forking str0m core.
- Library-shaped work only — deployment topology shows up as "what hooks should the kit
  expose", not "what should the operator do".

---

## Executive summary

1. **✅ DONE (v0.2.0) — BWE surfacing.** `Event::EgressBitrateEstimate` is now exposed as
   `Propagated::BandwidthEstimate { peer_id, estimate }` with a `BandwidthEstimate { bps }`
   type and a `sfu_bandwidth_estimate_bps{peer_id}` Prometheus gauge. The remaining gap:
   **no pacer, no per-subscriber budget allocator, no hysteresis, no audio-only fallback.**
   Parent OxPulse has `crates/sfu/src/{bandwidth,pacer}.rs` we can lift almost verbatim.
   **This is still the single largest correctness improvement remaining.** Medium (1 week).
2. **Add AV1 Dependency Descriptor parsing and a per-subscriber layer selector.**
   Without parsing the AV1 DD RTP header extension (RFC in progress, draft-ietf-avtext-framemarking
   for H.264/HEVC, RFC 7941-style for AV1) we cannot drop individual temporal/spatial layers
   of an AV1-SVC stream — we can only do coarse simulcast RID selection. rheomesh has a
   working parser at `sfu/src/rtp/dependency_descriptor.rs`; we can port it. Medium (1 week).
3. **✅ DONE (rust-dominant-speaker v0.1.1) — hysteresis constants exposed.**
   `DetectorConfig { c1, c2, c3, n1, n2, n3, tick_interval }` is now public API.
   Volfin & Cohen 2012 + mediasoup constants remain the deployed baseline at Jitsi,
   mediasoup, Element Call, and (via `rust-dominant-speaker`) us. Recent deep-learning ASD
   (TalkNet / LoCoNet / UniTalk Ge et al. 2025 arXiv:2505.21954) is **not applicable**
   to SFU audio-level streams because it requires server-side video decode — we're a
   forwarder. Remaining: (a) `current_top_k` ring for recent-speakers UX,
   (b) document the RNNoise / ten-vad pre-filter path for publishers.
4. **Opus RED we already have at parent level; the kit should expose a VAD/RED hint channel.**
   Opus LBRR + RFC 2198 RED are packet-level sender concerns and already work through
   str0m. The kit should expose a way for subscribers to *request* RED for their
   downstream (LiveKit does this via `SubscribedCodec`). Small (2-3 days).
5. **Encryption: SFrame + MLS-for-keys (not yet started, v0.3 goal remains open).**
   SFrame (RFC 9605, August 2024) plus MLS (RFC 9420) is the emerging industry standard
   for E2E over SFUs (draft-barnes-sframe-mls-00). Double Ratchet works at 1:1 and
   small-group scale but has O(N²) re-key cost; MLS is O(log N). Kit stays key-agnostic —
   we need only forward encrypted payloads and expose a key-epoch header extension. Medium
   (integration test complexity dominates; 1-2 weeks if crypto stack is elsewhere).
   Now targeting v0.4 given bandwidth of v0.3 encapsulation pass.
6. **L4S is coming but it is premature for us.** ECN-based LLLS (RFC 9330) is in active
   Chromium rollout but requires kernel + network path cooperation (ECT(1) marking,
   DualPI2 AQM). It belongs in the *pacer* design as a pluggable CC mode once a Rust L4S
   crate exists — today none does. Document the plugin seam, don't implement yet.

---

## Active speaker detection

### Current state

- `rust-dominant-speaker` implements the three-time-scale subband log-ratio test from
  Volfin & Cohen, "Dominant Speaker Identification for Multipoint Videoconferencing"
  (IEEE ICASSP 2012), with mediasoup's C++ `ActiveSpeakerObserver` constants (`C1=3.0`,
  `C2=2.0`, `C3=0.0`, `TICK_INTERVAL=300 ms`, `N1=13 / N2=5 / N3=10`).
- Input: raw RFC 6464 audio-level bytes (0 = loud, 127 = silent) per peer, fed via
  `Registry::record_audio_level`. Output: `Propagated::ActiveSpeakerChanged { peer_id }`
  on every change.
- Hysteresis lives in the election rule: a challenger must beat the incumbent on all
  three log-ratios AND hold the highest medium-ratio in the room.

### Academic / industry research

| Paper / system | Year / venue | Relevance |
|---|---|---|
| Volfin, I. & Cohen, I., "Dominant Speaker Identification for Multipoint Videoconferencing" | Speech Communication 55 (2013); IEEE ICASSP 2012 | Foundational. Still deployed. |
| Ge et al., "Towards Universal Active Speaker Detection in Real World Scenarios" (UniTalk benchmark) | arXiv:2505.21954, 2025 | Shows DL-based ASD (TalkNet, LoCoNet) collapses from >95% mAP on AVA-ActiveSpeaker to much worse on real overlapping-speech data. Requires **video+audio fusion** — not applicable to SFU-audio-only. |
| Storck, L., "Overlap and Speaker-Turn Awareness for Low-Latency Automatic Speech Recognition", MSc thesis, U. Bonn 2025 | thesis | Online overlap detection is still research-grade; not yet a drop-in for SFU. |
| Silero VAD (snakers4/silero-vad) | 2020 → 6.2.0 (2025) | Neural single-thread CPU VAD. **License is CC BY-NC 4.0 — non-commercial. Cannot be a default for oxpulse.** |
| RNNoise (Valin, J-M), "A Hybrid DSP/Deep Learning Approach to Real-Time Full-Band Speech Enhancement" | INTERSPEECH 2018 | BSD-3-Clause. Commercial-friendly. Runs well on mobile. Pair with RFC 6464 at the publisher to produce cleaner levels. |
| ten-vad (TEN-framework) | 2024 | MIT-licensed alternative to Silero. Small, CPU-friendly. |
| Jitsi `DominantSpeakerIdentification.java` / mediasoup `ActiveSpeakerObserver.cpp` | in production | Same algorithm; same constants we use. |

**Headline:** Volfin & Cohen 2012 is *not* obsolete for SFU audio-level streams. Newer
DL-based ASD targets a harder problem (AV fusion on full frames) and is orthogonal to
what an audio-level-only SFU can see. The useful step-change there requires the SFU to
be video-aware (e.g. a separate NIM/ASD service consuming decoded frames), which is
out of scope for a Rust kit that does byte-level forwarding.

What *is* actionable:

1. **Expose hysteresis constants.** `C1 / C2 / C3 / LEVEL_IDLE_TIMEOUT_MS / TICK_INTERVAL`
   are hard-coded as `pub(crate) const`. Let downstream pass a `DetectorConfig` struct.
   This lets product tune for e.g. meeting rooms (stable, long dwell) vs. casual calls
   (rapid turn-taking).
2. **Optional recent-speakers ring.** Jitsi exposes `recent-speakers-count` for UI tiles;
   we emit only "current dominant". Surface a `Vec<PeerId>` of the top-K active speakers
   — bounded small K, cheap to maintain alongside the incumbent check.
3. **Document the RNNoise/VAD-before-levels path.** Publishers that want crisp election
   should filter audio through RNNoise (or ten-vad) before the RFC 6464 level is computed.
   This is a client concern; the kit just needs to say so in the README.
4. **Do not adopt Silero as default.** License blocker.

### Recommendations for v0.2+

| # | Item | Effort | Expected impact | Release |
|---|---|---|---|---|
| 1 | Add `DetectorConfig` with tunable `c1/c2/c3/tick_ms/level_idle_ms` in `rust-dominant-speaker` | small (1-2 d) | tuning flexibility; lets downstream A/B | v0.2 |
| 2 | Add `current_top_k(&self, k: usize) -> Vec<u64>` helper | small (1 d) | enables grid-highlight UX without polling | v0.2 |
| 3 | README section: RNNoise / ten-vad pre-filter before RFC 6464 levels | small (0.5 d) | doc-only | v0.2 |
| 4 | Optional: `ActiveSpeakerChangedWithConfidence { peer_id, margin: f64 }` — expose the C2 log-ratio margin | small (1 d) | lets consumers smooth UI themselves | v0.3 |

**Contradicts current code:** nothing. We keep Volfin & Cohen; we just loosen the knobs.

---

## Bandwidth estimation and congestion control

### Current state

- `SfuConfig` has no BWE / pacer config.
- `Client` and `Registry` do not surface `str0m::Event::EgressBitrateEstimate`; we never
  propagate it.
- No TWCC feedback plumbing — str0m parses incoming TWCC internally but our kit never
  reads back the resulting estimate.
- No pacer: str0m has `LeakyBucketPacer` in `src/pacer/leaky.rs` but it is `pub(crate)`.
  Our out-path writes immediately from `handle_media_data_out`, no rate shaping, no ALR,
  no probing.
- No audio-only fallback at low bandwidth.

### Academic / industry research

| Algorithm | Status | Notes for us |
|---|---|---|
| Google Congestion Control (GoogCC) — Holmer et al., "A Google Congestion Control Algorithm for Real-Time Communication", draft-ietf-rmcat-gcc-02 (2016) + modern libWebRTC | Deployed in Chrome, Firefox, str0m 0.18 | **Already in str0m — trendline estimator, not the older Kalman. Use it; don't build our own.** |
| Transport-wide Congestion Control (TWCC) — draft-holmer-rmcat-transport-wide-cc-extensions-01 | Deployed | Feedback format; str0m parses and produces `TwccSendRecord`. We need to plumb it up. |
| SCReAM / SCReAMv2 — Johansson & Sarker, "Self-Clocked Rate Adaptation for Multimedia", RFC 8298 (2017), v2 per IETF 125 slides (2024) | Deployed Ericsson (5G, remote-vehicle) | SCReAMv2 adds **native L4S** (RFC 9330). Interesting as a pluggable alternative CC for cellular-heavy deployments, but **no Rust implementation exists** — only C++ at `EricssonResearch/scream`. Port cost is significant. |
| NADA — Zhu et al., RFC 8698 (2020) | Mostly academic | Reference implementation in ns-3. No mainstream deployment. |
| BBRv2 / BBRv3 | TCP; not for real-time media | Not applicable to RTP loss-tolerant flows. |
| L4S — Briscoe et al., "Low Latency, Low Loss, and Scalable Throughput (L4S) Internet Service: Architecture", RFC 9330 (2023) | Rollout: Apple QUIC full; WebKit issue 271191 in progress; Chromium `issues.webrtc.org/42225697` active | ECN-based. Requires network-path cooperation (DualPI2 AQM) + ECT(1) marking. Emerging but not deployable today in a general-purpose SFU. |
| Lag-Busting WebRTC (Tang et al., ACM 2024, `dl.acm.org/doi/pdf/10.1145/3793853.3798195`) | 2024 academic | Demonstrates L4S applied to WebRTC adaptive video; reference design for later. |
| ECN-based Congestion Control for WebRTC (poster, ACM 2024, `dl.acm.org/doi/epdf/10.1145/3730567.3768595`) | 2024 | Practical L4S client implementation. |

**What we already know internally.** `docs/superpowers/research/2026-04-21-rust-gcc-twcc-crates.md` in
parent `oxpulse-chat` (feat/rooms) already concluded: str0m has the algorithm, no standalone
Rust crate exists, the path is copy-adapt into our SFU layer. The parent already did this
in `crates/sfu/src/{bandwidth,pacer}.rs`. That code is not in the kit.

### Recommendations for v0.2+

| # | Item | Effort | Expected impact | Release |
|---|---|---|---|---|
| 1 | Add `Registry::bandwidth()` + `Registry::bandwidth_mut()` accessor; poll `Event::EgressBitrateEstimate` in `Client::handle_event` and surface via `Propagated::BandwidthEstimate { peer, bps }` | small (2-3 d) | exposes the estimate that's *already computed*; zero new algorithm code | v0.2 |
| 2 | Port parent's `BandwidthEstimator` wrapper (per-subscriber budget allocation) as a kit-level `bwe::BandwidthAllocator` with a `fn budget(&self, peer: ClientId) -> u64` API | medium (3-5 d) | enables layer selection downstream; same math parent already uses | v0.2 |
| 3 | Port parent's `Pacer` (LiveKit-style hysteresis — 3-consecutive upgrade, immediate downgrade) behind a `feature = "pacer"` flag | medium (3-5 d) | stops layer thrash on real networks; matches LiveKit/mediasoup behaviour | v0.2 |
| 4 | Add `AUDIO_ONLY_THRESHOLD_BPS` cutoff (LiveKit ≈100 kbps) — emit `Propagated::AudioOnlyDropVideo { peer }` below threshold | small (1-2 d) | survives HSPA+ / weak cellular | v0.2 |
| 5 | Define a `CongestionControl` trait for future SCReAM / L4S plug-in; default impl delegates to str0m GoogCC | small (1 d design, no body) | future-proof; no runtime cost | v0.2 |
| 6 | L4S mode: mark egress `ECT(1)` per-socket, consume `ECE` feedback; gated behind `feature = "l4s"` and a kernel-capability probe | large (>1 month; blocked on Rust L4S crate existence) | only meaningful once browsers ship L4S RX support | v0.3+ (research track) |
| 7 | SCReAMv2 pluggable CC — FFI port of EricssonResearch/scream C++ or a pure-Rust port | large (>1 month) | nice for cellular-heavy use; low ROI for general web use | later |

**Contradicts current code:** README lines 27-30 claim "Bandwidth estimation beyond str0m's
`Event::EgressBitrateEstimate`" is not included. We should surface that event at minimum;
it is misleading to list it as "not included" and then silently swallow it inside `Client`.

---

## Layer selection & simulcast / SVC

### Current state

- Simulcast RID constants (`q`/`h`/`f`) and `layer::matches(desired, data)` gate
  per-subscriber forwarding.
- `Client::set_desired_layer(rid)` lets the *application* pick a layer; the kit never
  adjusts automatically.
- `Client::active_rids` records what the publisher actually sent — but we never feed
  this back to a selector.
- **No AV1 Dependency Descriptor parsing** — we cannot drop individual T-layers of
  AV1-SVC. We can only coarsely drop whole simulcast RIDs.
- No Dynacast (LiveKit) — we never notify the publisher to stop encoding unsubscribed
  layers.

### Academic / industry research

- **LiveKit Dynacast** (Apache-2.0 reference in `livekit/protocol` `SubscribedQualityUpdate`):
  server computes the max RID any subscriber wants and tells the publisher to stop
  encoding higher layers via RTCP feedback. Saves publisher CPU + bandwidth. Already
  documented in our parent `docs/superpowers/research/2026-04-21-group-calls-architecture.md`
  §6.
- **mediasoup `VideoConsumer::UpdateTargetLayers`**: similar pattern, computes from the
  subscribed `Consumer`s.
- **AV1 Dependency Descriptor** (`urn:ietf:params:rtp-hdrext:av1-dependency-descriptor`,
  finalized in AV1 RTP Payload Format draft-ietf-payload-av1): per-packet temporal/spatial
  layer tags. Required for selective SVC forwarding. rheomesh implements this in
  `sfu/src/rtp/dependency_descriptor.rs` (MIT/Apache).
- **Video Frame Marking** (RFC 9626, Experimental, March 2025; was
  `draft-ietf-avtext-framemarking`) for H.264/HEVC/VP9: standardized header extension
  exposing temporal layer ID and other frame metadata without codec-specific parsing.
  Enables layer selection for non-AV1 codecs without full bitstream parsing.
- **Receiver-driven layer preference** (LiveKit's signalling API, mediasoup's consumer
  layer hint): client sends desired spatial/temporal pair; server best-fits.
- **AV1-SVC deployment** (Chromium libWebRTC, 2023+): L3T3 (3 spatial, 3 temporal = 9
  layers) is the target for modern Chrome publishers. Firefox recently added AV1 encode;
  Safari has AV1 decode, encode lagging. (*Verify exact browser version thresholds per
  caniuse / browser release notes before external publication.*)
- **ML-based predictive layer selection**: covered in academic literature (Wang et al.,
  "ML-Based Video QoE" MMSys 2023) but not deployed in any open-source SFU. High variance,
  low confidence.

### Recommendations for v0.2+

| # | Item | Effort | Expected impact | Release |
|---|---|---|---|---|
| 1 | Add `rtp::dependency_descriptor` parser (port rheomesh's MIT/Apache code, adapt to `Mid/Rid` model) | medium (5-7 d) | **unlocks AV1-SVC partial-layer dropping** | v0.2 |
| 2 | Parse RFC 9626 Video Frame Marking header extension | small (2-3 d) | temporal-layer selection for H.264/VP9/HEVC | v0.3 |
| 3 | Add a `LayerSelector` trait + default impl combining `desired_layer` + BWE budget + `active_rids` | small (2 d) | centralizes a currently-scattered decision | v0.2 |
| 4 | Dynacast: emit `Propagated::PublisherLayerHint { peer, max_rid }` when no subscriber wants layer `f` | medium (3-4 d) | cuts publisher upload / CPU significantly; aligns with LiveKit behaviour | v0.2 |
| 5 | Receiver-driven layer preference API: `Client::request_layer(mid, spatial, temporal)` | small (1-2 d) | UX: "show me only Alice in HD" | v0.3 |
| 6 | ML-based predictive selector | large, research-grade | speculative | not planned |

**Contradicts current code:** `Client::active_rids` has a leaky comment saying "Empty =
bootstrap / non-simulcast" — in practice empty can also mean the publisher just joined
and no MediaData has flowed. Our downstream fall-back heuristic needs explicit documentation
(it's in `client/mod.rs:88-89` but should be a first-class API contract).

---

## Error resilience

### Current state

- Keyframe request on discontiguous media (`track_in_media`) with throttling in
  `client/keyframe.rs`. This is our sole loss-recovery mechanism.
- No FEC (ULPFEC RFC 5109 or FlexFEC RFC 8627).
- No RED (RFC 2198).
- No NACK plumbing beyond what str0m emits.
- Opus LBRR / RED is the publisher's concern; kit neither advertises nor requests it.

### Academic / industry research

- **ULPFEC (RFC 5109, 2007)** — XOR-based FEC. Disabled by default in Chrome in recent
  milestones. Reason: costs bandwidth linearly and rarely helps on heavy loss because
  WebRTC already runs NACK efficiently, and FEC hurts latency more than it helps.
- **FlexFEC (RFC 8627, 2019)** — 2-D parity matrix. Better than ULPFEC; Chrome flag-gated
  but not default. Pion has an impl; webrtc-rs does not.
- **RED (RFC 2198)** for audio: re-send the last N audio frames inline. Default in recent
  Chrome for Opus (turned on by default in the early M100-series). Dramatically improves
  PLC quality at 3-5% loss. Controlled by `useinbandfec=1` / `usedtx=1` in Opus SDP and
  the `red/48000/2` fmtp for RFC 2198 redundancy.
- **Opus LBRR (Low Bit-Rate Redundancy)** — built into libopus (inband FEC); decoded
  transparently by the receiver's libopus, no SFU work required.
- **PLC + DNN hybrid**: Valin et al. "A Real-Time Wideband Neural Vocoder at 1.6 kb/s
  Using LPCNet" (INTERSPEECH 2019), follow-ups PLCNet / Opus DRED (Deep REDundancy,
  shipped in recent libopus) — ≈1 kbps redundant stream, ≈1 second lookback, uses a
  small neural decoder for concealment. Standardized as `opus-dred` in libopus; rolling
  out in Chromium. This is a **huge** audio-quality improvement at low loss cost.
  (*Verify exact Chromium / libopus milestones before external publication.*)
- **Chimera-FEC / rateless FEC** — research-grade; no production impl.

Chrome's own behaviour is telling: FlexFEC is essentially off; RED+LBRR+DRED for Opus is on;
NACK is on. Video uses NACK+keyframe, not FEC, because ULPFEC/FlexFEC cost too much.

### Recommendations for v0.2+

| # | Item | Effort | Expected impact | Release |
|---|---|---|---|---|
| 1 | Advertise + forward Opus RED (RFC 2198) when both peers support it (SDP `red/48000/2` fmtp) | small (2-3 d) | **large** audio-quality improvement on lossy links | v0.2 |
| 2 | Expose `ChannelPreferences { opus_red: bool, opus_dred: bool }` per subscriber | small (1 d) | lets downstream gate by platform / cost | v0.2 |
| 3 | Plumb RTCP XR (RFC 3611) loss-rate / jitter / RTT blocks into `SfuMetrics` | medium (4-5 d) | observability — see next section | v0.3 |
| 4 | FlexFEC (RFC 8627) video: do not implement. | — | Chrome deprecated; cost > benefit | — |
| 5 | Document the Opus DRED pass-through (SFU does nothing; 1.5+ receiver handles it) | tiny (0.5 d) | correctness + publicity | v0.2 |
| 6 | NACK shaping policy: throttle NACK storms when loss exceeds 10% (keyframe instead) | small (2 d) | prevents feedback amplification | v0.3 |

---

## Encryption & key management

### Current state

- Kit does not touch payloads — SRTP is handled by str0m per-session, and any E2E
  encryption is a publisher/subscriber concern at the codec-frame level.
- README explicitly says E2E is out of scope; parent OxPulse has SFrame in feat/rooms.
- No key-epoch plumbing in the kit (e.g. header extension with key index).

### Academic / industry research

- **SFrame — RFC 9605 (August 2024)** — IETF standard for frame-level AEAD over RTP.
  Reference impls: `cisco/libsframe` (C++), `livekit/client-sdk-js` e2ee/ (TS worker),
  element-call's `matrixKeyProvider.ts`.
- **MLS — RFC 9420 (July 2023)** — group key agreement with forward secrecy and
  post-compromise security, O(log N) update cost. Group sizes to thousands.
- **SFrame-over-MLS — `draft-barnes-sframe-mls-00`** (2024) — ties MLS epochs to SFrame
  key indices. The way forward for E2E video conferencing.
- **Double Ratchet** (Perrin & Marlinspike, Signal 2016) — pairwise 1:1 forward-secure
  messaging. Used at 1:1 today. **Not scalable for group SFU**: N² pairwise ratchets
  vs MLS's O(log N) tree operations.
- **Post-quantum** — HPKE is now MLS-compatible (draft-ietf-mls-combiner for hybrid
  X25519+Kyber). Matrix's vodozemac already has MLKEM support. Post-quantum MLS
  ciphersuites are draft-stage in IETF MLS-WG.
- **Element Call / LiveKit** both deploy SFrame via `livekit-client/e2ee/` worker
  (Apache-2.0). Parent's `docs/superpowers/research/2026-04-21-sframe-implementations-deep.md`
  covers this in detail.

### Recommendations for v0.2+

| # | Item | Effort | Expected impact | Release |
|---|---|---|---|---|
| 1 | Define an opaque `KeyEpochHeaderExt` RTP header extension (e.g. URN `urn:ietf:params:rtp-hdrext:sframe-key-id`); kit **forwards** it but does not interpret | small (1-2 d) | unblocks SFrame deployment by consumers | v0.2 |
| 2 | Add `Propagated::PeerKeyEpochChanged { peer, epoch }` so consumers can coordinate key rotation on membership change | small (2 d) | hooks for SFrame-over-MLS | v0.3 |
| 3 | Do **not** ship SFrame inside the kit — it's a client-side Insertable-Streams concern. Kit stays key-agnostic. | — | scope discipline | — |
| 4 | Document the SFrame + MLS architecture and the `draft-barnes-sframe-mls-00` in `docs/encryption.md`, with links to parent's implementation | small (1 d) | guidance for adopters | v0.2 |

**Contradicts current code:** nothing — the kit correctly stays out of crypto.

---

## SFU topology

Library-shaped reframe: the kit cannot deploy itself. The question is **what abstractions
does it expose** so that cascade / edge / relay patterns are implementable downstream.

### Current state

- Single `Registry` = single room = single process. No relay input / relay output
  abstraction. No "this track came from another SFU" distinction.
- `run_udp_loop` assumes one UDP socket per room. Fine for single-node, blocks cascade.

### Academic / industry research

- **rheomesh** (`sfu/src/relay/`) — has a first-class `RelayedTrack` + `relay::receiver` that
  lets a downstream SFU consume tracks from an upstream peer SFU. MIT/Apache — portable pattern.
- **atm0s-media-server** (`packages/media_core/src/cluster/`) — goes further: treats cluster
  membership via SDN-inspired routing. Heavier than we need but the `Cluster` trait
  boundary is instructive.
- **LiveKit Cloud** — cascade SFU via a "node-node" protocol carrying already-SRTP-
  protected tracks plus metadata. Proprietary.
- **Jitsi Octo** / **SelectIO** (academic, Grozev et al. "Last N: relevance-based
  selectivity for forwarding video in multimedia conferences", ACM MMSys 2015) — cascaded
  SFU with per-edge "last-N" speakers only forwarded upstream.
- **Edge-distributed SFU**: already covered by parent partner-edge + TURN pool.
  Relevant to us only insofar as the kit should *not* bake in a single-process assumption.

### Recommendations for v0.2+

| # | Item | Effort | Expected impact | Release |
|---|---|---|---|---|
| 1 | Introduce a `RelaySource` trait so a `Client` can be marked as "packets originate from another SFU" — skip keyframe requests going *back* upstream, forward them on | medium (5-7 d) | unlocks cascade-SFU consumers | v0.3 |
| 2 | Split `udp_loop` into `serve_socket(socket, registry, shutdown)` so consumers can multiplex multiple sockets / run multiple registries in one process | small (2-3 d) | needed for multi-room servers | v0.2 |
| 3 | `Forwarder` trait — current fanout logic becomes a default impl, lets consumers inject custom forwarders (e.g. last-N, geo-based) | medium (3-5 d) | extensibility for large rooms | v0.3 |
| 4 | `ClientOrigin { Local, RelayFromSFU }` enum + skip-self rule updated | small (1-2 d) | correctness for cascade | v0.3 |

---

## Observability

### Current state

- `SfuMetrics` has: `active_participants`, `forwarded_packets_total{kind}`,
  `layer_selection_total{layer}`, `dominant_speaker_changes_total`,
  `client_connect_total`, `client_disconnect_total`.
- No loss / jitter / RTT / MOS / BWE gauges.
- No RTCP XR block parsing.
- No per-peer quality scoring.

### Academic / industry research

- **RFC 3611 — RTCP XR Extended Reports** (Friedman, Caceres, Clark 2003). Seven base
  block types: Loss RLE, Duplicate RLE, Packet Receipt Times, Receiver Reference Time,
  DLRR, Statistics Summary, VoIP Metrics. str0m emits standard RTCP RR/SR; XR is
  extension territory.
- **ITU-T G.107 (E-model) / G.107.2 for wideband** — compute MOS from R-factor derived
  from loss/delay/codec impairments. Already well-understood; used by PJSIP, FreeSWITCH.
- **webrtc-stats W3C API** — client-side. Server-side gauges to mirror it would make
  correlation trivial.
- **LiveKit `ConnectionQualityInfo`** — `excellent/good/poor/lost` derived from loss,
  jitter, RTT. Apache-2.0 reference for a user-visible score.
- **ITU-T P.863 POLQA / P.1204 AVQM** — perceptual audio/video quality. Reference-based,
  not feasible server-side for live streams.

### Recommendations for v0.2+

| # | Item | Effort | Expected impact | Release |
|---|---|---|---|---|
| 1 | Consume RTCP RR blocks from `str0m::Event` and expose `fraction_lost`, `jitter`, `cumulative_lost` per peer as Prometheus gauges | small (2-3 d) | basic loss / jitter dashboard | v0.2 |
| 2 | Add `sfu_rtt_ms{peer}` gauge from RR DLSR/LSR | small (1 d) | RTT visibility | v0.2 |
| 3 | Add `sfu_bandwidth_estimate_bps{peer}` gauge (requires Bandwidth plumbing above) | small (1 d, depends on BWE item) | link to BWE changes | v0.2 |
| 4 | Compute MOS via G.107 E-model (or simplified R-factor) → `sfu_estimated_mos{peer}` | medium (3-5 d) | product-level quality SLI | v0.3 |
| 5 | LiveKit-style `connection_quality{peer}` = `excellent/good/poor/lost` enum label | small (2 d) | per-peer tile UX hint | v0.3 |
| 6 | Parse RTCP XR blocks (RFC 3611 VoIP Metrics block) if publisher sends them | medium (5-7 d) | richer observability; depends on client support | v0.3 |
| 7 | Cardinality discipline: clear `{peer_id}` label series on disconnect (parent already does this in `reap_dead`); port the pattern | small (1 d) | prevent runaway Prom cardinality | v0.2 |

---

## Rust ecosystem gap analysis

| Feature | oxpulse-sfu-kit v0.3.1 | webrtc-rs/sfu | rheomesh | atm0s-media-server | live777 |
|---|---|---|---|---|---|
| Underlying stack | str0m 0.18 | webrtc-rs | webrtc-rs | custom sans-io | webrtc-rs |
| Public API hides dep types | **yes** (v0.3 encapsulation pass) | no | no | no | no |
| Compile-time API leak guard | **yes** (`tests/encapsulation_surface.rs`) | no | no | no | no |
| Simulcast RID forwarding | yes (`q/h/f`) | partial | yes | yes | yes |
| AV1 SVC partial-layer drop | **no** | no | **yes** (dependency_descriptor) | yes | partial |
| Dynacast / publisher-layer hints | **no** | no | partial | yes (bitrate_allocator) | no |
| BWE surfacing | **yes** (v0.2, `Propagated::BandwidthEstimate`) | no BWE in webrtc-rs | via webrtc-rs | yes (in-house) | via webrtc-rs |
| Pacer / per-subscriber budget | **no** | no | partial | yes (`bitrate_allocator`) | no |
| TWCC plumbing to consumer | **no** | partial | yes | yes | yes |
| Per-peer RTCP stats (loss/jitter/rtt) | **yes** (v0.2, Prometheus gauges) | no | partial | yes | no |
| Dominant-speaker (Volfin & Cohen) | **yes** + `DetectorConfig` (best in class) | no | no | yes (audio_mixer) | no |
| Multi-room (`serve_socket`) | **yes** (v0.2) | partial | yes | yes | yes |
| WHIP / WHEP | **no** | partial | yes | yes | **yes** (headline feature) |
| Recording (MP4/WebM egress) | **no** | no | yes | yes | no |
| Cascade / relay | **no** | no | yes (`relay/`) | yes (cluster) | partial |
| Data channels fanout | **no** (out of scope?) | partial | yes | yes | no |
| Server-side mixing (MCU mode) | no | no | no | yes (`audio_mixer`) | no |
| Metrics | yes (Prometheus, per-peer) | no | partial | yes | yes |
| SFrame-aware header forwarding | no | no | no | no | no |
| License | MIT/Apache | MIT | MIT | MIT | MPL-2.0 |
| Health grade (go-code) | — | — | C | C | — |

**Reading the table:**

- We're the only one with a *serious* dominant-speaker port. Keep that identity.
- We're behind on AV1-SVC, BWE plumbing, and relay — exactly the items above.
- **We are intentionally smaller than atm0s/rheomesh.** Don't chase server-side mixing
  or WHIP/WHEP unless product demand appears. Those are different-shape kits.
- Good lift targets (MIT/Apache, portable): `rheomesh/sfu/src/rtp/dependency_descriptor.rs`,
  `rheomesh/sfu/src/relay/`, `atm0s/packages/media_core/src/endpoint/internal/bitrate_allocator/`.

---

## Prioritized backlog for oxpulse-sfu-kit v0.2+

Ranked by (impact × actionability / effort). v0.2 = next minor. v0.3 = 2 minors out. Later = speculative.

Legend: ✅ shipped, 🔜 next, — deferred

| # | Item | Area | Effort | Impact | Release |
|---|---|---|---|---|---|
| 1 | ✅ Surface `str0m::Event::EgressBitrateEstimate` → `Propagated::BandwidthEstimate` | BWE | small | High | v0.2.0 |
| 2 | ✅ RTCP RR → per-peer loss / jitter / RTT Prometheus gauges | Observability | small | High | v0.2.0 |
| 3 | ✅ Per-peer label cardinality scrub on disconnect (`SfuMetrics::reap_dead_peer`) | Observability | small | Medium | v0.2.0 |
| 4 | ✅ `DetectorConfig` — expose Volfin & Cohen constants in `rust-dominant-speaker` | ASD | small | Medium | rust-ds v0.1.1 |
| 5 | ✅ `serve_socket` split from `run_udp_loop` for multi-room usage | Topology | small | Medium | v0.2.0 |
| 6 | ✅ v0.3 encapsulation pass — all public types hide str0m surface | API | medium | Very High | v0.3.0 |
| 7 | ✅ `tests/encapsulation_surface.rs` — compile-time API leak guard | API | small | High | v0.3.0 |
| 8 | 🔜 Port parent's `BandwidthEstimator` + `Pacer` (TWCC → budget → RID selection with hysteresis + audio-only cutoff) behind `feature = "pacer"` | BWE / layer | medium (1 week) | **Very High** — correctness on real networks | v0.4 |
| 9 | 🔜 AV1 Dependency Descriptor parser (port rheomesh) | SVC | medium (1 week) | High (AV1-SVC near-future default) | v0.4 |
| 10 | 🔜 Opus RED (RFC 2198) + DRED advertisement / pass-through | Error resilience | small (2-3 d) | High on lossy links | v0.4 |
| 11 | 🔜 `LayerSelector` trait centralizing desired-layer + BWE + active-rids logic | SVC | small (2 d) | Medium | v0.4 |
| 12 | 🔜 Define `KeyEpochHeaderExt` RTP header forwarding for SFrame consumers | E2E | small (1-2 d) | Medium | v0.4 |
| 13 | 🔜 Dynacast-style `Propagated::PublisherLayerHint` | SVC | medium (3-4 d) | Medium-High | v0.4 |
| 14 | 🔜 `current_top_k` recent-speakers ring in `rust-dominant-speaker` | ASD | small (1 d) | Medium | rust-ds v0.2 |
| 15 | 🔜 Document RNNoise / ten-vad pre-filter, Opus DRED pass-through, SFrame architecture | docs | small (1-2 d) | Medium | v0.4 |
| 16 | MOS via G.107 E-model | Observability | medium (3-5 d) | Medium | v0.5 |
| 17 | RFC 9626 Video Frame Marking parser (H.264/VP9/HEVC) | SVC | small (2-3 d) | Medium | v0.5 |
| 18 | `CongestionControl` trait + `RelaySource` trait for cascade | Architecture | medium (1 week) | Medium | v0.5 |
| 19 | Receiver-driven layer preference API | SVC | small (1-2 d) | Medium | v0.5 |
| 20 | RTCP XR block parsing | Observability | medium (1 week) | Low-Medium | v0.5 |
| 21 | SCReAMv2 pluggable CC (Rust port) | BWE | large (>1 month) | Low (cellular-specific) | later |
| 22 | L4S mode (ECT(1) marking + ECE feedback) | BWE | large, blocked on ecosystem | Low today, rising | later |
| 23 | Predictive ML-based layer selection | SVC | large, research | Speculative | not planned |

---

## References

### RFCs

- RFC 2198 — RTP Payload for Redundant Audio Data (Perkins et al., 1997)
- RFC 3611 — RTP Control Protocol Extended Reports (RTCP XR) (Friedman, Caceres, Clark, 2003)
- RFC 5109 — RTP Payload Format for Generic FEC (Li, 2007)
- RFC 6464 — A Real-time Transport Protocol (RTP) Header Extension for Client-to-Mixer Audio Level Indication (Lennox et al., 2011)
- RFC 8298 — Self-Clocked Rate Adaptation for Multimedia (SCReAM) (Johansson & Sarker, 2017)
- RFC 8627 — RTP Payload Format for Flexible Forward Error Correction (FlexFEC) (Zanaty et al., 2019)
- RFC 8698 — Network-Assisted Dynamic Adaptation (NADA) (Zhu et al., 2020)
- RFC 9330 — L4S Architecture (Briscoe, De Schepper, Bagnulo, White, 2023)
- RFC 9420 — The Messaging Layer Security (MLS) Protocol (Barnes et al., 2023)
- RFC 9605 — Secure Frame (SFrame) (Omara, Uberti, Murillo, Barnes, 2024)
- RFC 9626 — Video Frame Marking RTP Header Extension (Zanaty, Berger, Nandakumar, Experimental, March 2025; was `draft-ietf-avtext-framemarking`)
- draft-ietf-rmcat-gcc-02 — A Google Congestion Control Algorithm for Real-Time Communication (Holmer et al., 2016)
- draft-barnes-sframe-mls-00 — Using MLS to Provide Keys for SFrame (Barnes, 2024)
- draft-holmer-rmcat-transport-wide-cc-extensions — Transport-wide Congestion Control (Holmer, de Quadros, 2016)

### Papers

- Volfin, I. & Cohen, I., "Dominant Speaker Identification for Multipoint Videoconferencing",
  Speech Communication 55 (2013) / IEEE ICASSP 2012.
- Valin, J-M., "A Hybrid DSP/Deep Learning Approach to Real-Time Full-Band Speech Enhancement"
  (RNNoise), INTERSPEECH 2018.
- Valin, J-M., Skoglund, J., "LPCNet: Improving Neural Speech Synthesis Through Linear
  Prediction", IEEE ICASSP 2019 / INTERSPEECH 2019. Basis for Opus DRED.
- Grozev, B., et al., "Last N: relevance-based selectivity for forwarding video in
  multimedia conferences", ACM MMSys 2015.
- Holmer, S., et al., "A Google Congestion Control Algorithm for Real-Time Communication",
  IETF draft, 2016 (see RFC list).
- Ge et al., "Towards Universal Active Speaker Detection in Real World Scenarios" (UniTalk),
  arXiv:2505.21954, 2025.
- Tang et al., "Lag-Busting WebRTC: L4S-Enabled Adaptive Video Streaming",
  ACM 2024, https://dl.acm.org/doi/pdf/10.1145/3793853.3798195.
- Fraunhofer authors, "Adaptable L4S Congestion Control for Cloud-Based Real-Time Streaming",
  IEEE 2024, https://ieeexplore.ieee.org/document/10539241.
- "Poster: ECN-based Congestion Control for WebRTC", ACM 2024,
  https://dl.acm.org/doi/epdf/10.1145/3730567.3768595.

### Reference implementations

- str0m 0.18 — `algesten/str0m`, MIT/Apache. Trendline GoogCC in `src/bwe/`;
  `LeakyBucketPacer` in `src/pacer/leaky.rs`.
- rheomesh — `h3poteto/rheomesh`, MIT/Apache. AV1 DD parser at `sfu/src/rtp/dependency_descriptor.rs`;
  relay in `sfu/src/relay/`.
- atm0s-media-server — `8xFF/atm0s-media-server`, MIT. Bitrate allocator at
  `packages/media_core/src/endpoint/internal/bitrate_allocator/`.
- live777 — `binbat/live777`, MPL-2.0. WHIP/WHEP reference.
- LiveKit — `livekit/protocol`, Apache-2.0. Dynacast `SubscribedQualityUpdate`.
- Jitsi — `jitsi/jitsi-videobridge`, Apache-2.0. `ConferenceSpeechActivity` +
  `DominantSpeakerIdentification` reference.
- mediasoup — `versatica/mediasoup`, ISC. `ActiveSpeakerObserver` — constants source for
  our port.
- SCReAM — `EricssonResearch/scream`, Apache-2.0. C++ reference.
- livekit/client-sdk-js — `livekit/client-sdk-js`, Apache-2.0. SFrame worker reference at
  `src/e2ee/worker/`.
- element-call — `element-hq/element-call`, AGPL-3.0. MLS-backed key provider pattern.
- RNNoise — `xiph/rnnoise`, BSD-3-Clause.
- ten-vad — `TEN-framework/ten-vad`, MIT.
- Silero VAD — `snakers4/silero-vad`, **CC BY-NC 4.0 — do not use commercially**.

### Internal prior research (in parent `oxpulse-chat` feat/rooms)

- `docs/superpowers/research/2026-04-21-rust-gcc-twcc-crates.md` — definitive survey of
  Rust BWE options; we borrow its "copy-adapt from str0m" conclusion.
- `docs/superpowers/research/2026-04-21-sframe-implementations-deep.md` — SFrame impl
  survey.
- `docs/superpowers/research/2026-04-21-group-calls-architecture.md` — topology
  comparison (mesh vs SFU vs MCU) + Dynacast detail.
- `docs/superpowers/research/2026-04-21-str0m-deep-reference.md` — str0m internals.
- `crates/sfu/src/{bandwidth,pacer}.rs` + `crates/sfu/src/registry/bwe.rs` — working
  reference code we should port into the kit.
