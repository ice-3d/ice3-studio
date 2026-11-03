mod common;

use studio_server::{ServerConfig, TestServer, start_test_server};

use common::rpc_with_approval_cookie;

#[tokio::test]
async fn component_get_returns_parameters_ports_and_part_summaries() {
    let server = start_test_server(ServerConfig::review_required("component-get-summary")).await;
    seed_rich_component(&server).await;

    let response = agent_rpc(
        &server,
        "component.get",
        serde_json::json!({ "componentId": "component-000001" }),
    )
    .await;
    let component = &response["result"]["component"];
    assert_eq!(response["result"]["revision"], 1, "{response}");
    assert_eq!(component["id"], "component-000001");
    assert_eq!(component["name"], "Pump station");

    let parameters = component["parameters"].as_array().unwrap();
    assert_eq!(parameters.len(), 2, "{response}");
    assert_eq!(parameters[0]["name"], "capacity");
    assert_eq!(parameters[0]["valueType"]["kind"], "number");
    assert_eq!(parameters[0]["valueType"]["minimum"], 0.0);
    assert_eq!(parameters[0]["valueType"]["maximum"], 10.0);
    assert_eq!(parameters[0]["default"], 4.0);
    assert_eq!(parameters[0]["unit"], "m3/h");
    assert_eq!(parameters[1]["valueType"]["kind"], "enumeration");
    assert_eq!(
        parameters[1]["valueType"]["values"],
        serde_json::json!(["horizontal", "vertical"])
    );

    let ports = component["ports"].as_array().unwrap();
    assert_eq!(ports.len(), 2, "{response}");
    assert_eq!(ports[0]["name"], "inlet");
    assert_eq!(ports[0]["partId"], "body");
    assert_eq!(ports[0]["medium"], "raw-water");
    assert_eq!(ports[0]["diameter"], 1.0);

    let parts = component["parts"].as_array().unwrap();
    assert_eq!(parts.len(), 2, "{response}");
    assert_eq!(parts[0]["id"], "body");
    assert_eq!(parts[0]["kind"], "box");
    assert_eq!(parts[0]["referencedDefinitionId"], serde_json::Value::Null);
    assert_eq!(parts[1]["id"], "cap");
    assert_eq!(parts[1]["parentId"], "body");
    assert_eq!(parts[1]["kind"], "component_reference");
    assert_eq!(parts[1]["referencedDefinitionId"], "component-000002");

    assert_eq!(component["partCount"], 2);
    assert_eq!(component["partsTruncated"], false);
    assert_eq!(component["instanceCount"], 2);
    assert_eq!(
        component["metadata"],
        serde_json::json!({ "family": "pumps" })
    );
}

#[tokio::test]
async fn component_get_truncates_part_summaries_beyond_the_limit() {
    let server = start_test_server(ServerConfig::review_required("component-get-truncate")).await;
    seed_wide_component(&server).await;

    let response = agent_rpc(
        &server,
        "component.get",
        serde_json::json!({ "componentId": "component-000001" }),
    )
    .await;
    let component = &response["result"]["component"];
    assert_eq!(component["partCount"], 150, "{response}");
    assert_eq!(component["partsTruncated"], true);
    assert_eq!(component["parts"].as_array().unwrap().len(), 100);
}

#[tokio::test]
async fn component_get_reports_unknown_id_as_structured_not_found() {
    let server = start_test_server(ServerConfig::review_required("component-get-missing")).await;
    let response = agent_rpc(
        &server,
        "component.get",
        serde_json::json!({ "componentId": "component-999999" }),
    )
    .await;
    assert_eq!(
        response["error"]["diagnosticCode"], "ICE3_STUDIO_REFERENCE_INVALID",
        "{response}"
    );
    assert_eq!(response["error"]["data"]["category"], "not_found");
    assert_eq!(response["error"]["data"]["retryable"], false);
    assert_eq!(response["error"]["data"]["objectId"], "component-999999");

    let empty = agent_rpc(
        &server,
        "component.get",
        serde_json::json!({ "componentId": "  " }),
    )
    .await;
    assert_eq!(
        empty["error"]["diagnosticCode"], "ICE3_STUDIO_API_INVALID",
        "{empty}"
    );
}

#[tokio::test]
async fn component_get_returns_the_full_definition_only_when_requested() {
    let server = start_test_server(ServerConfig::review_required("component-get-full")).await;
    seed_rich_component(&server).await;

    // 摘要模式：definition 键不出现。
    let summary = agent_rpc(
        &server,
        "component.get",
        serde_json::json!({ "componentId": "component-000001" }),
    )
    .await;
    assert_eq!(
        summary["result"]["definition"],
        serde_json::Value::Null,
        "{summary}"
    );

    // 完整模式：definition 与 component.update 的 payload 同构。
    let full = agent_rpc(
        &server,
        "component.get",
        serde_json::json!({ "componentId": "component-000001", "includeParts": true }),
    )
    .await;
    let definition = &full["result"]["definition"];
    assert_eq!(definition["id"], "component-000001", "{full}");
    let parts = definition["parts"].as_array().unwrap();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0]["id"], "body");
    assert_eq!(parts[0]["content"]["kind"], "box");
    assert_eq!(parts[0]["content"]["width"], 1.0);
    assert_eq!(parts[1]["content"]["definitionId"], "component-000002");
    assert_eq!(definition["ports"].as_array().unwrap().len(), 2);
    assert_eq!(definition["parameters"].as_array().unwrap().len(), 2);

    // 修改既有定义的完整工作流：读完整定义 → 加端口 → component.update → 生效。
    let mut updated = definition.clone();
    updated["ports"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "name": "drain",
            "partId": "body",
            "position": { "x": 0, "y": -1, "z": 0 },
            "direction": { "x": 0, "y": -1, "z": 0 },
            "medium": "raw-water",
            "diameter": 0.5
        }));
    let revision = summary["result"]["revision"].as_u64().unwrap();
    let apply = rpc_with_approval_cookie(
        &server,
        "document.apply",
        serde_json::json!({
            "expectedRevision": revision,
            "dryRun": false,
            "idempotencyKey": "component-add-port",
            "operations": [
                { "type": "component.update", "definition": updated }
            ]
        }),
    )
    .await;
    assert_eq!(apply["result"]["revision"], revision + 1, "{apply}");

    let after = agent_rpc(
        &server,
        "component.get",
        serde_json::json!({ "componentId": "component-000001" }),
    )
    .await;
    let ports = after["result"]["component"]["ports"].as_array().unwrap();
    assert_eq!(ports.len(), 3, "{after}");
    assert_eq!(ports[2]["name"], "drain");
    assert_eq!(ports[2]["partId"], "body");
}

#[tokio::test]
async fn component_get_is_available_to_read_only_callers() {
    let server = start_test_server(ServerConfig::review_required("component-get-read-only")).await;
    seed_rich_component(&server).await;

    let response = unauthenticated_rpc(
        &server,
        "component.get",
        serde_json::json!({ "componentId": "component-000001" }),
    )
    .await;
    assert_eq!(
        response["result"]["component"]["id"], "component-000001",
        "{response}"
    );

    let manifest =
        unauthenticated_rpc(&server, "system.agentManifest", serde_json::json!({})).await;
    let tool = manifest["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "component.get")
        .expect("manifest must advertise component.get");
    assert_eq!(tool["sideEffect"], "read");
    assert_eq!(tool["requiresApproval"], false);
}

async fn agent_rpc(
    server: &TestServer,
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    common::rpc_with_bearer(server, server.agent_token(), method, params).await
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

async fn seed_rich_component(server: &TestServer) {
    let response = rpc_with_approval_cookie(
        server,
        "document.apply",
        serde_json::json!({
            "expectedRevision": 0,
            "dryRun": false,
            "idempotencyKey": "component-get-seed",
            "operations": [
                {
                    "type": "component.define",
                    "definition": {
                        "id": "component-000001",
                        "name": "Pump station",
                        "parameters": [
                            {
                                "name": "capacity",
                                "valueType": {
                                    "kind": "number",
                                    "minimum": 0,
                                    "maximum": 10
                                },
                                "default": 4.0,
                                "unit": "m3/h"
                            },
                            {
                                "name": "orientation",
                                "valueType": {
                                    "kind": "enumeration",
                                    "values": ["horizontal", "vertical"]
                                },
                                "default": "horizontal",
                                "unit": null
                            }
                        ],
                        "parts": [
                            {
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
                            },
                            {
                                "id": "cap",
                                "parentId": "body",
                                "transform": {},
                                "content": {
                                    "kind": "component_reference",
                                    "definitionId": "component-000002",
                                    "parameters": {}
                                },
                                "materialRole": null,
                                "metadata": {},
                                "visible": true
                            }
                        ],
                        "ports": [
                            {
                                "name": "inlet",
                                "partId": "body",
                                "position": { "x": -1, "y": 0, "z": 0 },
                                "direction": { "x": 1, "y": 0, "z": 0 },
                                "medium": "raw-water",
                                "diameter": 1.0
                            },
                            {
                                "name": "outlet",
                                "partId": "body",
                                "position": { "x": 1, "y": 0, "z": 0 },
                                "direction": { "x": 1, "y": 0, "z": 0 },
                                "medium": "screened-water",
                                "diameter": 0.8
                            }
                        ],
                        "metadata": { "family": "pumps" }
                    }
                },
                {
                    "type": "component.define",
                    "definition": {
                        "id": "component-000002",
                        "name": "Cap",
                        "parameters": [],
                        "parts": [{
                            "id": "cap-body",
                            "parentId": null,
                            "transform": {},
                            "content": {
                                "kind": "cylinder",
                                "radius": 0.5,
                                "height": 0.2,
                                "segments": 12
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
                        "translation": { "x": 0, "y": 0, "z": 0 },
                        "rotation": [0, 0, 0, 1],
                        "scale": { "x": 1, "y": 1, "z": 1 }
                    },
                    "instance": {
                        "definitionId": "component-000001",
                        "parameters": { "capacity": 4.0 },
                        "overrides": {},
                        "metadata": {}
                    },
                    "material_id": null,
                    "metadata": {}
                },
                {
                    "type": "instance.create",
                    "node_id": "node-000002",
                    "parent_id": null,
                    "name": "Pump B",
                    "transform": {
                        "translation": { "x": 10, "y": 0, "z": 0 },
                        "rotation": [0, 0, 0, 1],
                        "scale": { "x": 1, "y": 1, "z": 1 }
                    },
                    "instance": {
                        "definitionId": "component-000001",
                        "parameters": { "capacity": 6.0 },
                        "overrides": {},
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

async fn seed_wide_component(server: &TestServer) {
    let parts = (0..150)
        .map(|index| {
            serde_json::json!({
                "id": format!("part-{index:03}"),
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
            })
        })
        .collect::<Vec<_>>();
    let response = rpc_with_approval_cookie(
        server,
        "document.apply",
        serde_json::json!({
            "expectedRevision": 0,
            "dryRun": false,
            "idempotencyKey": "component-get-wide",
            "operations": [
                {
                    "type": "component.define",
                    "definition": {
                        "id": "component-000001",
                        "name": "Wide assembly",
                        "parameters": [],
                        "parts": parts,
                        "ports": [],
                        "metadata": {}
                    }
                }
            ]
        }),
    )
    .await;
    assert_eq!(response["result"]["revision"], 1, "{response}");
}
