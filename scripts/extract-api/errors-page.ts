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

## Access, billing and lifecycle

<ErrorTable errors={[{"code":"FORBIDDEN","when":"The key does not have access to the requested resource."},{"code":"ENVIRONMENT_PROTECTED","when":"The key's environment is protected against this operation."},{"code":"THUMBNAILS_NOT_AVAILABLE","when":"The organization's plan does not include thumbnails."},{"code":"ORGANIZATION_PAYMENT_GRACE","when":"The organization is in its payment grace period."},{"code":"ORGANIZATION_CANCELLED","when":"The organization has been cancelled."},{"code":"ORGANIZATION_PURGED","when":"The organization has been purged."}]} />

## Not found

<ErrorTable errors={[{"code":"PROJECT_NOT_FOUND","when":"No project matches the \`project\` slug."},{"code":"PROJECT_NOT_IN_ENVIRONMENT","when":"The project is not published in the key's environment."},{"code":"TEMPLATE_NOT_FOUND","when":"No template matches the \`template\` slug in the published project."},{"code":"DOCUMENT_NOT_FOUND","when":"No stored document matches the supplied id."},{"code":"NOT_FOUND","when":"Generic 404: no resource matched the request."},{"code":"DOCUMENT_GONE","when":"The document existed but has been deleted (410)."}]} />

## Validation

<ErrorTable errors={[{"code":"VALIDATION_ERROR","when":"The request body failed validation, including unknown fields."},{"code":"MISSING_DATA","when":"Request body lacks the required \`data\` field."},{"code":"MISSING_TEMPLATE_SLUG","when":"Template slug is missing."},{"code":"PROJECT_REQUIRED_FOR_DOCUMENT","when":"Storing a document requires a \`project\`."},{"code":"ENVIRONMENT_MISMATCH","when":"The request targets an environment other than the key's."},{"code":"TEMPLATE_ERROR","when":"The template failed to render with the supplied data."},{"code":"EMPTY_DOCUMENT","when":"The render produced a document with no pages."},{"code":"RENDER_BODY_TOO_LARGE","when":"The render request body exceeds the size limit (413)."},{"code":"DOCUMENT_TOO_LARGE","when":"The rendered document exceeds the size limit (413)."}]} />

## Rate and quota

<ErrorTable errors={[{"code":"RATE_LIMIT_EXCEEDED","when":"Too many requests in the rate-limit window (429). Retryable."},{"code":"QUOTA_EXCEEDED","when":"The organization exceeded its plan quota. Retryable."},{"code":"OVERAGE_CAP_EXCEEDED","when":"The organization exceeded its overage cap."}]} />

## Server

<ErrorTable errors={[{"code":"INTERNAL_ERROR","when":"The API returned 5xx. Retryable."},{"code":"STORAGE_ACCESS_DENIED","when":"The API could not read or write document storage (502). Retryable."},{"code":"PUBLISH_IN_PROGRESS","when":"A publish holds the project in this environment (503, with \`Retry-After\`). Retryable."},{"code":"ORGANIZATION_MIGRATING","when":"The organization is being migrated (503). Retryable."},{"code":"RENDER_TIMEOUT","when":"The render did not finish in time (504). Retryable."}]} />
`;
}
