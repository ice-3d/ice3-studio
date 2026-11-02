mod common;

use futures_util::StreamExt;
use studio_server::{ServerConfig, start_test_server};

#[tokio::test]
async fn health_reports_api_and_schema_versions() {
    let server = start_test_server(ServerConfig::for_tests("wwtp-mini")).await;
    let body: serde_json::Value = reqwest::get(server.health_url())
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["status"], "ok");
    assert_eq!(body["apiVersion"], "1.0");
    assert_eq!(body["schemaVersion"], 2);
}

#[tokio::test]
async fn write_requires_the_local_session_token() {
    let server = start_test_server(ServerConfig::for_tests("wwtp-mini")).await;
    let response = reqwest::Client::new()
        .post(server.rpc_url())
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "document.apply",
            "params": {}
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["error"]["diagnosticCode"], "ICE3_STUDIO_AUTH_REQUIRED");
}

#[tokio::test]
async fn unknown_methods_return_a_stable_json_rpc_error() {
    let server = start_test_server(ServerConfig::for_tests("wwtp-mini")).await;
    let response = reqwest::Client::new()
        .post(server.rpc_url())
        .bearer_auth(server.token())
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "unknown.method",
            "params": {}
        }))
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["error"]["code"], -32601);
}

#[tokio::test]
async fn accepts_bounded_large_authoring_requests() {
    let server = start_test_server(ServerConfig::for_tests("large-request")).await;
    let filler = "x".repeat(3 * 1024 * 1024);
    let response = reqwest::Client::new()
        .post(server.rpc_url())
        .bearer_auth(server.token())
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 31,
            "method": "document.apply",
            "params": {
                "expectedRevision": 0,
                "dryRun": false,
                "idempotencyKey": "large-request",
                "operations": [{
                    "type": "node.create",
                    "node_id": "node-000001",
                    "parent_id": null,
                    "name": "Large request",
                    "transform": {
                        "translation": { "x": 0, "y": 0, "z": 0 },
                        "rotation": [0, 0, 0, 1],
                        "scale": { "x": 1, "y": 1, "z": 1 }
                    },
                    "content": { "type": "group" },
                    "material_id": null,
                    "metadata": { "payload": filler }
                }]
            }
        }))
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["result"]["revision"], 1, "{body}");
}

#[tokio::test]
async fn publishes_a_revision_event_after_a_successful_commit() {
    let server = start_test_server(ServerConfig::for_tests("wwtp-mini")).await;
    let (mut websocket, _) = tokio_tungstenite::connect_async(server.websocket_url())
        .await
        .unwrap();
    server.apply_fixture_batch().await.unwrap();
    let message = tokio::time::timeout(std::time::Duration::from_secs(1), websocket.next())
        .await
        .unwrap()
        .unwrap();
    let message = message.unwrap();
    let body: serde_json::Value = serde_json::from_str(message.to_text().unwrap()).unwrap();
    assert_eq!(body["revision"], 1);
    assert_eq!(body["event"], "revision.committed");
}

#[tokio::test]
async fn publishes_generated_api_schemas() {
    let server = start_test_server(ServerConfig::for_tests("wwtp-mini")).await;
    let schema = reqwest::get(server.schema_url("studio-api.schema.json"))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(schema.contains("\"apiVersion\": \"1.0\""));
    assert!(schema.contains("node.create"));
    assert!(schema.contains("evaluation.stats"));
}

#[tokio::test]
async fn persistence_failure_keeps_the_previous_in_memory_revision() {
    let server = start_test_server(ServerConfig::for_tests("wwtp-mini")).await;
    std::fs::create_dir(
        server
            .workspace_root()
            .join("wwtp-mini/revisions/00000001.json"),
    )
    .unwrap();

    let failed: serde_json::Value = reqwest::Client::new()
        .post(server.rpc_url())
        .bearer_auth(server.token())
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "document.apply",
            "params": {
                "expectedRevision": 0,
                "dryRun": false,
                "idempotencyKey": "persistence-failure",
                "operations": [{
                    "type": "node.create",
                    "node_id": "node-000001",
                    "parent_id": null,
                    "name": "Probe",
                    "transform": {
                        "translation": { "x": 0, "y": 0, "z": 0 },
                        "rotation": [0, 0, 0, 1],
                        "scale": { "x": 1, "y": 1, "z": 1 }
                    },
                    "content": { "type": "group" },
                    "material_id": null,
                    "metadata": {}
                }]
            }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(failed["error"]["code"], -32011);

    let document: serde_json::Value = reqwest::Client::new()
        .post(server.rpc_url())
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "document.get",
            "params": {}
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(document["result"]["revision"], 0);
    assert_eq!(
        document["result"]["scene"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
}

#[tokio::test]
async fn closing_a_project_releases_it_and_allows_reopen() {
    let server = start_test_server(ServerConfig::for_tests("wwtp-mini")).await;
    let client = reqwest::Client::new();

    let closed: serde_json::Value = client
        .post(server.rpc_url())
        .bearer_auth(server.token())
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 6,
            "method": "project.close",
            "params": {}
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(closed["result"]["closed"], true);

    let unavailable: serde_json::Value = client
        .post(server.rpc_url())
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "document.get",
            "params": {}
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(unavailable["error"]["code"], -32010);

    let reopened: serde_json::Value = client
        .post(server.rpc_url())
        .bearer_auth(server.token())
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 8,
            "method": "project.open",
            "params": { "projectId": "wwtp-mini" }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(reopened["result"]["opened"], true);
}

#[tokio::test]
async fn failed_open_keeps_the_current_project_active() {
    let server = start_test_server(ServerConfig::for_tests("wwtp-mini")).await;
    let client = reqwest::Client::new();
    let failed: serde_json::Value = client
        .post(server.rpc_url())
        .bearer_auth(server.token())
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 14,
            "method": "project.open",
            "params": { "projectId": "missing-project" }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        failed["error"]["diagnosticCode"],
        "ICE3_STUDIO_PROJECT_NOT_FOUND"
    );

    let current: serde_json::Value = client
        .post(server.rpc_url())
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 15,
            "method": "document.get",
            "params": {}
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(current["result"]["projectId"], "wwtp-mini");
}

#[tokio::test]
async fn malformed_json_returns_a_structured_json_rpc_error() {
    let server = start_test_server(ServerConfig::for_tests("wwtp-mini")).await;
    let response = reqwest::Client::new()
        .post(server.rpc_url())
        .header("content-type", "application/json")
        .body("{")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["error"]["code"], -32700);
    assert_eq!(body["id"], serde_json::Value::Null);
}

#[tokio::test]
async fn structurally_invalid_requests_use_invalid_request_code() {
    let server = start_test_server(ServerConfig::for_tests("wwtp-mini")).await;
    let body: serde_json::Value = reqwest::Client::new()
        .post(server.rpc_url())
        .json(&serde_json::json!({ "jsonrpc": "2.0" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["error"]["code"], -32600);
    assert_eq!(body["error"]["diagnosticCode"], "ICE3_STUDIO_API_INVALID");
}

#[tokio::test]
async fn rejects_an_unsupported_json_rpc_version() {
    let server = start_test_server(ServerConfig::for_tests("wwtp-mini")).await;
    let body: serde_json::Value = reqwest::Client::new()
        .post(server.rpc_url())
        .json(&serde_json::json!({
            "jsonrpc": "1.0",
            "id": 9,
            "method": "document.get",
            "params": {}
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["error"]["code"], -32600);
    assert_eq!(body["error"]["diagnosticCode"], "ICE3_STUDIO_API_INVALID");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("ICE3_STUDIO_API_INVALID")
    );
}

#[tokio::test]
async fn revision_conflicts_expose_a_stable_diagnostic_code() {
    let server = start_test_server(ServerConfig::for_tests("wwtp-mini")).await;
    server.apply_fixture_batch().await.unwrap();
    let body: serde_json::Value = reqwest::Client::new()
        .post(server.rpc_url())
        .bearer_auth(server.token())
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 10,
            "method": "document.apply",
            "params": {
                "expectedRevision": 0,
                "dryRun": false,
                "idempotencyKey": "stale",
                "operations": [{
                    "type": "node.delete",
                    "node_id": "node-000001"
                }]
            }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        body["error"]["diagnosticCode"],
        "ICE3_STUDIO_REVISION_CONFLICT"
    );
}

#[tokio::test]
async fn scene_query_filters_by_metadata_and_node_id() {
    let server = start_test_server(ServerConfig::for_tests("wwtp-mini")).await;
    let applied: serde_json::Value = reqwest::Client::new()
        .post(server.rpc_url())
        .bearer_auth(server.token())
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 11,
            "method": "document.apply",
            "params": {
                "expectedRevision": 0,
                "dryRun": false,
                "idempotencyKey": "query-fixture",
                "operations": [
                    {
                        "type": "node.create",
                        "node_id": "node-000001",
                        "parent_id": null,
                        "name": "Pump",
                        "transform": {
                            "translation": { "x": -1, "y": 0, "z": 0 },
                            "rotation": [0, 0, 0, 1],
                            "scale": { "x": 1, "y": 1, "z": 1 }
                        },
                        "content": { "type": "group" },
                        "material_id": null,
                        "metadata": { "role": "pump" }
                    },
                    {
                        "type": "node.create",
                        "node_id": "node-000002",
                        "parent_id": null,
                        "name": "Tank",
                        "transform": {
                            "translation": { "x": 1, "y": 0, "z": 0 },
                            "rotation": [0, 0, 0, 1],
                            "scale": { "x": 1, "y": 1, "z": 1 }
                        },
                        "content": { "type": "group" },
                        "material_id": null,
                        "metadata": { "role": "tank" }
                    }
                ]
            }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(applied["result"]["revision"], 1);

    let client = reqwest::Client::new();
    let by_metadata: serde_json::Value = client
        .post(server.rpc_url())
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 12,
            "method": "scene.query",
            "params": { "metadata": { "role": "pump" } }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(by_metadata["result"].as_array().unwrap().len(), 1);
    assert_eq!(by_metadata["result"][0]["id"], "node-000001");

    let by_id: serde_json::Value = client
        .post(server.rpc_url())
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 13,
            "method": "scene.query",
            "params": { "nodeId": "node-000002" }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(by_id["result"].as_array().unwrap().len(), 1);
    assert_eq!(by_id["result"][0]["id"], "node-000002");
}

#[tokio::test]
async fn bootstrap_sets_an_http_only_approval_cookie_once() {
    let server = start_test_server(ServerConfig::review_required("secure-project")).await;
    let url = format!(
        "{}/session/bootstrap?token=test-bootstrap-token",
        server.base_url()
    );
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let first = client.get(&url).send().await.unwrap();
    assert_eq!(first.status(), 303);
    let cookie = first.headers()["set-cookie"].to_str().unwrap();
    assert!(cookie.contains("ice3_studio_approval=test-approval-token"));
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("SameSite=Strict"));

    let second = client.get(&url).send().await.unwrap();
    assert_eq!(second.status(), 403);
}

#[tokio::test]
async fn serves_built_workbench_and_api_from_one_origin() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("assets")).unwrap();
    std::fs::write(
        root.path().join("index.html"),
        "<!doctype html><script src=\"/assets/main.js\"></script>",
    )
    .unwrap();
    std::fs::write(
        root.path().join("assets/main.js"),
        "console.log('workbench')",
    )
    .unwrap();
    let server = start_test_server(ServerConfig {
        viewer_dir: Some(root.path().to_path_buf()),
        ..ServerConfig::for_tests("generic-project")
    })
    .await;

    let index = reqwest::get(server.base_url()).await.unwrap();
    assert!(index.text().await.unwrap().contains("/assets/main.js"));
    let asset = reqwest::get(format!("{}/assets/main.js", server.base_url()))
        .await
        .unwrap();
    assert!(asset.text().await.unwrap().contains("workbench"));
}
