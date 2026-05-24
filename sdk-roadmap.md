# SDK Roadmap

**Version**: 1.2
**Audience**: Mickael, building the Poli Page SDK ecosystem.
**Purpose**: explain *what* to build, *in which order*, and *why* — so you
can drive each phase autonomously without round-tripping with Xavier on
prioritization.

This document is a companion to:
- `project-briefing.md` — what Poli Page is, how to test, working conventions
- `sdk-specification.md` v1.3 — the contract every SDK must implement

---

## Why a multi-repo structure

A single SDK is not enough to cover a language's market. A modern PHP
project is not "PHP" — it is Laravel, Symfony, WordPress, or Magento,
each with its own conventions and adoption barriers. Same story in
Python (Django vs FastAPI vs Flask), Node (Express vs NestJS vs
Next.js), and so on.

Two principles drive the split:

1. **Native ergonomics matter more than feature parity.** A Laravel
   developer adopts five times faster from
   `composer require poli-page/laravel` (with auto-discovery, a Facade,
   and a config file) than from a code snippet in a README. Same for
   `pip install poli-page-django` vs `pip install poli-page` + manual
   wiring. Framework users compare us to other framework-native
   packages, not to "generic SDKs they can plug in".

2. **Cadences are independent.** Laravel 12, Django 5, Next.js 16 all
   ship on their own schedule. If everything lived in one repo, every
   framework upgrade would block all others. Splitting per integration
   lets each track its host framework without coupling.

CMS plugins (WordPress, WooCommerce, Magento) are deferred. Their
ecosystem expects polished UX (admin pages, settings UI, marketplace
listing) that is closer to a product than to an SDK. We start with
recipes and revisit when traction signals demand.

---

## The 10-repo target

| Repo | Type | Registry | Package name |
|---|---|---|---|
| `poli-page/sdk-node` ✅ | Core | npm | `@poli-page/sdk` |
| `poli-page/nestjs` | Integration | npm | `@poli-page/nestjs` |
| `poli-page/nextjs` | Integration | npm | `@poli-page/nextjs` |
| `poli-page/sdk-go` | Core | Go modules | `github.com/poli-page/sdk-go` |
| `poli-page/sdk-python` | Core | PyPI | `poli-page` |
| `poli-page/fastapi` | Integration | PyPI | `poli-page-fastapi` |
| `poli-page/django` | Integration | PyPI | `poli-page-django` |
| `poli-page/sdk-php` | Core | Packagist | `poli-page/sdk` |
| `poli-page/laravel` | Integration | Packagist | `poli-page/laravel` |
| `poli-page/symfony-bundle` | Integration | Packagist | `poli-page/symfony-bundle` |

Go has only one repo by design — the Go ecosystem does not use
framework-integration packages; idiomatic libraries expose
`http.Handler`-shaped APIs that Gin, Echo, Fiber, Chi, and stdlib all
consume identically. Recipes inside `sdk-go/examples/` cover the
popular routers.

---

## Roadmap — work one language at a time

The order below minimizes context switching and aligns with persona
priorities. Finish a phase fully before starting the next.

### Phase 0 — Node SDK polish (P0)

| Order | Deliverable | Why this slot |
|---|---|---|
| 0.1 | `sdk-node` updated to v1.3 spec | Reference implementation. Adds `render.document`, `documents.*` namespace (4 methods), `metadata` field (Stripe-style), simplified version resolution (exact-semver-or-`draft` only — `latest` and partial semver retired), `STORAGE_REQUIRED`, `GONE`, `PAYMENT_REQUIRED`, `ORGANIZATION_CANCELLED`, `ORGANIZATION_PURGED`, `QUOTA_EXCEEDED`, `OVERAGE_CAP_EXCEEDED`, `INVALID_VERSION_FORMAT`, `VERSION_REQUIRED`, `INVALID_VERSION_FOR_KEY_ENV` error codes. |
| 0.2 | Recipes inside `sdk-node/examples/`: Express, Fastify, Koa, plain Node, AWS Lambda | Validates the public API ergonomically before locking it in framework wrappers. |

**Estimated effort**: 3-5 days.

**Gate before moving to P1**: the `@poli-page/sdk` v1.1 public API
must be stable. Breaking changes after this phase will ripple into
the framework integrations and force coordinated bumps.

### Phase 1 — Node framework integrations (P1)

| Order | Deliverable | Why this slot |
|---|---|---|
| 1.1 | `poli-page/nestjs` | Enterprise / structured audience, well-defined Module-and-Provider pattern, port is mechanical from the stable Node SDK. |
| 1.2 | `poli-page/nextjs` | Largest modern Node volume — Vercel + SaaS audience overlaps strongly with Poli Page early adopters. App Router is now stable. |

**Estimated effort**: 3-5 days.

NestJS first because the integration is more mechanical (DI patterns
are well-established), allowing the Next.js integration to benefit
from any internal refactoring or lessons learned.

### Phase 2 — Go SDK (P2)

| Order | Deliverable | Why this slot |
|---|---|---|
| 2.1 | `poli-page/sdk-go` | **Required at MVP** for the Tomasz persona (vision §2.2). Idiomatic Go, no external runtime dependencies beyond stdlib if possible. |
| 2.2 | Recipes inside `sdk-go/examples/`: net/http (stdlib), Gin, Echo, Fiber, Chi | Go convention — no framework wrappers, just usage examples. Single package serves all routers via `http.Handler`-shaped APIs. |

**Estimated effort**: 3-5 days.

Go is positioned right after the Node ecosystem closes because (a) it
is required at MVP for Tomasz, (b) the Node ecosystem is most likely
to capture Camille and Marc, leaving Go as the natural next priority,
and (c) the spec maturity reached after P1 means the Go port is
straightforward.

### Phase 3 — Python (P3)

| Order | Deliverable | Why this slot |
|---|---|---|
| 3.1 | `poli-page/sdk-python` | Foundation. Sync + async client (Python ecosystem is split, both flavours ship in the same package). |
| 3.2 | Recipes inside `sdk-python/examples/`: Flask, plain script, Celery worker, Jupyter | Covers the data-scientist / scripting audience that does not use a framework. High value, low cost. |
| 3.3 | `poli-page/fastapi` | More vocal community, gives feedback fast, momentum in 2026. Validates the async path. |
| 3.4 | `poli-page/django` | Larger install base but a more conservative audience. |

**Estimated effort**: 5-7 days.

### Phase 4 — PHP (P4)

| Order | Deliverable | Why this slot |
|---|---|---|
| 4.1 | `poli-page/sdk-php` | Foundation. PSR-18 (HTTP client) + PSR-17 (factories) so users plug in Guzzle or Symfony HTTP. |
| 4.2 | Recipes inside `sdk-php/examples/`: WordPress (shortcode + REST hook), WooCommerce (invoice email), Magento, Slim, plain PHP-FPM | The non-Laravel PHP market is overwhelmingly WordPress + WooCommerce — high impact for the cost of a few snippets. |
| 4.3 | `poli-page/laravel` | ~70% of modern PHP. Adoption is fast once a quality package exists. |
| 4.4 | `poli-page/symfony-bundle` | Enterprise audience, slower adoption but high quality of integration is expected. |

**Estimated effort**: 5-7 days.

---

## Cross-cutting conventions

These apply to every repo, core or integration.

- **Public, MIT-licensed**, under the `poli-page` GitHub org.
- **Conventional Commits** (`feat:`, `fix:`, `docs:`, `chore:`,
  `refactor:`, `test:`).
- **TDD**: write a failing test first. CI is red-blocking.
- **CHANGELOG.md** in Keep a Changelog format. Update in the same
  commit as every version bump.
- **README.md** covers: install, 5-line quick start, dependency on
  core SDK if applicable, publishing target (registry + package
  name), link to https://docs.poli.page, contributing, license.
- **Integration manifest declares the core SDK as a dependency**
  (`composer.json`, `package.json`, `pyproject.toml`). Integrations
  never reimplement HTTP, retries, or error mapping.
- **Recipes live in two places**: `examples/` subfolder of each
  core SDK repo (executable, used as integration tests and canonical
  references) AND on https://docs.poli.page (with surrounding
  context and prose). The doc imports or syncs from the repo to keep
  the two aligned.

---

## Estimated effort (Claude-assisted, solo developer)

With Claude-assisted development, the effort estimates are
substantially lower than traditional solo timelines.

| Phase | Approximate duration |
|---|---|
| P0 (`sdk-node` polish + recipes) | 3-5 days |
| P1 (`nestjs` + `nextjs`) | 3-5 days |
| P2 (`sdk-go` + recipes) | 3-5 days |
| P3 (`sdk-python` + recipes + `fastapi` + `django`) | 5-7 days |
| P4 (`sdk-php` + recipes + `laravel` + `symfony-bundle`) | 5-7 days |
| **Total** | **~3-4 weeks** |

These are working estimates assuming the spec stays stable
throughout. Adjust as feedback comes in from each released phase.

---

## Communication and review

- Day-to-day questions and decisions: Xavier directly, in French.
- Anything that touches the public API contract (in
  `sdk-specification.md`): discuss with Xavier *before* implementing
  — a change there ripples through every existing SDK and integration.
- When stuck, prefer asking early over building something that may
  need to be undone.

---

## Changelog

- **v1.2 — 2026-05-11** — Refreshed P0 description to match
  `sdk-specification.md` v1.3:
  - Bump the spec reference from v1.1 to v1.3 (top of document)
  - P0 deliverable 0.1 now lists `documents.preview`, the
    simplified version resolution (exact-semver-or-`draft`), and
    the three new error codes
    (`INVALID_VERSION_FORMAT`, `VERSION_REQUIRED`,
    `INVALID_VERSION_FOR_KEY_ENV`) alongside the existing ones
- **v1.1 — 2026-05-01** — Roadmap repriorized after the user-journey
  redesign:
  - Phase 2 (Go SDK) moved up from final phase to position 2 (after
    Node + Node integrations) because Go is required at MVP for the
    Tomasz persona
  - Phase 1 internal order: NestJS before Next.js (mechanical port
    benefits Next.js)
  - Estimated effort recalibrated for Claude-assisted development:
    total ~3-4 weeks instead of ~6 months
  - Recipes convention clarified: present both in `examples/` and on
    docs.poli.page
  - References updated to spec v1.1 (new methods, namespaces, error
    codes)
- **v1.0 — Initial** — Original roadmap with Go in P3.
