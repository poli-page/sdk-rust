//! `render_to_file` end-to-end test against a wiremock server.

use std::error::Error as _;
use std::time::Duration;

use poli_page::{PoliPage, RenderInput};
use serde_json::json;
use tempfile::tempdir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const PDF_STUB: &[u8] = b"%PDF-1.4 stub-from-stream";

fn descriptor_wire(presigned_url: &str) -> serde_json::Value {
    json!({
        "documentId": "doc_abc",
        "organizationId": "org_x",
        "projectId": "proj_42",
        "projectSlug": "billing",
        "templateId": "tpl_1",
        "templateSlug": "invoice",
        "environment": "live",
        "apiKeyId": "key_x",
        "format": "A4",
        "orientation": "portrait",
        "locale": "en-US",
        "pageCount": 1,
        "sizeBytes": PDF_STUB.len() as i64,
        "createdAt": "2026-05-24T00:00:00Z",
        "metadata": {},
        "presignedPdfUrl": presigned_url,
        "expiresAt": "2026-05-24T00:15:00Z"
    })
}

async fn make_servers() -> (MockServer, String) {
    let server = MockServer::start().await;
    let presigned = format!("{}/presigned/x.pdf", server.uri());
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(ResponseTemplate::new(200).set_body_json(descriptor_wire(&presigned)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/presigned/x.pdf"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(PDF_STUB))
        .mount(&server)
        .await;
    (server, "/presigned/x.pdf".to_string())
}

fn input() -> RenderInput {
    RenderInput {
        project: "p".into(),
        template: "t".into(),
        data: json!({}),
        ..Default::default()
    }
}

#[tokio::test]
async fn render_to_file_writes_pdf_bytes_to_destination() {
    let (server, _) = make_servers().await;
    let client = PoliPage::builder()
        .api_key("pp_sandbox_x")
        .base_url(server.uri())
        .timeout(Duration::from_secs(2))
        .max_retries(0)
        .build()
        .unwrap();

    let dir = tempdir().unwrap();
    let target = dir.path().join("out.pdf");
    poli_page::render_to_file(&client, input(), &target)
        .await
        .expect("render_to_file");

    let bytes = std::fs::read(&target).unwrap();
    assert_eq!(bytes, PDF_STUB);
}

#[tokio::test]
async fn render_to_file_creates_missing_parent_directories() {
    let (server, _) = make_servers().await;
    let client = PoliPage::builder()
        .api_key("pp_sandbox_x")
        .base_url(server.uri())
        .timeout(Duration::from_secs(2))
        .max_retries(0)
        .build()
        .unwrap();

    let dir = tempdir().unwrap();
    let nested = dir.path().join("does/not/exist/yet/out.pdf");
    poli_page::render_to_file(&client, input(), &nested)
        .await
        .expect("render_to_file with nested path");
    assert!(nested.exists());
    assert_eq!(std::fs::read(&nested).unwrap(), PDF_STUB);
}

#[tokio::test]
async fn render_to_file_overwrites_existing_file() {
    let (server, _) = make_servers().await;
    let client = PoliPage::builder()
        .api_key("pp_sandbox_x")
        .base_url(server.uri())
        .timeout(Duration::from_secs(2))
        .max_retries(0)
        .build()
        .unwrap();

    let dir = tempdir().unwrap();
    let target = dir.path().join("out.pdf");
    std::fs::write(&target, b"old contents that should be replaced").unwrap();
    poli_page::render_to_file(&client, input(), &target)
        .await
        .expect("render_to_file overwriting");
    assert_eq!(std::fs::read(&target).unwrap(), PDF_STUB);
}

#[tokio::test]
async fn render_to_file_surfaces_download_failed_on_presigned_404() {
    let server = MockServer::start().await;
    let presigned = format!("{}/presigned/missing.pdf", server.uri());
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(ResponseTemplate::new(200).set_body_json(descriptor_wire(&presigned)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/presigned/missing.pdf"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    let client = PoliPage::builder()
        .api_key("pp_sandbox_x")
        .base_url(server.uri())
        .timeout(Duration::from_secs(2))
        .max_retries(0)
        .build()
        .unwrap();
    let dir = tempdir().unwrap();
    let err = poli_page::render_to_file(&client, input(), dir.path().join("out.pdf"))
        .await
        .expect_err("download must fail");
    assert!(matches!(err, poli_page::Error::Download { .. }));
    assert_eq!(err.status(), Some(404));
}

// ---------------------------------------------------------------------------
// io_error: the file cannot be written, the target is left as it was
// ---------------------------------------------------------------------------

/// The names in `dir`, sorted.
fn listing(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("read dir")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn assert_io_error(err: &poli_page::Error) {
    assert!(matches!(err, poli_page::Error::Io { .. }), "{err:?}");
    assert_eq!(err.code(), "io_error");
    assert_eq!(err.status(), None);
    assert_eq!(err.request_id(), None);
    assert!(!err.is_retryable());
    assert!(!err.is_network_error());
    // The message carries the I/O error's own text, which is the source.
    let source = err
        .source()
        .and_then(|s| s.downcast_ref::<std::io::Error>())
        .expect("the io::Error is the source");
    assert_eq!(err.message(), format!("Failed to write PDF: {source}"));
    assert_eq!(err.to_string(), err.message());
    let payload = serde_json::to_value(err.to_payload()).expect("payload");
    assert_eq!(payload["code"], "io_error");
    assert!(payload["status"].is_null());
}

/// A server that answers the render and the download, and counts both.
async fn counted_servers() -> MockServer {
    let (server, _) = make_servers().await;
    server
}

async fn client_for(server: &MockServer) -> PoliPage {
    PoliPage::builder()
        .api_key("pp_sandbox_x")
        .base_url(server.uri())
        .timeout(Duration::from_secs(2))
        .max_retries(0)
        .build()
        .unwrap()
}

#[tokio::test]
async fn render_to_file_is_io_error_when_a_parent_directory_cannot_be_created() {
    let server = counted_servers().await;
    let client = client_for(&server).await;
    let dir = tempdir().unwrap();
    // A file where a parent directory should be.
    std::fs::write(dir.path().join("taken"), b"a file").unwrap();
    let err = poli_page::render_to_file(&client, input(), dir.path().join("taken/out.pdf"))
        .await
        .expect_err("parent is a file");
    assert_io_error(&err);
    // Checked before the render: nothing was rendered.
    assert!(server.received_requests().await.unwrap().is_empty());
    assert_eq!(listing(dir.path()), ["taken"]);
}

#[tokio::test]
async fn render_to_file_is_io_error_when_the_target_cannot_be_replaced() {
    let server = counted_servers().await;
    let client = client_for(&server).await;
    let dir = tempdir().unwrap();
    // A non-empty directory at the target path: the final rename fails on
    // every platform.
    let target = dir.path().join("out.pdf");
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("keep.txt"), b"kept").unwrap();
    let err = poli_page::render_to_file(&client, input(), &target)
        .await
        .expect_err("target is a directory");
    assert_io_error(&err);
    assert_eq!(listing(dir.path()), ["out.pdf"], "no temporary file left");
    assert_eq!(std::fs::read(target.join("keep.txt")).unwrap(), b"kept");
}

#[cfg(unix)]
#[tokio::test]
async fn render_to_file_is_io_error_when_the_directory_is_read_only() {
    use std::os::unix::fs::PermissionsExt;
    let server = counted_servers().await;
    let client = client_for(&server).await;
    let dir = tempdir().unwrap();
    let target = dir.path().join("out.pdf");
    std::fs::write(&target, b"the previous PDF").unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
    // Root writes anyway: nothing to check then.
    let probe = dir.path().join("probe");
    if std::fs::write(&probe, b"").is_ok() {
        std::fs::remove_file(&probe).unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        eprintln!("skipped: this user can write to a read-only directory");
        return;
    }
    let result = poli_page::render_to_file(&client, input(), &target).await;
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    let err = result.expect_err("read-only directory");
    assert_io_error(&err);
    assert_eq!(
        err.source()
            .and_then(|s| s.downcast_ref::<std::io::Error>())
            .map(std::io::Error::kind),
        Some(std::io::ErrorKind::PermissionDenied)
    );
    assert!(server.received_requests().await.unwrap().is_empty());
    assert_eq!(std::fs::read(&target).unwrap(), b"the previous PDF");
}

#[tokio::test]
async fn render_to_file_creates_no_file_when_the_render_fails() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({
            "error": "TEMPLATE_NOT_FOUND",
            "detail": "Template 't' not found.",
        })))
        .mount(&server)
        .await;
    let client = client_for(&server).await;
    let dir = tempdir().unwrap();
    let err = poli_page::render_to_file(&client, input(), dir.path().join("out.pdf"))
        .await
        .expect_err("render fails");
    assert_eq!(err.code(), "TEMPLATE_NOT_FOUND");
    assert!(listing(dir.path()).is_empty(), "{:?}", listing(dir.path()));
}

#[tokio::test]
async fn render_to_file_keeps_the_target_on_a_download_failure() {
    let server = MockServer::start().await;
    let presigned = format!("{}/presigned/missing.pdf", server.uri());
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(ResponseTemplate::new(200).set_body_json(descriptor_wire(&presigned)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/presigned/missing.pdf"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&server)
        .await;
    let client = client_for(&server).await;
    let dir = tempdir().unwrap();
    let target = dir.path().join("out.pdf");
    std::fs::write(&target, b"the previous PDF").unwrap();
    let err = poli_page::render_to_file(&client, input(), &target)
        .await
        .expect_err("download fails");
    assert_eq!(err.code(), "DOWNLOAD_FAILED");
    assert_eq!(std::fs::read(&target).unwrap(), b"the previous PDF");
    assert_eq!(listing(dir.path()), ["out.pdf"]);
}

#[tokio::test]
async fn render_to_file_leaves_no_temporary_file_on_success() {
    let (server, _) = make_servers().await;
    let client = client_for(&server).await;
    let dir = tempdir().unwrap();
    poli_page::render_to_file(&client, input(), dir.path().join("out.pdf"))
        .await
        .expect("render_to_file");
    assert_eq!(listing(dir.path()), ["out.pdf"]);
}

// ---------------------------------------------------------------------------
// A symbolic link stays a link; an existing file keeps its permissions
// ---------------------------------------------------------------------------

/// Makes `link` point at `to`. `false` when the platform refuses (Windows
/// without the symlink privilege or developer mode): the caller skips.
fn make_symlink(to: &std::path::Path, link: &std::path::Path) -> bool {
    #[cfg(unix)]
    let made = std::os::unix::fs::symlink(to, link);
    #[cfg(windows)]
    let made = std::os::windows::fs::symlink_file(to, link);
    match made {
        Ok(()) => true,
        Err(e) => {
            eprintln!("skipped: cannot create a symbolic link here: {e}");
            false
        }
    }
}

#[tokio::test]
async fn render_to_file_writes_through_a_symlinked_target() {
    let (server, _) = make_servers().await;
    let client = client_for(&server).await;
    let dir = tempdir().unwrap();
    let store = dir.path().join("store");
    std::fs::create_dir(&store).unwrap();
    let real = store.join("real.pdf");
    std::fs::write(&real, b"the previous PDF").unwrap();
    let link = dir.path().join("out.pdf");
    if !make_symlink(&real, &link) {
        return;
    }
    poli_page::render_to_file(&client, input(), &link)
        .await
        .expect("render_to_file through a link");
    assert!(
        std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink(),
        "the link stays a link"
    );
    assert_eq!(std::fs::read_link(&link).unwrap(), real);
    assert_eq!(std::fs::read(&real).unwrap(), PDF_STUB);
    // The temporary file went next to the real file, and is gone.
    assert_eq!(listing(dir.path()), ["out.pdf", "store"]);
    assert_eq!(listing(&store), ["real.pdf"]);
}

#[tokio::test]
async fn render_to_file_failing_through_a_symlink_keeps_the_link_and_the_file() {
    let server = MockServer::start().await;
    let presigned = format!("{}/presigned/missing.pdf", server.uri());
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(ResponseTemplate::new(200).set_body_json(descriptor_wire(&presigned)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/presigned/missing.pdf"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&server)
        .await;
    let client = client_for(&server).await;
    let dir = tempdir().unwrap();
    let real = dir.path().join("real.pdf");
    std::fs::write(&real, b"the previous PDF").unwrap();
    let link = dir.path().join("out.pdf");
    if !make_symlink(&real, &link) {
        return;
    }
    let err = poli_page::render_to_file(&client, input(), &link)
        .await
        .expect_err("download fails");
    assert_eq!(err.code(), "DOWNLOAD_FAILED");
    assert!(std::fs::symlink_metadata(&link)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(std::fs::read(&real).unwrap(), b"the previous PDF");
    assert_eq!(listing(dir.path()), ["out.pdf", "real.pdf"]);
}

#[cfg(unix)]
#[tokio::test]
async fn render_to_file_writes_the_file_a_dangling_symlink_names() {
    let (server, _) = make_servers().await;
    let client = client_for(&server).await;
    let dir = tempdir().unwrap();
    let link = dir.path().join("out.pdf");
    std::os::unix::fs::symlink("store/new.pdf", &link).unwrap();
    poli_page::render_to_file(&client, input(), &link)
        .await
        .expect("render_to_file through a dangling link");
    assert!(std::fs::symlink_metadata(&link)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(
        std::fs::read(dir.path().join("store/new.pdf")).unwrap(),
        PDF_STUB
    );
}

#[cfg(unix)]
#[tokio::test]
async fn render_to_file_keeps_the_permission_mode_of_an_existing_target() {
    use std::os::unix::fs::PermissionsExt;
    let (server, _) = make_servers().await;
    let client = client_for(&server).await;
    let dir = tempdir().unwrap();
    for mode in [0o600, 0o640, 0o444] {
        let target = dir.path().join(format!("out-{mode:o}.pdf"));
        std::fs::write(&target, b"the previous PDF").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(mode)).unwrap();
        poli_page::render_to_file(&client, input(), &target)
            .await
            .expect("render_to_file");
        assert_eq!(std::fs::read(&target).unwrap(), PDF_STUB);
        let kept = std::fs::metadata(&target).unwrap().permissions().mode() & 0o7777;
        assert_eq!(kept, mode, "mode {mode:o}");
    }
}

#[cfg(unix)]
#[tokio::test]
async fn control_a_new_target_gets_the_default_mode() {
    use std::os::unix::fs::PermissionsExt;
    let (server, _) = make_servers().await;
    let client = client_for(&server).await;
    let dir = tempdir().unwrap();
    let target = dir.path().join("out.pdf");
    poli_page::render_to_file(&client, input(), &target)
        .await
        .expect("render_to_file");
    // A plain new file: what `File::create` would give under this umask.
    let control = dir.path().join("control");
    std::fs::write(&control, b"").unwrap();
    let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o7777;
    assert_eq!(mode(&target), mode(&control));
}
