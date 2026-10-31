mod common;

use std::fs;
use std::path::Path;

use common::{rpc_with_approval_cookie, rpc_with_bearer};
use reqwest::header::CONTENT_TYPE;
use serde_json::json;
use studio_server::{ServerConfig, TestServer, start_test_server};

#[tokio::test]
async fn registers_and_reads_a_reference_without_geometry_in_rpc() {
    let server = start_test_server(ServerConfig::review_required("reference-rpc")).await;
    write_reference_fixture(server.workspace_root());
    let registered = register(&server).await;
    assert_eq!(
        registered["result"]["descriptor"]["stats"]["equipment"], 204,
        "{registered}"
    );
    let asset_id = registered["result"]["assetId"].as_str().unwrap();
    let asset = rpc_with_bearer(
        &server,
        server.agent_token(),
        "referenceAsset.get",
        json!({ "assetId": asset_id }),
    )
    .await;

    assert_eq!(
        asset["result"]["descriptor"]["stats"]["equipment"], 204,
        "{asset}"
    );
    assert!(asset["result"].get("positions").is_none());
    assert!(asset.to_string().len() < 64 * 1024);
    assert!(
        !asset
            .to_string()
            .contains(server.workspace_root().to_str().unwrap())
    );
}

#[tokio::test]
async fn reference_registration_requires_approval_and_rejects_escape() {
    let server = start_test_server(ServerConfig::review_required("reference-auth")).await;
    let denied = rpc_with_bearer(
        &server,
        server.agent_token(),
        "referenceAsset.register",
        json!({
            "sourcePath": "../source.glb",
            "preparedPath": null,
            "tilesetPath": null,
            "equipmentManifestPath": "../equipment.json"
        }),
    )
    .await;
    assert_eq!(
        denied["error"]["diagnosticCode"], "ICE3_STUDIO_AUTH_REQUIRED",
        "{denied}"
    );

    write_reference_fixture(server.workspace_root());
    let approved = rpc_with_approval_cookie(
        &server,
        "referenceAsset.register",
        json!({
            "sourcePath": "../source.glb",
            "preparedPath": null,
            "tilesetPath": null,
            "equipmentManifestPath": "../equipment.json"
        }),
    )
    .await;
    assert_eq!(
        approved["error"]["diagnosticCode"], "ICE3_STUDIO_REFERENCE_INVALID",
        "{approved}"
    );
}

#[tokio::test]
async fn binding_pages_are_bounded_and_asset_routes_are_read_only() {
    let server = start_test_server(ServerConfig::review_required("reference-routes")).await;
    write_reference_fixture(server.workspace_root());
    let registered = register(&server).await;
    let asset_id = registered["result"]["assetId"].as_str().unwrap();

    let page = rpc_with_bearer(
        &server,
        server.agent_token(),
        "referenceAsset.bindings",
        json!({ "assetId": asset_id, "cursor": null, "limit": 100 }),
    )
    .await;
    assert_eq!(page["result"]["items"].as_array().unwrap().len(), 100);
    assert!(page["result"]["nextCursor"].is_string(), "{page}");

    let oversized = rpc_with_bearer(
        &server,
        server.agent_token(),
        "referenceAsset.bindings",
        json!({ "assetId": asset_id, "cursor": null, "limit": 101 }),
    )
    .await;
    assert_eq!(
        oversized["error"]["diagnosticCode"], "ICE3_STUDIO_BUDGET_EXCEEDED",
        "{oversized}"
    );

    let endpoint = rpc_with_bearer(
        &server,
        server.agent_token(),
        "referenceAsset.get",
        json!({ "assetId": asset_id }),
    )
    .await;
    let tileset_url = endpoint["result"]["tilesetUrl"].as_str().unwrap();
    let prepared_url = endpoint["result"]["preparedPreviewUrl"].as_str().unwrap();
    let client = reqwest::Client::new();

    let tileset = client
        .get(format!("{}{tileset_url}", server.base_url()))
        .send()
        .await
        .unwrap();
    assert_eq!(tileset.status(), reqwest::StatusCode::OK);
    assert_eq!(
        tileset.headers()[CONTENT_TYPE],
        "application/json; charset=utf-8"
    );
    let tileset_body: serde_json::Value = tileset.json().await.unwrap();
    assert_eq!(
        tileset_body["root"]["content"]["uri"],
        "tiled/coarse/root.glb"
    );

    let prepared = client
        .head(format!("{}{prepared_url}", server.base_url()))
        .send()
        .await
        .unwrap();
    assert_eq!(prepared.status(), reqwest::StatusCode::OK);
    assert_eq!(prepared.headers()[CONTENT_TYPE], "model/gltf-binary");

    let write = client
        .post(format!("{}{tileset_url}", server.base_url()))
        .send()
        .await
        .unwrap();
    assert_eq!(write.status(), reqwest::StatusCode::METHOD_NOT_ALLOWED);
}

async fn register(server: &TestServer) -> serde_json::Value {
    rpc_with_approval_cookie(
        server,
        "referenceAsset.register",
        json!({
            "sourcePath": "refs/source.glb",
            "preparedPath": "refs/prepared.glb",
            "tilesetPath": "refs/tiled",
            "equipmentManifestPath": "refs/equipment.json"
        }),
    )
    .await
}

fn write_reference_fixture(root: &Path) {
    fs::create_dir_all(root.join("refs/tiled")).unwrap();
    fs::write(
        root.join("refs/tiled/tileset.json"),
        br#"{"asset":{"version":"1.1"},"root":{"content":{"uri":"coarse/root.glb"}}}"#,
    )
    .unwrap();
    fs::write(
        root.join("refs/equipment.json"),
        serde_json::to_vec(&json!({
            "assetId": "ice3-virtual-wwtp",
            "equipment": (0..204).map(|index| {
                let tag = if index == 0 {
                    "AE-101".to_owned()
                } else {
                    format!("EQ-{index:03}")
                };
                json!({
                    "selectionId": format!("ice3-virtual-wwtp/equipment/{tag}"),
                    "tag": tag,
                    "name": format!("Equipment {index:03}"),
                    "stage": "biological",
                    "area": "BiologicalTrainA",
                    "bounds": {
                        "min": [index as f64, 0.0, 0.0],
                        "max": [index as f64 + 2.0, 2.0, 2.0]
                    }
                })
            }).collect::<Vec<_>>()
        }))
        .unwrap(),
    )
    .unwrap();
    write_glb(&root.join("refs/source.glb"));
    write_glb(&root.join("refs/prepared.glb"));
}

fn write_glb(path: &Path) {
    let document = json!({
        "asset": {
            "version": "2.0",
            "extras": {
                "ice3": {
                    "assetId": "ice3-virtual-wwtp",
                    "bounds": {
                        "min": [-625.0, -1.0, -390.0],
                        "max": [625.0, 31.0, 390.0]
                    },
                    "stats": {
                        "nodeCount": 1761,
                        "meshCount": 1632,
                        "materialCount": 75
                    }
                }
            }
        },
        "accessors": [
            { "count": 17_273_886, "componentType": 5125, "type": "SCALAR" }
        ],
        "meshes": [{
            "primitives": [{
                "attributes": { "POSITION": 0 },
                "indices": 0,
                "mode": 4
            }]
        }],
        "nodes": [{ "mesh": 0 }]
    });
    let mut json_bytes = serde_json::to_vec(&document).unwrap();
    while !json_bytes.len().is_multiple_of(4) {
        json_bytes.push(b' ');
    }
    let mut glb = Vec::new();
    glb.extend_from_slice(b"glTF");
    glb.extend_from_slice(&2u32.to_le_bytes());
    glb.extend_from_slice(&((20 + json_bytes.len()) as u32).to_le_bytes());
    glb.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
    glb.extend_from_slice(&0x4E4F_534A_u32.to_le_bytes());
    glb.extend_from_slice(&json_bytes);
    fs::write(path, glb).unwrap();
}
