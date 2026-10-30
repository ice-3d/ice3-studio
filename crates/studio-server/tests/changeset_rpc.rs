mod common;

use studio_server::{ServerConfig, TestServer, start_test_server};

use common::{
    FAILED_TOOLCHAIN, SUCCESS_TOOLCHAIN, replace_toolchain_body, start_with_toolchain,
};

async fn rpc(server: &TestServer, method: &str, params: serde_json::Value) -> serde_json::Value {
    reqwest::Client::new()
        .post(server.rpc_url())
        .bearer_auth(server.token())
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

#[tokio::test]
async fn stages_evaluates_and_commits_one_change_set() {
    let server = start_test_server(ServerConfig::for_tests("generic-project")).await;
    let created = rpc(
        &server,
        "changeset.create",
        serde_json::json!({
            "baseRevision": 0,
            "title": "Add assembly"
        }),
    )
    .await;
    let change_set_id = created["result"]["id"].as_str().unwrap();

    let added = rpc(
        &server,
        "changeset.addOperations",
        serde_json::json!({
            "changeSetId": change_set_id,
            "expectedVersion": 0,
            "entries": [{
                "source": { "kind": "agent", "label": "test-agent" },
                "operation": {
                    "type": "node.create",
                    "node_id": "node-000001",
                    "parent_id": null,
                    "name": "Assembly",
                    "transform": {
                        "translation": { "x": 0, "y": 0, "z": 0 },
                        "rotation": [0, 0, 0, 1],
                        "scale": { "x": 1, "y": 1, "z": 1 }
                    },
                    "content": { "type": "group" },
                    "material_id": null,
                    "metadata": {}
                }
            }]
        }),
    )
    .await;
    assert_eq!(added["result"]["version"], 1);

    let evaluated = rpc(
        &server,
        "changeset.evaluate",
        serde_json::json!({
            "changeSetId": change_set_id,
            "expectedVersion": 1,
            "includeCandidate": true
        }),
    )
    .await;
    assert_eq!(evaluated["result"]["status"], "ready");
    let digest = evaluated["result"]["candidateDigest"].as_str().unwrap();

    let committed = rpc(
        &server,
        "changeset.commit",
        serde_json::json!({
            "changeSetId": change_set_id,
            "expectedVersion": 1,
            "approvedDigest": digest,
            "idempotencyKey": "approve-add-assembly"
        }),
    )
    .await;
    assert_eq!(committed["result"]["revision"], 1);

    let current = rpc(&server, "document.get", serde_json::json!({})).await;
    assert_eq!(
        current["result"]["scene"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

async fn evaluated_node_change_set(
    server: &TestServer,
    title: &str,
    node_name: &str,
) -> serde_json::Value {
    let created = rpc(
        server,
        "changeset.create",
        serde_json::json!({ "baseRevision": 0, "title": title }),
    )
    .await;
    let change_set_id = created["result"]["id"].as_str().unwrap();
    rpc(
        server,
        "changeset.addOperations",
        serde_json::json!({
            "changeSetId": change_set_id,
            "expectedVersion": 0,
            "entries": [{
                "source": { "kind": "agent", "label": "test-agent" },
                "operation": {
                    "type": "node.create",
                    "node_id": "node-000001",
                    "parent_id": null,
                    "name": node_name,
                    "transform": {
                        "translation": { "x": 0, "y": 0, "z": 0 },
                        "rotation": [0, 0, 0, 1],
                        "scale": { "x": 1, "y": 1, "z": 1 }
                    },
                    "content": { "type": "group" },
                    "material_id": null,
                    "metadata": {}
                }
            }]
        }),
    )
    .await;
    rpc(
        server,
        "changeset.evaluate",
        serde_json::json!({
            "changeSetId": change_set_id,
            "expectedVersion": 1,
            "includeCandidate": false
        }),
    )
    .await
}

#[tokio::test]
async fn caches_candidate_preview_by_digest() {
    let server = start_with_toolchain("candidate-preview", SUCCESS_TOOLCHAIN).await;
    let evaluated = evaluated_node_change_set(&server, "First", "Assembly A").await;
    let digest = evaluated["result"]["candidateDigest"].as_str().unwrap();

    let first = rpc(
        &server,
        "export.runtimePreview",
        serde_json::json!({ "candidateDigest": digest }),
    )
    .await;
    let second = rpc(
        &server,
        "export.runtimePreview",
        serde_json::json!({ "candidateDigest": digest }),
    )
    .await;

    assert_eq!(first["result"]["assetId"], second["result"]["assetId"]);
    assert_eq!(
        first["result"]["url"],
        format!("/previews/candidates/{digest}")
    );
}

#[tokio::test]
async fn failed_candidate_preview_preserves_previous_candidate() {
    let server = start_with_toolchain("candidate-failure", SUCCESS_TOOLCHAIN).await;
    let valid = evaluated_node_change_set(&server, "Valid", "Assembly A").await;
    let valid_digest = valid["result"]["candidateDigest"].as_str().unwrap();
    rpc(
        &server,
        "export.runtimePreview",
        serde_json::json!({ "candidateDigest": valid_digest }),
    )
    .await;

    replace_toolchain_body(&server, FAILED_TOOLCHAIN).await;
    let bad = evaluated_node_change_set(&server, "Bad", "Assembly B").await;
    let bad_digest = bad["result"]["candidateDigest"].as_str().unwrap();
    let response = rpc(
        &server,
        "export.runtimePreview",
        serde_json::json!({ "candidateDigest": bad_digest }),
    )
    .await;

    assert_eq!(
        response["error"]["diagnosticCode"],
        "ICE3_STUDIO_RUNTIME_PREVIEW_FAILED"
    );
    let previous = reqwest::get(format!(
        "{}/previews/candidates/{valid_digest}",
        server.base_url()
    ))
    .await
    .unwrap();
    assert!(previous.status().is_success());
}
