#!/usr/bin/env bash
# Primary publishing path for the `poli-page` Rust SDK.
# Spec §16.3 — local, manual, gated on a confirmation prompt.
#
# Usage:
#   ./scripts/release.sh              # full release: verify, publish, tag, push
#   ./scripts/release.sh --dry-run    # everything except cargo publish + git push tag
#
# Pre-flight asserts on `main`, clean working tree, target tag doesn't yet
# exist on local OR remote, and that a crates.io login token is present in
# the user's ~/.cargo/credentials.toml (or via CARGO_REGISTRY_TOKEN).

set -euo pipefail

# ─── Colors ──────────────────────────────────────────────────────────────────
if [[ -t 1 && -z "${NO_COLOR:-}" ]]; then
    bold=$'\e[1m'; dim=$'\e[2m'; red=$'\e[31m'; green=$'\e[32m'
    yellow=$'\e[33m'; cyan=$'\e[36m'; reset=$'\e[0m'
else
    bold='' dim='' red='' green='' yellow='' cyan='' reset=''
fi

step() { printf "\n%s%s%s\n" "$cyan$bold" "──── $* ────" "$reset"; }
ok()   { printf "  %s✓%s %s\n" "$green" "$reset" "$*"; }
warn() { printf "  %s⚠ %s%s\n" "$yellow" "$*" "$reset"; }
die()  { printf "\n%s✗ %s%s\n" "$red" "$*" "$reset" >&2; exit 1; }

# ─── Args ────────────────────────────────────────────────────────────────────
DRY_RUN=0
for arg in "$@"; do
    case "$arg" in
        --dry-run) DRY_RUN=1 ;;
        -h|--help)
            sed -n '2,8p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *) die "Unknown argument: $arg (try --help)" ;;
    esac
done

cd "$(dirname "$0")/.."

# ─── Pre-flight ──────────────────────────────────────────────────────────────
step "Pre-flight"

VERSION=$(awk -F\" '/^version =/ {print $2; exit}' Cargo.toml)
[[ -n "$VERSION" ]] || die "Could not read version from Cargo.toml"
TAG="v$VERSION"
ok "Version in Cargo.toml: $VERSION (tag would be $TAG)"

[[ "$(git rev-parse --abbrev-ref HEAD)" == "main" ]] \
    || die "Not on main branch. Switch with: git checkout main"
ok "On main branch"

if ! git diff-index --quiet HEAD --; then
    die "Working tree is dirty. Commit or stash changes before releasing."
fi
ok "Working tree clean"

if git rev-parse "$TAG" >/dev/null 2>&1; then
    die "Local tag $TAG already exists. Either bump the version or delete the tag."
fi

if git ls-remote --tags origin "$TAG" 2>/dev/null | grep -q "refs/tags/$TAG"; then
    die "Remote tag $TAG already exists on origin. Pick a higher version."
fi
ok "Tag $TAG not yet on remote"

if [[ -z "${CARGO_REGISTRY_TOKEN:-}" && ! -f "$HOME/.cargo/credentials.toml" ]]; then
    die "No crates.io credentials. Run 'cargo login' or set CARGO_REGISTRY_TOKEN."
fi
ok "crates.io credentials present"

# ─── Verify ──────────────────────────────────────────────────────────────────
step "Verify"

cargo fmt --all --check
ok "cargo fmt --all --check"

cargo clippy --all-targets --all-features --locked -- -D warnings
ok "cargo clippy --all-targets --all-features -- -D warnings"

RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features --locked
ok "cargo doc --no-deps --all-features (RUSTDOCFLAGS=-D warnings)"

cargo test --all-features --locked
ok "cargo test --all-features"

cargo test --no-default-features --features blocking --locked
ok "cargo test --no-default-features --features blocking"

if command -v cargo-deny >/dev/null 2>&1; then
    cargo deny check
    ok "cargo deny check"
else
    warn "cargo-deny not installed locally; skipping (CI still enforces it)"
fi

if command -v cargo-audit >/dev/null 2>&1; then
    cargo audit
    ok "cargo audit"
else
    warn "cargo-audit not installed locally; skipping (CI still enforces it)"
fi

if [[ -n "${POLI_PAGE_API_KEY:-}" ]]; then
    cargo test --features integration --test integration -- --ignored
    ok "integration tests against develop API"
    cargo run --example demo --quiet
    ok "demo end-to-end smoke against develop API"
else
    warn "POLI_PAGE_API_KEY not set; skipping integration tests + demo smoke"
fi

cargo publish --dry-run --locked
ok "cargo publish --dry-run"

# ─── Confirm + Publish ───────────────────────────────────────────────────────
step "Ready to publish $bold$VERSION$reset to crates.io"

if [[ "$DRY_RUN" == 1 ]]; then
    printf "\n%s--dry-run requested. Stopping before cargo publish + tag push.%s\n" "$dim" "$reset"
    exit 0
fi

printf "\nType '%spublish%s' to publish: " "$bold" "$reset"
read -r confirm
[[ "$confirm" == "publish" ]] || die "Aborted (got: '$confirm')"

step "Publishing"
cargo publish --locked
ok "Published to crates.io"

# Give crates.io a moment to ingest before the tag push announces the version.
sleep 5

step "Tagging"
git tag "$TAG"
git push origin "$TAG"
ok "Pushed $TAG to origin"

printf "\n%s✓ Done.%s\n" "$green$bold" "$reset"
printf "  Crates.io:   https://crates.io/crates/poli-page/%s\n" "$VERSION"
printf "  docs.rs:     https://docs.rs/poli-page/%s (builds in ~5 min)\n" "$VERSION"
printf "  Github tag:  https://github.com/poli-page/sdk-rust/releases/tag/%s\n" "$TAG"
