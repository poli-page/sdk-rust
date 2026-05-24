# Poli Page SDK — Engineering Guide

**Version**: 1.2
**Status**: SHOULD/MAY engineering practices for every official Poli Page SDK
**Audience**: SDK authors building Poli Page client libraries in any language.
**Reference implementation**: [`@poli-page/sdk`](https://github.com/poli-page/sdk-node) (Node.js).

This document defines **how** to build, test, validate, and ship an SDK so that the fleet stays consistent and production-grade. The wire-and-runtime contract every SDK MUST implement is in `sdk-specification.md` (v1.3) — read that first.

Companion docs (all co-located in the `sdk-node` repo):
- `sdk-specification.md` (v1.3) — wire-and-runtime contract every SDK MUST implement.
- `sdk-roadmap.md` (v1.2) — what to build, in which order, and why (multi-language fleet).
- `implementation-plan-master-mickael.md` — operating manual for the full client-side scope (templates, SDKs, integrations, quickstarts).
- `implementation-plan-sdk-node.md` — per-session plan for the canonical Node SDK.
- `PLAN.md` — unified live plan: current state, gaps to v1.0, sequencing.

---

## Conventions

This guide uses **RFC 2119** keywords. Most provisions are **SHOULD** — the goal is consistent quality across the fleet without micromanaging language-specific tooling. **MUST** appears only for items where divergence breaks the contract or the fleet's collective trust signals.

Each section ends with a non-normative **Suggested tooling per ecosystem** table covering Node/TS, Python, PHP, Go, Ruby, Rust, Java/Kotlin, .NET. Tables age — update them freely as ecosystems evolve. Rows that don't apply to a given ecosystem are marked `n/a`.

---

## Table of contents

1. [Working method](#1-working-method)
2. [Repository hygiene](#2-repository-hygiene)
3. [Versioning & release channels](#3-versioning--release-channels)
4. [CI gates](#4-ci-gates)
5. [Distribution](#5-distribution)
6. [Release flow](#6-release-flow)
7. [API-reference site](#7-api-reference-site)
8. [Pre-push hook](#8-pre-push-hook)
9. [Demo (cross-reference)](#9-demo-cross-reference)
10. [Open questions per ecosystem](#10-open-questions-per-ecosystem)

---

## 1. Working method

### 1.1 TDD is the working method

Every SDK MUST ship as a sequence of **RED → GREEN → refactor** cycles. Write the smallest possible failing test that captures the next bit of behavior, write the minimum code to make it pass, refactor while green. Never "write it all then add tests".

### 1.2 What to test

- Every public method of the client class / package.
- Every error path — 4xx code mapping, 5xx retry behavior, network failure, timeout, malformed JSON, 2xx with wrong Content-Type.
- Every retry edge case — exponential backoff, jitter range, max attempts, never retrying 4xx (except 429), `Retry-After` honored as-is, sleep cancellable.
- Every input variant — project mode (`project + template + version`) vs inline mode (`template`), each rendering endpoint (PDF, stream, preview, thumbnails).
- Idempotency-key reuse across retries.
- Constructor validation (missing `apiKey`).
- The isomorphism boundary — main entry has no forbidden imports (see §4.6).

### 1.3 What NOT to over-test

- Don't test the language standard library or the HTTP client library — assume they work.
- Don't test private helpers in isolation if they're already exercised by a public-method test.
- Don't snapshot massive objects when an assertion on the field that matters would be clearer.

### 1.4 Test layout

- Tests live in `tests/` (or the language's idiomatic location).
- One test file per source file, mirroring the structure: `src/client.<ext>` → `tests/client.test.<ext>` (or `test_client.py`, `ClientTest.php`, `client_test.go`).
- Group integration tests under `tests/integration/` so they can run separately from the unit suite.
- **Unit tests** mock the HTTP transport (a local HTTP server or mocked client), assert request shape and response handling. These are 90 %+ of the suite.
- **Integration tests** hit the real API with a `pp_test_*` key from `POLI_PAGE_API_KEY`. Render a known template, verify the PDF is non-empty and `Content-Type: application/pdf`. Keep them few and idempotent.

### 1.5 Suggested tooling per ecosystem

| Ecosystem    | Test runner                              | Mock HTTP                          | Lint / format                           |
| ------------ | ---------------------------------------- | ---------------------------------- | --------------------------------------- |
| Node / TS    | Vitest                                   | local `http.createServer` or `msw` | ESLint + Prettier                       |
| Python       | pytest + pytest-asyncio                  | `respx` / `pytest-httpx`           | Ruff (lint + format)                    |
| PHP          | PHPUnit (or Pest)                        | Guzzle MockHandler / PSR-18 stub   | PHP-CS-Fixer (Pint for Laravel)         |
| Go           | `go test` + testify                      | `httptest.NewServer`               | `gofmt`, `go vet`, `staticcheck`        |
| Ruby         | RSpec                                    | WebMock / VCR                      | RuboCop                                 |
| Rust         | `cargo test`                             | `wiremock` / `mockito`             | `rustfmt`, `cargo clippy`               |
| Java / Kotlin| JUnit 5 + AssertJ                        | WireMock / MockWebServer           | Checkstyle / ktlint                     |
| .NET (C#)    | xUnit                                    | `WireMock.Net` / `HttpClient` handler | `dotnet format`                       |

---

## 2. Repository hygiene

### 2.1 Required files at the repo root

| File              | Purpose                                                                       |
| ----------------- | ----------------------------------------------------------------------------- |
| `README.md`       | Install, 5-line quick start, methods table, configuration table, error handling, runtime support, links to API reference and `docs.poli.page`. |
| `LICENSE`         | MIT, in the brand's name.                                                     |
| `CHANGELOG.md`    | [Keep a Changelog](https://keepachangelog.com/) format. Updated in the same commit as every version bump. |
| `MIGRATION.md`    | Documents breaking changes between major versions. Even a one-paragraph stub is the right precedent. |
| `CONTRIBUTING.md` | Local dev, integration tests, releasing, prerelease channel.                  |
| `SECURITY.md`     | How to report a security issue (mailbox, response time SLO).                  |

In addition to the root files, the `.github/` directory SHOULD contain:

| Path                                       | Purpose                                                                       |
| ------------------------------------------ | ----------------------------------------------------------------------------- |
| `.github/ISSUE_TEMPLATE/bug_report.yml`    | YAML form capturing SDK version, runtime, repro, and (optional) requestId.    |
| `.github/ISSUE_TEMPLATE/feature_request.yml` | YAML form. Flag whether the change is Node-only or cross-SDK.               |
| `.github/ISSUE_TEMPLATE/config.yml`        | Disable blank issues; link to platform docs and the security mailbox.         |
| `.github/PULL_REQUEST_TEMPLATE.md`         | Conventional-commit prompt + verification checklist (lint/typecheck/test/pack). |
| `.github/CODEOWNERS`                       | Default reviewers. CI + release paths SHOULD require an explicit owner.       |
| `.github/dependabot.yml`                   | Weekly updates for the language ecosystem and `github-actions`. See §4.12.    |

### 2.2 Code conventions

- Follow the dominant style guide of the language. Pin formatter and linter major versions in the manifest so contributors and CI agree.
- **No commented-out code.** Delete it; git remembers.
- **No `TODO` without a linked issue** — `// TODO(#42): refactor` is fine, `// TODO: refactor` is not.
- **No debug prints** in committed code.
- **Default to no comments.** Add one only when the *why* is non-obvious — a hidden constraint, a workaround, a surprising invariant. Comments that restate what the code already says are noise.
- **Robustness over shortcuts.** No hacks to make a test pass or a corner case go away. If something is broken, fix the cause. Document genuine third-party workarounds inline with a `Why:` comment.

### 2.3 Branch protection on `main`

`main` SHOULD require:
- Pull request review (when there are 2+ regular maintainers).
- All required CI checks passing.
- Linear history (squash or rebase merge).

Direct push to `main` SHOULD be reserved for documentation hot-fixes.

### 2.4 Suggested tooling per ecosystem

| Ecosystem    | Issue/PR templates | Branch protection check  |
| ------------ | ------------------ | ------------------------ |
| All          | `.github/*` (GitHub-native) | GitHub Settings → Branches |

---

## 3. Versioning & release channels

### 3.1 Semantic Versioning

Every SDK MUST follow [SemVer 2.0.0](https://semver.org/spec/v2.0.0.html):

- **MAJOR** — breaking changes to the contract or the public API surface.
- **MINOR** — new features, backwards-compatible.
- **PATCH** — bug fixes, no API change.

Pre-1.0 versions (`0.x.y`) MAY break in minor bumps; treat them as experimental.

### 3.2 CHANGELOG discipline

Update `CHANGELOG.md` in the **same commit** as the version bump. Use Keep a Changelog sections: `Added`, `Changed`, `Deprecated`, `Removed`, `Fixed`, `Security`. Mark breaking changes with `**BREAKING**:` at the start of the bullet.

### 3.3 MIGRATION.md authoring

Every major bump SHOULD add a section to `MIGRATION.md` describing what changed and how to adapt. Even a stub is the right precedent — empty migration docs invite "I'll write it later" debt.

### 3.4 Prerelease channel

Every SDK SHOULD support a prerelease channel for validating breaking changes or large features before promoting them to stable. Pattern:

| Stable                   | Prerelease                                  |
| ------------------------ | ------------------------------------------- |
| `1.2.3`                  | `1.2.3-rc.1`, `2.0.0-beta.0`, `1.3.0-alpha.2` |
| Default install resolves | Opt-in only                                  |

Each ecosystem has its own dist-tag / channel mechanism. Document the opt-in command in `CONTRIBUTING.md` and the README.

#### Promotion flow

1. Bump version with prerelease suffix (e.g. `2.0.0-rc.1`). Move CHANGELOG entries under that heading.
2. Publish to the prerelease channel.
3. Tag the commit (`v2.0.0-rc.1`) and push.
4. When ready, bump to the stable version (drop the suffix), move CHANGELOG entries, publish to the stable channel.

Stable and prerelease channels MUST never point at the same version — once a prerelease is promoted, the next prerelease starts a new pre-suffix sequence.

### 3.5 Suggested tooling per ecosystem

| Ecosystem    | Stable channel        | Prerelease channel                     | Opt-in command (user)                                 |
| ------------ | --------------------- | -------------------------------------- | ----------------------------------------------------- |
| Node / TS    | npm dist-tag `latest` | npm dist-tag `next`                    | `npm install @poli-page/sdk@next`                     |
| Python       | PyPI standard         | PyPI prerelease (auto-detected by pip) | `pip install --pre poli-page`                         |
| PHP          | Packagist stable      | Packagist `dev-*` branch alias         | `composer require poli-page/sdk:^2.0@beta`            |
| Go           | Module path version   | Pre-release semver tag                 | `go get github.com/poli-page/sdk-go@v2.0.0-rc.1`      |
| Ruby         | RubyGems stable       | RubyGems prerelease (`--pre`)          | `gem install poli-page --pre`                          |
| Rust         | crates.io stable      | crates.io prerelease semver            | `cargo add poli-page@2.0.0-rc.1`                      |
| Java/Kotlin  | Maven Central release | Maven Central staging / Sonatype OSSRH | `<version>2.0.0-rc.1</version>`                        |
| .NET (C#)    | NuGet stable          | NuGet prerelease                       | `dotnet add package PoliPage --version 2.0.0-rc.1`    |

---

## 4. CI gates

Every SDK MUST ship a single CI workflow that runs on `push` (any branch) and `pull_request` targeting `main`. The workflow MUST be green-blocking for merge.

The workflow runs the following gates **in order**. Steps SHOULD short-circuit gracefully when the relevant manifest or directory does not yet exist (so a freshly scaffolded repo has a green pipeline from day one).

### 4.1 Language-version matrix

CI MUST run against every currently-supported language version (and at least one EOL version that's still in widespread use).

| Ecosystem    | Versions in matrix       |
| ------------ | ------------------------ |
| Node         | 20, 22, 24                |
| Python       | 3.11, 3.12, 3.13          |
| PHP          | 8.2, 8.3, 8.4             |
| Go           | 1.22, 1.23, 1.24          |
| Ruby         | 3.2, 3.3, 3.4             |
| Rust         | stable, MSRV (whatever's pinned in `rust-toolchain.toml`) |
| Java/Kotlin  | LTS (17, 21) + current    |
| .NET (C#)    | 8.0 LTS + current         |

Bump matrix entries when a new minor lands and the previous one drops out of upstream support.

CI SHOULD also include at least one job on a non-Linux OS for ecosystems where path handling, line endings, or shell differences are realistic regression sources (Node, Python, .NET). Run the full language-version matrix on `ubuntu-latest` and a single LTS-version job on `windows-latest` (and `macos-latest` if your CI minutes allow). Set `defaults.run.shell: bash` so heredocs and `mktemp` work across runners — Windows runners ship Git Bash.

### 4.2 Lint / format

Run the configured linter and formatter as **separate** CI steps. Fail the build on any violation.

### 4.3 Type check (where applicable)

Languages with a separate type-checking step (TypeScript `tsc --noEmit`, Python `mypy`/`pyright`, Java/Kotlin compile, Rust `cargo check`) MUST run it.

### 4.4 Unit tests

Run the full unit suite. Fail on any test failure or unexpected skip.

### 4.5 Type tests (where applicable)

Languages where the type system is itself a public API surface (TypeScript) SHOULD ship type-level tests asserting that invalid combos fail to type-check. The Node SDK uses Vitest's `--typecheck` flag for this.

### 4.6 Build (where applicable)

Languages with a build artifact (TypeScript → JS, Rust → binary/lib, Go → binary) MUST build in CI. Fail on any error or warning treated as error.

### 4.7 Package-publication validator

Before a release, the package manifest and the artifact it produces MUST pass an ecosystem-native publication validator. This catches manifest misconfiguration (export-map ordering, file allowlist, dual-package hazards, missing types, etc.) before users hit it.

### 4.8 Install smoke

After packaging, CI MUST install the packaged tarball/wheel/jar into a throwaway directory and exercise the SDK's main entry from a minimal program. File existence is not enough — module resolution must succeed.

For Node, this means both `import { PoliPage } from '@poli-page/sdk'` (ESM) and `const { PoliPage } = require('@poli-page/sdk')` (CJS) MUST run without error.

### 4.9 Bundle / binary size budget (where applicable)

Languages where artifact size is published or visible to users (Node/npm, Rust crates, .NET NuGet) SHOULD fail CI when the artifact exceeds a documented budget. The Node SDK's budget is **50 KB** for both ESM and CJS main entries.

The budget exists to catch accidental fat dependencies. Bump it deliberately when justified; don't drift silently.

### 4.10 Integration tests (gated)

Integration tests hit the real API. They MUST NOT run on every push (cost, flakiness, secret exposure). Run them:

- On a separate workflow / job triggered by a label, schedule, or release tag.
- Locally via the pre-push hook (§8) with `SKIP_INTEGRATION` opt-out.
- In the manual release script (§6).

The CI environment MUST inject the sandbox key as `POLI_PAGE_API_KEY` from a secret. Never log the key.

### 4.11 Supply-chain audit

CI SHOULD run a supply-chain integrity check on every push. Two cheap, high-value signals:

1. **Registry-signature verification** of installed dependencies. Catches typosquats, registry-side tampering, and unsigned packages slipping into the dep graph.
2. **Static analysis** of the SDK source itself (CodeQL or the language-native equivalent). Run on `push`, `pull_request`, and at least weekly so a quiet repo still surfaces newly-discovered CVE-class patterns. Use the "security and quality" query set when available.

Findings from static analysis SHOULD land in the repository's security tab so triage is centralised. Do **not** suppress findings inline without a `Why:` comment.

### 4.12 Dependency automation

Every SDK SHOULD enable automated dependency updates (Dependabot, Renovate, or the ecosystem-idiomatic equivalent) with at minimum:

- **Weekly cadence** for the language package ecosystem and (where applicable) `github-actions`.
- **Grouping** of dev-only minor/patch updates into a single PR to reduce review churn.
- A modest `open-pull-requests-limit` (≤ 5 is plenty) to avoid drowning maintainers.

Tag automation PRs with a `dependencies` label so they're easy to batch-review.

### 4.13 Suggested tooling per ecosystem

| Ecosystem    | Lint / format                  | Type check        | Build       | Package validator           | Install smoke                          | Size budget                          | Signature audit                  | Static analysis           | Dep automation                     |
| ------------ | ------------------------------ | ----------------- | ----------- | --------------------------- | -------------------------------------- | ------------------------------------ | -------------------------------- | ------------------------- | ---------------------------------- |
| Node / TS    | ESLint + Prettier              | `tsc --noEmit`    | `tsup`      | `attw` + `publint`          | `npm pack` + install in tmpdir + import  | `size-limit`                       | `npm audit signatures`           | CodeQL (`javascript-typescript`) | `dependabot` (npm + github-actions) |
| Python       | Ruff                           | `mypy` / `pyright`| `python -m build` | `twine check` + `pyroma` | `pip install ./dist/*.whl` + `python -c "import poli_page"` | n/a (small wheel)                  | `pip-audit` / `safety`           | CodeQL (`python`)         | `dependabot` (pip + github-actions) |
| PHP          | PHP-CS-Fixer / Pint            | PHPStan / Psalm   | n/a         | `composer validate --strict` | `composer install` + autoload smoke   | n/a                                  | `composer audit`                 | Psalm taint analysis      | `dependabot` (composer + github-actions) |
| Go           | `gofmt` + `go vet` + `staticcheck` | n/a (compiler) | `go build`  | `goreleaser check`          | `go run ./cmd/smoke`                  | binary size via `goreleaser`         | `govulncheck`                    | CodeQL (`go`)             | `dependabot` (gomod + github-actions) |
| Ruby         | RuboCop                        | Sorbet (`srb tc`) optional | `gem build` | `gem-release` checks    | `gem install ./poli-page-*.gem` + smoke | n/a                                | `bundle audit`                   | CodeQL (`ruby`)           | `dependabot` (bundler + github-actions) |
| Rust         | `rustfmt` + `cargo clippy`     | `cargo check`     | `cargo build` | `cargo publish --dry-run` | `cargo install --path .` + run example | `cargo bloat`                       | `cargo audit` / `cargo deny`     | `cargo clippy` (deny)     | `dependabot` (cargo + github-actions) |
| Java/Kotlin  | Checkstyle / ktlint            | compiler          | Gradle/Maven | `mvn verify` / `gradle check` | install to local Maven, depend on it | n/a (JAR size in CI report)          | OWASP `dependency-check`         | CodeQL (`java`)           | `dependabot` (maven/gradle + github-actions) |
| .NET (C#)    | `dotnet format`                | compiler          | `dotnet build` | `dotnet pack` + `meziantou.analyzer` | `dotnet add package` from local feed | n/a                              | `dotnet list package --vulnerable` | CodeQL (`csharp`)       | `dependabot` (nuget + github-actions) |

---

## 5. Distribution

### 5.1 Manifest shape

Every SDK MUST publish a well-formed manifest with at minimum:

- Canonical package name (per `sdk-roadmap.md`).
- Version (semver).
- Description (one sentence).
- Repository URL.
- Homepage (`https://poli.page`).
- License (MIT).
- Author (Poli Page).
- Engines / runtime requirements (minimum language version).
- Keywords / topics for discovery.
- Bug-tracker URL.

### 5.2 Dual-format support (where the ecosystem demands it)

Some ecosystems split the world into "old format" and "new format" runtimes. The SDK MUST ship both formats from a single package whenever the split is real for users:

| Ecosystem    | Formats to ship                                                  |
| ------------ | ---------------------------------------------------------------- |
| Node / TS    | ESM (`import`) **and** CJS (`require`), with separate types for each |
| Python       | sdist (`.tar.gz`) **and** wheel (`.whl`)                          |
| Java/Kotlin  | Compiled JAR; sources JAR; javadoc JAR                            |
| .NET (C#)    | Multi-target frameworks where relevant (`netstandard2.0`, `net8.0`)|

The cost of "single format only" is silent breakage in real-world deployments months after release, when a user migrates a runtime.

### 5.3 Sub-export pattern for runtime-specific helpers

Per `sdk-specification.md` §9, the main entry MUST be free of runtime-specific OS APIs. Helpers that need them (filesystem, child processes) MUST be exposed via a sub-package, sub-export, or feature flag:

| Ecosystem    | Pattern                                                          |
| ------------ | ---------------------------------------------------------------- |
| Node / TS    | Sub-export in `package.json` `exports` map (`./node`, `./browser`) |
| Python       | n/a — same package                                                |
| Go           | Same package; deferred I/O via `io.Reader`/`io.Writer`            |
| Rust         | Cargo feature flag (`fs`, `tokio`, etc.)                          |
| Java/Kotlin  | Same package; deferred I/O via streams                            |
| .NET (C#)    | Same package; deferred I/O via `Stream`                            |

### 5.4 Source maps / debug symbols

Every SDK SHOULD ship source maps or debug symbols so users can step into the SDK from their debugger. The Node SDK ships `.d.ts.map` files for "Go to Definition".

### 5.5 Tree-shakability / dead-code elimination

Where the language has a tree-shaking story (Node ESM, Rust, .NET trimming), the SDK MUST opt in:

- Node: `"sideEffects": false` in `package.json`.
- Rust: avoid `lazy_static!` of unused globals; keep features minimal.
- .NET: avoid reflection where the trimming analyzer can't see it.

### 5.6 Suggested tooling per ecosystem

| Ecosystem    | Manifest file        | Build artifact                 | Format split                        |
| ------------ | -------------------- | ------------------------------ | ----------------------------------- |
| Node / TS    | `package.json`       | `dist/index.{js,cjs,d.ts,d.cts}` | dual `import`/`require` via `exports` |
| Python       | `pyproject.toml`     | `dist/*.{whl,tar.gz}`          | sdist + wheel via `python -m build` |
| PHP          | `composer.json`      | n/a (source-distributed)       | n/a                                  |
| Go           | `go.mod`             | n/a (source-distributed)       | n/a                                  |
| Ruby         | `*.gemspec`          | `*.gem`                        | n/a                                  |
| Rust         | `Cargo.toml`         | `*.crate` on crates.io         | optional features                    |
| Java/Kotlin  | `pom.xml` / `build.gradle.kts` | JAR + sources + javadoc | multi-release JAR optional           |
| .NET (C#)    | `*.csproj` / `*.nuspec` | `*.nupkg`                   | multi-target frameworks              |

---

## 6. Release flow

### 6.1 Manual release script as the only blessed path

Every SDK MUST ship a manual release script as the **default and primary** publishing path. Auto-publish from CI on tag push is NOT the default — it removes a critical human gate where the maintainer reviews the tarball before it ships.

The script MUST do, in order:

1. **Pre-flight**: assert on `main`, working tree clean, target tag does not yet exist.
2. **Verify**: run lint, typecheck, unit tests; integration tests if `POLI_PAGE_API_KEY` is set.
3. **Build**: produce the dist/release artifact.
4. **Pack**: produce the publishable tarball / wheel / gem and **show the contents and total size** to the maintainer.
5. **Confirm**: prompt the user before publishing. Abort cleanly on `n`.
6. **Publish**: push the artifact to the registry with the correct visibility flag (e.g. `--access public`).
7. **Tag**: create the local `vX.Y.Z` tag. **Do not push** — pushing the tag is a separate manual step.

The script MUST support a `--dry-run` flag that does everything **except** the actual publish. CI SHOULD invoke `--dry-run` on PRs that touch the manifest to catch breakage early.

### 6.2 What the maintainer does before running the script

1. Bump version in the manifest.
2. Move `[Unreleased]` to `[X.Y.Z] - YYYY-MM-DD` in `CHANGELOG.md`.
3. If a major bump, add a section to `MIGRATION.md`.
4. Commit (`chore(release): X.Y.Z`).
5. From a clean `main`, run the release script.
6. Push the local tag (`git push origin vX.Y.Z`) when ready.

### 6.3 Recommended: manual-trigger CI workflow with provenance

For ecosystems that support signed artifact attestation (npm `--provenance`, PyPI Trusted Publisher, NuGet Repository Signature Validation), an SDK SHOULD add a `workflow_dispatch`-only release workflow that:

- Takes the version as an input and refuses to run if it does not match the manifest exactly (catches "forgot to bump").
- Refuses to run if the `vX.Y.Z` tag already exists.
- Runs the same gates as the local script (lint, typecheck, test, type tests, build, package validator).
- Publishes with the provenance/signing flag and the chosen dist-tag (`latest` vs prerelease).
- Creates and pushes the `vX.Y.Z` tag on success.

This provides signed Sigstore attestation without sacrificing manual control. Gate the workflow on a deployment **environment** with required reviewers so the maintainer still confirms publish from the GitHub UI. **Do not auto-trigger this workflow on tag push** — the manual gate is the point.

The Node SDK ships this as `.github/workflows/release.yml` (uses `npm publish --access public --provenance` with `id-token: write`; `NPM_TOKEN` lives as a repo secret until npm Trusted Publisher is configured). The local `scripts/publish.sh` is kept as the offline fallback for emergency releases.

### 6.4 No auto-publish from CI by default

There MUST NOT be a CI workflow that publishes on `git push --tags` or on `release: created` without manual intervention. A maintainer accidentally pushing a tag should not result in a published version.

### 6.5 Suggested tooling per ecosystem

| Ecosystem    | Publish command                 | Pack/inspect command         | Provenance/signing                            |
| ------------ | ------------------------------- | ---------------------------- | --------------------------------------------- |
| Node / TS    | `npm publish --access public`   | `npm pack`                   | `npm publish --provenance` (via OIDC)         |
| Python       | `twine upload dist/*`           | `python -m build`            | PyPI Trusted Publisher                        |
| PHP          | `composer` auto-publishes via Packagist webhook on tag | n/a            | n/a                                            |
| Go           | tag push triggers proxy fetch   | `go list -m`                 | `goreleaser` SLSA provenance                   |
| Ruby         | `gem push *.gem`                | `gem build *.gemspec`        | RubyGems MFA + signed gems (optional)         |
| Rust         | `cargo publish`                 | `cargo package`              | crates.io built-in checksums                   |
| Java/Kotlin  | `mvn deploy` / `gradle publish` | `mvn package`                | GPG-signed artifacts to OSSRH                  |
| .NET (C#)    | `dotnet nuget push`             | `dotnet pack`                | NuGet Repository Signature                     |

---

## 7. API-reference site

Every SDK SHOULD generate an API-reference site from doc-comments in the source and host it on GitHub Pages (or the language's idiomatic doc host).

### 7.1 Requirements

- Auto-generated on push to `main` and on release.
- Linked from the README.
- Carries the same `@example` blocks that ship in the editor IntelliSense.
- Versioned; users browsing the site MUST be able to tell which SDK version the page describes.

### 7.2 Doc-comment hygiene

Every public symbol (classes, methods, functions, types, error codes) MUST carry a doc-comment with:

- A one-sentence description.
- An `@example` block showing realistic usage (not `foo()` / `bar()` toys).
- For methods, parameter and return descriptions when not obvious.

### 7.3 Suggested tooling per ecosystem

| Ecosystem    | Generator                       | Hosting                          |
| ------------ | ------------------------------- | -------------------------------- |
| Node / TS    | TypeDoc                         | GitHub Pages (Actions deploy)    |
| Python       | Sphinx + autodoc / `mkdocs` + `mkdocstrings` | Read the Docs / GitHub Pages |
| PHP          | phpDocumentor                   | GitHub Pages                     |
| Go           | `go doc` → pkg.go.dev (automatic) | pkg.go.dev                     |
| Ruby         | YARD                            | GitHub Pages                     |
| Rust         | rustdoc                         | docs.rs (automatic)              |
| Java/Kotlin  | Javadoc / Dokka                 | GitHub Pages                     |
| .NET (C#)    | DocFX                           | GitHub Pages                     |

---

## 8. Pre-push hook

Every SDK SHOULD ship a pre-push git hook that runs lint, typecheck, unit tests, and integration tests before the push leaves the maintainer's machine. The hook MUST support an opt-out env var (`SKIP_INTEGRATION=1`) for doc-only changes that don't justify hitting the API.

### 8.1 Why pre-push, not pre-commit

Commit speed matters during a working session; push speed is fine to spend on validation. Catching a broken change before it lands in CI saves a feedback round-trip. Catching it before it lands on the remote saves a force-push.

### 8.2 Suggested tooling per ecosystem

| Ecosystem    | Hook installer                              |
| ------------ | ------------------------------------------- |
| Node / TS    | `simple-git-hooks` or `husky`               |
| Python       | `pre-commit` (despite the name, supports `pre-push`) |
| PHP          | `captainhook` or raw `.git/hooks/pre-push`  |
| Go           | raw `.git/hooks/pre-push` script + `Makefile` target |
| Ruby         | `overcommit`                                 |
| Rust         | `cargo-husky`                                |
| Java/Kotlin  | `pre-commit` or `gradle-git-hooks`           |
| .NET (C#)    | `Husky.Net`                                  |

---

## 9. Examples and quickstart (cross-reference)

The dedicated `sdk-demo-specification.md` has been retired. The runnable-example story is now split into two pieces, both defined in `implementation-plan-master-mickael.md`:

- **`examples/` recipes** — per-framework or per-runtime usage examples (Express, Fastify, Koa, plain Node, AWS Lambda for Node; net/http, Gin, Echo, Fiber, Chi for Go; Flask, Celery, Jupyter for Python; WordPress, WooCommerce, Slim for PHP). Each recipe is a self-contained runnable program with its own README. Per `sdk-roadmap.md`, recipes live both in the SDK repo's `examples/` folder AND on docs.poli.page (the doc syncs from the repo).
- **`examples/quickstart/`** — one canonical 30-second quickstart in every SDK and integration repo. It renders the shared `welcome` template against the API using a sandbox key, and is the manual smoke test before every release. Spec is in `implementation-plan-templates-and-quickstarts.md` (when produced).

Connections to the engineering practices in this guide:

- The quickstart MUST exercise the SDK's main surface in one short program. If the quickstart needs more than ~30 lines or non-trivial setup, the SDK surface needs work.
- The quickstart's **API-key UX** (env → `.env` → prompt-and-persist) is the same shape every SDK uses. Reuse the resolution order from the Node SDK when porting.
- The quickstart is the **manual smoke** referenced by §4.10 (gated integration tests) and §6.1 (manual release script).
- Recipes under `examples/` MUST install cleanly against the latest published SDK (or `file:..` during development) — they are exercised in CI when feasible.

---

## 10. Open questions per ecosystem

These are items where the right answer for an ecosystem hasn't been settled yet. Document the decision in the SDK's `CONTRIBUTING.md` once made; update this guide if the answer is reusable across ecosystems.

- **Python**: ship a single sync+async client (the `httpx`/`requests` pattern) or two clients (`PoliPage` and `AsyncPoliPage`)? The Python ecosystem is genuinely split; pick one and document why.
- **PHP**: lean on PSR-18 (HTTP client interface) so users plug in Guzzle or Symfony HTTP, or bundle Guzzle and accept the dependency? PSR-18 is more idiomatic but adds a setup step for users who don't have a PSR-18 client.
- **Go**: how to expose retry policy hooks (the §3.3 callbacks) idiomatically? Options interface, functional options pattern, or middleware? The Go ecosystem prefers minimal surface — discuss before exposing four callbacks.
- **Rust**: sync + async (`reqwest::blocking` + `reqwest`) or async-only? Async-only is the modern default but blocks adoption from sync-only codebases.
- **Java/Kotlin**: blocking `HttpClient` or async `WebClient` / `OkHttp`? Same trade-off as Rust.
- **.NET**: target `net8.0` only or also `netstandard2.0` for legacy users? The latter doubles compatibility and roughly doubles surface to validate.

When in doubt, the rule from `sdk-specification.md` §10 applies: open a discussion in `poli-page/sdk-node` or with Xavier directly. Public-API decisions ripple across the fleet; don't make them solo.

---

## Reference implementation

The Node.js SDK is the engineering reference. Useful files to study:

- `package.json` — manifest shape, exports map, scripts, size-limit budget, engines, `prepublishOnly`.
- `tsup.config.ts` — dual ESM+CJS build, sourcemap, `target` pinned to minimum Node.
- `.github/workflows/ci.yml` — full CI gate sequence including attw, publint, install smoke, size-limit, type tests, `npm audit signatures`. Ubuntu × Node 20/22/24 plus a Windows × Node 22 job.
- `.github/workflows/codeql.yml` — JavaScript/TypeScript CodeQL on push, PR, and weekly schedule.
- `.github/workflows/docs.yml` — TypeDoc auto-deploy to GitHub Pages.
- `.github/workflows/release.yml` — `workflow_dispatch` release with `--provenance` via OIDC. Validates input version vs manifest, runs the gates, publishes, then tags.
- `.github/dependabot.yml` — weekly npm + github-actions updates, grouped dev-deps.
- `scripts/publish.sh` — manual release script with pre-flight, pack-inspect, confirm, publish, tag (offline fallback to the GH Actions release workflow).
- `tests/isomorphism.test.ts` — regression test asserting the main entry has no `node:*` imports.
- `CONTRIBUTING.md` — local dev, integration tests, release flow, prerelease channel policy.

When this guide is silent, copy what the Node SDK does. When the Node SDK is wrong (it sometimes will be), open a PR to fix both.

---

## Changelog

- **v1.2 — 2026-05-19** — Hardened the publish-readiness baseline based on first-publish audit of the Node SDK. §2.1 grew a `.github/` directory table (issue templates, PR template, `CODEOWNERS`, `dependabot.yml`). §4.1 grew an OS-matrix note (Linux full × Node matrix + single Windows LTS job; `defaults.run.shell: bash`). New gates §4.11 *Supply-chain audit* (registry-signature verification + CodeQL on `push`/`pull_request`/weekly) and §4.12 *Dependency automation* (weekly Dependabot, grouped dev-deps). §4.13 tooling table extended with Signature audit / Static analysis / Dep automation columns across all eight ecosystems. §6.3 promoted from MAY → SHOULD and rewritten to spell out the workflow contract (input vs manifest check, tag-doesn't-exist check, environment-gated publish, tag-on-success). Reference-implementation list updated to call out the new files (`release.yml`, `codeql.yml`, `dependabot.yml`).
- **v1.1 — 2026-05-12** — Relocated from `poli-page/docs/onboarding/micka/` into the `sdk-node` repo root, alongside the contract and roadmap. Companion-docs section repointed: `sdk-specification.md` bumped to v1.3 reference, `sdk-roadmap.md` to v1.2; `sdk-demo-specification.md` discarded (replaced by the `examples/` recipes + `examples/quickstart/` story owned by `implementation-plan-master-mickael.md`); `agent-guide.md` no longer referenced as a peer companion — its TDD/Conventional-Commits/working-method content is captured by `CONTRIBUTING.md` in each repo. §9 rewritten to describe the new examples-and-quickstart split.
- **v1.0 — Initial** — SHOULD/MAY engineering practices for every official Poli Page SDK (CI gates, release flow, repo hygiene, prerelease channel, pre-push hook, demo cross-reference, open questions per ecosystem).
