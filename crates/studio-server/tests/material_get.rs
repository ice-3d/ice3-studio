mod common;

use studio_server::{ServerConfig, TestServer, start_test_server};

use common::{rpc_with_approval_cookie, rpc_with_bearer};

async fn seed_materials(server: &TestServer) {
    let response = rpc_with_approval_cookie(
        server,
        "document.apply",
        serde_json::json!({
            "expectedRevision": 0,
            "dryRun": false,
            "idempotencyKey": "material-get-fixture",
            "operations": [
                {
                    "type": "material.upsert",
                    "materialId": "material-000001",
                    "name": "Brushed steel",
                    "baseColor": [0.31, 0.35, 0.37, 1.0],
                    "opacity": 0.9,
                    "edgeColor": [0.1, 0.1, 0.1, 1.0],
                    "shading": "flat",
                    "surface": {
                        "metallic": 0.8,
                        "roughness": 0.25,
                        "emissive": [0.01, 0.0, 0.0],
                        "uvScale": [2.0, 3.0],
                        "baseColorTexture": "textures/steel.png",
                        "normalTexture": "textures/steel-normal.png",
                        "normalScale": 1.2,
                        "ormTexture": "textures/steel-orm.png"
                    }
                },
                {
                    "type": "material.upsert",
                    "materialId": "material-000002",
                    "name": "Paint",
                    "baseColor": [0.2, 0.3, 0.4, 1.0],
                    "opacity": 1.0,
                    "edgeColor": null
                }
            ]
        }),
    )
    .await;
    assert_eq!(response["result"]["revision"], 1, "{response}");
}

async fn agent_rpc(
    server: &TestServer,
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    rpc_with_bearer(server, server.agent_token(), method, params).await
}

fn assert_f32(value: &serde_json::Value, expected: f32) {
    assert_eq!(value.as_f64().unwrap() as f32, expected, "{value}");
}

#[tokio::test]
async fn material_get_returns_explicit_surface_and_effective_copy() {
    let server = start_test_server(ServerConfig::review_required("material-get-surface")).await;
    seed_materials(&server).await;
    let revision =
        agent_rpc(&server, "document.get", serde_json::json!({})).await["result"]["revision"]
            .clone();

    let response = agent_rpc(
        &server,
        "material.get",
        serde_json::json!({ "materialId": "material-000001" }),
    )
    .await;
    assert_eq!(response["result"]["revision"], revision, "{response}");
    assert_eq!(response["result"]["material"]["name"], "Brushed steel");
    assert_f32(&response["result"]["material"]["baseColor"][1], 0.35);
    assert_f32(&response["result"]["material"]["edgeColor"][0], 0.1);
    assert_eq!(response["result"]["material"]["shading"], "flat");
    assert_f32(&response["result"]["material"]["surface"]["metallic"], 0.8);
    assert_eq!(
        response["result"]["effectiveSurface"],
        response["result"]["material"]["surface"]
    );

    let after = agent_rpc(&server, "document.get", serde_json::json!({})).await;
    assert_eq!(after["result"]["revision"], revision);
}

#[tokio::test]
async fn material_get_defaults_effective_surface_without_adding_one() {
    let server = start_test_server(ServerConfig::review_required("material-get-default")).await;
    seed_materials(&server).await;
    let response = agent_rpc(
        &server,
        "material.get",
        serde_json::json!({ "materialId": "material-000002" }),
    )
    .await;
    assert!(
        response["result"]["material"].get("surface").is_none(),
        "{response}"
    );
    assert_eq!(response["result"]["effectiveSurface"]["metallic"], 0.0);
    assert_f32(&response["result"]["effectiveSurface"]["roughness"], 0.9);
    assert_eq!(
        response["result"]["effectiveSurface"]["uvScale"],
        serde_json::json!([1.0, 1.0])
    );
}

#[tokio::test]
async fn material_get_errors_are_stable_and_non_retryable() {
    let server = start_test_server(ServerConfig::review_required("material-get-errors")).await;
    seed_materials(&server).await;

    let overlong = "x".repeat(257);
    for (material_id, category) in [
        ("material-999999", "not_found"),
        ("not-a-material-id", "invalid_id"),
        ("", "invalid_id"),
        (overlong.as_str(), "invalid_id"),
    ] {
        let response = agent_rpc(
            &server,
            "material.get",
            serde_json::json!({ "materialId": material_id }),
        )
        .await;
        assert_eq!(
            response["error"]["diagnosticCode"], "ICE3_STUDIO_REFERENCE_INVALID",
            "{response}"
        );
        assert_eq!(
            response["error"]["data"]["category"], category,
            "{response}"
        );
        assert_eq!(response["error"]["data"]["retryable"], false, "{response}");
    }
}

#[tokio::test]
async fn material_get_is_available_to_read_only_callers_and_advertised() {
    let server = start_test_server(ServerConfig::review_required("material-get-read")).await;
    seed_materials(&server).await;
    let response = reqwest::Client::new()
        .post(server.rpc_url())
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "material.get",
            "params": { "materialId": "material-000001" }
        }))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(
        response["result"]["material"]["id"], "material-000001",
        "{response}"
    );

    let manifest = agent_rpc(&server, "system.agentManifest", serde_json::json!({})).await;
    let tool = manifest["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "material.get")
        .expect("manifest must advertise material.get");
    assert_eq!(tool["sideEffect"], "read");
    assert_eq!(tool["requiresApproval"], false);
    assert_eq!(
        tool["inputSchemaRef"],
        "/schemas/studio-api.schema.json#/types/MaterialGetParams"
    );
    assert_eq!(
        tool["outputSchemaRef"],
        "/schemas/studio-api.schema.json#/types/MaterialGetResult"
    );
}
