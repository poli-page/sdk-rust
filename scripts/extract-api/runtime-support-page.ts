// Build the reference/runtime-support.mdx page — the Rust SDK's MSRV and OS
// matrix. Driven by Cargo.toml's `rust-version` field.

export function buildRuntimeSupportPage(crateVersion: string, msrv: string): string {
  return `---
title: Runtime support
description: Supported Rust toolchains and operating systems for poli-page v${crateVersion}.
---

import RuntimeMatrix from '@preset/components/RuntimeMatrix.astro';

The Rust SDK is built and tested against the matrix below.

<RuntimeMatrix matrix={{
  runtimes: ['${msrv} (MSRV)', 'stable', 'beta'],
  os: ['linux', 'macos', 'windows'],
  cells: {
    '${msrv} (MSRV)': { linux: 'tested', macos: 'tested', windows: 'tested' },
    'stable': { linux: 'tested', macos: 'tested', windows: 'tested' },
    'beta': { linux: 'supported', macos: 'supported', windows: 'supported' },
  },
}} />

The minimum supported Rust version is **${msrv}**. The crate uses standard-library APIs available on stable Rust.

## Async runtime

The async API depends on \`tokio\`. See [Async runtime](../../concepts/async-runtime/) for setup and the synchronous \`blocking\` feature.

## Cargo features

- \`default = ["rustls-tls"]\` — uses \`rustls\` for TLS; no system libraries needed.
- \`native-tls\` — uses the platform's native TLS stack. Mutually exclusive with \`rustls-tls\`.
- \`blocking\` — exposes \`poli_page::blocking::PoliPage\`, a synchronous wrapper.
`;
}
