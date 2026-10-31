use std::collections::BTreeMap;

use studio_core::document::{Document, NodeContent, SceneNode};
use studio_core::geometry::Bounds;
use studio_core::ids::NodeId;
use studio_core::math::{Transform, Vec3};
use studio_core::reference_asset::{
    ReferenceAssetDescriptor, ReferenceAssetStats, ReferenceInstanceContent,
};
use studio_core::validation::{DiagnosticCode, validate_document};

#[test]
fn reference_instance_and_descriptor_round_trip() {
    let descriptor = ReferenceAssetDescriptor {
        asset_id: "asset-000001".into(),
        source_digest: "a".repeat(64),
        prepared_digest: Some("b".repeat(64)),
        tileset_digest: Some("c".repeat(64)),
        bounds: Bounds {
            min: Vec3 {
                x: -625.0,
                y: -1.0,
                z: -390.0,
            },
            max: Vec3 {
                x: 625.0,
                y: 31.0,
                z: 390.0,
            },
        },
        stats: ReferenceAssetStats {
            source_nodes: 1761,
            effective_triangles: 5_757_962,
            unique_meshes: 1632,
            materials: 75,
            equipment: 204,
        },
        imported_at: "2026-10-31T00:00:00Z".into(),
    };
    let binding = SceneNode {
        id: NodeId::from("node-000001"),
        name: "AE-101".into(),
        parent_id: None,
        transform: Transform::IDENTITY,
        material_id: None,
        content: NodeContent::ReferenceInstance(ReferenceInstanceContent {
            asset_id: descriptor.asset_id.clone(),
            selection_id: "ice3-virtual-wwtp/equipment/AE-101".into(),
        }),
        metadata: BTreeMap::new(),
        visible: true,
    };
    let value = serde_json::to_value(binding).unwrap();
    assert_eq!(value["content"]["type"], "reference_instance");
    assert_eq!(
        value["content"]["selectionId"],
        "ice3-virtual-wwtp/equipment/AE-101"
    );
}

#[test]
fn reference_instance_rejects_empty_asset_and_selection_ids() {
    for content in [
        ReferenceInstanceContent {
            asset_id: String::new(),
            selection_id: "ice3-virtual-wwtp/equipment/AE-101".into(),
        },
        ReferenceInstanceContent {
            asset_id: "asset-000001".into(),
            selection_id: String::new(),
        },
    ] {
        let mut document = Document::new("reference-validation");
        document.scene.nodes_mut().push(SceneNode {
            id: NodeId::from("node-000001"),
            name: "AE-101".into(),
            parent_id: None,
            transform: Transform::IDENTITY,
            material_id: None,
            content: NodeContent::ReferenceInstance(content),
            metadata: BTreeMap::new(),
            visible: true,
        });

        let diagnostics = validate_document(&document);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, DiagnosticCode::ReferenceInvalid);
        assert_eq!(diagnostics[0].pointer, "/scene/nodes/0/content");
    }
}
