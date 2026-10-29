// Build per-method reference pages from the rustdoc JSON output.
//
// Method targets are hard-coded — there are only eight public methods on the
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

// Error tables: what each method can return, from `src/` (client.rs,
// render.rs, documents.rs, fs.rs) and the API spec's routes. Every code must
// be defined in `src/error.rs`; `buildMethodPages` checks it.

// Every HTTP call: `Error::Timeout` / `Error::Connection` (client.rs).
const TRANSPORT = ['timeout', 'network_error'];
// Every `/v1/*` route: a non-`pp_` bearer goes the session path and gets
// MISSING_API_KEY; a purged org refuses all; a BYO org whose role cannot be
// assumed gets STORAGE_ACCESS_DENIED; INTERNAL_ERROR is a 5xx, a non-JSON
// error body or a response the SDK cannot parse.
const ANY_V1_CALL = ['MISSING_API_KEY', 'INVALID_API_KEY', 'ORGANIZATION_PURGED', 'STORAGE_ACCESS_DENIED', 'INTERNAL_ERROR'];
// Org-state gates on writes (any method but GET).
const WRITE_GATES = ['ORGANIZATION_PAYMENT_GRACE', 'ORGANIZATION_CANCELLED', 'ORGANIZATION_MIGRATING'];
// `POST /v1/render`. No ENVIRONMENT_MISMATCH: `RenderInput` sends no
// `environment`. RATE_LIMIT_EXCEEDED throttles `/v1/render*` only.
const RENDER = [
  'VALIDATION_ERROR', 'MISSING_DATA', 'MISSING_TEMPLATE_SLUG', 'PROJECT_REQUIRED_FOR_DOCUMENT',
  'TEMPLATE_ERROR', 'TEMPLATE_UNSAFE', 'EMPTY_DOCUMENT', 'PROJECT_NOT_FOUND', 'PROJECT_NOT_IN_ENVIRONMENT', 'TEMPLATE_NOT_FOUND',
  'RENDER_BODY_TOO_LARGE', 'DOCUMENT_TOO_LARGE', 'QUOTA_EXCEEDED', 'OVERAGE_CAP_EXCEEDED',
  'RATE_LIMIT_EXCEEDED', 'PUBLISH_IN_PROGRESS', 'RENDER_TIMEOUT',
  ...WRITE_GATES, ...ANY_V1_CALL, ...TRANSPORT,
];
// The presigned-URL fetch after the render (`Error::Download`).
const RENDER_THEN_DOWNLOAD = [...RENDER, 'DOWNLOAD_FAILED'];
// `GET /v1/documents/:id` and `/preview`: reads, so no write gate.
const DOCUMENT_READ = ['DOCUMENT_NOT_FOUND', 'FORBIDDEN', 'DOCUMENT_GONE', ...ANY_V1_CALL, ...TRANSPORT];

const METHODS: readonly MethodTarget[] = [
  {
    slug: 'render-pdf',
    displayName: 'client.render.pdf',
    exampleFile: 'render-pdf.rs',
    errorCodes: RENDER_THEN_DOWNLOAD,
    locate: (td) => findMethodOn(td, 'Render', 'pdf'),
  },
  {
    slug: 'render-pdf-stream',
    displayName: 'client.render.pdf_stream',
    exampleFile: 'render-pdf-stream.rs',
    errorCodes: RENDER_THEN_DOWNLOAD,
    locate: (td) => findMethodOn(td, 'Render', 'pdf_stream'),
  },
  {
    slug: 'render-document',
    displayName: 'client.render.document',
    exampleFile: 'render-document.rs',
    errorCodes: RENDER,
    locate: (td) => findMethodOn(td, 'Render', 'document'),
  },
  {
    slug: 'documents-get',
    displayName: 'client.documents.get',
    exampleFile: 'documents-get.rs',
    errorCodes: DOCUMENT_READ,
    locate: (td) => findMethodOn(td, 'Documents', 'get'),
  },
  {
    slug: 'documents-preview',
    displayName: 'client.documents.preview',
    exampleFile: 'documents-preview.rs',
    errorCodes: DOCUMENT_READ,
    locate: (td) => findMethodOn(td, 'Documents', 'preview'),
  },
  {
    slug: 'documents-thumbnails',
    displayName: 'client.documents.thumbnails',
    exampleFile: 'documents-thumbnails.rs',
    errorCodes: [
      'DOCUMENT_NOT_FOUND', 'FORBIDDEN', 'DOCUMENT_GONE', 'THUMBNAILS_NOT_AVAILABLE', 'VALIDATION_ERROR',
      'QUOTA_EXCEEDED', 'OVERAGE_CAP_EXCEEDED', ...WRITE_GATES, ...ANY_V1_CALL, ...TRANSPORT,
    ],
    locate: (td) => findMethodOn(td, 'Documents', 'thumbnails'),
  },
  {
    slug: 'documents-delete',
    displayName: 'client.documents.delete',
    exampleFile: 'documents-delete.rs',
    // No DOCUMENT_GONE: deleting a deleted document answers 204.
    errorCodes: [
      'DOCUMENT_NOT_FOUND', 'FORBIDDEN', 'ENVIRONMENT_PROTECTED', ...WRITE_GATES, ...ANY_V1_CALL, ...TRANSPORT,
    ],
    locate: (td) => findMethodOn(td, 'Documents', 'delete'),
  },
  {
    slug: 'render-to-file',
    displayName: 'render_to_file',
    exampleFile: 'render-to-file.rs',
    // invalid_options: creating, writing or flushing the file failed (fs.rs).
    errorCodes: [...RENDER_THEN_DOWNLOAD, 'invalid_options'],
    locate: (td) => findTopLevelFunction(td, 'render_to_file'),
  },
];

// Codes `src/error.rs` defines: the `error_codes` constants and the fixed
// codes `Error::code` returns for the SDK-side variants.
function definedErrorCodes(repoRoot: string): Set<string> {
  const src = readFileSync(join(repoRoot, 'src', 'error.rs'), 'utf8');
  const constants = [...src.matchAll(/pub const [A-Z_]+: &str = "([^"]+)";/g)].map((m) => m[1]!);
  const reserved = [...src.matchAll(/Error::\w+(?: \{ \.\. \})? => "([^"]+)",/g)].map((m) => m[1]!);
  if (constants.length === 0 || reserved.length === 0) {
    throw new Error('extractor: could not read the error codes from src/error.rs');
  }
  return new Set([...constants, ...reserved]);
}

export function buildMethodPages(
  td: RustdocCrate,
  repoRoot: string,
): Array<{ slug: string; mdx: string }> {
  const defined = definedErrorCodes(repoRoot);
  const pages: Array<{ slug: string; mdx: string }> = [];
  for (const target of METHODS) {
    const unknown = target.errorCodes.filter((code) => !defined.has(code));
    if (unknown.length > 0) {
      throw new Error(`extractor: ${target.displayName} lists codes not defined in src/error.rs: ${unknown.join(', ')}`);
    }
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
