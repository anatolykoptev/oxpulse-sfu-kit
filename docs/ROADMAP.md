# oxpulse-sfu-kit — Security & Feature Roadmap

> **Design principle:** Every version must be deployable by a single engineer
> in under one hour. Security upgrades never break existing API surface.
> Clients include attorneys, physicians, and government officials — correctness
> and auditability matter more than velocity.

---

## ✅ Shipped

### v0.4.0 — BWE / Simulcast / Relay foundation
- `pacer` feature — Kalman BWE adaptive layer switching, `AudioOnlyMode`
- `av1-dd` feature — AV1 Dependency Descriptor parser + temporal-layer drop
- `vfm` feature — RFC 9626 Video Frame Marking (H.264/VP9/HEVC)
- `LayerSelector` + `BestFitSelector` — per-subscriber simulcast layer selection
- Dynacast `PublisherLayerHint`, `AudioCodecHint`, `KeyEpoch` SFrame seam

### v0.5.0 — RelaySource (cascade SFU Phase 1)
- `ClientOrigin::RelayFromSfu` — marks relay clients, excludes from speaker detector
- `Propagated::UpstreamKeyframeRequest` — PLI/FIR routed upstream not to relay peer
- `Propagated::PublisherLayerHintForUpstream` — Dynacast hint for inter-SFU signalling
- Signaling: cascade detection, `ServerMsg::UpgradeRelay`, relay-ready watch channel

### v0.6.0 — Kalman BWE + TWCC
- `kalman-bwe` feature — GoogCC-inspired Kalman delay + loss BWE
- TWCC feedback ingestion (`Registry::on_twcc_feedback`)
- `BandwidthEstimator` with 4-input `combined_bps`
- Auto audio-level extraction from `MediaData.ext_vals.audio_level`
- `Propagated::ClientBudgetHint` — browser DataChannel budget ceiling

---

## Phase 1 — Security Hardening  ·  v0.7.0  ·  Q2 2026

*Goal: close all known attack surfaces before regulated-industry GA.*

### Relay authentication (CRITICAL fixes from security audit)
- ✅ Room token verification in SFU (`room_auth.rs`, `SIGNALING_SFU_SECRET`)
- ✅ JWT replay protection (JTI nonce store, 1 000-entry eviction)
- ✅ Relay JWT migrated to RFC 7519 (`jsonwebtoken` HS256, drops homegrown HMAC/b64)
- ✅ Upstream allow-list in relay client (wss://*.oxpulse.chat only)
- ✅ `RELAY_JWT_SECRET` startup validation (refuses default, enforces ≥ 32 bytes)
- ✅ `RELAY_JWT_SECRET` SSRF fix — JWT fields used, not unsigned body fields
- ✅ TURN credential TTL: 24h → 1h

### App-facing event hook
- `Registry::set_event_handler(fn)` — callers receive `UpstreamKeyframeRequest`,
  `PublisherLayerHintForUpstream`, `PublisherLayerHint` without polling `poll_all`
- Unblocks `serve_socket` / `run_udp_loop` consumers from driving the registry manually

### Cascade relay Phase 2
- ICE/UDP path for relay client (currently stub; completes the outbound offerer)
- Media forwarding from upstream edge to local Registry
- `upstream_room_token` plumbed through to upstream SFU join (LOW-1 from audit)

---

## Phase 2 — Cryptographic Upgrade  ·  v0.8.0  ·  Q3 2026

*Goal: FIPS 140-3 readiness and elimination of shared-secret relay auth.*

### FIPS 140-3 cryptographic provider
- Replace `ring` crate with `aws-lc-rs` (AWS libcrypto fork, FIPS 140-3 validated)
- Enable FIPS mode via `aws_lc_rs::fips::enable()` at startup
- All HMAC, SHA-2, AES-GCM, X25519 operations through validated provider
- Required for US federal procurement (senators, DoD-adjacent clients)
- No API surface change; provider swap is internal

### mTLS + Ed25519 between SFU edges (replaces shared-secret relay JWT)
- Each partner-edge generates an Ed25519 keypair at registration
- Public key registered with oxpulse-chat signaling (replaces `RELAY_JWT_SECRET`)
- Relay tokens signed by signaling's private key, verified by edge public keys
- One compromised edge cannot forge tokens for others (vs. current shared secret)
- Implementation: `rcgen` for cert generation, `rustls` for mTLS handshake

### Room token: asymmetric (Ed25519) signing
- Replace HS256 room tokens with Ed25519 EdDSA (signaling holds private key)
- SFU edges hold only public key — cannot forge tokens even if fully compromised
- `jsonwebtoken` supports EdDSA in v9+; backward-compatible migration path

---

## Phase 3 — Key Transparency & Forward Secrecy  ·  v0.9.0  ·  Q4 2026

*Goal: cryptographic proof that keys have not been silently replaced (MITM prevention).*

### Key Transparency (KT) log
- Append-only Merkle log of SFrame public keys per room
- Clients can verify their key was not replaced between sessions
- Prevents MITM even against a fully compromised signaling server
- Reference: Google Key Transparency, Apple's PQ3 implementation
- Implementation: `transparency-dev/trillian`-compatible log or lightweight custom

### SFrame proper group key management
- Replace P2P DataChannel key exchange with MLS (RFC 9420)
- O(log N) RotateKey on member join/leave (vs. current O(N) P2P renegotiation)
- Forward secrecy: departed member's key material is cryptographically erased
- Post-compromise security: re-joins cannot access prior session content
- Implementation: `openmls` crate (Apache-2.0, production-ready Rust MLS)
- Wire format: `draft-ietf-mimi-content-08` for interop

### MLS post-quantum hybrid
- MLS `CipherSuite: MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519` (baseline)
- Upgrade path to `MLS_256_XWING_AES256GCM_SHA512_Ed448` (post-quantum hybrid)
- `XWING` = X25519 + ML-KEM-768 hybrid (NIST winner, already in BoringSSL)

---

## Phase 4 — Transport & Compute Hardening  ·  v1.0.0  ·  2027

*Goal: protocol-level post-quantum + operator-blind execution.*

### Media over QUIC (MoQ)
- Replace DTLS 1.2/SRTP with QUIC transport (TLS 1.3 natively, no legacy cipher suites)
- Post-quantum handshake via ML-KEM-768 (already in BoringSSL/AWS-LC)
- No head-of-line blocking, 0-RTT reconnect
- IETF standard `draft-ietf-moq-transport` maturing in 2026
- Requires str0m QUIC support or separate QUIC media stack

### Confidential Computing for SFU (TEE)
- Run SFU binary inside Intel TDX or AMD SEV-SNP enclave
- Even the operator (oxpulse.chat infra team) cannot see call content
- Remote attestation: clients verify SGX measurement before sending media
- Reference: Signal uses Intel SGX for Contact Discovery Service
- Provider support: AWS Nitro Enclaves, Azure Confidential VMs, Oracle OCI CC
- Implementation timeline: ~3 months; requires hardware-specific build

### SCReAMv2 pluggable CC
- `CongestionControl` trait (v0.4.0 seam) wired to real SCReAMv2 implementation
- Blocked on str0m exposing raw TWCC feedback bytes
- Upstream feature request: `algesten/str0m#issues`

---

## Security Audit Status

| Finding | Severity | Status |
|---------|----------|--------|
| SSRF via unsigned upstream_url | CRITICAL | ✅ Fixed v0.7 |
| Default `RELAY_JWT_SECRET` | CRITICAL | ✅ Fixed v0.7 |
| SFU relay_source without auth | CRITICAL | ✅ Fixed v0.7 |
| JWT replay (no JTI) | HIGH | ✅ Fixed v0.7 |
| TURN TTL 24h | HIGH | ✅ Fixed v0.7 |
| Panic on UTF-8 room_id | HIGH | ✅ Fixed v0.7 |
| Homegrown HMAC/base64url JWT | MEDIUM | ✅ Fixed v0.7 |
| Upstream allow-list | MEDIUM | ✅ Fixed v0.7 |
| Shared symmetric relay secret | LOW | → Phase 2 Ed25519 |
| P2P DataChannel key distribution | MEDIUM | → Phase 3 MLS |
| DTLS 1.2 (no PQ) | MEDIUM | → Phase 4 MoQ |
| No FIPS 140-3 provider | — | → Phase 2 aws-lc-rs |
| No Key Transparency | — | → Phase 3 KT |
| No operator-blind execution | — | → Phase 4 TEE |

---

## Dependencies & Blockers

| Feature | Blocker |
|---------|---------|
| SCReAMv2 | str0m raw TWCC bytes exposure |
| Media over QUIC | str0m QUIC transport; MoQ spec maturity |
| Confidential Computing | Hardware (Intel TDX, AMD SEV-SNP) |
| Post-quantum DTLS | str0m upgrade; IETF draft finalization |
