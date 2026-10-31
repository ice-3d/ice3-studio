use std::path::PathBuf;
use std::time::Duration;

use serde_json::json;
use sha2::{Digest, Sha256};
use studio_toolchain_client::{
    ToolchainClient, ToolchainClientError, ToolchainLaunchConfig, validate_workspace_path,
};

fn test_client(mode: &str) -> ToolchainClient {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake-sidecar.mjs");
    ToolchainClient::spawn(ToolchainLaunchConfig {
        program: node_binary(),
        args: vec![fixture],
        environment: vec![("FAKE_SIDECAR_MODE".into(), mode.into())],
        workspace_root: std::env::temp_dir(),
        request_timeout: Duration::from_millis(250),
    })
    .unwrap()
}

fn node_binary() -> PathBuf {
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .map(|directory| directory.join("node"))
        .find(|candidate| candidate.is_file())
        .expect("node executable")
}

#[test]
fn negotiates_contract_and_reports_versions() {
    let client = test_client("normal");
    let capabilities = client.capabilities().unwrap();
    assert_eq!(capabilities.contract_version, "1.0.0");
    assert_eq!(capabilities.package_version, "0.2.0");
    assert!(
        capabilities
            .operations
            .contains(&"asset.preview".to_string())
    );
}

#[test]
fn rejects_an_incompatible_major_contract() {
    let client = test_client("incompatible");
    assert!(matches!(
        client.capabilities(),
        Err(ToolchainClientError::Incompatible { .. })
    ));
}

#[test]
fn rejects_paths_outside_the_workspace() {
    let root = std::env::temp_dir();
    assert!(validate_workspace_path(&root, "../outside.glb").is_err());
    assert!(validate_workspace_path(&root, "/absolute.glb").is_err());
    assert!(validate_workspace_path(&root, "cache/result.glb").is_ok());
}

#[test]
fn maps_invalid_stdout_to_protocol_error() {
    let client = test_client("invalid-json");
    assert!(matches!(
        client.capabilities(),
        Err(ToolchainClientError::Protocol { .. })
    ));
}

#[test]
fn maps_log_contamination_to_protocol_error() {
    let client = test_client("log-contamination");
    assert!(matches!(
        client.capabilities(),
        Err(ToolchainClientError::Protocol { .. })
    ));
}

#[test]
fn maps_missing_response_to_timeout() {
    let client = test_client("missing-response");
    assert!(matches!(
        client.capabilities(),
        Err(ToolchainClientError::Timeout { .. })
    ));
}

#[test]
fn maps_child_exit_to_crashed() {
    let client = test_client("crash");
    assert!(matches!(
        client.capabilities(),
        Err(ToolchainClientError::Crashed { .. })
    ));
}

#[test]
fn maps_oversized_response_to_protocol_error() {
    let client = test_client("oversized");
    assert!(matches!(
        client.capabilities(),
        Err(ToolchainClientError::Protocol { .. })
    ));
}

#[test]
fn preview_returns_a_typed_asset_result() {
    let client = test_client("normal");
    let result = client
        .asset_preview(json!({
            "sourcePath": "cache/source.glb",
            "outputPath": "cache/toolchain/result.glb",
            "assetId": "preview",
            "budget": "unrestricted"
        }))
        .unwrap();
    assert_eq!(result.asset_id, "preview");
    assert_eq!(result.primary_path, "cache/toolchain/result.glb");
    assert_eq!(result.byte_length, 12);
}

#[test]
fn loads_and_hashes_a_managed_manifest() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("toolchain/dist")).unwrap();
    std::fs::create_dir_all(root.path().join("runtime")).unwrap();
    let runtime = root.path().join("runtime/node");
    let entry = root.path().join("toolchain/dist/main.js");
    std::fs::write(&runtime, b"runner").unwrap();
    std::fs::write(&entry, b"entry").unwrap();
    let hash = |bytes: &[u8]| format!("{:x}", Sha256::digest(bytes));
    std::fs::write(
        root.path().join("toolchain/manifest.json"),
        serde_json::to_vec(&json!({
            "schemaVersion": 1,
            "contractVersion": "1.0.0",
            "packageVersion": "0.2.0",
            "nodeVersion": "v24.22.1",
            "platform": if cfg!(target_os = "macos") { "macos" } else { std::env::consts::OS },
            "architecture": if cfg!(target_arch = "aarch64") {
                "arm64"
            } else if cfg!(target_arch = "x86_64") {
                "x64"
            } else {
                std::env::consts::ARCH
            },
            "entrypoint": "runtime/node",
            "packageEntry": "toolchain/dist/main.js",
            "sha256": "unused-by-loader",
            "files": {
                "runtime/node": hash(b"runner"),
                "toolchain/dist/main.js": hash(b"entry")
            }
        }))
        .unwrap(),
    )
    .unwrap();
    let config = ToolchainLaunchConfig::from_manifest(root.path()).unwrap();
    assert_eq!(config.program, runtime.canonicalize().unwrap());
    assert_eq!(config.args[0], entry.canonicalize().unwrap());
}
