// Build the reference/types.mdx page from the rustdoc JSON. v0.1 is a flat
// list — one heading per exported struct/enum/type alias with its doc summary.

import { docSummary, type Item, type RustdocCrate } from './rustdoc-types.js';

const PUBLIC_TYPES: readonly string[] = [
  'PoliPage',
  'PoliPageBuilder',
  'Render',
  'Documents',
  'PdfByteStream',
  'RenderInput',
  'ProjectModeInput',
  'InlineModeInput',
  'PreviewResult',
  'DocumentDescriptor',
  'DocumentPreviewResult',
  'Thumbnail',
  'ThumbnailOptions',
  'ThumbnailFormat',
  'PageFormat',
  'Orientation',
  'Environment',
  'MetadataValue',
  'RenderMetadata',
  'RetryEvent',
  'Error',
];

export function buildTypesPage(td: RustdocCrate): string {
  const blocks: string[] = [];
  for (const name of PUBLIC_TYPES) {
    const item = findPublicType(td, name);
    if (!item) continue;
    const summary = docSummary(item.docs) || `(see source)`;
    blocks.push(`### \`${name}\`\n\n${summary}\n`);
  }

  return `---
title: Types
description: Public types and traits exported from the poli-page crate.
---

The Rust SDK exposes the public types below. Import them from the crate root:

\`\`\`rust
use poli_page::{${PUBLIC_TYPES.slice(0, 5).join(', ')}};
\`\`\`

${blocks.join('\n')}
`;
}

function findPublicType(td: RustdocCrate, name: string): Item | undefined {
  // Prefer items in our crate that have a struct/enum/type_alias inner.
  for (const item of Object.values(td.index)) {
    if (item.crate_id !== 0) continue;
    if (item.name !== name) continue;
    const inner = item.inner;
    if (inner.struct || inner.enum || inner.type_alias) return item;
  }
  return undefined;
}
