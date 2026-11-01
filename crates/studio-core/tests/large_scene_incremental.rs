mod support;

use std::collections::{BTreeMap, BTreeSet};

use studio_core::document::{Document, NodeContent, SceneNode};
use studio_core::geometry::Primitive;
use studio_core::ids::{ComponentId, NodeId};
use studio_core::large_scene::cache::MemoryGeometryStore;
use studio_core::large_scene::component::{
    ComponentDefinition, ComponentInstance, ComponentParameter, ComponentPart, ParameterType,
    ParameterValue, PartContent, PartTransform, ScalarBinding,
};
use studio_core::large_scene::evaluate::{
    evaluate_large_scene_selected, evaluate_large_scene_with_cache,
};
use studio_core::math::{Transform, Vec3};
use support::shared_pump_document;

#[test]
fn changing_an_unbound_parameter_reuses_part_geometry() {
    let mut document = parameterized_box_document();
    let mut store = MemoryGeometryStore::default();
    let first = evaluate_large_scene_with_cache(&document, &mut store).unwrap();

    let NodeContent::ComponentInstance(instance) = &mut document.scene.nodes_mut()[0].content
    else {
        panic!("fixture node must be a component instance");
    };
    instance
        .parameters
        .insert("unused".into(), ParameterValue::Number(2.0));
    let unchanged_geometry = evaluate_large_scene_with_cache(&document, &mut store).unwrap();

    assert_eq!(unchanged_geometry.stats.cache_hits, 1);
    assert_eq!(unchanged_geometry.stats.generated_meshes, 0);
    assert_eq!(
        first.meshes.keys().collect::<Vec<_>>(),
        unchanged_geometry.meshes.keys().collect::<Vec<_>>()
    );

    let NodeContent::ComponentInstance(instance) = &mut document.scene.nodes_mut()[0].content
    else {
        panic!("fixture node must be a component instance");
    };
    instance
        .parameters
        .insert("width".into(), ParameterValue::Number(3.0));
    let changed_geometry = evaluate_large_scene_with_cache(&document, &mut store).unwrap();

    assert_eq!(changed_geometry.stats.cache_hits, 0);
    assert_eq!(changed_geometry.stats.generated_meshes, 1);
    assert_ne!(
        first.meshes.keys().collect::<Vec<_>>(),
        changed_geometry.meshes.keys().collect::<Vec<_>>()
    );
}

#[test]
fn transform_only_change_reuses_the_unchanged_shared_mesh() {
    let mut store = MemoryGeometryStore::default();
    let first_document = shared_pump_document();
    let first = evaluate_large_scene_with_cache(&first_document, &mut store).unwrap();

    let mut second_document = first_document;
    second_document.scene.nodes_mut()[1].transform.translation = Vec3 {
        x: 9.0,
        y: 0.0,
        z: 0.0,
    };
    let second = evaluate_large_scene_with_cache(&second_document, &mut store).unwrap();

    assert_eq!(second.stats.cache_hits, 1);
    assert_eq!(second.stats.generated_meshes, 0);
    assert_eq!(
        first.meshes.keys().collect::<Vec<_>>(),
        second.meshes.keys().collect::<Vec<_>>()
    );
}

#[test]
fn selected_evaluation_expands_only_named_source_nodes() {
    let mut document = Document::new("selected-scene");
    document.scene.nodes_mut().extend([
        SceneNode {
            id: NodeId::from("box-000001"),
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
        },
        SceneNode {
            id: NodeId::from("box-000002"),
            name: "Unrelated".into(),
            parent_id: None,
            transform: Transform::IDENTITY,
            material_id: None,
            content: NodeContent::Primitive(Primitive::Box {
                width: 2.0,
                height: 2.0,
                depth: 2.0,
            }),
            metadata: BTreeMap::new(),
            visible: true,
        },
    ]);
    let mut store = MemoryGeometryStore::default();
    let selected = [NodeId::from("box-000001")]
        .into_iter()
        .collect::<BTreeSet<_>>();
    let scene = evaluate_large_scene_selected(&document, &mut store, None, &selected).unwrap();

    assert_eq!(scene.nodes.len(), 1);
    assert_eq!(scene.nodes[0].id, "box-000001");
    assert_eq!(scene.meshes.len(), 1);
}

fn parameterized_box_document() -> Document {
    let component_id = ComponentId::from("component-000001");
    let mut document = Document::new("parameter-dependencies");
    document.components.push(ComponentDefinition {
        id: component_id.clone(),
        name: "Parameterized Box".into(),
        parameters: vec![
            ComponentParameter {
                name: "width".into(),
                value_type: ParameterType::Number {
                    minimum: Some(1.0),
                    maximum: Some(10.0),
                },
                default: ParameterValue::Number(2.0),
                unit: Some("m".into()),
            },
            ComponentParameter {
                name: "unused".into(),
                value_type: ParameterType::Number {
                    minimum: Some(1.0),
                    maximum: Some(10.0),
                },
                default: ParameterValue::Number(1.0),
                unit: None,
            },
        ],
        parts: vec![ComponentPart {
            id: "body".into(),
            parent_id: None,
            transform: PartTransform::default(),
            content: PartContent::Box {
                width: ScalarBinding::Parameter {
                    param: "width".into(),
                },
                height: ScalarBinding::Literal(1.0),
                depth: ScalarBinding::Literal(1.0),
            },
            subtract: Vec::new(),
            repeat: None,
            material_role: None,
            metadata: BTreeMap::new(),
            visible: true,
        }],
        ports: Vec::new(),
        metadata: BTreeMap::new(),
    });
    document.scene.nodes_mut().push(SceneNode {
        id: NodeId::from("node-000001"),
        name: "Parameterized".into(),
        parent_id: None,
        transform: Transform::IDENTITY,
        material_id: None,
        content: NodeContent::ComponentInstance(ComponentInstance {
            definition_id: component_id,
            parameters: [
                ("width".into(), ParameterValue::Number(2.0)),
                ("unused".into(), ParameterValue::Number(1.0)),
            ]
            .into_iter()
            .collect(),
            overrides: BTreeMap::new(),
            metadata: BTreeMap::new(),
        }),
        metadata: BTreeMap::new(),
        visible: true,
    });
    document
}
