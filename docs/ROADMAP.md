# oxpulse-sfu-kit — Library Roadmap

> Public Rust library for WebRTC SFU primitives. Roadmap covers API surface,
> protocol features, and algorithm implementations — not operational security
> or deployment concerns (those live in oxpulse-partner-edge).

---

## ✅ Shipped

### v0.4.0 — BWE / Simulcast / Relay foundation
- `pacer`, `av1-dd`, `vfm` features; `LayerSelector`, Dynacast, `KeyEpoch` SFrame seam

### v0.5.0 — RelaySource
- `ClientOrigin::RelayFromSfu`, `UpstreamKeyframeRequest`, cascade SFU Phase 1

### v0.6.0 — Kalman BWE
- `kalman-bwe`: Kalman delay + loss BWE, TWCC ingestion, `update_pacer_layers`

---

## v0.7.0 — Q2 2026

- **App-facing event hook** — `Registry::set_event_handler` for relay/Dynacast events
- **Cascade relay Phase 2** — ICE/UDP path, media forwarding, `upstream_room_token` plumbing
- **SFrame key-epoch forwarding** through relay hops

---

## v0.8.0 — Q3 2026

- **FIPS 140-3 provider** — `feature = "fips"` swaps `ring` → `aws-lc-rs` validated provider
- **Asymmetric room token verification** — `feature = "ed25519-auth"`, SFU holds only public key
- **mTLS between relay edges** — `feature = "mtls-relay"` replaces shared-secret JWT

---

## v0.9.0 — Q4 2026

- **MLS group key management** — `feature = "mls"` (`openmls` crate, RFC 9420)
  - O(log N) RotateKey on member join/leave
  - Forward secrecy + post-compromise security
  - Hybrid post-quantum: XWING (X25519 + ML-KEM-768)
- **Key epoch forwarding** — SFrame `KeyEpoch` carried through relay chain

---

## v1.0.0 — 2027

- **Media over QUIC transport** — `feature = "moq"`, TLS 1.3 native (blocked on str0m QUIC)
- **SCReAMv2 CC plugin** — `CongestionControl` trait wired (blocked on str0m TWCC raw bytes)
- **Stable API guarantee** — semver-stable from v1.0; no breaking changes without major bump

---

## Blockers

| Feature | Upstream dependency |
|---------|-------------------|
| SCReAMv2 | str0m raw TWCC bytes |
| Media over QUIC | str0m QUIC transport |
| Post-quantum DTLS | IETF draft finalization |
