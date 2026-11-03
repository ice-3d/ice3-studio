mod support;

use std::collections::BTreeMap;
use studio_core::large_scene::component::{
    ComponentPart, CylinderCut, PartContent, PartCut, PartTransform, ScalarBinding,
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

fn signed_volume(mesh: &studio_core::geometry::EvaluatedMesh) -> f32 {
    let mut total = 0.0;
    for triangle in mesh.indices.as_chunks::<3>().0 {
        let a = mesh.vertices[triangle[0] as usize].position;
        let b = mesh.vertices[triangle[1] as usize].position;
        let c = mesh.vertices[triangle[2] as usize].position;
        total += (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
            + a[2] * (b[0] * c[1] - b[1] * c[0]))
            / 6.0;
    }
    total
}

#[test]
fn cylinder_cut_removes_material_from_a_box_part() {
    let mut document = shared_pump_document();
    document.components[0].parts[0].subtract = vec![PartCut::Cylinder(CylinderCut {
        radius: ScalarBinding::Literal(0.2),
        height: ScalarBinding::Literal(3.0),
        x: ScalarBinding::Literal(0.0),
        y: ScalarBinding::Literal(0.0),
        z: ScalarBinding::Literal(0.0),
        segments: 32,
        axis: "z".to_string(),
    })];
    let evaluated = evaluate_large_scene(&document).unwrap();
    let mesh = evaluated.meshes.values().next().expect("shared cut mesh");
    assert!(mesh.indices.len() / 3 > 12, "cut adds cap/wall triangles");
    // Box is 2 x 1 x 1 = 2; a through hole of radius 0.2 removes ~0.126.
    assert!(
        signed_volume(mesh) < 1.95,
        "volume {} should be below the solid box",
        signed_volume(mesh)
    );
}
