mod common;

use studio_server::{ServerConfig, start_test_server};

use common::rpc_with_bearer;

#[tokio::test]
async fn evaluates_and_measures_component_instances_by_derived_identity() {
    let server = start_test_server(ServerConfig::for_tests("large-scene")).await;
    let component = serde_json::json!({
        "id": "component-000001",
        "name": "Pump",
        "parameters": [],
        "parts": [{
            "id": "body",
            "parentId": null,
            "transform": {
                "translation": { "kind": "literal", "value": { "x": 0, "y": 0, "z": 0 } },
                "rotation": [0, 0, 0, 1],
                "scale": { "kind": "literal", "value": { "x": 1, "y": 1, "z": 1 } }
            },
            "content": {
                "kind": "box",
                "width": { "kind": "literal", "value": 1 },
                "height": { "kind": "literal", "value": 1 },
                "depth": { "kind": "literal", "value": 1 }
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
        &server,
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
