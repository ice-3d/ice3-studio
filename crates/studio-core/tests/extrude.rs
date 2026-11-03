//! Extrusion (sketch → prism) geometry tests: triangle counts, enclosed volume
//! and rejection of degenerate profiles.

use studio_core::geometry::{TriangleMesh, extrude_profile};

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
fn square_profile_extrudes_to_a_box() {
    let mesh = extrude_profile(&[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]], 2.0)
        .expect("valid square");
    // Two caps (2 triangles each) + four side walls (2 triangles each).
    assert_eq!(mesh.indices.len() / 3, 12);
    assert!(
        (volume(&mesh) - 2.0).abs() < 1e-5,
        "volume {}",
        volume(&mesh)
    );
}

#[test]
fn clockwise_profile_is_reoriented() {
    let mesh = extrude_profile(&[[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]], 1.0)
        .expect("valid square");
    assert!(
        (volume(&mesh) - 1.0).abs() < 1e-5,
        "volume {}",
        volume(&mesh)
    );
}

#[test]
fn concave_profile_triangulates() {
    // L-shaped polygon, area 3.
    let profile = [
        [0.0, 0.0],
        [2.0, 0.0],
        [2.0, 1.0],
        [1.0, 1.0],
        [1.0, 2.0],
        [0.0, 2.0],
    ];
    let mesh = extrude_profile(&profile, 1.0).expect("valid L");
    // 4 cap triangles per side + 6 walls × 2.
    assert_eq!(mesh.indices.len() / 3, 20);
    assert!(
        (volume(&mesh) - 3.0).abs() < 1e-5,
        "volume {}",
        volume(&mesh)
    );
}

#[test]
fn duplicate_closing_point_is_tolerated() {
    let mesh = extrude_profile(
        &[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.0, 0.0]],
        1.0,
    )
    .expect("closing duplicate dropped");
    assert!((volume(&mesh) - 1.0).abs() < 1e-5);
}

#[test]
fn degenerate_profiles_are_rejected() {
    // Fewer than three points.
    assert!(extrude_profile(&[[0.0, 0.0], [1.0, 0.0]], 1.0).is_err());
    // Collinear points (zero area).
    assert!(extrude_profile(&[[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]], 1.0).is_err());
    // Non-positive height.
    assert!(extrude_profile(&[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]], 0.0).is_err());
    // Non-finite coordinate.
    assert!(extrude_profile(&[[0.0, f32::NAN], [1.0, 0.0], [1.0, 1.0]], 1.0).is_err());
}
