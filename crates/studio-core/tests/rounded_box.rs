//! `PartContent::RoundedBox`: fillets, bevels and their failure modes.

mod support;

use std::collections::BTreeMap;

use studio_core::geometry::EvaluatedMesh;
use studio_core::large_scene::component::{
    ComponentParameter, ComponentPart, ParameterType, ParameterValue, PartContent, PartTransform,
    ScalarBinding,
};
use studio_core::large_scene::evaluate::evaluate_large_scene;
use support::shared_pump_document;

fn rounded_document(radius: f32, segments: u32) -> studio_core::document::Document {
    let mut document = shared_pump_document();
    document.components[0].parts = vec![ComponentPart {
        id: "shell".into(),
        parent_id: None,
        transform: PartTransform::default(),
        content: PartContent::RoundedBox {
            width: ScalarBinding::Literal(2.0),
            height: ScalarBinding::Literal(2.0),
            depth: ScalarBinding::Literal(2.0),
            radius: ScalarBinding::Literal(radius),
            segments,
        },
        subtract: Vec::new(),
        repeat: None,
        material_role: None,
        metadata: BTreeMap::new(),
        visible: true,
    }];
    document
}

fn mesh_of(mesh: &EvaluatedMesh) -> f32 {
    let mut total: f32 = 0.0;
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

fn analytic_volume(edge: f32, radius: f32) -> f32 {
    let a = edge - 2.0 * radius;
    a * a * a
        + 6.0 * a * a * radius
        + 3.0 * std::f32::consts::PI * a * radius * radius
        + 4.0 / 3.0 * std::f32::consts::PI * radius * radius * radius
}

#[test]
fn rounded_part_rounds_the_corners_of_the_box() {
    let document = rounded_document(0.25, 8);
    let scene = evaluate_large_scene(&document).unwrap();
    let mesh = scene.meshes.values().next().expect("one shared mesh");
    let volume = mesh_of(mesh);
    let expected = analytic_volume(2.0, 0.25);
    assert!(
        (volume - expected).abs() / expected < 0.02,
        "volume {volume} vs {expected}"
    );
    assert!(volume < 8.0, "rounding must remove material, got {volume}");
}

#[test]
fn a_single_segment_produces_a_bevel() {
    let bevel = rounded_document(0.25, 1);
    let scene = evaluate_large_scene(&bevel).unwrap();
    let mesh = scene.meshes.values().next().expect("one shared mesh");
    let bevel_volume = mesh_of(mesh);
    let bevel_triangles = mesh.indices.len() / 3;

    let round = rounded_document(0.25, 8);
    let scene = evaluate_large_scene(&round).unwrap();
    let mesh = scene.meshes.values().next().expect("one shared mesh");
    let round_volume = mesh_of(mesh);

    assert!(
        bevel_triangles < mesh.indices.len() / 3,
        "bevel {bevel_triangles} should be cheaper than {} triangles",
        mesh.indices.len() / 3
    );
    assert!(
        bevel_volume < round_volume,
        "bevel {bevel_volume} must cut more material than the fillet {round_volume}"
    );
    assert!(bevel_volume > 1.5_f32.powi(3));
}

#[test]
fn radius_at_half_the_edge_is_rejected() {
    let document = rounded_document(1.0, 4);
    assert!(evaluate_large_scene(&document).is_err());
}

#[test]
fn radius_can_come_from_a_component_parameter() {
    let mut document = rounded_document(0.1, 4);
    document.components[0].parameters = vec![ComponentParameter {
        name: "fillet".into(),
        value_type: ParameterType::Number {
            minimum: None,
            maximum: None,
        },
        default: ParameterValue::Number(0.3),
        unit: None,
    }];
    if let PartContent::RoundedBox { radius, .. } = &mut document.components[0].parts[0].content {
        *radius = ScalarBinding::Parameter {
            param: "fillet".into(),
        };
    }
    let scene = evaluate_large_scene(&document).unwrap();
    let mesh = scene.meshes.values().next().expect("one shared mesh");
    let volume = mesh_of(mesh);
    let expected = analytic_volume(2.0, 0.3);
    assert!(
        (volume - expected).abs() / expected < 0.03,
        "volume {volume} vs {expected}"
    );
}
