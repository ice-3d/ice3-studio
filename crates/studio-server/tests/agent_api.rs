mod common;

use std::path::Path;

use studio_server::{ServerConfig, TestServer, start_test_server};

use common::{rpc_with_approval_cookie, rpc_with_bearer};

#[tokio::test]
async fn manifest_access_matches_request_authority() {
    let server = start_test_server(ServerConfig::review_required("agent-manifest-access")).await;
    let read_only =
        unauthenticated_rpc(&server, "system.agentManifest", serde_json::json!({})).await;
    assert_eq!(
        read_only["result"]["access"]["canRead"], true,
        "{read_only}"
    );
    assert_eq!(read_only["result"]["access"]["canPropose"], false);
    assert_eq!(read_only["result"]["access"]["canCommit"], false);

    let agent = rpc_with_bearer(
        &server,
        server.agent_token(),
        "system.agentManifest",
        serde_json::json!({}),
    )
    .await;
    assert_eq!(agent["result"]["access"]["canRead"], true, "{agent}");
    assert_eq!(agent["result"]["access"]["canPropose"], true);
    assert_eq!(agent["result"]["access"]["canCommit"], false);
    assert_eq!(agent["result"]["access"]["canApplyDirectly"], false);

    let approval =
        rpc_with_approval_cookie(&server, "system.agentManifest", serde_json::json!({})).await;
    assert_eq!(approval["result"]["access"]["canRead"], true, "{approval}");
    assert_eq!(approval["result"]["access"]["canPropose"], true);
    assert_eq!(approval["result"]["access"]["canCommit"], true);
    assert_eq!(approval["result"]["access"]["canApplyDirectly"], true);

    let both =
        rpc_with_bearer_and_approval(&server, "system.agentManifest", serde_json::json!({})).await;
    assert_eq!(
        both["result"]["access"]["canCommit"], true,
        "approval capability must not be hidden by a simultaneous agent token: {both}"
    );

    let expected_tools = [
        "system.agentManifest",
        "scene.context",
        "scene.search",
        "evaluation.query",
        "changeset.create",
        "changeset.addOperations",
        "changeset.evaluate",
        "changeset.getCandidate",
        "changeset.commit",
        "document.apply",
        "instance.patch",
        "component.patch",
        "connection.patch",
        "material.patch",
    ];
    let tools = agent["result"]["tools"].as_array().unwrap();
    let schema: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/studio-api.schema.json"),
        )
        .unwrap(),
    )
    .unwrap();
    for expected in expected_tools {
        let tool = tools
            .iter()
            .find(|tool| tool["name"] == expected)
            .unwrap_or_else(|| panic!("missing tool {expected}: {agent}"));
        assert!(
            tool["inputSchemaRef"]
                .as_str()
                .is_some_and(|value| value.contains("#/types/")),
            "tool {expected} must reference a generated schema definition: {tool}"
        );
        for field in ["inputSchemaRef", "outputSchemaRef"] {
            let Some(reference) = tool[field].as_str() else {
                continue;
            };
            let type_name = reference.split("#/types/").nth(1).unwrap();
            assert!(
                schema["types"].get(type_name).is_some(),
                "tool {expected} references missing schema type {type_name}"
            );
        }
    }
}

#[tokio::test]
async fn autonomous_manifest_exposes_direct_write_access() {
    let server = start_test_server(ServerConfig::for_tests("agent-manifest-autonomous")).await;
    let manifest = rpc_with_bearer(
        &server,
        server.token(),
        "system.agentManifest",
        serde_json::json!({}),
    )
    .await;

    assert_eq!(
        manifest["result"]["access"]["canPropose"], true,
        "{manifest}"
    );
    assert_eq!(manifest["result"]["access"]["canCommit"], true);
    assert_eq!(manifest["result"]["access"]["canApplyDirectly"], true);
    assert_eq!(manifest["result"]["access"]["canDiscard"], true);
}

#[tokio::test]
async fn context_reports_bounded_scene_structure() {
    let server = start_test_server(ServerConfig::for_tests("agent-context")).await;
    let revision = apply_scene_fixture(&server).await;
    let context = rpc_with_bearer(
        &server,
        server.token(),
        "scene.context",
        serde_json::json!({}),
    )
    .await;

    assert_eq!(context["result"]["revision"], revision, "{context}");
    assert_eq!(context["result"]["units"], "meters");
    assert_eq!(context["result"]["coordinateSystem"]["handedness"], "right");
    assert_eq!(context["result"]["coordinateSystem"]["upAxis"], "Y");
    assert_eq!(context["result"]["counts"]["sourceNodes"], 2);
    assert_eq!(context["result"]["counts"]["derivedNodes"], 2);
    assert_eq!(context["result"]["counts"]["componentDefinitions"], 1);
    assert_eq!(context["result"]["counts"]["instances"], 2);
    assert_eq!(context["result"]["counts"]["instanceSets"], 0);
    assert_eq!(context["result"]["access"]["canRead"], true);
    assert_eq!(context["result"]["limits"]["searchDefaultLimit"], 25);
    assert_eq!(
        context["result"]["facets"]["contentKinds"],
        serde_json::json!([{ "kind": "component_instance", "count": 2 }])
    );
    assert!(context["result"]["bounds"].is_object());
}

#[tokio::test]
async fn source_search_filters_and_pages_deterministically() {
    let server = start_test_server(ServerConfig::for_tests("agent-source-search")).await;
    let revision = apply_scene_fixture(&server).await;

    let by_name = search(
        &server,
        serde_json::json!({
            "query": "pump",
            "limit": 25
        }),
    )
    .await;
    assert_eq!(by_name["result"]["revision"], revision, "{by_name}");
    assert_eq!(by_name["result"]["items"].as_array().unwrap().len(), 2);

    let by_metadata = search(
        &server,
        serde_json::json!({
            "metadata": { "area": "A" }
        }),
    )
    .await;
    assert_eq!(by_metadata["result"]["items"].as_array().unwrap().len(), 1);
    assert_eq!(by_metadata["result"]["items"][0]["id"], "node-000001");

    let first = search(
        &server,
        serde_json::json!({
            "query": "pump",
            "limit": 1,
            "fields": ["id", "scope", "kind", "name"]
        }),
    )
    .await;
    let items = first["result"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], "node-000001");
    assert!(items[0].get("metadata").is_none());
    let cursor = first["result"]["nextCursor"].as_str().unwrap();
    let second = search(
        &server,
        serde_json::json!({
            "query": "pump",
            "limit": 1,
            "fields": ["id", "scope", "kind", "name"],
            "cursor": cursor
        }),
    )
    .await;
    assert_eq!(
        second["result"]["items"][0]["id"], "node-000002",
        "{second}"
    );
    assert!(second["result"]["nextCursor"].is_null());
}

#[tokio::test]
async fn source_search_rejects_query_and_revision_drift() {
    let server = start_test_server(ServerConfig::for_tests("agent-search-drift")).await;
    let revision = apply_scene_fixture(&server).await;
    let first = search(
        &server,
        serde_json::json!({
            "query": "pump",
            "limit": 1
        }),
    )
    .await;
    let cursor = first["result"]["nextCursor"].as_str().unwrap();

    let mismatched = search(
        &server,
        serde_json::json!({
            "query": "changed",
            "limit": 1,
            "cursor": cursor
        }),
    )
    .await;
    assert_eq!(
        mismatched["error"]["diagnosticCode"], "ICE3_STUDIO_REVISION_CONFLICT",
        "{mismatched}"
    );

    rpc_with_bearer(
        &server,
        server.token(),
        "document.apply",
        serde_json::json!({
            "expectedRevision": revision,
            "dryRun": false,
            "idempotencyKey": "agent-search-drift-move",
            "operations": [{
                "type": "node.update",
                "node_id": "node-000001",
                "name": null,
                "transform": {
                    "translation": { "x": 5, "y": 0, "z": 0 },
                    "rotation": [0, 0, 0, 1],
                    "scale": { "x": 1, "y": 1, "z": 1 }
                },
                "visible": null,
                "metadata": null
            }]
        }),
    )
    .await;
    let stale = search(
        &server,
        serde_json::json!({
            "query": "pump",
            "limit": 1,
            "cursor": cursor
        }),
    )
    .await;
    assert_eq!(
        stale["error"]["diagnosticCode"], "ICE3_STUDIO_REVISION_CONFLICT",
        "{stale}"
    );
}

#[tokio::test]
async fn source_search_rejects_unknown_projection() {
    let server = start_test_server(ServerConfig::for_tests("agent-search-projection")).await;
    apply_scene_fixture(&server).await;
    let response = search(
        &server,
        serde_json::json!({
            "fields": ["id", "scope", "kind", "unknown"]
        }),
    )
    .await;
    assert_eq!(
        response["error"]["diagnosticCode"], "ICE3_STUDIO_API_INVALID",
        "{response}"
    );
}

#[tokio::test]
async fn derived_search_returns_world_node_identity_without_payloads() {
    let server = start_test_server(ServerConfig::for_tests("agent-derived-search")).await;
    let revision = apply_derived_fixture(&server).await;
    let response = search(
        &server,
        serde_json::json!({
            "scope": "derived",
            "query": "body",
            "limit": 25
        }),
    )
    .await;

    assert_eq!(response["result"]["revision"], revision, "{response}");
    let items = response["result"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["id"], "node-000001/body");
    assert_eq!(items[1]["id"], "node-000010/000000/body");
    assert_eq!(items[0]["ownerId"], "node-000001");
    assert_eq!(items[1]["ownerId"], "node-000010/000000");
    assert!(items[0]["bounds"].is_object());
    assert!(items[0].get("positions").is_none());
    assert!(items[0].get("indices").is_none());

    let both = search(
        &server,
        serde_json::json!({
            "scope": "both",
            "query": "pump",
            "limit": 25,
            "fields": ["id", "scope", "kind"]
        }),
    )
    .await;
    let both_items = both["result"]["items"].as_array().unwrap();
    assert_eq!(both_items.len(), 4);
    assert_eq!(both_items[0]["scope"], "derived");
    assert_eq!(both_items[1]["scope"], "derived");
    assert_eq!(both_items[2]["scope"], "source");
    assert_eq!(both_items[3]["scope"], "source");
}

#[tokio::test]
async fn evaluation_query_returns_requested_summaries_and_payloads() {
    let server = start_test_server(ServerConfig::for_tests("agent-evaluation-query")).await;
    apply_derived_fixture(&server).await;
    let node_ids = serde_json::json!(["node-000010/000000/body", "node-000001/body"]);

    let summary = evaluation_query(
        &server,
        serde_json::json!({
            "nodeIds": node_ids
        }),
    )
    .await;
    let items = summary["result"]["items"].as_array().unwrap();
    assert_eq!(items[0]["nodeId"], "node-000010/000000/body", "{summary}");
    assert_eq!(items[1]["nodeId"], "node-000001/body");
    assert_eq!(items[0]["vertexCount"], 24);
    assert_eq!(items[0]["indexCount"], 36);
    assert!(items[0]["geometryId"].is_string());
    assert!(items[0]["bounds"].is_object());
    assert!(items[0].get("positions").is_none());
    assert!(items[0].get("indices").is_none());

    let positions = evaluation_query(
        &server,
        serde_json::json!({
            "nodeIds": ["node-000001/body"],
            "include": ["summary", "positions"]
        }),
    )
    .await;
    let item = &positions["result"]["items"][0];
    assert_eq!(item["positions"].as_array().unwrap().len(), 24);
    assert!(item.get("indices").is_none());

    let full = evaluation_query(
        &server,
        serde_json::json!({
            "nodeIds": ["node-000001/body"],
            "include": ["summary", "positions", "indices"]
        }),
    )
    .await;
    let item = &full["result"]["items"][0];
    assert_eq!(item["positions"].as_array().unwrap().len(), 24);
    assert_eq!(item["indices"].as_array().unwrap().len(), 36);
}

#[tokio::test]
async fn evaluation_query_enforces_unique_bounded_existing_nodes() {
    let server = start_test_server(ServerConfig::for_tests("agent-evaluation-limits")).await;
    apply_derived_fixture(&server).await;

    let duplicate = evaluation_query(
        &server,
        serde_json::json!({
            "nodeIds": ["node-000001/body", "node-000001/body"]
        }),
    )
    .await;
    assert_eq!(
        duplicate["error"]["diagnosticCode"], "ICE3_STUDIO_API_INVALID",
        "{duplicate}"
    );

    let too_many = evaluation_query(
        &server,
        serde_json::json!({
            "nodeIds": (0..33).map(|index| format!("node-{index}")).collect::<Vec<_>>()
        }),
    )
    .await;
    assert_eq!(
        too_many["error"]["diagnosticCode"], "ICE3_STUDIO_API_INVALID",
        "{too_many}"
    );

    let unknown = evaluation_query(
        &server,
        serde_json::json!({
            "nodeIds": ["node-000001/body", "missing/body"]
        }),
    )
    .await;
    assert_eq!(
        unknown["error"]["diagnosticCode"], "ICE3_STUDIO_REFERENCE_INVALID",
        "{unknown}"
    );
}

async fn unauthenticated_rpc(
    server: &TestServer,
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    reqwest::Client::new()
        .post(server.rpc_url())
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

async fn evaluation_query(server: &TestServer, params: serde_json::Value) -> serde_json::Value {
    rpc_with_bearer(server, server.token(), "evaluation.query", params).await
}

async fn search(server: &TestServer, params: serde_json::Value) -> serde_json::Value {
    rpc_with_bearer(server, server.token(), "scene.search", params).await
}

async fn apply_scene_fixture(server: &TestServer) -> u64 {
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
    let response = rpc_with_bearer(
        server,
        server.token(),
        "document.apply",
        serde_json::json!({
            "expectedRevision": 0,
            "dryRun": false,
            "idempotencyKey": "agent-api-scene",
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
                    "metadata": { "area": "A" }
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
                    "metadata": { "area": "B" }
                }
            ]
        }),
    )
    .await;
    response["result"]["revision"].as_u64().unwrap()
}

async fn apply_derived_fixture(server: &TestServer) -> u64 {
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
    let response = rpc_with_bearer(
        server,
        server.token(),
        "document.apply",
        serde_json::json!({
            "expectedRevision": 0,
            "dryRun": false,
            "idempotencyKey": "agent-derived-fixture",
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
                        "translation": { "x": 4, "y": 0, "z": 0 },
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
    response["result"]["revision"].as_u64().unwrap()
}

async fn rpc_with_bearer_and_approval(
    server: &TestServer,
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    reqwest::Client::new()
        .post(server.rpc_url())
        .bearer_auth(server.agent_token())
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
