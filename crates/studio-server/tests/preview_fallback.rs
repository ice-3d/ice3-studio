//! 预览链路不得依赖外部 toolchain。内核本身能产出 GLB，toolchain 只是可选优化器。

mod common;

use common::rpc_with_bearer;
use serde_json::json;
use studio_server::{ServerConfig, start_test_server};

#[tokio::test]
async fn preview_is_available_without_an_external_toolchain() {
    let server = start_test_server(ServerConfig::for_tests("no-toolchain-preview")).await;
    let token = server.agent_token();

    let applied = rpc_with_bearer(
        &server,
        token,
        "document.apply",
        json!({
            "expectedRevision": 0,
            "dryRun": false,
            "idempotencyKey": "no-toolchain-1",
            "operations": [{
                "type": "node.create",
                "node_id": "node-000001",
                "parent_id": null,
                "name": "settling tank",
                "transform": {
                    "translation": { "x": 0, "y": 0, "z": 0 },
                    "rotation": [0, 0, 0, 1],
                    "scale": { "x": 1, "y": 1, "z": 1 }
                },
                "content": { "type": "primitive", "kind": "box", "width": 12, "height": 4, "depth": 12 },
                "material_id": null,
                "metadata": {}
            }]
        }),
    )
    .await;
    assert!(applied.get("error").is_none(), "apply failed: {applied}");

    let response = rpc_with_bearer(&server, token, "export.runtimePreview", json!({})).await;
    assert!(
        response.get("error").is_none(),
        "preview must not require an external toolchain: {response}"
    );
    let result = &response["result"];
    let revision = result["revision"].as_u64().expect("revision");
    assert_eq!(revision, 1);

    // 预览文件必须落盘且是合法 GLB；Viewer 通过 /previews/{revision} 读取它。
    let preview_path =
        std::path::PathBuf::from(result["previewPath"].as_str().expect("previewPath"));
    let bytes = std::fs::read(&preview_path).expect("preview glb");
    assert!(bytes.len() > 12, "preview glb is empty");
    assert_eq!(&bytes[..4], b"glTF", "preview is not a GLB");
}
