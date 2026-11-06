mod common;

use studio_server::changeset::ChangeSetEvaluateParams;
use studio_server::{ServerConfig, TestServer, start_test_server, start_test_server_at};
use ts_rs::TS;

use common::{rpc_with_approval_cookie, rpc_with_bearer};

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

#[test]
fn evaluate_params_typescript_marks_defaulted_field_optional() {
    assert!(ChangeSetEvaluateParams::decl().contains("includeCandidate?: boolean"));
}

#[tokio::test]
async fn capabilities_advertise_iteration_impact() {
    let server = start_test_server(ServerConfig::for_tests("impact-capabilities")).await;
    let caps = rpc(&server, "system.capabilities", serde_json::json!({})).await;

    assert!(
        caps["result"]["methods"]
            .as_array()
            .unwrap()
            .iter()
            .any(|method| method == "changeset.impact"),
        "{caps}"
    );
}

#[tokio::test]
async fn stages_evaluates_and_commits_one_change_set() {
    let server = start_test_server(ServerConfig::for_tests("generic-project")).await;
    let created = rpc(
        &server,
        "changeset.create",
        serde_json::json!({
            "baseRevision": 0,
            "title": "Add assembly",
            "idempotencyKey": "add-assembly-create"
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
            "idempotencyKey": format!("add-{change_set_id}"),
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
        serde_json::json!({
            "baseRevision": 0,
            "title": title,
            "idempotencyKey": format!("create-{title}")
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
            "idempotencyKey": format!("add-{change_set_id}"),
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
    let server = start_test_server(ServerConfig::for_tests("candidate-preview")).await;
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
async fn unknown_candidate_preview_preserves_previous_candidate() {
    let server = start_test_server(ServerConfig::for_tests("candidate-failure")).await;
    let valid = evaluated_node_change_set(&server, "Valid", "Assembly A").await;
    let valid_digest = valid["result"]["candidateDigest"].as_str().unwrap();
    rpc(
        &server,
        "export.runtimePreview",
        serde_json::json!({ "candidateDigest": valid_digest }),
    )
    .await;

    let bad_digest = "not-a-registered-candidate";
    let response = rpc(
        &server,
        "export.runtimePreview",
        serde_json::json!({ "candidateDigest": bad_digest }),
    )
    .await;

    assert!(response["error"].is_object(), "{response}");
    let previous = reqwest::get(format!(
        "{}/previews/candidates/{valid_digest}",
        server.base_url()
    ))
    .await
    .unwrap();
    assert!(previous.status().is_success());
}

async fn create_and_evaluate_with_token(server: &TestServer, token: &str) -> serde_json::Value {
    // 每次调用必须拿到独立的变更集，幂等键因此要唯一。
    static NEXT_CREATE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let create_seq = NEXT_CREATE.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let created = rpc_with_bearer(
        server,
        token,
        "changeset.create",
        serde_json::json!({
            "baseRevision": 0,
            "title": "Protected proposal",
            "idempotencyKey": format!("protected-proposal-create-{create_seq}")
        }),
    )
    .await;
    let change_set_id = created["result"]["id"].as_str().unwrap();
    let added = rpc_with_bearer(
        server,
        token,
        "changeset.addOperations",
        serde_json::json!({
            "changeSetId": change_set_id,
            "expectedVersion": 0,
            "idempotencyKey": format!("add-{change_set_id}"),
            "entries": [{
                "source": { "kind": "agent" },
                "operation": {
                    "type": "node.create",
                    "node_id": "node-000001",
                    "parent_id": null,
                    "name": "Protected",
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
    let evaluated = rpc_with_bearer(
        server,
        token,
        "changeset.evaluate",
        serde_json::json!({
            "changeSetId": change_set_id,
            "expectedVersion": 1,
            "includeCandidate": false
        }),
    )
    .await;
    evaluated["result"].clone()
}

fn commit_params(evaluated: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "changeSetId": evaluated["changeSetId"],
        "expectedVersion": evaluated["version"],
        "approvedDigest": evaluated["candidateDigest"],
        "idempotencyKey": "approval"
    })
}

#[tokio::test]
async fn proposal_token_cannot_commit_or_apply_directly() {
    let server = start_test_server(ServerConfig::review_required("protected-project")).await;
    let evaluated = create_and_evaluate_with_token(&server, server.agent_token()).await;

    let commit = rpc_with_bearer(
        &server,
        server.agent_token(),
        "changeset.commit",
        commit_params(&evaluated),
    )
    .await;
    assert_eq!(
        commit["error"]["diagnosticCode"],
        "ICE3_STUDIO_AUTH_REQUIRED"
    );

    let direct = rpc_with_bearer(
        &server,
        server.agent_token(),
        "document.apply",
        serde_json::json!({
            "expectedRevision": 0,
            "dryRun": false,
            "idempotencyKey": "direct",
            "operations": [{
                "type": "node.delete",
                "node_id": "node-000001"
            }]
        }),
    )
    .await;
    assert_eq!(
        direct["error"]["diagnosticCode"],
        "ICE3_STUDIO_AUTH_REQUIRED"
    );
}

#[tokio::test]
async fn approval_cookie_commits_the_exact_candidate() {
    let server = start_test_server(ServerConfig::review_required("approved-project")).await;
    let evaluated = create_and_evaluate_with_token(&server, server.agent_token()).await;
    let committed =
        rpc_with_approval_cookie(&server, "changeset.commit", commit_params(&evaluated)).await;
    assert_eq!(committed["result"]["revision"], 1, "{committed}");
}

async fn commit_candidate(
    server: &TestServer,
    change_set: &serde_json::Value,
) -> serde_json::Value {
    rpc_with_approval_cookie(server, "changeset.commit", commit_params(change_set)).await
}

#[tokio::test]
async fn restart_restores_active_change_set_and_candidate() {
    let root = tempfile::tempdir().unwrap();
    let first = start_test_server_at(root.path()).await;
    let prepared = create_and_evaluate_with_token(&first, first.agent_token()).await;
    let change_set_id = prepared["changeSetId"].as_str().unwrap().to_owned();
    let version = prepared["version"].clone();
    let digest = prepared["candidateDigest"].clone();
    first.stop().await;

    let second = start_test_server_at(root.path()).await;
    let restored = rpc(
        &second,
        "changeset.get",
        serde_json::json!({ "changeSetId": change_set_id }),
    )
    .await;
    assert_eq!(restored["result"]["version"], version);
    assert_eq!(restored["result"]["candidateDigest"], digest);
}

#[tokio::test]
async fn concurrent_commits_from_one_base_have_one_winner() {
    let server = start_test_server(ServerConfig::review_required("concurrent")).await;
    let first = create_and_evaluate_with_token(&server, server.agent_token()).await;
    let second = create_and_evaluate_with_token(&server, server.agent_token()).await;

    let results = tokio::join!(
        commit_candidate(&server, &first),
        commit_candidate(&server, &second),
    );
    let successes = [&results.0, &results.1]
        .iter()
        .filter(|value| value["result"]["revision"] == 1)
        .count();
    assert_eq!(successes, 1);
}

#[tokio::test]
async fn empty_change_set_is_blocked_instead_of_ready() {
    let server = start_test_server(ServerConfig::review_required("empty-proposal")).await;
    let created = rpc_with_bearer(
        &server,
        server.agent_token(),
        "changeset.create",
        serde_json::json!({
            "baseRevision": 0,
            "title": "Empty",
            "idempotencyKey": "empty-proposal-create"
        }),
    )
    .await;
    let evaluated = rpc_with_bearer(
        &server,
        server.agent_token(),
        "changeset.evaluate",
        serde_json::json!({
            "changeSetId": created["result"]["id"],
            "expectedVersion": 0,
            "includeCandidate": false
        }),
    )
    .await;
    assert_eq!(evaluated["result"]["status"], "blocked");
    assert_eq!(
        evaluated["result"]["diagnostics"][0]["code"],
        "ICE3_STUDIO_OPERATION_INVALID"
    );
}

#[tokio::test]
async fn repeated_commit_returns_the_original_result() {
    let server = start_test_server(ServerConfig::review_required("idempotent-commit")).await;
    let evaluated = create_and_evaluate_with_token(&server, server.agent_token()).await;
    let params = commit_params(&evaluated);
    let first = rpc_with_approval_cookie(&server, "changeset.commit", params.clone()).await;
    let second = rpc_with_approval_cookie(&server, "changeset.commit", params).await;

    assert_eq!(first["result"], second["result"]);
    let current = rpc(&server, "document.get", serde_json::json!({})).await;
    assert_eq!(current["result"]["revision"], 1);
}

#[tokio::test]
async fn change_set_writes_replay_instead_of_repeating_side_effects() {
    let server = start_test_server(ServerConfig::review_required("idempotent-writes")).await;

    let create_params = serde_json::json!({
        "baseRevision": 0,
        "title": "Idempotent proposal",
        "idempotencyKey": "idem-create"
    });
    let first = rpc(&server, "changeset.create", create_params.clone()).await;
    let replay = rpc(&server, "changeset.create", create_params.clone()).await;
    assert_eq!(
        first["result"]["id"], replay["result"]["id"],
        "{first}\n{replay}"
    );

    let reused = rpc(
        &server,
        "changeset.create",
        serde_json::json!({
            "baseRevision": 0,
            "title": "A different proposal",
            "idempotencyKey": "idem-create"
        }),
    )
    .await;
    assert_eq!(
        reused["error"]["diagnosticCode"], "ICE3_STUDIO_OPERATION_INVALID",
        "{reused}"
    );

    let change_set_id = first["result"]["id"].as_str().unwrap();
    let add_params = || {
        serde_json::json!({
            "changeSetId": change_set_id,
            "expectedVersion": 0,
            "idempotencyKey": "idem-add",
            "entries": [{
                "source": { "kind": "agent", "label": "idempotent" },
                "operation": {
                    "type": "node.create",
                    "nodeId": "node-000001",
                    "parentId": null,
                    "name": "Idempotent node",
                    "transform": {
                        "translation": { "x": 0, "y": 0, "z": 0 },
                        "rotation": [0, 0, 0, 1],
                        "scale": { "x": 1, "y": 1, "z": 1 }
                    },
                    "content": { "type": "group" },
                    "materialId": null,
                    "metadata": {}
                }
            }]
        })
    };
    let added = rpc(&server, "changeset.addOperations", add_params()).await;
    let retried = rpc(&server, "changeset.addOperations", add_params()).await;
    assert_eq!(added["result"]["version"], 1, "{added}");
    assert_eq!(retried["result"]["version"], 1, "{retried}");
    assert_eq!(
        added["result"]["entries"].as_array().unwrap().len(),
        retried["result"]["entries"].as_array().unwrap().len(),
        "a replayed add must not append a second entry:\n{added}\n{retried}"
    );

    let discard_params = serde_json::json!({
        "changeSetId": change_set_id,
        "idempotencyKey": "idem-discard"
    });
    let discarded =
        rpc_with_approval_cookie(&server, "changeset.discard", discard_params.clone()).await;
    let discarded_again =
        rpc_with_approval_cookie(&server, "changeset.discard", discard_params).await;
    assert_eq!(discarded["result"]["status"], "discarded", "{discarded}");
    assert_eq!(
        discarded_again["result"]["status"], "discarded",
        "{discarded_again}"
    );
}
