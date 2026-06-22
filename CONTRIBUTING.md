# Contributing to `poli-page`

Thanks for your interest. A few short rules below; the
[implementation plan](sdk-rust-specifications.md) and the
[Node SDK CONTRIBUTING](https://github.com/poli-page/sdk-node/blob/main/CONTRIBUTING.md)
provide the wider context.

## Working method

We use **TDD**: write a failing test first, then the minimum code to pass.
Every Phase-1+ commit in the history follows that pattern.

## Commit messages

[Conventional Commits](https://www.conventionalcommits.org/):
`feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`.

## Local development

The repo uses `rust-toolchain.toml` to pin the stable channel, so the
right toolchain installs automatically the first time you `cargo` anything.

```bash
cargo fmt --all                                            # format
cargo clippy --all-targets --all-features -- -D warnings   # lint
cargo test --all-features                                  # unit + doctests + wiremock
cargo doc --no-deps --all-features                         # check the rustdoc
```

The `blocking` Cargo feature gates the synchronous wrapper:

```bash
cargo build --features blocking
cargo test  --features blocking
```

To verify against the MSRV (Rust 1.93):

```bash
rustup install 1.93 --profile minimal
cargo +1.93 build --locked --all-features
```

`--locked` is critical — without it, a regenerated `Cargo.lock` can pull
dep versions that have silently bumped past 1.93 and mask the regression.

## Pre-push hook

Install once:

```bash
./scripts/install-hooks.sh
```

The hook runs `cargo fmt --check`, `cargo clippy -D warnings`, and
`cargo test --all-features` on every `git push`. Skips integration
tests by default; opt in with:

```bash
RUN_INTEGRATION=1 git push
```

## Integration tests

Integration tests hit the live API. They're gated by both the
`integration` Cargo feature **and** `--ignored`:

```bash
export POLI_PAGE_API_KEY=pp_test_...
cargo test --features integration --test integration -- --ignored
```

Override the target template / project via env vars:

```bash
POLI_PAGE_TEST_PROJECT=billing \
POLI_PAGE_TEST_TEMPLATE=invoice \
POLI_PAGE_TEST_VERSION=1.0.0 \
cargo test --features integration --test integration -- --ignored
```

The CI workflow `.github/workflows/integration.yml` runs the same suite
nightly with `POLI_PAGE_API_KEY` injected as a GitHub secret.

## Releases

Releases are **tag-driven**. Pushing a `vX.Y.Z` tag triggers
`.github/workflows/release.yml`, which runs the full verify gate, pauses
for a manual approval on the `crates-io-publish` environment, and then
publishes to crates.io via **Trusted Publishing** (OIDC — no token is
stored in the repo) and cuts the GitHub Release. `scripts/release.sh` is
kept only as a local emergency fallback; the bootstrap of `0.9.0` (the
first publish, which crates.io requires to be done by hand) is already
behind us.

1. Bump `version` in `Cargo.toml`.
2. Move `[Unreleased]` → `[X.Y.Z] - YYYY-MM-DD` in `CHANGELOG.md`. Keep the
   heading exactly `## [X.Y.Z] - YYYY-MM-DD` — the workflow extracts that
   section verbatim as the GitHub Release notes.
3. If MAJOR, add a section to `MIGRATION.md`.
4. Commit on `main`: `chore(release): vX.Y.Z`, push, and wait for `CI` to
   go green (`gh run watch`).
5. Tag the same version and push the tag:
   ```bash
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```
   The tag is only the trigger — the version of record is `Cargo.toml`. The
   `verify` job refuses to publish if `vX.Y.Z` ≠ the `Cargo.toml` version,
   so always bump-commit *then* tag.
6. Watch the run (`gh run watch` or the Actions tab). After `verify`
   passes, the `publish` job waits on the `crates-io-publish` environment
   for **your approval** — click *Review deployments → Approve*. On
   approval it publishes via OIDC and creates the Release.
7. Confirm `https://docs.rs/poli-page/<version>` built (~5 minutes after
   publish).

No crates.io token is needed for normal releases. Trusted Publishing is
configured on the crate (repo `poli-page/sdk-rust`, workflow `release.yml`,
environment `crates-io-publish`). A token in `~/.cargo/credentials.toml` is
only needed for the local `scripts/release.sh` fallback.

### Stable vs. prerelease

Cargo uses semver prerelease suffixes natively. `cargo add poli-page`
ignores prereleases; users opt in by pinning explicitly:

```bash
cargo add poli-page@1.0.0-rc.1
```

To cut a prerelease:

1. Set `version` in `Cargo.toml` to e.g. `2.0.0-rc.1`.
2. Move CHANGELOG entries under `[2.0.0-rc.1] - YYYY-MM-DD`.
3. Commit, push, then `git tag v2.0.0-rc.1 && git push origin v2.0.0-rc.1`.
   The workflow detects the `-rc.1` suffix and marks the GitHub Release as a
   prerelease; crates.io publishes it as a prerelease automatically.

To promote to stable: bump version to the form without the suffix
(`2.0.0`), move CHANGELOG entries to the stable heading, commit, and push
tag `v2.0.0`.

Stable and prerelease tags must never point at the same commit. Once a
prerelease promotes, the next prerelease starts a new suffix sequence
(e.g. `2.1.0-beta.0`).
