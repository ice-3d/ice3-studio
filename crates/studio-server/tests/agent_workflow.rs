mod common;

use studio_server::{ServerConfig, TestServer, start_test_server};

use common::{rpc_with_approval_cookie, rpc_with_bearer};

#[tokio::test]
async fn agent_inspects_proposes_and_waits_for_human_approval() {
    let server = start_test_server(ServerConfig::review_required("agent-workflow")).await;
    let project_id = "agent-workflow";
    seed_instance(&server).await;

    let manifest = agent_rpc(&server, "system.agentManifest", serde_json::json!({})).await;
    assert_eq!(
        manifest["result"]["access"]["canCommit"], false,
        "{manifest}"
    );

    let context = agent_rpc(&server, "scene.context", serde_json::json!({})).await;
    assert_eq!(context["result"]["counts"]["instances"], 1, "{context}");

    let search = agent_rpc(
        &server,
        "scene.search",
        serde_json::json!({ "query": "Pump" }),
    )
    .await;
    assert_eq!(
        search["result"]["items"][0]["id"], "node-000001",
        "{search}"
    );

    let geometry = agent_rpc(
        &server,
        "evaluation.query",
        serde_json::json!({ "nodeIds": ["node-000001/body"] }),
    )
    .await;
    assert_eq!(geometry["result"]["items"][0]["vertexCount"], 24);

    let created = agent_rpc(
        &server,
        "changeset.create",
        serde_json::json!({
            "baseRevision": 1,
            "title": "Agent pump update",
            "idempotencyKey": "agent-workflow-create"
        }),
    )
    .await;
    let change_set_id = created["result"]["id"].as_str().unwrap();
    assert_eq!(created["result"]["version"], 0, "{created}");

    let added = agent_rpc(
        &server,
        "changeset.addOperations",
        serde_json::json!({
            "changeSetId": change_set_id,
            "expectedVersion": 0,
            "idempotencyKey": "agent-workflow-add",
            "entries": [{
                "source": { "kind": "agent", "label": "test-agent" },
                "operation": {
                    "type": "instance.patch",
                    "nodeId": "node-000001",
                    "setParameters": {
                        "capacity": 6.0
                    },
                    "setMetadata": { "agentReviewed": "true" },
                    "clearMaterial": false
                }
            }]
        }),
    )
    .await;
    assert_eq!(added["result"]["version"], 1, "{added}");

    let evaluated = agent_rpc(
        &server,
        "changeset.evaluate",
        serde_json::json!({
            "changeSetId": change_set_id,
            "expectedVersion": 1,
            "includeCandidate": false
        }),
    )
    .await;
    assert_eq!(
        evaluated["result"]["diff"]["updatedNodeIds"],
        serde_json::json!(["node-000001"]),
        "{evaluated}"
    );
    let candidate_digest = evaluated["result"]["candidateDigest"].as_str().unwrap();

    let denied = agent_rpc(
        &server,
        "changeset.commit",
        serde_json::json!({
            "changeSetId": change_set_id,
            "expectedVersion": 1,
            "approvedDigest": candidate_digest,
            "idempotencyKey": "agent-workflow-denied"
        }),
    )
    .await;
    assert_eq!(
        denied["error"]["diagnosticCode"], "ICE3_STUDIO_AUTH_REQUIRED",
        "{denied}"
    );

    let committed = rpc_with_approval_cookie(
        &server,
        "changeset.commit",
        serde_json::json!({
            "changeSetId": change_set_id,
            "expectedVersion": 1,
            "approvedDigest": candidate_digest,
            "idempotencyKey": "agent-workflow-approved"
        }),
    )
    .await;
    assert_eq!(committed["result"]["revision"], 2, "{committed}");

    let reopened = rpc_with_approval_cookie(
        &server,
        "project.open",
        serde_json::json!({ "projectId": project_id }),
    )
    .await;
    assert_eq!(reopened["result"]["opened"], true);
    let document = rpc_with_approval_cookie(&server, "document.get", serde_json::json!({})).await;
    let node = document["result"]["scene"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["id"] == "node-000001")
        .unwrap();
    assert_eq!(node["transform"]["translation"]["x"], 3.0);
    assert_eq!(node["metadata"]["agentReviewed"], "true");
    assert_eq!(node["content"]["parameters"]["capacity"], 6.0);
    assert_eq!(
        node["content"]["overrides"]["body"]["metadata"]["finish"],
        "paint"
    );
    assert_eq!(
        document["result"]["components"][0]["parts"][0]["id"],
        "body"
    );
}

async fn agent_rpc(
    server: &TestServer,
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    rpc_with_bearer(server, server.agent_token(), method, params).await
}

async fn seed_instance(server: &TestServer) {
    let response = rpc_with_approval_cookie(
        server,
        "document.apply",
        serde_json::json!({
            "expectedRevision": 0,
            "dryRun": false,
            "idempotencyKey": "agent-workflow-seed",
            "operations": [
                {
                    "type": "component.define",
                    "definition": {
                        "id": "component-000001",
                        "name": "Pump",
                        "parameters": [{
                            "name": "capacity",
                            "valueType": {
                                "kind": "number",
                                "minimum": 0,
                                "maximum": 10
                            },
                            "default": 4.0,
                            "unit": "m3/h"
                        }],
                        "parts": [{
                            "id": "body",
                            "parentId": null,
                            "transform": {},
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
                    }
                },
                {
                    "type": "instance.create",
                    "node_id": "node-000001",
                    "parent_id": null,
                    "name": "Pump A",
                    "transform": {
                        "translation": { "x": 3, "y": 0, "z": 0 },
                        "rotation": [0, 0, 0, 1],
                        "scale": { "x": 1, "y": 1, "z": 1 }
                    },
                    "instance": {
                        "definitionId": "component-000001",
                        "parameters": {
                            "capacity": 4.0
                        },
                        "overrides": {
                            "body": {
                                "materialId": null,
                                "visible": true,
                                "metadata": { "finish": "paint" },
                                "parameters": {}
                            }
                        },
                        "metadata": {}
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
