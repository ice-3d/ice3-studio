//! BSP CSG subtraction: volume checks for box/cylinder/sphere cuts, plus
//! backward-compatible `PartCut` deserialization.

use studio_core::document::NodeContent;
use studio_core::geometry::{Primitive, TriangleMesh, csg_subtract, evaluate_node_content};
use studio_core::large_scene::component::PartCut;

fn primitive_mesh(primitive: Primitive) -> TriangleMesh {
    let evaluated = evaluate_node_content(&NodeContent::Primitive(primitive)).expect("primitive");
    TriangleMesh {
        positions: evaluated
            .vertices
            .iter()
            .map(|vertex| vertex.position)
            .collect(),
        indices: evaluated.indices,
    }
}

fn box_mesh(width: f32, height: f32, depth: f32) -> TriangleMesh {
    primitive_mesh(Primitive::Box {
        width,
        height,
        depth,
    })
}

fn volume(mesh: &TriangleMesh) -> f32 {
    let mut total = 0.0;
    for triangle in mesh.indices.as_chunks::<3>().0 {
        let a = mesh.positions[triangle[0] as usize];
        let b = mesh.positions[triangle[1] as usize];
        let c = mesh.positions[triangle[2] as usize];
        total += (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
            + a[2] * (b[0] * c[1] - b[1] * c[0]))
            / 6.0;
    }
    total
}

#[test]
fn box_minus_through_box_removes_volume() {
    let base = box_mesh(2.0, 2.0, 2.0);
    // Wider than the base in X so it cuts cleanly through.
    let cut = box_mesh(3.0, 0.5, 0.5);
    let result = csg_subtract(&base, &cut).expect("csg subtract");
    let expected = 8.0 - 2.0 * 0.5 * 0.5;
    assert!(
        (volume(&result) - expected).abs() < 1e-2,
        "volume {} vs {expected}",
        volume(&result)
    );
}

#[test]
fn box_minus_cylinder_removes_volume() {
    let base = box_mesh(2.0, 2.0, 2.0);
    let cut = primitive_mesh(Primitive::Cylinder {
        radius: 0.5,
        height: 4.0,
        segments: 32,
    });
    let result = csg_subtract(&base, &cut).expect("csg subtract");
    // 32-gon area is ~99.4% of the circle; height through the box is 2.
    let expected = 8.0 - 0.994 * std::f32::consts::PI * 0.25 * 2.0;
    assert!(
        (volume(&result) - expected).abs() < 0.05,
        "volume {} vs {expected}",
        volume(&result)
    );
}

#[test]
fn box_minus_inner_sphere_removes_volume() {
    let base = box_mesh(2.0, 2.0, 2.0);
    let cut = primitive_mesh(Primitive::Sphere {
        radius: 0.6,
        segments: 32,
        rings: 16,
    });
    let result = csg_subtract(&base, &cut).expect("csg subtract");
    let expected = 8.0 - 4.0 / 3.0 * std::f32::consts::PI * 0.216;
    assert!(
        (volume(&result) - expected).abs() < 0.05,
        "volume {} vs {expected}",
        volume(&result)
    );
}

#[test]
fn non_intersecting_cut_keeps_volume() {
    let base = box_mesh(1.0, 1.0, 1.0);
    let cut = primitive_mesh(Primitive::Box {
        width: 0.1,
        height: 0.1,
        depth: 0.1,
    });
    // Move the cut far away by baking coordinates.
    let far = TriangleMesh {
        positions: cut
            .positions
            .iter()
            .map(|position| [position[0] + 10.0, position[1], position[2]])
            .collect(),
        indices: cut.indices,
    };
    let result = csg_subtract(&base, &far).expect("csg subtract");
    assert!(
        (volume(&result) - 1.0).abs() < 1e-2,
        "volume {}",
        volume(&result)
    );
}

#[test]
fn part_cut_json_is_backward_compatible() {
    let box_cut: PartCut =
        serde_json::from_str(r#"{"width":1,"height":1,"depth":1,"x":0,"y":0,"z":0}"#)
            .expect("legacy box cut");
    assert!(matches!(box_cut, PartCut::Box(_)));

    let cylinder: PartCut =
        serde_json::from_str(r#"{"radius":1,"height":2,"x":0,"y":0,"z":0}"#).expect("cylinder cut");
    assert!(matches!(cylinder, PartCut::Cylinder(_)));

    let sphere: PartCut =
        serde_json::from_str(r#"{"radius":1,"x":0,"y":0,"z":0}"#).expect("sphere cut");
    assert!(matches!(sphere, PartCut::Sphere(_)));
}
