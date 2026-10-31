mod common;

use std::fs;
use std::path::Path;

use common::{rpc_with_approval_cookie, rpc_with_bearer};
use serde_json::json;
use studio_server::{ServerConfig, TestServer, start_test_server};

#[tokio::test]
async fn transform_and_material_edits_on_bindings_are_local() {
    let (server, asset_id) = reference_fixture_server("reference-edit").await;
    let committed = commit_operations(
        &server,
        vec![binding_create("node-000001", &asset_id, "AE-101")],
    )
    .await;
    assert_eq!(committed["result"]["revision"], 2, "{committed}");

    let edit = create_change_set(
        &server,
        vec![
            json!({
                "type": "node.update",
                "node_id": "node-000001",
                "name": null,
                "transform": {
                    "translation": { "x": 4, "y": 0, "z": 0 },
                    "rotation": [0, 0, 0, 1],
                    "scale": { "x": 1, "y": 1, "z": 1 }
                },
                "visible": null,
                "metadata": null
            }),
            json!({
                "type": "material.assign",
                "node_id": "node-000001",
                "material_id": "material-000001"
            }),
        ],
    )
    .await;
    let impact = request_impact(&server, &edit).await;
    assert_eq!(impact["scope"], "local", "{impact}");
    assert_eq!(impact["affectedSourceNodes"], 1, "{impact}");
    assert_eq!(impact["fullSceneRebuildRequired"], false, "{impact}");
    assert_eq!(impact["generatedGeometry"], 0, "{impact}");

    let committed = commit_evaluated(&server, &edit).await;
    assert_eq!(committed["result"]["revision"], 3, "{committed}");
    let denied =
        rpc_with_bearer(&server, server.agent_token(), "export.sourceGlb", json!({})).await;
    assert_eq!(
        denied["error"]["diagnosticCode"], "ICE3_STUDIO_REFERENCE_EXPORT_UNSUPPORTED",
        "{denied}"
    );
}

#[tokio::test]
async fn context_aggregates_reference_stats_without_inline_geometry() {
    let (server, asset_id) = reference_fixture_server("reference-context").await;
    let operations = (1..=204)
        .map(|index| {
            let tag = if index == 1 {
                "AE-101".to_owned()
            } else {
                format!("EQ-{index:03}")
            };
            binding_create(&format!("node-{index:06}"), &asset_id, &tag)
        })
        .collect();
    commit_operations(&server, operations).await;

    let context = rpc_with_bearer(&server, server.agent_token(), "scene.context", json!({})).await;
    assert_eq!(context["result"]["counts"]["sourceNodes"], 204, "{context}");
    assert_eq!(
        context["result"]["counts"]["referenceAssets"], 1,
        "{context}"
    );
    assert_eq!(
        context["result"]["counts"]["effectiveTriangles"], 5_757_962,
        "{context}"
    );
    assert_eq!(
        context["result"]["counts"]["uniqueMeshes"], 1632,
        "{context}"
    );
    assert!(context.to_string().len() < 16 * 1024);
    assert!(context.to_string().find("positions").is_none(), "{context}");

    let stats = rpc_with_bearer(&server, server.agent_token(), "evaluation.stats", json!({})).await;
    assert_eq!(stats["result"]["effectiveTriangles"], 5_757_962, "{stats}");
    assert_eq!(stats["result"]["uniqueMeshes"], 1632, "{stats}");
}

#[tokio::test]
async fn unknown_reference_assets_and_selection_ids_block_evaluation() {
    let (server, asset_id) = reference_fixture_server("reference-validation").await;
    for content in [
        binding_create("node-000001", "asset-999999", "AE-101"),
        binding_create("node-000001", &asset_id, "missing-equipment"),
    ] {
        let change_set = create_change_set(&server, vec![content]).await;
        let evaluated = rpc_with_bearer(
            &server,
            server.agent_token(),
            "changeset.evaluate",
            json!({
                "changeSetId": change_set["id"],
                "expectedVersion": change_set["version"],
                "includeCandidate": false
            }),
        )
        .await;
        assert_eq!(evaluated["result"]["status"], "blocked", "{evaluated}");
        assert_eq!(
            evaluated["result"]["diagnostics"][0]["code"], "ICE3_STUDIO_REFERENCE_INVALID",
            "{evaluated}"
        );
    }

    let rejected = rpc_with_approval_cookie(
        &server,
        "document.apply",
        json!({
            "expectedRevision": 1,
            "dryRun": false,
            "idempotencyKey": "invalid-reference-direct-apply",
            "operations": [binding_create("node-000001", "asset-999999", "AE-101")]
        }),
    )
    .await;
    assert_eq!(
        rejected["error"]["diagnosticCode"], "ICE3_STUDIO_REFERENCE_INVALID",
        "{rejected}"
    );
    let document = rpc_with_bearer(&server, server.agent_token(), "document.get", json!({})).await;
    assert_eq!(document["result"]["revision"], 1, "{document}");
    assert!(
        document["result"]["scene"]["nodes"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn baseline_reference_export_uses_registered_source() {
    let (server, asset_id) = reference_fixture_server("reference-export").await;
    commit_operations(
        &server,
        vec![binding_create("node-000001", &asset_id, "AE-101")],
    )
    .await;
    let registered = rpc_with_bearer(
        &server,
        server.agent_token(),
        "referenceAsset.get",
        json!({ "assetId": asset_id }),
    )
    .await;
    let exported = rpc_with_approval_cookie(&server, "export.sourceGlb", json!({})).await;

    assert_eq!(
        exported["result"]["sourceDigest"], registered["result"]["descriptor"]["sourceDigest"],
        "{exported}"
    );
}

async fn reference_fixture_server(project_id: &str) -> (TestServer, String) {
    let server = start_test_server(ServerConfig::review_required(project_id)).await;
    write_reference_fixture(server.workspace_root());
    let registered = rpc_with_approval_cookie(
        &server,
        "referenceAsset.register",
        json!({
            "sourcePath": "refs/source.glb",
            "preparedPath": "refs/prepared.glb",
            "tilesetPath": "refs/tiled",
            "equipmentManifestPath": "refs/equipment.json"
        }),
    )
    .await;
    let asset_id = registered["result"]["assetId"].as_str().unwrap().to_owned();
    let material = rpc_with_approval_cookie(
        &server,
        "document.apply",
        json!({
            "expectedRevision": 0,
            "dryRun": false,
            "idempotencyKey": "reference-fixture-material",
            "operations": [{
                "type": "material.upsert",
                "material_id": "material-000001",
                "name": "Highlight",
                "base_color": [0.2, 0.4, 0.6, 1.0],
                "opacity": 1.0,
                "edge_color": null
            }]
        }),
    )
    .await;
    assert_eq!(material["result"]["revision"], 1, "{material}");
    (server, asset_id)
}

fn binding_create(node_id: &str, asset_id: &str, selection_tag: &str) -> serde_json::Value {
    json!({
        "type": "node.create",
        "node_id": node_id,
        "parent_id": null,
        "name": selection_tag,
        "transform": {
            "translation": { "x": 0, "y": 0, "z": 0 },
            "rotation": [0, 0, 0, 1],
            "scale": { "x": 1, "y": 1, "z": 1 }
        },
        "content": {
            "type": "reference_instance",
            "assetId": asset_id,
            "selectionId": format!("ice3-virtual-wwtp/equipment/{selection_tag}")
        },
        "material_id": null,
        "metadata": {}
    })
}

async fn create_change_set(
    server: &TestServer,
    operations: Vec<serde_json::Value>,
) -> serde_json::Value {
    let document = rpc_with_bearer(server, server.agent_token(), "document.get", json!({})).await;
    let base_revision = document["result"]["revision"].as_u64().unwrap();
    let created = rpc_with_bearer(
        server,
        server.agent_token(),
        "changeset.create",
        json!({
            "baseRevision": base_revision,
            "title": "Reference binding edit"
        }),
    )
    .await;
    let id = created["result"]["id"].as_str().unwrap();
    let entries = operations
        .into_iter()
        .map(|operation| {
            json!({
                "source": { "kind": "agent", "label": "reference-test" },
                "operation": operation
            })
        })
        .collect::<Vec<_>>();
    let added = rpc_with_bearer(
        server,
        server.agent_token(),
        "changeset.addOperations",
        json!({
            "changeSetId": id,
            "expectedVersion": created["result"]["version"],
            "entries": entries
        }),
    )
    .await;
    assert!(added.get("error").is_none(), "{added}");
    added["result"].clone()
}

async fn request_impact(server: &TestServer, change_set: &serde_json::Value) -> serde_json::Value {
    let response = rpc_with_bearer(
        server,
        server.agent_token(),
        "changeset.impact",
        json!({
            "changeSetId": change_set["id"],
            "expectedVersion": change_set["version"]
        }),
    )
    .await;
    assert!(response.get("error").is_none(), "{response}");
    response["result"]["impact"].clone()
}

async fn commit_operations(
    server: &TestServer,
    operations: Vec<serde_json::Value>,
) -> serde_json::Value {
    let change_set = create_change_set(server, operations).await;
    commit_evaluated(server, &change_set).await
}

async fn commit_evaluated(
    server: &TestServer,
    change_set: &serde_json::Value,
) -> serde_json::Value {
    let evaluated = rpc_with_bearer(
        server,
        server.agent_token(),
        "changeset.evaluate",
        json!({
            "changeSetId": change_set["id"],
            "expectedVersion": change_set["version"],
            "includeCandidate": false
        }),
    )
    .await;
    assert_eq!(evaluated["result"]["status"], "ready", "{evaluated}");
    rpc_with_approval_cookie(
        server,
        "changeset.commit",
        json!({
            "changeSetId": change_set["id"],
            "expectedVersion": change_set["version"],
            "approvedDigest": evaluated["result"]["candidateDigest"],
            "idempotencyKey": format!("commit-{}", change_set["id"].as_str().unwrap())
        }),
    )
    .await
}

fn write_reference_fixture(root: &Path) {
    fs::create_dir_all(root.join("refs/tiled")).unwrap();
    fs::write(
        root.join("refs/tiled/tileset.json"),
        br#"{"asset":{"version":"1.1"}}"#,
    )
    .unwrap();
    fs::write(
        root.join("refs/equipment.json"),
        serde_json::to_vec(&json!({
            "assetId": "ice3-virtual-wwtp",
            "equipment": (1..=204).map(|index| {
                let tag = if index == 1 {
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
    while json_bytes.len() % 4 != 0 {
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
