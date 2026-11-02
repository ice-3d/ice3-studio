mod common;

use sha2::{Digest, Sha256};
use studio_server::{ServerConfig, start_test_server};

#[tokio::test]
async fn committed_preview_is_written_by_studio_without_driver_state() {
    let server = start_test_server(ServerConfig::for_tests("native-preview")).await;
    server.apply_fixture_batch().await.unwrap();
    let response = common::rpc(&server, "export.runtimePreview", serde_json::json!({})).await;
    let preview = &response["result"];
    let path = std::path::PathBuf::from(preview["previewPath"].as_str().unwrap());
    let bytes = std::fs::read(&path).unwrap();

    assert_eq!(&bytes[..4], b"glTF");
    assert!(path.ends_with("cache/preview/00000001.glb"));
    assert_eq!(
        preview["contentHash"],
        format!("{:x}", Sha256::digest(&bytes))
    );
    let served = reqwest::get(format!(
        "{}/previews/{}",
        server.base_url(),
        preview["revision"]
    ))
    .await
    .unwrap();
    assert!(served.status().is_success());
    assert!(
        !server
            .workspace_root()
            .join("native-preview/cache/toolchain")
            .exists()
    );
}

#[tokio::test]
async fn candidate_preview_uses_the_native_candidate_path() {
    let server = start_test_server(ServerConfig::for_tests("native-candidate")).await;
    let candidate = common::create_and_evaluate_candidate(&server).await;
    let digest = candidate["candidateDigest"].as_str().unwrap();
    let response = common::rpc(
        &server,
        "export.runtimePreview",
        serde_json::json!({ "candidateDigest": digest }),
    )
    .await;

    assert_eq!(response["result"]["assetId"], digest);
    let preview = server
        .workspace_root()
        .join("native-candidate")
        .join("cache/preview/candidates")
        .join(format!("{digest}.glb"));
    assert!(preview.is_file());
    let bytes = std::fs::read(&preview).unwrap();
    assert_eq!(
        response["result"]["contentHash"],
        format!("{:x}", Sha256::digest(&bytes))
    );
    let served = reqwest::get(format!(
        "{}/previews/candidates/{digest}",
        server.base_url()
    ))
    .await
    .unwrap();
    assert!(served.status().is_success());
    assert!(
        !server
            .workspace_root()
            .join("native-candidate/cache/toolchain")
            .exists()
    );
}
