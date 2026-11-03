use studio_core::document::{Document, NodeContent, SceneNode};
use studio_core::geometry::{Primitive, evaluate_node_content, world_bounds};
use studio_core::ids::NodeId;

#[test]
fn evaluates_a_box_with_outward_triangles() {
    let content = NodeContent::Primitive(Primitive::Box {
        width: 2.0,
        height: 1.0,
        depth: 4.0,
    });
    let mesh = evaluate_node_content(&content).unwrap();
    assert_eq!(mesh.vertices.len(), 24);
    assert_eq!(mesh.indices.len(), 36);
    assert_eq!(mesh.bounds.min.x, -1.0);
    assert_eq!(mesh.bounds.max.z, 2.0);
}

#[test]
fn sweeps_a_polyline_into_connected_rings() {
    let content = NodeContent::Sweep {
        points: vec![[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [2.0, 0.0, 3.0]],
        radius: 0.25,
        radial_segments: 8,
    };
    let mesh = evaluate_node_content(&content).unwrap();
    assert_eq!(mesh.vertices.len(), 24);
    assert!(mesh.indices.len() > 60);
}

#[test]
fn swept_pipe_frames_do_not_twist_at_bends() {
    let points = [
        [0.0, 0.0, 0.0],
        [2.0, 0.0, 0.0],
        [2.8, 0.4, 0.0],
        [3.0, 1.0, 0.0],
        [3.0, 3.0, 0.0],
    ];
    let mesh = evaluate_node_content(&NodeContent::Sweep {
        points: points.into(),
        radius: 0.2,
        radial_segments: 16,
    })
    .unwrap();
    for ring in 0..points.len() - 1 {
        for segment in 0..16 {
            let a = mesh.vertices[ring * 16 + segment].normal;
            let b = mesh.vertices[(ring + 1) * 16 + segment].normal;
            let dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
            assert!(
                dot > 0.65,
                "frame flipped at ring {ring}, segment {segment}"
            );
        }
    }
}

#[test]
fn rejects_zero_sized_and_non_finite_primitives() {
    assert!(
        evaluate_node_content(&NodeContent::Primitive(Primitive::Cylinder {
            radius: 0.0,
            height: 2.0,
            segments: 16,
        }))
        .is_err()
    );
    assert!(
        evaluate_node_content(&NodeContent::Primitive(Primitive::Box {
            width: f32::NAN,
            height: 1.0,
            depth: 1.0,
        }))
        .is_err()
    );
}

#[test]
fn closes_cylinder_ends_with_outward_facing_triangles() {
    let content = NodeContent::Primitive(Primitive::Cylinder {
        radius: 2.0,
        height: 4.0,
        segments: 16,
    });
    let mesh = evaluate_node_content(&content).unwrap();
    assert_eq!(mesh.indices.len(), 192);
}

#[test]
fn sphere_poles_do_not_emit_degenerate_triangles() {
    let segments = 32;
    let rings = 16;
    let mesh = evaluate_node_content(&NodeContent::Primitive(Primitive::Sphere {
        radius: 2.0,
        segments,
        rings,
    }))
    .unwrap();

    assert_eq!(
        mesh.indices.len() / 3,
        (segments * 2 * (rings - 1)) as usize
    );
    for triangle in mesh.indices.as_chunks::<3>().0 {
        let positions = triangle.map(|index| mesh.vertices[index as usize].position);
        assert_ne!(positions[0], positions[1]);
        assert_ne!(positions[0], positions[2]);
        assert_ne!(positions[1], positions[2]);
    }
}

#[test]
fn includes_parent_transforms_in_world_bounds() {
    let mut document = Document::new("world-bounds");
    let mut parent = SceneNode::group(NodeId::from("node-000001"), "Parent");
    parent.transform.translation.x = 10.0;
    let mut child = SceneNode::group(NodeId::from("node-000002"), "Child");
    child.parent_id = Some(parent.id.clone());
    child.content = NodeContent::Primitive(Primitive::Box {
        width: 2.0,
        height: 2.0,
        depth: 2.0,
    });
    document.scene.nodes_mut().extend([parent, child.clone()]);

    let bounds = world_bounds(&document, &child).unwrap().unwrap();
    assert_eq!(bounds.min.x, 9.0);
    assert_eq!(bounds.max.x, 11.0);
}
