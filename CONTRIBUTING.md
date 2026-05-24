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

Integration tests hit `https://api-develop.poli.page`. They're gated by
both the `integration` Cargo feature **and** `--ignored`:

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

Releases are **manual**. There is no CI workflow that auto-publishes — by
design (crates.io has no Trusted Publishing yet). The only supported
publishing path is `scripts/release.sh`.

1. Bump `version` in `Cargo.toml`.
2. Move `[Unreleased]` → `[X.Y.Z] - YYYY-MM-DD` in `CHANGELOG.md`.
3. If MAJOR, add a section to `MIGRATION.md`.
4. Commit on `main`: `chore(release): vX.Y.Z`.
5. From a clean main branch, run:
   ```bash
   ./scripts/release.sh
   ./scripts/release.sh --dry-run   # everything except `cargo publish` and the tag push
   ```
   The script runs pre-flight checks (clean tree, on `main`, tag doesn't
   already exist), `cargo fmt --check`, `cargo clippy -D warnings`,
   `cargo doc -D warnings`, `cargo test --all-features`, `cargo deny check`,
   `cargo audit`, integration tests (if `POLI_PAGE_API_KEY` is set),
   `cargo run --example demo` end-to-end, and `cargo publish --dry-run` —
   then asks you to confirm before the actual publish + tag push.
6. After publish + tag, visit `https://docs.rs/poli-page/<version>` to
   confirm the rustdoc build succeeded (~5 minutes after publish).

You must be logged in to crates.io with a token scoped to the `poli-page`
crate. The token lives in `~/.cargo/credentials.toml` on your machine and
never enters CI.

### Stable vs. prerelease

Cargo uses semver prerelease suffixes natively. `cargo add poli-page`
ignores prereleases; users opt in by pinning explicitly:

```bash
cargo add poli-page@1.0.0-rc.1
```

To cut a prerelease:

1. Set `version` in `Cargo.toml` to e.g. `2.0.0-rc.1`.
2. Move CHANGELOG entries under `[2.0.0-rc.1] - YYYY-MM-DD`.
3. Commit, run `./scripts/release.sh`, confirm at the prompt — crates.io
   publishes it as a prerelease.

To promote to stable: bump version to the form without the suffix
(`2.0.0`), move CHANGELOG entries to the stable heading, run
`./scripts/release.sh` again.

Stable and prerelease tags must never point at the same commit. Once a
prerelease promotes, the next prerelease starts a new suffix sequence
(e.g. `2.1.0-beta.0`).
