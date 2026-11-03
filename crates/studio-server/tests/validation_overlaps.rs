mod common;

use studio_server::{ServerConfig, TestServer, start_test_server};

use common::{rpc, rpc_with_approval_cookie};

#[tokio::test]
async fn validation_run_without_params_keeps_the_existing_shape() {
    let server = start_test_server(ServerConfig::review_required("validation-run-default")).await;
    seed_overlapping_boxes(&server).await;

    let legacy = rpc(&server, "validation.run", serde_json::json!({})).await;
    let diagnostics = legacy["result"].as_array().unwrap();
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic["code"] != "ICE3_STUDIO_GEOMETRY_OVERLAP"),
        "{legacy}"
    );

    let null_params = rpc(&server, "validation.run", serde_json::Value::Null).await;
    assert!(null_params["result"].as_array().is_some(), "{null_params}");
}

#[tokio::test]
async fn validation_run_reports_overlaps_when_requested() {
    let server = start_test_server(ServerConfig::review_required("validation-run-overlaps")).await;
    seed_overlapping_boxes(&server).await;

    let response = rpc(
        &server,
        "validation.run",
        serde_json::json!({ "includeOverlaps": true }),
    )
    .await;
    let diagnostics = response["result"].as_array().unwrap();
    let overlaps: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "ICE3_STUDIO_GEOMETRY_OVERLAP")
        .collect();
    assert_eq!(overlaps.len(), 1, "{response}");
    assert_eq!(overlaps[0]["objectId"], "node-000001|node-000002");
    assert!(
        overlaps[0]["message"]
            .as_str()
            .unwrap()
            .contains("spatial overlap between Skid A and Skid B")
    );
    assert_eq!(overlaps[0]["pointer"], "/scene");
}

#[tokio::test]
async fn validation_run_rejects_unknown_fields() {
    let server = start_test_server(ServerConfig::review_required("validation-run-invalid")).await;
    let response = rpc(
        &server,
        "validation.run",
        serde_json::json!({ "includeOverlaps": false, "includeEverything": true }),
    )
    .await;
    assert_eq!(
        response["error"]["diagnosticCode"], "ICE3_STUDIO_API_INVALID",
        "{response}"
    );
}

async fn seed_overlapping_boxes(server: &TestServer) {
    let response = rpc_with_approval_cookie(
        server,
        "document.apply",
        serde_json::json!({
            "expectedRevision": 0,
            "dryRun": false,
            "idempotencyKey": "validation-overlap-seed",
            "operations": [
                {
                    "type": "node.create",
                    "node_id": "node-000001",
                    "parent_id": null,
                    "name": "Skid A",
                    "transform": {
                        "translation": { "x": 0, "y": 0, "z": 0 },
                        "rotation": [0, 0, 0, 1],
                        "scale": { "x": 1, "y": 1, "z": 1 }
                    },
                    "content": {
                        "type": "primitive",
                        "kind": "box",
                        "width": 2.0,
                        "height": 2.0,
                        "depth": 2.0
                    },
                    "material_id": null,
                    "metadata": {}
                },
                {
                    "type": "node.create",
                    "node_id": "node-000002",
                    "parent_id": null,
                    "name": "Skid B",
                    "transform": {
                        "translation": { "x": 1, "y": 0, "z": 0 },
                        "rotation": [0, 0, 0, 1],
                        "scale": { "x": 1, "y": 1, "z": 1 }
                    },
                    "content": {
                        "type": "primitive",
                        "kind": "box",
                        "width": 2.0,
                        "height": 2.0,
                        "depth": 2.0
                    },
                    "material_id": null,
                    "metadata": {}
                }
            ]
        }),
    )
    .await;
    assert_eq!(response["result"]["revision"], 1, "{response}");
}
