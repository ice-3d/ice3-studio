use std::collections::BTreeMap;

use studio_core::document::{Document, NodeContent, SceneNode};
use studio_core::geometry::Primitive;
use studio_core::ids::{ComponentId, MaterialId, NodeId};
use studio_core::large_scene::cache::MemoryGeometryStore;
use studio_core::large_scene::component::{
    ComponentDefinition, ComponentInstance, ComponentPart, ParameterValue, PartContent,
    PartTransform, ScalarBinding,
};
use studio_core::large_scene::evaluate::evaluate_large_scene;
use studio_core::math::Transform;
use studio_server::scene_session::{SceneSourceIndex, build_full_scene_patch, build_scene_patch};

#[test]
fn transform_only_diff_contains_no_geometry_change() {
    let mut document = one_box_document();
    let previous_scene = evaluate_large_scene(&document).unwrap();
    let index = SceneSourceIndex::capture(&document, &previous_scene, None).unwrap();
    let node_id = document.scene.nodes()[0].id.clone();
    document.scene.nodes_mut()[0].transform.translation.x += 4.0;
    let diff = studio_core::diff::DocumentDiff {
        updated_node_ids: vec![node_id.clone()],
        ..Default::default()
    };
    let mut store = MemoryGeometryStore::default();
    let patch =
        build_scene_patch(&document, &diff, &previous_scene, &index, &mut store, None).unwrap();

    assert!(patch.geometry.is_empty());
    assert_eq!(patch.instances.len(), 1);
    assert_eq!(patch.instances[0].node_id, node_id.to_string());
}

#[test]
fn component_change_marks_every_direct_instance() {
    let (document, component_id, first_node, second_node) = two_instance_component_fixture();
    let previous_scene = evaluate_large_scene(&document).unwrap();
    let index = SceneSourceIndex::capture(&document, &previous_scene, None).unwrap();
    let diff = studio_core::diff::DocumentDiff {
        updated_component_ids: vec![component_id],
        ..Default::default()
    };

    let affected = index.affected_source_nodes(&diff).unwrap();
    assert_eq!(affected, [first_node, second_node].into_iter().collect());
}

#[test]
fn nested_component_change_reaches_the_outer_instance() {
    let (document, leaf_component, node_id) = nested_component_fixture();
    let scene = evaluate_large_scene(&document).unwrap();
    let index = SceneSourceIndex::capture(&document, &scene, None).unwrap();
    let diff = studio_core::diff::DocumentDiff {
        updated_component_ids: vec![leaf_component],
        ..Default::default()
    };

    assert_eq!(
        index.affected_source_nodes(&diff).unwrap(),
        [node_id].into_iter().collect()
    );
}

#[test]
fn material_change_selects_referencing_nodes_without_geometry() {
    let mut document = one_box_document();
    let material_id = MaterialId::from("material-000001");
    document.materials.push(studio_core::document::Material {
        id: material_id.clone(),
        name: "Steel".into(),
        base_color: [0.5, 0.5, 0.5, 1.0],
        opacity: 1.0,
        edge_color: None,
    });
    document.scene.nodes_mut()[0].material_id = Some(material_id.clone());
    let previous_scene = evaluate_large_scene(&document).unwrap();
    let index = SceneSourceIndex::capture(&document, &previous_scene, None).unwrap();
    let diff = studio_core::diff::DocumentDiff {
        updated_material_ids: vec![material_id],
        ..Default::default()
    };

    assert_eq!(
        index.affected_source_nodes(&diff).unwrap(),
        [NodeId::from("node-000001")].into_iter().collect()
    );
}

#[test]
fn initial_patch_contains_every_derived_identity() {
    let mut document = one_box_document();
    document.revision = studio_core::ids::Revision::new(7);
    let scene = evaluate_large_scene(&document).unwrap();
    let patch = build_full_scene_patch(&document, &scene, None).unwrap();

    assert!(patch.full_rebuild);
    assert_eq!(patch.base_revision, 0);
    assert_eq!(patch.target_revision, 7);
    assert_eq!(patch.geometry.len(), 1);
    assert_eq!(patch.instances.len(), 1);
}

#[test]
fn unsupported_diff_requests_a_conservative_rebuild() {
    let document = one_box_document();
    let scene = evaluate_large_scene(&document).unwrap();
    let index = SceneSourceIndex::capture(&document, &scene, None).unwrap();
    let diff = studio_core::diff::DocumentDiff {
        updated_relation_ids: vec![studio_core::ids::RelationId::from("route-000001")],
        ..Default::default()
    };

    assert!(index.affected_source_nodes(&diff).is_none());
}

#[test]
fn added_relations_do_not_force_a_full_scene_rebuild() {
    let document = one_box_document();
    let scene = evaluate_large_scene(&document).unwrap();
    let index = SceneSourceIndex::capture(&document, &scene, None).unwrap();
    let diff = studio_core::diff::DocumentDiff {
        added_relation_ids: vec![studio_core::ids::RelationId::from("route-000001")],
        added_node_ids: vec![NodeId::from("connection-000001")],
        ..Default::default()
    };

    assert_eq!(
        index.affected_source_nodes(&diff).unwrap(),
        [NodeId::from("connection-000001")].into_iter().collect()
    );
}

fn one_box_document() -> Document {
    let mut document = Document::new("scene-patch");
    document.scene.nodes_mut().push(SceneNode {
        id: NodeId::from("node-000001"),
        name: "Primary".into(),
        parent_id: None,
        transform: Transform::IDENTITY,
        material_id: None,
        content: NodeContent::Primitive(Primitive::Box {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        }),
        metadata: BTreeMap::new(),
        visible: true,
    });
    document
}

fn two_instance_component_fixture() -> (Document, ComponentId, NodeId, NodeId) {
    let mut document = Document::new("component-scene");
    let component_id = ComponentId::from("component-000001");
    document.components.push(ComponentDefinition {
        id: component_id.clone(),
        name: "Vessel".into(),
        parameters: Vec::new(),
        parts: vec![ComponentPart {
            id: "body".into(),
            parent_id: None,
            transform: PartTransform::default(),
            content: PartContent::Box {
                width: ScalarBinding::Literal(2.0),
                height: ScalarBinding::Literal(1.0),
                depth: ScalarBinding::Literal(1.0),
            },
            material_role: None,
            metadata: BTreeMap::new(),
            visible: true,
        }],
        ports: Vec::new(),
        metadata: BTreeMap::new(),
    });
    let first = NodeId::from("node-000001");
    let second = NodeId::from("node-000002");
    for (id, x) in [(first.clone(), 0.0), (second.clone(), 4.0)] {
        let mut node = SceneNode::group(id, "Vessel");
        node.transform.translation.x = x;
        node.content = NodeContent::ComponentInstance(ComponentInstance {
            definition_id: component_id.clone(),
            parameters: BTreeMap::new(),
            overrides: BTreeMap::new(),
            metadata: BTreeMap::new(),
        });
        document.scene.nodes_mut().push(node);
    }
    (document, component_id, first, second)
}

fn nested_component_fixture() -> (Document, ComponentId, NodeId) {
    let mut document = Document::new("nested-component-scene");
    let leaf = ComponentId::from("component-000001");
    document.components.push(ComponentDefinition {
        id: leaf.clone(),
        name: "Leaf".into(),
        parameters: Vec::new(),
        parts: vec![ComponentPart {
            id: "body".into(),
            parent_id: None,
            transform: PartTransform::default(),
            content: PartContent::Box {
                width: ScalarBinding::Literal(1.0),
                height: ScalarBinding::Literal(1.0),
                depth: ScalarBinding::Literal(1.0),
            },
            material_role: None,
            metadata: BTreeMap::new(),
            visible: true,
        }],
        ports: Vec::new(),
        metadata: BTreeMap::new(),
    });
    let outer = ComponentId::from("component-000002");
    document.components.push(ComponentDefinition {
        id: outer.clone(),
        name: "Outer".into(),
        parameters: Vec::new(),
        parts: vec![ComponentPart {
            id: "nested".into(),
            parent_id: None,
            transform: PartTransform::default(),
            content: PartContent::ComponentReference {
                definition_id: leaf.clone(),
                parameters: BTreeMap::<String, ParameterValue>::new(),
            },
            material_role: None,
            metadata: BTreeMap::new(),
            visible: true,
        }],
        ports: Vec::new(),
        metadata: BTreeMap::new(),
    });
    let node_id = NodeId::from("node-000003");
    let mut node = SceneNode::group(node_id.clone(), "Outer");
    node.content = NodeContent::ComponentInstance(ComponentInstance {
        definition_id: outer,
        parameters: BTreeMap::new(),
        overrides: BTreeMap::new(),
        metadata: BTreeMap::new(),
    });
    document.scene.nodes_mut().push(node);
    (document, leaf, node_id)
}
