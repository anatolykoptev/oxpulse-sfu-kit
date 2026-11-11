# Contributing

Issues and pull requests are welcome. The kit stays small: SFU primitives on top of str0m.
Signaling, TURN, mixing and ingestion endpoints are out of scope (see the README).

## Build and test

The toolchain is stable Rust; the minimum supported version is 1.88.

```sh
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo test --all-targets --no-default-features
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
```

CI (`ci.yml`) runs exactly these, plus clippy with `--no-default-features` and with
`--no-default-features --features pacer`, and runs the tests on stable, beta and 1.88. Every
public item needs a doc comment (`missing_docs` is denied). Please build one cargo job at a time
with `CARGO_BUILD_JOBS=2` on small machines.

A pull request that changes only documents (Markdown, `docs/`, licence files, templates) skips the
heavy jobs.

## Commits

Conventional Commits: `feat:`, `fix:`, `perf:`, `refactor:`, `docs:`, `test:`, `ci:`, `chore:`.
release-please reads them to choose the next version and write `CHANGELOG.md`: `feat:` raises the
minor version, `fix:` the patch, and `feat!:` or a `BREAKING CHANGE:` footer is a breaking release
(before 1.0 it raises the minor version). Do not edit the version or the changelog by hand.

## Pull requests

- Keep a change to one concern and say how it was tested.
- Add a test with a behaviour change; the integration tests in `tests/` drive `Registry` and
  `Client` directly.
- Update [docs/API_SPEC.md](docs/API_SPEC.md) when you change the BWE, pacer or registry API.
- Update [docs/INDEX.md](docs/INDEX.md) when you add or rename a document.
- Contributions are licensed under the project's licence, `MIT OR Apache-2.0`.
