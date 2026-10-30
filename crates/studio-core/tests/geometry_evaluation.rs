use studio_core::document::NodeContent;
use studio_core::geometry::{Primitive, evaluate_node_content};

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
