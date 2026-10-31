mod support;

use std::collections::{BTreeMap, BTreeSet};

use studio_core::document::{Document, NodeContent, SceneNode};
use studio_core::geometry::Primitive;
use studio_core::ids::NodeId;
use studio_core::large_scene::cache::MemoryGeometryStore;
use studio_core::large_scene::evaluate::{
    evaluate_large_scene_selected, evaluate_large_scene_with_cache,
};
use studio_core::math::{Transform, Vec3};
use support::shared_pump_document;

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
