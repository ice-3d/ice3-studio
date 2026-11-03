mod support;

use std::collections::BTreeMap;
use studio_core::large_scene::component::{
    ComponentPart, PartContent, PartTransform, ScalarBinding,
};
use studio_core::large_scene::evaluate::evaluate_large_scene;
use support::shared_pump_document;

#[test]
fn identical_instances_share_one_mesh_with_stable_part_identity() {
    let document = shared_pump_document();
    let evaluated = evaluate_large_scene(&document).unwrap();
    assert_eq!(evaluated.nodes.len(), 2);
    assert_eq!(evaluated.meshes.len(), 1);
    assert_eq!(evaluated.nodes[0].id, "node-000001/body");
    assert_eq!(evaluated.nodes[0].owner_id, "node-000001");
    assert_eq!(evaluated.nodes[1].id, "node-000002/body");
    assert_eq!(evaluated.stats.effective_triangles, 24);
    assert_eq!(evaluated.stats.unique_triangles, 12);
}

#[test]
fn group_parts_create_hierarchy_without_empty_meshes() {
    let mut document = shared_pump_document();
    document.components[0].parts.push(ComponentPart {
        id: "group".into(),
        parent_id: None,
        transform: PartTransform::default(),
        content: PartContent::Group,
        subtract: Vec::new(),
        repeat: None,
        material_role: None,
        metadata: BTreeMap::new(),
        visible: true,
    });
    let evaluated = evaluate_large_scene(&document).unwrap();
    assert_eq!(evaluated.nodes.len(), 4);
    assert_eq!(evaluated.meshes.len(), 1);
    assert!(
        evaluated
            .nodes
            .iter()
            .filter(|node| node.mesh_key.is_none())
            .count()
            >= 2
    );
}

#[test]
fn extrude_part_evaluates_to_a_prism() {
    let mut document = shared_pump_document();
    document.components[0].parts[0].content = PartContent::Extrude {
        profile: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        height: ScalarBinding::Literal(2.0),
    };
    let evaluated = evaluate_large_scene(&document).unwrap();
    let mesh = evaluated.meshes.values().next().expect("one shared mesh");
    assert_eq!(mesh.indices.len() / 3, 12, "square prism has 12 triangles");
    assert_eq!(evaluated.stats.effective_triangles, 24, "two instances");
}
