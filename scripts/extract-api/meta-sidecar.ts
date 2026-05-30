// Build the `_meta.json` sidecar emitted alongside the generated reference
// pages. The shared LangSwitcher component reads sibling SDKs' sidecar files at
// build time to render "this method also exists in: …" affordances.

interface MetaSidecar {
  language: 'rust';
  package: { kind: 'crate'; name: string; version: string };
  extractedAt: string;
  extractorVersion: string;
  client: { name: 'PoliPage'; kind: 'struct' };
  methods: ReadonlyArray<{ slug: string; name: string }>;
  errors: ReadonlyArray<{ code: string }>;
}

export function buildMetaSidecar(packageVersion: string): MetaSidecar {
  return {
    language: 'rust',
    package: { kind: 'crate', name: 'poli-page', version: packageVersion },
    extractedAt: new Date().toISOString(),
    extractorVersion: '0.1.0',
    client: { name: 'PoliPage', kind: 'struct' },
    methods: [
      { slug: 'render-pdf', name: 'client.render.pdf' },
      { slug: 'render-pdf-stream', name: 'client.render.pdf_stream' },
      { slug: 'render-preview', name: 'client.render.preview' },
      { slug: 'render-document', name: 'client.render.document' },
      { slug: 'documents-get', name: 'client.documents.get' },
      { slug: 'documents-preview', name: 'client.documents.preview' },
      { slug: 'documents-thumbnails', name: 'client.documents.thumbnails' },
      { slug: 'documents-delete', name: 'client.documents.delete' },
      { slug: 'render-to-file', name: 'render_to_file' },
    ],
    errors: [
      { code: 'invalid_options' },
      { code: 'network_error' },
      { code: 'timeout' },
      { code: 'aborted' },
      { code: 'DOWNLOAD_FAILED' },
      { code: 'INTERNAL_ERROR' },
      { code: 'MISSING_API_KEY' },
      { code: 'INVALID_API_KEY' },
      { code: 'PAYMENT_REQUIRED' },
      { code: 'FORBIDDEN' },
      { code: 'ORGANIZATION_CANCELLED' },
      { code: 'ORGANIZATION_PURGED' },
      { code: 'NOT_FOUND' },
      { code: 'VERSION_NOT_FOUND' },
      { code: 'DOCUMENT_NOT_FOUND' },
      { code: 'GONE' },
      { code: 'VALIDATION_ERROR' },
      { code: 'MISSING_DATA' },
      { code: 'MISSING_PROJECT_OR_TEMPLATE' },
      { code: 'MISSING_TEMPLATE_SLUG' },
      { code: 'PROJECT_REQUIRED_FOR_DOCUMENT' },
      { code: 'INVALID_VERSION_FORMAT' },
      { code: 'VERSION_REQUIRED' },
      { code: 'INVALID_VERSION_FOR_KEY_ENV' },
      { code: 'QUOTA_EXCEEDED' },
      { code: 'OVERAGE_CAP_EXCEEDED' },
    ],
  };
}
