mod common;

use studio_server::{ServerConfig, start_test_server};

use common::rpc_with_bearer;

#[tokio::test]
async fn evaluates_and_measures_component_instances_by_derived_identity() {
    let server = start_test_server(ServerConfig::for_tests("large-scene")).await;
    apply_instance_and_set(&server).await;

    let evaluated = rpc_with_bearer(
        &server,
        server.token(),
        "evaluation.evaluate",
        serde_json::json!({}),
    )
    .await;
    assert!(evaluated["result"]["node-000001/body"].is_object());
    assert!(evaluated["result"]["node-000010/000000/body"].is_object());

    let bounds = rpc_with_bearer(
        &server,
        server.token(),
        "scene.bounds",
        serde_json::json!({}),
    )
    .await;
    assert!(bounds["result"]["node-000001/body"].is_object());
    assert!(bounds["result"]["node-000010/000000/body"].is_object());
}

#[tokio::test]
async fn reports_large_scene_statistics_without_returning_geometry() {
    let server = start_test_server(ServerConfig::for_tests("large-stats")).await;
    apply_two_shared_instances(&server).await;

    let stats = rpc_with_bearer(
        &server,
        server.token(),
        "evaluation.stats",
        serde_json::json!({}),
    )
    .await;
    assert_eq!(stats["result"]["sourceNodes"], 2);
    assert_eq!(stats["result"]["expandedNodes"], 2);
    assert_eq!(stats["result"]["uniqueMeshes"], 1);
    assert_eq!(stats["result"]["effectiveTriangles"], 24);
    assert_eq!(stats["result"]["uniqueTriangles"], 12);
    assert!(stats["result"].get("vertices").is_none());
}

async fn apply_instance_and_set(server: &studio_server::TestServer) {
    let component = serde_json::json!({
        "id": "component-000001",
        "name": "Pump",
        "parameters": [],
        "parts": [{
            "id": "body",
            "parentId": null,
            "transform": {
                "translation": { "x": 0, "y": 0, "z": 0 },
                "rotation": [0, 0, 0, 1],
                "scale": { "x": 1, "y": 1, "z": 1 }
            },
            "content": {
                "kind": "box",
                "width": 1,
                "height": 1,
                "depth": 1
            },
            "materialRole": null,
            "metadata": {},
            "visible": true
        }],
        "ports": [],
        "metadata": {}
    });
    let instance = serde_json::json!({
        "definitionId": "component-000001",
        "parameters": {},
        "overrides": {},
        "metadata": {}
    });
    let applied = rpc_with_bearer(
        server,
        server.token(),
        "document.apply",
        serde_json::json!({
            "expectedRevision": 0,
            "dryRun": false,
            "idempotencyKey": "large-scene-instances",
            "operations": [
                { "type": "component.define", "definition": component },
                {
                    "type": "instance.create",
                    "node_id": "node-000001",
                    "parent_id": null,
                    "name": "Pump A",
                    "transform": {
                        "translation": { "x": 0, "y": 0, "z": 0 },
                        "rotation": [0, 0, 0, 1],
                        "scale": { "x": 1, "y": 1, "z": 1 }
                    },
                    "instance": instance,
                    "material_id": null,
                    "metadata": {}
                },
                {
                    "type": "instanceSet.create",
                    "node_id": "node-000010",
                    "parent_id": null,
                    "name": "Pump Row",
                    "transform": {
                        "translation": { "x": 0, "y": 0, "z": 0 },
                        "rotation": [0, 0, 0, 1],
                        "scale": { "x": 1, "y": 1, "z": 1 }
                    },
                    "set": {
                        "definitionId": "component-000001",
                        "layout": {
                            "kind": "linear",
                            "origin": { "x": 0, "y": 0, "z": 0 },
                            "direction": { "x": 1, "y": 0, "z": 0 },
                            "spacing": 2,
                            "count": 1
                        },
                        "sharedParameters": {},
                        "itemOverrides": {},
                        "metadata": {}
                    },
                    "material_id": null,
                    "metadata": {}
                }
            ]
        }),
    )
    .await;
    assert!(applied["result"]["revision"].is_number(), "{applied}");
}

async fn apply_two_shared_instances(server: &studio_server::TestServer) {
    let component = serde_json::json!({
        "id": "component-000001",
        "name": "Pump",
        "parameters": [],
        "parts": [{
            "id": "body",
            "parentId": null,
            "transform": {
                "translation": { "x": 0, "y": 0, "z": 0 },
                "rotation": [0, 0, 0, 1],
                "scale": { "x": 1, "y": 1, "z": 1 }
            },
            "content": {
                "kind": "box",
                "width": 1,
                "height": 1,
                "depth": 1
            },
            "materialRole": null,
            "metadata": {},
            "visible": true
        }],
        "ports": [],
        "metadata": {}
    });
    let instance = serde_json::json!({
        "definitionId": "component-000001",
        "parameters": {},
        "overrides": {},
        "metadata": {}
    });
    let applied = rpc_with_bearer(
        server,
        server.token(),
        "document.apply",
        serde_json::json!({
            "expectedRevision": 0,
            "dryRun": false,
            "idempotencyKey": "large-stats",
            "operations": [
                { "type": "component.define", "definition": component },
                {
                    "type": "instance.create",
                    "node_id": "node-000001",
                    "parent_id": null,
                    "name": "Pump A",
                    "transform": {
                        "translation": { "x": 0, "y": 0, "z": 0 },
                        "rotation": [0, 0, 0, 1],
                        "scale": { "x": 1, "y": 1, "z": 1 }
                    },
                    "instance": instance,
                    "material_id": null,
                    "metadata": {}
                },
                {
                    "type": "instance.create",
                    "node_id": "node-000002",
                    "parent_id": null,
                    "name": "Pump B",
                    "transform": {
                        "translation": { "x": 4, "y": 0, "z": 0 },
                        "rotation": [0, 0, 0, 1],
                        "scale": { "x": 1, "y": 1, "z": 1 }
                    },
                    "instance": instance,
                    "material_id": null,
                    "metadata": {}
                }
            ]
        }),
    )
    .await;
    assert!(applied["result"]["revision"].is_number(), "{applied}");
}
