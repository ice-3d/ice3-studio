#![allow(
    dead_code,
    reason = "integration tests use different subsets of shared helpers"
)]

use studio_server::TestServer;

pub async fn rpc(
    server: &TestServer,
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    rpc_with_bearer(server, server.token(), method, params).await
}

pub async fn create_and_evaluate_candidate(server: &TestServer) -> serde_json::Value {
    // 每次调用必须拿到独立的变更集，幂等键因此要唯一。
    static NEXT_CREATE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let create_seq = NEXT_CREATE.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let created = rpc(
        server,
        "changeset.create",
        serde_json::json!({
            "baseRevision": 0,
            "title": "candidate preview",
            "idempotencyKey": format!("candidate-preview-create-{create_seq}")
        }),
    )
    .await;
    let change_set_id = created["result"]["id"].as_str().unwrap();
    rpc(
        server,
        "changeset.addOperations",
        serde_json::json!({
            "changeSetId": change_set_id,
            "expectedVersion": 0,
            "idempotencyKey": format!("candidate-preview-add-{change_set_id}"),
            "entries": [{
                "source": { "kind": "agent", "label": "native-preview" },
                "operation": {
                    "type": "node.create",
                    "node_id": "node-000001",
                    "parent_id": null,
                    "name": "Candidate tank",
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
    let evaluated = rpc(
        server,
        "changeset.evaluate",
        serde_json::json!({
            "changeSetId": change_set_id,
            "expectedVersion": 1,
            "includeCandidate": false
        }),
    )
    .await;
    assert_eq!(evaluated["result"]["status"], "ready");
    evaluated["result"].clone()
}

pub async fn rpc_with_bearer(
    server: &TestServer,
    token: &str,
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    reqwest::Client::new()
        .post(server.rpc_url())
        .bearer_auth(token)
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

pub async fn rpc_with_approval_cookie(
    server: &TestServer,
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    reqwest::Client::new()
        .post(server.rpc_url())
        .header(
            reqwest::header::COOKIE,
            format!("ice3_studio_approval={}", server.approval_token()),
        )
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
