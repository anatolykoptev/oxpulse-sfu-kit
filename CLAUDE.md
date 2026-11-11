# CLAUDE.md

Notes for coding agents working in oxpulse-sfu-kit: a Rust library of multi-client SFU primitives
on top of the sans-I/O WebRTC crate str0m (per-peer `Client`, room-level `Registry`, simulcast
layer selection, optional bandwidth estimation and pacing). It is published to crates.io. Every
document is indexed in [docs/INDEX.md](docs/INDEX.md).

## Layout

- `src/client/`, `src/registry/`: per-peer state machine, room routing and fanout.
- `src/bwe/`: Kalman and GoogCC bandwidth estimators, per-subscriber state, pacer config.
- `src/av1/`, `src/vfm/`, `src/sframe.rs`: AV1 Dependency Descriptor, RFC 9626 frame marking and
  the SFrame key-epoch header-extension seam.
- `tests/` (integration, including the deterministic pacer/BWE scenarios), `examples/`
  (`basic-sfu`, `synthetic_room` load generator), `benches/`.
- `docs/API_SPEC.md` is the integration-oriented spec for the BWE, pacer and registry surface.
  `.claude/CLAUDE.md` holds one machine-specific build rule.

## Gate

CI (`.github/workflows/ci.yml`) runs these jobs; run the same commands locally before a PR:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo clippy --all-targets --no-default-features -- -D warnings
cargo clippy --all-targets --no-default-features --features pacer -- -D warnings
cargo test --all-targets --all-features        # also on beta and on the MSRV, 1.88
cargo test --all-targets --no-default-features
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
```

Build with `CARGO_BUILD_JOBS=2` and one cargo build at a time (see `.claude/CLAUDE.md`). A
docs-only PR (Markdown, `docs/`, licence files, issue and PR templates) skips the heavy jobs
through the `docs-scope` job in `ci.yml`; that block is canonical across the fleet and compared
byte for byte, so do not edit it here.

## Invariants that are easy to break

- **Every feature combination must build.** The clippy matrix includes a pacer-only build because
  one arm of `poll_all` runs only there; a change gated on `kalman-bwe` or `googcc-bwe` must not
  break `--no-default-features` or `--features pacer`.
- **MSRV is 1.88** (`rust-version` in `Cargo.toml`, tested in CI). No newer language or std
  features.
- **`missing_docs` is denied.** Every public item needs a doc comment, and `cargo doc` runs with
  warnings as errors.
- **Registry-level BWE/pacer orchestration has a single driver.** `kalman-bwe` plus `pacer` builds
  feed the pacer through `update_pacer_layers`; pacer-only builds drive it from `poll_all`. Read
  the ownership section of `docs/API_SPEC.md` before touching either path.
- **The kit forwards media opaquely.** It never decrypts or inspects SFrame payloads; the key id in
  the RTP header extension is a non-authoritative hint. Do not add payload processing.
- **A relay client must be marked before insertion.** `Client::set_origin(ClientOrigin::RelayFromSfu(..))`
  goes before `Registry::insert`.
- **Versions come from release-please** (Conventional Commits, one version for the crate). Never
  tag or edit the version, `CHANGELOG.md` or `.release-please-manifest.json` by hand;
  `publish.yml` publishes to crates.io when a release is created and checks that the tag matches
  `Cargo.toml`.
- **Licence is `MIT OR Apache-2.0`** (`LICENSE-MIT`, `LICENSE-APACHE`). Do not change the files or
  the `license` field.
- **This repository is public.** No hostnames, operator paths, account names, tokens or personal
  data in any tracked file, tests and fixtures included.
