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

---

## v0.6.0 — Planned

- App-facing event hook (`Registry::set_event_handler`) for relay and Dynacast events.
- `RelaySource` trait (v0.5 uses a plain `ClientOrigin` enum; v0.6 may add a constructor
  pattern to prevent the call-order footgun).
- WHIP ingestion endpoint.
- SCReAMv2 CC plugin (via `CongestionControl` trait, blocked on str0m TWCC API).

---

## CongestionControl — blocked on str0m TWCC API

`CongestionControl` trait shipped in v0.4.0 (`src/cc.rs`) but is a dead seam.
str0m 0.18 absorbs raw TWCC packets internally; only the finished
`EgressBitrateEstimate` event surfaces. Plugging in SCReAM/L4S requires
`Event::TwccFeedback { peer_id, raw_bytes }` from str0m.

**Upstream feature request:** file at https://github.com/algesten/str0m/issues.
