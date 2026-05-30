// Builds the `reference/client.mdx` page from the rustdoc JSON of the
// `PoliPage` struct.

import { docSummary, findStruct, type RustdocCrate } from './rustdoc-types.js';

export function buildClientPage(td: RustdocCrate): string {
  const poliPage = findStruct(td, 'PoliPage');
  const summary = docSummary(poliPage?.docs ?? null);
  const lede = summary || 'The Poli Page Rust SDK client. Cheap to clone — internally an Arc<ClientInner>.';

  // Surface the simplest signature: `PoliPage::new`. The builder is documented
  // on its own page; here we want the “you can construct one” line.
  const signature = 'PoliPage::new(api_key: impl Into<String>) -> Result<PoliPage, Error>';

  return `---
title: Client
description: The PoliPage struct — the only entry point to the Rust SDK.
---

import MethodSignature from '@preset/components/MethodSignature.astro';

<MethodSignature lang="rust" code={\`${signature}\`} />

${lede}

## Constructor

The constructor accepts only the API key. For non-default options (timeout, retries, base URL, hooks) use [\`PoliPage::builder\`](./methods/render-pdf/). The full builder surface (every \`with_*\`-style setter) is the \`PoliPageBuilder\` type — see [Types](../types/).

## Namespaces

The client exposes two namespaces:

- [\`render\`](./methods/render-pdf/) — render PDFs (in memory, streaming, to file, or as a stored document).
- [\`documents\`](./methods/documents-get/) — fetch, preview, thumbnail, or delete stored documents.

The free-standing helper [\`render_to_file\`](./methods/render-to-file/) ships at crate root.

## Blocking variant

Behind the \`blocking\` Cargo feature, \`poli_page::blocking::PoliPage\` mirrors the async surface with synchronous methods. See [Async runtime](../../concepts/async-runtime/).

## See also
- [Types](../types/)
- [Errors](../errors/)
- [Runtime support](../runtime-support/)
`;
}
