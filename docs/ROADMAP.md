# oxpulse-sfu-kit Roadmap

## v0.5.0 — RelaySource (✅ shipped)

Mark a `Client` as originating from an upstream SFU relay connection.

### Added
- `ClientOrigin { Local, RelayFromSfu(String) }` enum; zero cost when unused.
- `Client::set_origin()` / `Client::origin()` / `Client::is_relay()`.
- `TrackIn::relay_source: bool` — stamps relay origin at track-open time.
- `Propagated::UpstreamKeyframeRequest` — PLI/FIR routed to application layer instead of relay peer.
- `Propagated::PublisherLayerHintForUpstream` — Dynacast hint for inter-SFU signalling.
- Relay clients excluded from dominant-speaker detector.

### Known limitation
`serve_socket` / `run_udp_loop` silently drop application-facing events
(`UpstreamKeyframeRequest`, `PublisherLayerHintForUpstream`). Callers that need
these must drive the registry directly via `poll_all` + `fanout_pending` and
inspect the `Propagated` enum. A per-event hook is a candidate for v0.6.

### Call-order contract
`client.set_origin(ClientOrigin::RelayFromSfu(...))` must be called **before**
`registry.insert(client)`. The insert path reads `is_relay()` to skip
dominant-speaker detector registration.

## RelaySource — cascade SFU topology (✅ Phase 1 complete)

**Partner-edge:** `RelaySource` feature shipped (v0.5.0+). `ClientOrigin::RelayFromSfu` marks relay clients, `UpstreamKeyframeRequest` routes keyframes upstream, relay clients excluded from speaker detector.

**Signaling:** Cascade detection + async relay trigger shipped in oxpulse-chat `feat/rooms` (2026-04-23). `ServerMsg::UpgradeRelay` pushes migration to live peers after 6s settling delay.

**Phase 2 remaining:**
- ICE path for relay client UDP traffic (currently stub in `relay/client.rs`)
- Media forwarding from upstream edge to local Registry
- SFrame key-epoch forwarding through relay hops

---

## v0.6.0 — Planned

- App-facing event hook (`Registry::set_event_handler`) for relay and Dynacast events.
- `RelaySource` trait (v0.5 uses a plain `ClientOrigin` enum; v0.6 may add a constructor
  pattern to prevent the call-order footgun).
- WHIP ingestion endpoint.
- SCReAMv2 CC plugin (via `CongestionControl` trait, blocked on str0m TWCC API).

---

## CongestionControl — partially resolved in v0.6.0

`CongestionControl` trait shipped in v0.4.0 (`src/cc.rs`) as a dead seam.
In v0.6.0, TWCC feedback ingestion is now wired internally: `Registry::on_twcc_feedback`
accepts `TwccFeedback` and feeds it into `BandwidthEstimator`. Applications can now
drive Kalman delay + loss-based BWE without depending on str0m's internal GoogCC.

The plugin trait API (`CongestionControl` as an open seam for SCReAMv2/L4S) remains
future work. The current `DefaultGoogCC` no-op is still in place; plugging in
alternative CC algorithms (SCReAM, L4S) requires a future str0m TWCC raw-bytes API.

**Upstream feature request:** https://github.com/algesten/str0m/issues.
