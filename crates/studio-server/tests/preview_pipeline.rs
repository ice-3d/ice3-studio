mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;

use studio_core::document::Document;
use studio_server::preview::PreviewAdapter;

use common::{FAILED_TOOLCHAIN, SUCCESS_TOOLCHAIN};

fn fake_toolchain(root: &std::path::Path, body: &str) -> std::path::PathBuf {
    let path = root.join("fake-toolchain.mjs");
    fs::write(&path, body).unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).unwrap();
    path
}

#[tokio::test]
async fn writes_deterministic_source_and_publishes_successful_preview() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    fs::create_dir_all(&project).unwrap();
    let adapter = fake_toolchain(&project, SUCCESS_TOOLCHAIN);
    let preview = PreviewAdapter::new(&project, adapter)
        .unwrap()
        .generate(&Document::new("preview-test"), &project, "preview-test")
        .unwrap();

    assert!(preview.preview_path.exists());
    assert_eq!(preview.source_digest.len(), 64);
}

#[tokio::test]
async fn retains_no_partial_preview_when_adapter_fails() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("failed-project");
    fs::create_dir_all(&project).unwrap();
    let adapter = fake_toolchain(&project, FAILED_TOOLCHAIN);
    let error = PreviewAdapter::new(&project, adapter)
        .unwrap()
        .generate(&Document::new("failed"), &project, "failed")
        .unwrap_err();

    assert!(error.to_string().contains("planned failure"));
    assert!(!project.join("cache/preview/00000000.glb").exists());
}
