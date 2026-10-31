mod common;

use studio_core::document::{Document, NodeContent, SceneNode};
use studio_core::geometry::Primitive;
use studio_core::gpu_contract::{GeometryBackendKind, GeometryEvaluationMode, ResidentGeometryId};
use studio_core::ids::NodeId;
use studio_core::large_scene::evaluate::evaluate_large_scene;
use studio_core::scene_contract::SceneGeometryReadback;
use studio_server::preview_packet::{
    PreviewGeometryManifest, PreviewGeometryNode, PreviewGeometryPacket, PreviewGeometryRange,
    PreviewGeometryRangeUsage, PreviewGeometryRecord, build_full_preview_packet,
    build_preview_delta, build_scene_preview_delta, decode_preview_packet, encode_preview_packet,
};
use studio_server::{ServerConfig, start_test_server};

fn sample_packet() -> PreviewGeometryPacket {
    PreviewGeometryPacket {
        manifest: PreviewGeometryManifest {
            schema_version: 1,
            transport_version: 1,
            revision: 4,
            parent_revision: None,
            source_digest: "a".repeat(64),
            backend: GeometryBackendKind::Gpu,
            residency_mode: GeometryEvaluationMode::Resident,
            nodes: vec![PreviewGeometryNode {
                node_id: "pump-1/body".into(),
                parent_id: None,
                name: "Pump/Body".into(),
                matrix: [
                    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
                ],
                material_id: None,
                geometry_id: Some("component/body".into()),
                visible: true,
            }],
            geometries: vec![PreviewGeometryRecord {
                geometry_id: "component/body".into(),
                resident_geometry_id: Some(ResidentGeometryId("resident-body".into())),
                topology_digest: "b".repeat(64),
                attribute_digest: "c".repeat(64),
                vertex_count: 2,
                index_count: 0,
                index_width: 4,
                ranges: vec![PreviewGeometryRange {
                    byte_offset: 0,
                    byte_length: 48,
                    usage: PreviewGeometryRangeUsage::VertexAttributes,
                }],
            }],
            materials: Vec::new(),
            removed_node_ids: Vec::new(),
            removed_geometry_ids: Vec::new(),
        },
        binary: vec![0; 48],
    }
}

fn transform_only_successor(base: &PreviewGeometryPacket) -> PreviewGeometryPacket {
    let mut next = base.clone();
    next.manifest.revision = 5;
    next.manifest.parent_revision = Some(4);
    next.manifest.nodes[0].matrix[12] = 4.0;
    next
}

#[test]
fn packet_round_trip_preserves_dirty_ranges_and_geometry_identity() {
    let packet = sample_packet();
    let bytes = encode_preview_packet(&packet);
    let decoded = decode_preview_packet(&bytes).unwrap();
    assert_eq!(decoded.manifest.revision, 4);
    assert_eq!(decoded.manifest.geometries[0].geometry_id, "component/body");
    assert_eq!(decoded.manifest.geometries[0].ranges[0].byte_length, 48);
    assert_eq!(decoded.binary, packet.binary);
}

#[test]
fn transform_only_delta_contains_no_geometry_chunk() {
    let base = sample_packet();
    let next = transform_only_successor(&base);
    let delta = build_preview_delta(&base, &next);
    assert_eq!(delta.binary.len(), 0);
    assert_eq!(delta.manifest.geometries.len(), 0);
    assert_eq!(delta.manifest.nodes.len(), 1);
    assert_eq!(delta.manifest.parent_revision, Some(4));
}

#[test]
fn scene_preview_delta_contains_only_changed_attributes() {
    let base = sample_packet();
    let mut document = Document::new("scene-preview-delta");
    document.revision = studio_core::ids::Revision::new(5);
    let delta = build_scene_preview_delta(
        &base,
        &document,
        vec![PreviewGeometryNode {
            node_id: "pump-1/body".into(),
            parent_id: None,
            name: "Pump/Body".into(),
            matrix: base.manifest.nodes[0].matrix,
            material_id: None,
            geometry_id: Some("component/body".into()),
            visible: true,
        }],
        vec![SceneGeometryReadback {
            geometry_id: "component/body".into(),
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]],
            indices: vec![0, 1, 0],
            topology_changed: false,
        }],
        Vec::new(),
        Vec::new(),
    );
    let decoded = decode_preview_packet(&encode_preview_packet(&delta)).unwrap();

    assert_eq!(decoded.manifest.parent_revision, Some(4));
    assert_eq!(decoded.manifest.geometries.len(), 1);
    assert_eq!(decoded.manifest.geometries[0].ranges.len(), 1);
    assert_eq!(decoded.binary.len(), 2 * 3 * 4);
}

#[tokio::test]
async fn production_transform_preview_writes_no_geometry_payload() {
    let server = start_test_server(ServerConfig {
        compute_mode: studio_core::gpu_contract::GeometryBackendMode::Gpu,
        preview_packet_enabled: true,
        scene_sessions_enabled: true,
        ..ServerConfig::for_tests("scene-preview-e2e")
    })
    .await;
    common::replace_toolchain_body(&server, common::SUCCESS_TOOLCHAIN).await;
    let rpc = |method: &'static str, params: serde_json::Value| {
        common::rpc_with_bearer(&server, server.token(), method, params)
    };
    let created = rpc(
        "document.apply",
        serde_json::json!({
            "expectedRevision": 0,
            "idempotencyKey": "create-box",
            "operations": [{
                "type": "node.create",
                "node_id": "box-000001",
                "parent_id": null,
                "name": "Box",
                "transform": {
                    "translation": { "x": 0, "y": 0, "z": 0 },
                    "rotation": [0, 0, 0, 1],
                    "scale": { "x": 1, "y": 1, "z": 1 }
                },
                "content": {
                    "type": "primitive",
                    "kind": "box",
                    "width": 1.0,
                    "height": 1.0,
                    "depth": 1.0
                },
                "material_id": null,
                "metadata": {}
            }]
        }),
    )
    .await;
    assert_eq!(created["result"]["revision"], 1, "{created}");
    let capabilities = rpc("system.capabilities", serde_json::json!({})).await;
    if capabilities["result"]["compute"]["sceneSession"]["enabled"] != true {
        return;
    }
    rpc("evaluation.waitScene", serde_json::json!({ "revision": 1 })).await;
    let first_preview = rpc("export.runtimePreview", serde_json::json!({})).await;
    assert!(first_preview.get("error").is_none(), "{first_preview}");
    let moved = rpc(
        "document.apply",
        serde_json::json!({
            "expectedRevision": 1,
            "idempotencyKey": "move-box",
            "operations": [{
                "type": "node.update",
                "node_id": "box-000001",
                "name": null,
                "transform": {
                    "translation": { "x": 4, "y": 0, "z": 0 },
                    "rotation": [0, 0, 0, 1],
                    "scale": { "x": 1, "y": 1, "z": 1 }
                },
                "visible": null,
                "metadata": null
            }]
        }),
    )
    .await;
    assert_eq!(moved["result"]["revision"], 2, "{moved}");
    rpc("evaluation.waitScene", serde_json::json!({ "revision": 2 })).await;
    let second_preview = rpc("export.runtimePreview", serde_json::json!({})).await;
    assert!(second_preview.get("error").is_none(), "{second_preview}");
    assert_eq!(second_preview["result"]["packetUrl"], "/preview-packets/2");
    let response = reqwest::Client::new()
        .get(format!("{}/preview-packets/2", server.base_url()))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let bytes = response.bytes().await.unwrap();
    let packet = decode_preview_packet(&bytes)
        .unwrap_or_else(|error| panic!("invalid preview packet ({} bytes): {error}", bytes.len()));

    assert_eq!(packet.manifest.parent_revision, Some(1));
    assert!(packet.manifest.geometries.is_empty());
    assert!(packet.binary.is_empty());
}

#[test]
fn invalid_ranges_are_rejected_before_upload() {
    let mut bytes = encode_preview_packet(&sample_packet());
    bytes[40..48].copy_from_slice(&u64::MAX.to_le_bytes());
    assert!(decode_preview_packet(&bytes).is_err());
}

#[test]
fn geometry_range_outside_payload_is_rejected() {
    let mut packet = sample_packet();
    packet.manifest.geometries[0].ranges[0].byte_length = 64;
    let bytes = encode_preview_packet(&packet);
    assert!(decode_preview_packet(&bytes).is_err());
}

#[test]
fn attribute_delta_omits_unchanged_topology_range() {
    let mut base = sample_packet();
    base.manifest.geometries[0]
        .ranges
        .push(PreviewGeometryRange {
            byte_offset: 48,
            byte_length: 12,
            usage: PreviewGeometryRangeUsage::Indices,
        });
    base.binary.resize(60, 1);
    let mut next = base.clone();
    next.manifest.revision = 5;
    next.manifest.parent_revision = Some(4);
    next.manifest.geometries[0].attribute_digest = "d".repeat(64);
    next.binary[0] = 9;

    let delta = build_preview_delta(&base, &next);
    assert_eq!(delta.binary.len(), 48);
    assert_eq!(delta.manifest.geometries[0].ranges.len(), 1);
    assert_eq!(
        delta.manifest.geometries[0].ranges[0].usage,
        PreviewGeometryRangeUsage::VertexAttributes
    );
}

#[test]
fn full_packet_uses_stable_geometry_ids_and_aligned_payload_ranges() {
    let mut document = Document::new("preview-packet");
    let mut node = SceneNode::group(NodeId::from("pump-1"), "Pump");
    node.content = NodeContent::Primitive(Primitive::Box {
        width: 1.0,
        height: 1.0,
        depth: 1.0,
    });
    document.scene.nodes_mut().push(node);
    let scene = evaluate_large_scene(&document).unwrap();

    let packet = build_full_preview_packet(
        &document,
        &scene,
        GeometryBackendKind::Cpu,
        GeometryEvaluationMode::Cold,
    );
    let decoded = decode_preview_packet(&encode_preview_packet(&packet)).unwrap();

    assert_eq!(
        decoded.manifest.nodes[0].geometry_id,
        scene.nodes[0].mesh_key
    );
    assert_eq!(decoded.manifest.geometries.len(), 1);
    assert!(
        decoded.manifest.geometries[0]
            .ranges
            .iter()
            .all(|range| range.byte_offset % 4 == 0)
    );
}

#[tokio::test]
async fn capabilities_and_runtime_preview_negotiate_a_versioned_packet_url() {
    let server = start_test_server(ServerConfig {
        preview_packet_enabled: true,
        ..ServerConfig::for_tests("packet-preview")
    })
    .await;
    common::replace_toolchain_body(&server, common::SUCCESS_TOOLCHAIN).await;

    let capabilities = common::rpc_with_bearer(
        &server,
        server.token(),
        "system.capabilities",
        serde_json::json!({}),
    )
    .await;
    assert_eq!(
        capabilities["result"]["compute"]["previewGeometryPacket"],
        "1"
    );
    let preview = common::rpc_with_bearer(
        &server,
        server.token(),
        "export.runtimePreview",
        serde_json::json!({}),
    )
    .await;
    let packet_url = preview["result"]["packetUrl"].as_str().unwrap();
    assert_eq!(packet_url, "/preview-packets/0");

    let response = reqwest::Client::new()
        .get(format!("{}{packet_url}", server.base_url()))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .unwrap(),
        "application/vnd.ice3.preview-geometry; version=1"
    );
    assert!(decode_preview_packet(&response.bytes().await.unwrap()).is_ok());
}
