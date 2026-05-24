# Security Policy

## Reporting a Vulnerability

Please report security vulnerabilities to **security@poli.page**.

Do not file public GitHub issues for security concerns.
We aim to respond within 48 hours.

## Supported Versions

Only the latest minor version of `poli-page` receives security updates.

## Supply-chain posture

- `Cargo.lock` is committed and used in CI (`--locked` on every build step).
- `cargo audit` runs on every PR — fails on any RustSec advisory affecting
  the resolved dep set.
- `cargo deny check` enforces license policy (MIT / Apache-2.0 / BSD permissive
  only), source policy (only crates.io), and a small ban list.
- All GitHub Actions are pinned to commit SHAs, not floating tags.
- Per-crate scoped publish token (set up via the crates.io UI) — no CI tokens,
  no automatic publishing.
- `#![forbid(unsafe_code)]` at the crate root.

See [`deny.toml`](deny.toml) and the [CI workflow](.github/workflows/ci.yml)
for the full enforcement rules.
