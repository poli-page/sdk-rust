// Build the reference/errors.mdx page — canonical error tables, grouped by
// source. v0.1 hard-codes the codes (they're a small fixed list and we want
// the convention shared across SDKs).

export function buildErrorsPage(): string {
  return `---
title: Errors
description: All error codes raised by Error, grouped by source.
---

import ErrorTable from '@preset/components/ErrorTable.astro';

Every fallible operation in the crate returns \`Result<T, poli_page::Error>\`. SDK-internal codes are lowercase; codes from the API are uppercase.

## SDK-internal

<ErrorTable errors={[{"code":"invalid_options","when":"Builder validation failed (empty api_key, malformed base_url, etc.). Returned synchronously from build()."},{"code":"network_error","when":"TCP/TLS-level failure reaching the API. Retryable."},{"code":"timeout","when":"Per-attempt timeout exceeded. Retryable."},{"code":"aborted","when":"Future was dropped or externally cancelled. Not retryable."},{"code":"DOWNLOAD_FAILED","when":"Presigned-URL second-hop fetch failed. Not retried by the SDK."},{"code":"INTERNAL_ERROR","when":"Response body failed to parse, or another internal invariant tripped."}]} />

## Authentication

<ErrorTable errors={[{"code":"MISSING_API_KEY","when":"No API key in the request."},{"code":"INVALID_API_KEY","when":"The API key is malformed or revoked."}]} />

## Billing and lifecycle

<ErrorTable errors={[{"code":"PAYMENT_REQUIRED","when":"Organization billing is past due."},{"code":"FORBIDDEN","when":"The key does not have access to the requested resource."},{"code":"ORGANIZATION_CANCELLED","when":"The organization has been cancelled."},{"code":"ORGANIZATION_PURGED","when":"The organization has been purged."}]} />

## Not found

<ErrorTable errors={[{"code":"NOT_FOUND","when":"The project/template slug does not exist or is not published."},{"code":"VERSION_NOT_FOUND","when":"The pinned version does not exist for this template."},{"code":"DOCUMENT_NOT_FOUND","when":"No stored document matches the supplied id."},{"code":"GONE","when":"The resource existed but has been deleted."}]} />

## Validation

<ErrorTable errors={[{"code":"VALIDATION_ERROR","when":"\`data\` does not satisfy the template schema."},{"code":"MISSING_DATA","when":"Request body lacks the required \`data\` field."},{"code":"MISSING_PROJECT_OR_TEMPLATE","when":"Project mode call without both project and template."},{"code":"MISSING_TEMPLATE_SLUG","when":"Template slug is missing."},{"code":"PROJECT_REQUIRED_FOR_DOCUMENT","when":"documents.preview requires a project selector."},{"code":"INVALID_VERSION_FORMAT","when":"The version string is not a valid semver."},{"code":"VERSION_REQUIRED","when":"Project mode requires an explicit version selector."},{"code":"INVALID_VERSION_FOR_KEY_ENV","when":"Sandbox key targeting a live-only version, or vice versa."}]} />

## Rate and quota

<ErrorTable errors={[{"code":"QUOTA_EXCEEDED","when":"Per-key rate limit or monthly quota reached. Retryable."},{"code":"OVERAGE_CAP_EXCEEDED","when":"Hard overage cap reached. Not retryable."}]} />

## Server

<ErrorTable errors={[{"code":"INTERNAL_ERROR","when":"The API returned 5xx. Retryable."}]} />
`;
}
