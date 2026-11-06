mod common;

use studio_server::{ServerConfig, TestServer, start_test_server};

use common::{rpc_with_approval_cookie, rpc_with_bearer};

#[tokio::test]
async fn impact_reuses_unchanged_part_geometry_for_local_patch() {
    let server = start_test_server(ServerConfig::review_required("impact-local")).await;
    seed_parameterized_scene(&server).await;
    warm_stats(&server).await;

    let changes =
        create_change_set_with_operations(&server, vec![instance_patch_transform("node-000001")])
            .await;
    let impact = rpc_with_bearer(
        &server,
        server.agent_token(),
        "changeset.impact",
        serde_json::json!({
            "changeSetId": changes["id"],
            "expectedVersion": changes["version"]
        }),
    )
    .await;

    assert_eq!(impact["result"]["impact"]["scope"], "local", "{impact}");
    assert_eq!(
        impact["result"]["impact"]["generatedGeometry"], 0,
        "{impact}"
    );
    assert!(
        impact["result"]["impact"]["cacheHits"]
            .as_u64()
            .unwrap_or_default()
            > 0,
        "{impact}"
    );
}

#[tokio::test]
async fn impact_reuses_geometry_for_an_unbound_parameter_change() {
    let server = start_test_server(ServerConfig::review_required("impact-parameter")).await;
    seed_parameterized_scene(&server).await;
    warm_stats(&server).await;

    let changes = create_change_set_with_operations(
        &server,
        vec![instance_patch_parameter("node-000001", "unused", 2.0)],
    )
    .await;
    let impact = request_impact(&server, &changes).await;

    assert_eq!(impact["scope"], "local", "{impact}");
    assert_eq!(impact["generatedGeometry"], 0, "{impact}");
    assert!(
        impact["cacheHits"].as_u64().unwrap_or_default() > 0,
        "{impact}"
    );
    assert_eq!(impact["reasons"][0], "instance_parameters", "{impact}");
}

#[tokio::test]
async fn relation_change_requires_full_scene_rebuild() {
    let server = start_test_server(ServerConfig::review_required("impact-relation")).await;
    seed_parameterized_scene(&server).await;
    warm_stats(&server).await;

    let changes =
        create_change_set_with_operations(&server, vec![relation_update("relation-000001")]).await;
    let impact = request_impact(&server, &changes).await;

    assert_eq!(impact["scope"], "full_rebuild", "{impact}");
    assert_eq!(impact["fullSceneRebuildRequired"], true, "{impact}");
    assert!(
        impact["reasons"]
            .as_array()
            .unwrap()
            .iter()
            .any(|reason| reason == "full_rebuild"),
        "{impact}"
    );
}

#[tokio::test]
async fn component_change_is_targeted_instead_of_local() {
    let server = start_test_server(ServerConfig::review_required("impact-component")).await;
    seed_parameterized_scene(&server).await;
    warm_stats(&server).await;

    let changes = create_change_set_with_operations(
        &server,
        vec![serde_json::json!({
            "type": "component.patch",
            "componentId": "component-000001",
            "name": "Renamed Parameterized Box"
        })],
    )
    .await;
    let impact = request_impact(&server, &changes).await;

    assert_eq!(impact["scope"], "targeted", "{impact}");
    assert_eq!(impact["components"]["updated"], 1, "{impact}");
    assert_eq!(impact["affectedSourceNodes"], 2, "{impact}");
    assert_eq!(impact["reasons"][0], "component_definition", "{impact}");
}

#[tokio::test]
async fn impact_digest_matches_evaluate_and_version_drift_fails_closed() {
    let server = start_test_server(ServerConfig::review_required("impact-digest")).await;
    seed_parameterized_scene(&server).await;
    warm_stats(&server).await;
    let changes =
        create_change_set_with_operations(&server, vec![instance_patch_transform("node-000001")])
            .await;

    let impact = request_impact_result(&server, &changes).await;
    let evaluated = rpc_with_bearer(
        &server,
        server.agent_token(),
        "changeset.evaluate",
        serde_json::json!({
            "changeSetId": changes["id"],
            "expectedVersion": changes["version"],
            "includeCandidate": false
        }),
    )
    .await;

    assert_eq!(
        impact["candidateDigest"], evaluated["result"]["candidateDigest"],
        "{impact}\n{evaluated}"
    );

    rpc_with_bearer(
        &server,
        server.agent_token(),
        "changeset.addOperations",
        serde_json::json!({
            "changeSetId": changes["id"],
            "expectedVersion": changes["version"],
            "idempotencyKey": "impact-drift-add",
            "entries": [{
                "source": { "kind": "agent", "label": "drift" },
                "operation": instance_patch_transform("node-000001")
            }]
        }),
    )
    .await;
    let stale = rpc_with_bearer(
        &server,
        server.agent_token(),
        "changeset.impact",
        serde_json::json!({
            "changeSetId": changes["id"],
            "expectedVersion": changes["version"]
        }),
    )
    .await;
    assert_eq!(
        stale["error"]["diagnosticCode"], "ICE3_STUDIO_OPERATION_INVALID",
        "{stale}"
    );
}

#[tokio::test]
async fn impact_after_project_revision_advance_is_stale_and_unavailable() {
    let server = start_test_server(ServerConfig::review_required("impact-stale-base")).await;
    seed_parameterized_scene(&server).await;
    warm_stats(&server).await;
    let changes =
        create_change_set_with_operations(&server, vec![instance_patch_transform("node-000001")])
            .await;

    let advanced = rpc_with_approval_cookie(
        &server,
        "document.apply",
        serde_json::json!({
            "expectedRevision": 1,
            "dryRun": false,
            "idempotencyKey": "impact-stale-base-advance",
            "operations": [instance_patch_transform("node-000002")]
        }),
    )
    .await;
    assert_eq!(advanced["result"]["revision"], 2, "{advanced}");

    let result = request_impact_result(&server, &changes).await;

    assert_eq!(result["status"], "stale", "{result}");
    assert_eq!(result["impact"]["scope"], "unavailable", "{result}");
    assert_eq!(result["impact"]["generatedGeometry"], 0, "{result}");
    assert_eq!(result["impact"]["affectedSourceNodes"], 0, "{result}");
    assert!(result["candidateDigest"].is_null(), "{result}");
}

#[tokio::test]
async fn blocked_candidate_impact_is_unavailable() {
    let server = start_test_server(ServerConfig::review_required("impact-blocked")).await;
    seed_parameterized_scene(&server).await;
    let changes = create_change_set_with_operations(&server, vec![out_of_range_parameter()]).await;
    let result = request_impact_result(&server, &changes).await;

    assert_eq!(result["status"], "blocked", "{result}");
    assert_eq!(result["impact"]["scope"], "unavailable", "{result}");
    assert_eq!(result["impact"]["generatedGeometry"], 0, "{result}");
    assert!(result["candidateDigest"].is_null(), "{result}");
}

#[tokio::test]
async fn repeated_impact_analysis_does_not_warm_the_committed_scene_cache() {
    let server = start_test_server(ServerConfig::review_required("impact-no-cache-write")).await;
    seed_parameterized_scene(&server).await;
    warm_stats(&server).await;
    let changes = create_change_set_with_operations(
        &server,
        vec![instance_patch_parameter("node-000001", "width", 3.0)],
    )
    .await;

    let first = request_impact(&server, &changes).await;
    let second = request_impact(&server, &changes).await;

    assert_eq!(first["generatedGeometry"], 1, "{first}");
    assert_eq!(first["cacheMisses"], 1, "{first}");
    assert_eq!(
        first["generatedGeometry"], second["generatedGeometry"],
        "{first}\n{second}"
    );
    assert_eq!(first["cacheHits"], second["cacheHits"], "{first}\n{second}");
    assert_eq!(
        first["cacheMisses"], second["cacheMisses"],
        "{first}\n{second}"
    );
}

async fn request_impact(server: &TestServer, change_set: &serde_json::Value) -> serde_json::Value {
    request_impact_result(server, change_set).await["impact"].clone()
}

async fn request_impact_result(
    server: &TestServer,
    change_set: &serde_json::Value,
) -> serde_json::Value {
    let response = rpc_with_bearer(
        server,
        server.agent_token(),
        "changeset.impact",
        serde_json::json!({
            "changeSetId": change_set["id"],
            "expectedVersion": change_set["version"]
        }),
    )
    .await;
    assert!(response.get("error").is_none(), "{response}");
    response["result"].clone()
}

async fn warm_stats(server: &TestServer) {
    rpc_with_approval_cookie(server, "evaluation.stats", serde_json::json!({})).await;
}

async fn create_change_set_with_operations(
    server: &TestServer,
    operations: Vec<serde_json::Value>,
) -> serde_json::Value {
    let created = rpc_with_bearer(
        server,
        server.agent_token(),
        "changeset.create",
        serde_json::json!({
            "baseRevision": 1,
            "title": "Impact fixture",
            "idempotencyKey": "impact-fixture-create"
        }),
    )
    .await;
    let added = rpc_with_bearer(
        server,
        server.agent_token(),
        "changeset.addOperations",
        serde_json::json!({
            "changeSetId": created["result"]["id"],
            "expectedVersion": created["result"]["version"],
            "idempotencyKey": format!(
                "{}-add",
                created["result"]["id"].as_str().unwrap_or("impact")
            ),
            "entries": operations.into_iter().map(|operation| {
                serde_json::json!({
                    "source": { "kind": "agent", "label": "impact-test" },
                    "operation": operation
                })
            }).collect::<Vec<_>>()
        }),
    )
    .await;
    assert!(
        created.get("error").is_none(),
        "changeset.create failed: {created}"
    );
    assert!(
        added.get("error").is_none(),
        "addOperations failed: {added}"
    );
    serde_json::json!({
        "id": added["result"]["id"],
        "version": added["result"]["version"]
    })
}

async fn seed_parameterized_scene(server: &TestServer) {
    let response = rpc_with_approval_cookie(
        server,
        "document.apply",
        serde_json::json!({
            "expectedRevision": 0,
            "dryRun": false,
            "idempotencyKey": "impact-seed",
            "operations": [
                {
                    "type": "component.define",
                    "definition": {
                        "id": "component-000001",
                        "name": "Parameterized Box",
                        "parameters": [
                            {
                                "name": "width",
                                "valueType": {
                                    "kind": "number",
                                    "minimum": 1,
                                    "maximum": 10
                                },
                                "default": 2,
                                "unit": "m"
                            },
                            {
                                "name": "unused",
                                "valueType": {
                                    "kind": "number",
                                    "minimum": 1,
                                    "maximum": 10
                                },
                                "default": 1,
                                "unit": null
                            }
                        ],
                        "parts": [{
                            "id": "body",
                            "parentId": null,
                            "transform": {},
                            "content": {
                                "kind": "box",
                                "width": { "param": "width" },
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
                instance_node("node-000001", 0),
                instance_node("node-000002", 4),
                {
                    "type": "relation.create",
                    "relation": {
                        "id": "relation-000001",
                        "relationType": "process",
                        "fromNodeId": "node-000001",
                        "toNodeId": "node-000002",
                        "metadata": {}
                    }
                }
            ]
        }),
    )
    .await;
    assert_eq!(response["result"]["revision"], 1, "{response}");
}

fn instance_node(node_id: &str, x: i32) -> serde_json::Value {
    serde_json::json!({
        "type": "instance.create",
        "node_id": node_id,
        "parent_id": null,
        "name": node_id,
        "transform": {
            "translation": { "x": x, "y": 0, "z": 0 },
            "rotation": [0, 0, 0, 1],
            "scale": { "x": 1, "y": 1, "z": 1 }
        },
        "instance": {
            "definitionId": "component-000001",
            "parameters": {
                "width": 2,
                "unused": 1
            },
            "overrides": {},
            "metadata": {}
        },
        "material_id": null,
        "metadata": {}
    })
}

fn instance_patch_transform(node_id: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "instance.patch",
        "nodeId": node_id,
        "transform": {
            "translation": { "x": 2, "y": 0, "z": 0 },
            "rotation": [0, 0, 0, 1],
            "scale": { "x": 1, "y": 1, "z": 1 }
        }
    })
}

fn instance_patch_parameter(node_id: &str, name: &str, value: f32) -> serde_json::Value {
    serde_json::json!({
        "type": "instance.patch",
        "nodeId": node_id,
        "setParameters": {
            name: value
        }
    })
}

fn relation_update(relation_id: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "relation.update",
        "relation_id": relation_id,
        "relation_type": "process",
        "from_node_id": null,
        "to_node_id": null,
        "metadata": null
    })
}

fn out_of_range_parameter() -> serde_json::Value {
    serde_json::json!({
        "type": "instance.patch",
        "nodeId": "node-000001",
        "setParameters": {
            "unused": 100
        }
    })
}
