use std::collections::BTreeMap;

use studio_core::PROJECT_SCHEMA_VERSION;
use studio_core::document::{Document, NodeContent, SceneNode};
use studio_core::ids::{ComponentId, NodeId};
use studio_core::large_scene::component::ComponentInstance;
use studio_core::math::Transform;

#[test]
fn creates_a_version_two_document_with_empty_large_scene_collections() {
    let document = Document::new("large");
    assert_eq!(document.schema_version, PROJECT_SCHEMA_VERSION);
    assert_eq!(document.schema_version, 2);
    assert!(document.components.is_empty());
}

#[test]
fn migrates_version_one_without_changing_existing_nodes() {
    let mut document = Document::new("legacy");
    document.scene.nodes_mut().push(SceneNode {
        id: NodeId::from("node-000001"),
        name: "Legacy".into(),
        parent_id: None,
        transform: Transform::IDENTITY,
        material_id: None,
        content: NodeContent::Group,
        metadata: BTreeMap::new(),
        visible: true,
    });
    document.schema_version = 1;

    let migrated = document.clone().migrate().unwrap();
    assert_eq!(migrated.schema_version, 2);
    assert_eq!(migrated.scene, document.scene);
    assert!(migrated.components.is_empty());
}

#[test]
fn serializes_a_component_instance_node() {
    let node = SceneNode {
        id: NodeId::from("node-000002"),
        name: "Equipment".into(),
        parent_id: None,
        transform: Transform::IDENTITY,
        material_id: None,
        content: NodeContent::ComponentInstance(ComponentInstance {
            definition_id: ComponentId::from("component-000001"),
            parameters: BTreeMap::new(),
            overrides: BTreeMap::new(),
            metadata: BTreeMap::new(),
        }),
        metadata: BTreeMap::new(),
        visible: true,
    };
    let json = serde_json::to_value(node).unwrap();
    assert_eq!(json["content"]["type"], "component_instance");
    assert_eq!(json["content"]["definitionId"], "component-000001");
}
