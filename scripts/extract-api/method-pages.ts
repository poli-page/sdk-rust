// Build per-method reference pages from the rustdoc JSON output.
//
// Method targets are hard-coded — there are only nine public methods on the
// Rust surface, and we want to control the canonical slug + example file
// mapping. This is the same approach sdk-node's extractor takes.

import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import {
  docSummary,
  findMethodOn,
  findTopLevelFunction,
  firstSentence,
  renderType,
  type Item,
  type RustdocCrate,
  type TypeNode,
} from './rustdoc-types.js';

interface MethodTarget {
  readonly slug: string;
  readonly displayName: string;
  readonly exampleFile: string;
  readonly errorCodes: readonly string[];
  readonly locate: (td: RustdocCrate) => Item | undefined;
}

const METHODS: readonly MethodTarget[] = [
  {
    slug: 'render-pdf',
    displayName: 'client.render.pdf',
    exampleFile: 'render-pdf.rs',
    errorCodes: ['VALIDATION_ERROR', 'NOT_FOUND', 'QUOTA_EXCEEDED', 'timeout', 'network_error', 'INTERNAL_ERROR'],
    locate: (td) => findMethodOn(td, 'Render', 'pdf'),
  },
  {
    slug: 'render-pdf-stream',
    displayName: 'client.render.pdf_stream',
    exampleFile: 'render-pdf-stream.rs',
    errorCodes: ['VALIDATION_ERROR', 'NOT_FOUND', 'QUOTA_EXCEEDED', 'timeout', 'network_error', 'INTERNAL_ERROR'],
    locate: (td) => findMethodOn(td, 'Render', 'pdf_stream'),
  },
  {
    slug: 'render-preview',
    displayName: 'client.render.preview',
    exampleFile: 'render-preview.rs',
    errorCodes: ['VALIDATION_ERROR', 'NOT_FOUND', 'QUOTA_EXCEEDED', 'INTERNAL_ERROR'],
    locate: (td) => findMethodOn(td, 'Render', 'preview'),
  },
  {
    slug: 'render-document',
    displayName: 'client.render.document',
    exampleFile: 'render-document.rs',
    errorCodes: ['VALIDATION_ERROR', 'NOT_FOUND', 'QUOTA_EXCEEDED', 'INTERNAL_ERROR'],
    locate: (td) => findMethodOn(td, 'Render', 'document'),
  },
  {
    slug: 'documents-get',
    displayName: 'client.documents.get',
    exampleFile: 'documents-get.rs',
    errorCodes: ['DOCUMENT_NOT_FOUND', 'INVALID_API_KEY', 'INTERNAL_ERROR'],
    locate: (td) => findMethodOn(td, 'Documents', 'get'),
  },
  {
    slug: 'documents-preview',
    displayName: 'client.documents.preview',
    exampleFile: 'documents-preview.rs',
    errorCodes: ['DOCUMENT_NOT_FOUND', 'INVALID_API_KEY', 'INTERNAL_ERROR'],
    locate: (td) => findMethodOn(td, 'Documents', 'preview'),
  },
  {
    slug: 'documents-thumbnails',
    displayName: 'client.documents.thumbnails',
    exampleFile: 'documents-thumbnails.rs',
    errorCodes: ['DOCUMENT_NOT_FOUND', 'VALIDATION_ERROR', 'INVALID_API_KEY', 'INTERNAL_ERROR'],
    locate: (td) => findMethodOn(td, 'Documents', 'thumbnails'),
  },
  {
    slug: 'documents-delete',
    displayName: 'client.documents.delete',
    exampleFile: 'documents-delete.rs',
    errorCodes: ['DOCUMENT_NOT_FOUND', 'INVALID_API_KEY', 'INTERNAL_ERROR'],
    locate: (td) => findMethodOn(td, 'Documents', 'delete'),
  },
  {
    slug: 'render-to-file',
    displayName: 'render_to_file',
    exampleFile: 'render-to-file.rs',
    errorCodes: ['VALIDATION_ERROR', 'NOT_FOUND', 'QUOTA_EXCEEDED', 'timeout', 'network_error', 'INTERNAL_ERROR'],
    locate: (td) => findTopLevelFunction(td, 'render_to_file'),
  },
];

export function buildMethodPages(
  td: RustdocCrate,
  repoRoot: string,
): Array<{ slug: string; mdx: string }> {
  const pages: Array<{ slug: string; mdx: string }> = [];
  for (const target of METHODS) {
    const item = target.locate(td);
    if (!item) {
      throw new Error(`extractor: could not find ${target.displayName} in rustdoc JSON`);
    }
    const examplePath = join(repoRoot, 'examples', target.exampleFile);
    const example = readFileSync(examplePath, 'utf8');
    pages.push({ slug: target.slug, mdx: renderMethodPage(target, item, example) });
  }
  return pages;
}

function renderMethodPage(target: MethodTarget, item: Item, example: string): string {
  const fn = item.inner.function!;
  const summary = docSummary(item.docs);
  const lede = summary || `${target.displayName} method.`;
  const descLine = escapeFrontmatter(firstSentence(summary) || `${target.displayName} method.`);
  const signature = renderSignature(target.displayName, fn);

  // Parameters table: every input that isn't `self`.
  const params = fn.sig.inputs
    .filter(([name]) => name !== 'self')
    .map(([name, type]) => ({
      name,
      type: renderType(type),
      required: true,
      description: '(see method docs)',
    }));
  const parametersBlock = params.length === 0
    ? ''
    : `\n## Parameters\n\n<ParamsTable params={${JSON.stringify(params)}} />\n`;

  const returnType = renderType(fn.sig.output);
  const returnsBlock = returnType === '()'
    ? ''
    : `\n## Returns\n\n\`${returnType}\`\n`;

  const errorsBlock = target.errorCodes.length === 0
    ? ''
    : `\n## Errors\n\n<ErrorTable errors={${JSON.stringify(
        target.errorCodes.map((code) => ({
          code,
          when: 'See [errors](../../../production/errors/) for the full description.',
        })),
      )}} />\n`;

  return `---
title: ${target.displayName}
description: ${descLine}
sidebar:
  label: ${target.displayName}
---

import MethodSignature from '@preset/components/MethodSignature.astro';
import ParamsTable from '@preset/components/ParamsTable.astro';
import ErrorTable from '@preset/components/ErrorTable.astro';

<MethodSignature lang="rust" code={\`${signature}\`} />

${lede}
${parametersBlock}${returnsBlock}${errorsBlock}
## Example

\`\`\`rust
${example.trimEnd()}
\`\`\`

## See also
- [Errors](../../../production/errors/)
- [Configuration](../../../concepts/configuration/)
`;
}

function renderSignature(
  displayName: string,
  fn: NonNullable<Item['inner']['function']>,
): string {
  const inputs: string[] = [];
  for (const [name, type] of fn.sig.inputs) {
    if (name === 'self') {
      inputs.push(renderSelf(type));
    } else {
      inputs.push(`${name}: ${renderType(type)}`);
    }
  }
  const ret = renderType(fn.sig.output);
  const asyncKw = fn.header.is_async ? 'async ' : '';
  return `${asyncKw}fn ${displayName}(${inputs.join(', ')}) -> ${ret}`;
}

function renderSelf(type: TypeNode): string {
  // `self` shows up in rustdoc as a borrowed_ref over `Self` (or owned).
  if ('borrowed_ref' in type) {
    const r = (type as { borrowed_ref: { is_mutable: boolean } }).borrowed_ref;
    return r.is_mutable ? '&mut self' : '&self';
  }
  return 'self';
}

function escapeFrontmatter(s: string): string {
  return s.replace(/"/g, '\\"').replace(/\n/g, ' ').slice(0, 150);
}
