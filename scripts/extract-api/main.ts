// Rust API extractor: invokes `cargo +nightly rustdoc --output-format json`
// against the crate, then transforms the JSON into the §4b reference-page MDX
// shape per the SDK docs convention.
//
// Nightly is pinned here so the extractor is deterministic regardless of what
// the local user has installed. The CI workflow installs the same toolchain.
//
// If rustdoc JSON's `format_version` ever drifts, bump NIGHTLY_TOOLCHAIN to a
// newer pinned date and re-validate the transform output.

import { execSync } from 'node:child_process';
import { readFileSync, writeFileSync, mkdirSync, rmSync, existsSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { buildClientPage } from './rustdoc-to-mdx.js';
import { buildMethodPages } from './method-pages.js';
import { buildTypesPage } from './types-page.js';
import { buildErrorsPage } from './errors-page.js';
import { buildRuntimeSupportPage } from './runtime-support-page.js';
import { buildMetaSidecar } from './meta-sidecar.js';
import type { RustdocCrate } from './rustdoc-types.js';

export const NIGHTLY_TOOLCHAIN = 'nightly-2026-05-15';
export const CRATE_NAME = 'poli-page';
export const CRATE_JSON_NAME = 'poli_page'; // rustdoc replaces '-' with '_'

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = resolve(HERE, '..', '..');
const REFERENCE_OUT = resolve(REPO_ROOT, 'docs', 'src', 'content', 'docs', 'reference');
const TARGET_DOC_JSON = resolve(REPO_ROOT, 'target', 'doc', `${CRATE_JSON_NAME}.json`);

interface CargoToml {
  readonly version: string;
  readonly rust_version: string;
}

function readCrateMetadata(): CargoToml {
  // Minimal TOML extraction — we only need `version` and `rust-version` from
  // [package]. Avoids pulling a TOML parser as a dependency.
  const cargo = readFileSync(resolve(REPO_ROOT, 'Cargo.toml'), 'utf8');
  const match = (key: string): string => {
    const m = cargo.match(new RegExp(`^${key}\\s*=\\s*"([^"]+)"`, 'm'));
    if (!m) throw new Error(`Cargo.toml: missing ${key}`);
    return m[1]!;
  };
  return { version: match('version'), rust_version: match('rust-version') };
}

function runRustdoc(): void {
  console.log(`extractor: running cargo +${NIGHTLY_TOOLCHAIN} rustdoc (json)`);
  execSync(
    `cargo +${NIGHTLY_TOOLCHAIN} rustdoc --lib -- -Z unstable-options --output-format json`,
    { cwd: REPO_ROOT, stdio: 'inherit' },
  );
  if (!existsSync(TARGET_DOC_JSON)) {
    throw new Error(`extractor: rustdoc JSON not found at ${TARGET_DOC_JSON}`);
  }
}

function run(): void {
  const pkg = readCrateMetadata();

  // 1. Clear previous output.
  if (existsSync(REFERENCE_OUT)) rmSync(REFERENCE_OUT, { recursive: true, force: true });
  mkdirSync(REFERENCE_OUT, { recursive: true });
  mkdirSync(join(REFERENCE_OUT, 'methods'), { recursive: true });

  // 2. Run rustdoc.
  runRustdoc();
  const td = JSON.parse(readFileSync(TARGET_DOC_JSON, 'utf8')) as RustdocCrate;

  // 3. Build each page.
  writeFileSync(join(REFERENCE_OUT, 'client.mdx'), buildClientPage(td), 'utf8');
  for (const m of buildMethodPages(td, REPO_ROOT)) {
    writeFileSync(join(REFERENCE_OUT, 'methods', `${m.slug}.mdx`), m.mdx, 'utf8');
  }
  writeFileSync(join(REFERENCE_OUT, 'types.mdx'), buildTypesPage(td), 'utf8');
  writeFileSync(join(REFERENCE_OUT, 'errors.mdx'), buildErrorsPage(), 'utf8');
  writeFileSync(
    join(REFERENCE_OUT, 'runtime-support.mdx'),
    buildRuntimeSupportPage(pkg.version, pkg.rust_version),
    'utf8',
  );
  writeFileSync(
    join(REFERENCE_OUT, '_meta.json'),
    JSON.stringify(buildMetaSidecar(pkg.version), null, 2) + '\n',
    'utf8',
  );

  console.log(`extractor: wrote ${REFERENCE_OUT}`);
}

run();
