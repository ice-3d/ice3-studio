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

#[tokio::test]
async fn visibility_falls_back_to_cpu_when_scene_sessions_are_disabled() {
    let server = start_test_server(ServerConfig::for_tests("visibility-cpu-fallback")).await;
    let applied = rpc(
        &server,
        "document.apply",
        serde_json::json!({
            "expectedRevision": 0,
            "idempotencyKey": "visibility-cpu-fallback-create",
            "operations": [
                {
                    "type": "node.create",
                    "node_id": "node-000001",
                    "parent_id": null,
                    "name": "Near",
                    "transform": {
                        "translation": { "x": 0, "y": 0, "z": 0 },
                        "rotation": [0, 0, 0, 1],
                        "scale": { "x": 1, "y": 1, "z": 1 }
                    },
                    "content": {
                        "type": "primitive",
                        "kind": "box",
                        "width": 1,
                        "height": 1,
                        "depth": 1
                    },
                    "material_id": null,
                    "metadata": {}
                },
                {
                    "type": "node.create",
                    "node_id": "node-000002",
                    "parent_id": null,
                    "name": "Far",
                    "transform": {
                        "translation": { "x": 100, "y": 0, "z": 0 },
                        "rotation": [0, 0, 0, 1],
                        "scale": { "x": 1, "y": 1, "z": 1 }
                    },
                    "content": {
                        "type": "primitive",
                        "kind": "box",
                        "width": 1,
                        "height": 1,
                        "depth": 1
                    },
                    "material_id": null,
                    "metadata": {}
                }
            ]
        }),
    )
    .await;
    assert_eq!(applied["result"]["revision"], 1, "{applied}");
    rpc(&server, "evaluation.stats", serde_json::json!({})).await;

    let visible = rpc(
        &server,
        "evaluation.visibleInstances",
        serde_json::json!({
            "targetRevision": 1,
            "planes": [
                [1, 0, 0, 100],
                [-1, 0, 0, 10],
                [0, 1, 0, 100],
                [0, -1, 0, 100],
                [0, 0, 1, 100],
                [0, 0, -1, 100]
            ]
        }),
    )
    .await;

    assert_eq!(visible["result"]["revision"], 1, "{visible}");
    assert_eq!(visible["result"]["visibleIndices"], serde_json::json!([0]));
    assert_eq!(
        visible["result"]["visibleNodeIds"],
        serde_json::json!(["node-000001"])
    );
}

#[tokio::test]
async fn visibility_rejects_a_stale_target_revision() {
    let server = start_test_server(ServerConfig::for_tests("visibility-stale-target")).await;
    rpc(
        &server,
        "document.apply",
        serde_json::json!({
            "expectedRevision": 0,
            "idempotencyKey": "visibility-stale-target-create",
            "operations": [box_node("node-000001", 0.0)]
        }),
    )
    .await;

    let visible = rpc(
        &server,
        "evaluation.visibleInstances",
        serde_json::json!({
            "targetRevision": 0,
            "planes": [
                [1, 0, 0, 100],
                [-1, 0, 0, 100],
                [0, 1, 0, 100],
                [0, -1, 0, 100],
                [0, 0, 1, 100],
                [0, 0, -1, 100]
            ]
        }),
    )
    .await;

    assert_eq!(
        visible["error"]["diagnosticCode"], "ICE3_STUDIO_REVISION_CONFLICT",
        "{visible}"
    );
}

#[tokio::test]
async fn project_switch_replaces_the_cached_scene_index() {
    let server = start_test_server(ServerConfig::for_tests("scene-project-a")).await;
    let first = rpc(
        &server,
        "document.apply",
        serde_json::json!({
            "expectedRevision": 0,
            "idempotencyKey": "scene-project-a-create",
            "operations": [box_node("node-000001", 0.0)]
        }),
    )
    .await;
    assert_eq!(first["result"]["revision"], 1, "{first}");
    let first_stats = rpc(&server, "evaluation.stats", serde_json::json!({})).await;
    assert_eq!(first_stats["result"]["expandedNodes"], 1);

    let created = rpc(
        &server,
        "project.create",
        serde_json::json!({ "projectId": "scene-project-b" }),
    )
    .await;
    assert!(created.get("error").is_none(), "{created}");
    let second = rpc(
        &server,
        "document.apply",
        serde_json::json!({
            "expectedRevision": 0,
            "idempotencyKey": "scene-project-b-create",
            "operations": [box_node("node-000002", 4.0), box_node("node-000003", 8.0)]
        }),
    )
    .await;
    assert_eq!(second["result"]["revision"], 1, "{second}");
    let second_stats = rpc(&server, "evaluation.stats", serde_json::json!({})).await;
    assert_eq!(second_stats["result"]["expandedNodes"], 2);

    rpc(
        &server,
        "project.open",
        serde_json::json!({ "projectId": "scene-project-a" }),
    )
    .await;
    let reopened_stats = rpc(&server, "evaluation.stats", serde_json::json!({})).await;
    assert_eq!(reopened_stats["result"]["expandedNodes"], 1);
}

#[tokio::test]
async fn undo_and_redo_rebuild_the_incremental_scene_cache() {
    let server = start_test_server(ServerConfig::for_tests("scene-undo-redo")).await;
    rpc(
        &server,
        "document.apply",
        serde_json::json!({
            "expectedRevision": 0,
            "idempotencyKey": "scene-undo-redo-create",
            "operations": [box_node("node-000001", 0.0)]
        }),
    )
    .await;
    let initial = rpc(&server, "evaluation.stats", serde_json::json!({})).await;

    let cylinder = rpc(
        &server,
        "document.apply",
        serde_json::json!({
            "expectedRevision": 1,
            "idempotencyKey": "scene-undo-redo-cylinder",
            "operations": [{
                "type": "primitive.update",
                "node_id": "node-000001",
                "primitive": {
                    "type": "primitive",
                    "kind": "cylinder",
                    "radius": 1,
                    "height": 2,
                    "segments": 16
                }
            }]
        }),
    )
    .await;
    assert_eq!(cylinder["result"]["revision"], 2, "{cylinder}");
    let cylinder_stats = rpc(&server, "evaluation.stats", serde_json::json!({})).await;

    let undone = rpc(&server, "document.undo", serde_json::json!({})).await;
    assert_eq!(undone["result"]["revision"], 3, "{undone}");
    let undone_stats = rpc(&server, "evaluation.stats", serde_json::json!({})).await;
    assert_eq!(
        undone_stats["result"]["effectiveTriangles"], initial["result"]["effectiveTriangles"],
        "{undone}\n{undone_stats}"
    );

    let redone = rpc(&server, "document.redo", serde_json::json!({})).await;
    assert_eq!(redone["result"]["revision"], 4, "{redone}");
    let redone_stats = rpc(&server, "evaluation.stats", serde_json::json!({})).await;
    assert_eq!(
        redone_stats["result"]["effectiveTriangles"],
        cylinder_stats["result"]["effectiveTriangles"]
    );
}

fn box_node(node_id: &str, x: f32) -> serde_json::Value {
    serde_json::json!({
        "type": "node.create",
        "node_id": node_id,
        "parent_id": null,
        "name": node_id,
        "transform": {
            "translation": { "x": x, "y": 0, "z": 0 },
            "rotation": [0, 0, 0, 1],
            "scale": { "x": 1, "y": 1, "z": 1 }
        },
        "content": {
            "type": "primitive",
            "kind": "box",
            "width": 1,
            "height": 1,
            "depth": 1
        },
        "material_id": null,
        "metadata": {}
    })
}
