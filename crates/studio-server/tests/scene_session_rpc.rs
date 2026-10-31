mod common;

use common::rpc_with_bearer;
use studio_core::gpu_contract::GeometryBackendMode;
use studio_server::{ServerConfig, TestServer, start_test_server};

async fn rpc(server: &TestServer, method: &str, params: serde_json::Value) -> serde_json::Value {
    rpc_with_bearer(server, server.token(), method, params).await
}

#[tokio::test]
async fn scene_sessions_are_disabled_by_default() {
    let server = start_test_server(ServerConfig::for_tests("scene-default")).await;
    let status = rpc(&server, "system.capabilities", serde_json::json!({})).await;

    assert_eq!(
        status["result"]["compute"]["sceneSession"]["enabled"],
        false
    );
}

#[tokio::test]
async fn optional_scene_failure_keeps_the_committed_document() {
    let config = ServerConfig {
        compute_mode: GeometryBackendMode::Gpu,
        scene_sessions_enabled: true,
        ..ServerConfig::for_tests("scene-failure")
    };
    let server = start_test_server(config).await;
    let response = rpc(
        &server,
        "document.apply",
        serde_json::json!({
            "expectedRevision": 0,
            "idempotencyKey": "scene-create",
            "operations": [{
                "type": "node.create",
                "node_id": "node-000001",
                "parent_id": null,
                "name": "Primary",
                "transform": {
                    "translation": { "x": 0, "y": 0, "z": 0 },
                    "rotation": [0, 0, 0, 1],
                    "scale": { "x": 1, "y": 1, "z": 1 }
                },
                "content": {"type": "group"},
                "material_id": null,
                "metadata": {}
            }]
        }),
    )
    .await;

    assert_eq!(response["result"]["revision"], 1, "{response}");
    let document = rpc(&server, "document.get", serde_json::json!({})).await;
    assert_eq!(document["result"]["revision"], 1);
}
