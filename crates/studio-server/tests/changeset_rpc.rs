use studio_server::{ServerConfig, TestServer, start_test_server};

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
