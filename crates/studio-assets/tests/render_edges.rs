use serde_json::json;
use studio_assets::{edge_color_from_material, generate_edge_indices, material_suppresses_edges};

#[test]
fn emits_boundary_edges_for_one_triangle() {
    let positions = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    let edges = generate_edge_indices(&positions, &[0, 1, 2], 30.0).unwrap();
    assert_eq!(edges, vec![0, 1, 1, 2, 0, 2]);
}

#[test]
fn drops_coplanar_shared_edges_and_keeps_quad_boundaries() {
    let (positions, indices) = unit_quad();
    let edges = generate_edge_indices(&positions, &indices, 30.0).unwrap();
    assert_eq!(edges, vec![0, 1, 0, 2, 1, 3, 2, 3]);
}

#[test]
fn emits_cube_crease_edges_in_first_seen_order() {
    let (positions, indices) = unit_cube();
    let edges = generate_edge_indices(&positions, &indices, 30.0).unwrap();
    assert_eq!(edges.len(), 24);
    assert_eq!(
        edges,
        generate_edge_indices(&positions, &indices, 30.0).unwrap()
    );
}

#[test]
fn zero_crease_uses_legacy_tolerance_for_shared_edges() {
    let (quad_positions, quad_indices) = unit_quad();
    assert_eq!(
        generate_edge_indices(&quad_positions, &quad_indices, 0.0)
            .unwrap()
            .len(),
        10
    );

    let (cube_positions, cube_indices) = unit_cube();
    assert_eq!(
        generate_edge_indices(&cube_positions, &cube_indices, 0.0)
            .unwrap()
            .len(),
        36
    );
}

#[test]
fn covers_spec_crease_angle_matrix() {
    let (positions, indices) = unit_cube();
    assert_eq!(
        generate_edge_indices(&positions, &indices, 60.0)
            .unwrap()
            .len(),
        24
    );
    assert_eq!(
        generate_edge_indices(&positions, &indices, 90.0)
            .unwrap()
            .len(),
        24
    );
    assert_eq!(
        generate_edge_indices(&positions, &indices, 180.0)
            .unwrap()
            .len(),
        0
    );
}

#[test]
fn accepts_empty_triangles_and_rejects_degenerate_triangles() {
    let positions = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]];
    assert!(
        generate_edge_indices(&positions, &[], 30.0)
            .unwrap()
            .is_empty()
    );

    for indices in [vec![0, 1, 2], vec![0, 0, 2], vec![0, 1, 1], vec![0, 1, 0]] {
        assert_eq!(
            generate_edge_indices(&positions, &indices, 30.0)
                .unwrap_err()
                .code(),
            "ICE3_STUDIO_ASSET_INVALID_GEOMETRY"
        );
    }

    let coincident = [[0.0, 0.0, 0.0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    assert_eq!(
        generate_edge_indices(&coincident, &[0, 1, 2], 30.0)
            .unwrap_err()
            .code(),
        "ICE3_STUDIO_ASSET_INVALID_GEOMETRY"
    );
}

#[test]
fn accepts_small_nonzero_triangles() {
    let positions = [[12.0, 0.0, 0.0], [12.0, 0.001, 0.0], [12.0, 0.0, 0.001]];
    assert_eq!(
        generate_edge_indices(&positions, &[0, 1, 2], 30.0)
            .unwrap()
            .len(),
        6
    );
}

#[test]
fn rejects_out_of_range_and_non_finite_inputs() {
    let errors = [
        generate_edge_indices(&[[0.0; 3]], &[0, 1, 2], 30.0).unwrap_err(),
        generate_edge_indices(
            &[[0.0, f32::NAN, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            &[0, 1, 2],
            30.0,
        )
        .unwrap_err(),
        generate_edge_indices(&[[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], &[0, 1], 30.0)
            .unwrap_err(),
        generate_edge_indices(
            &[[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            &[0, 1, 2],
            f32::NAN,
        )
        .unwrap_err(),
    ];

    for error in errors {
        assert_eq!(error.code(), "ICE3_STUDIO_ASSET_INVALID_GEOMETRY");
    }
}

#[test]
fn resolves_edge_color_and_material_suppression() {
    let custom = json!({"ice3":{"edgeColor":"#10203080"}});
    assert_eq!(
        edge_color_from_material(&custom).unwrap(),
        [16.0 / 255.0, 32.0 / 255.0, 48.0 / 255.0, 128.0 / 255.0]
    );

    let six_digit = json!({"ice3":{"edgeColor":"#102030"}});
    assert_eq!(
        edge_color_from_material(&six_digit).unwrap(),
        [16.0 / 255.0, 32.0 / 255.0, 48.0 / 255.0, 1.0]
    );

    let default = json!({});
    assert_eq!(
        edge_color_from_material(&default).unwrap(),
        [230.0 / 255.0, 242.0 / 255.0, 1.0, 1.0]
    );

    let suppressed = json!({"ice3":{"edges":false}});
    assert!(material_suppresses_edges(&suppressed));
    assert!(!material_suppresses_edges(&json!({"ice3":{"edges":true}})));
}

#[test]
fn rejects_invalid_edge_color_metadata() {
    for invalid in [
        json!({"ice3":{"edgeColor":"red"}}),
        json!({"ice3":{"edgeColor":"#12345"}}),
        json!({"ice3":{"edgeColor":"#zzzzzz"}}),
        json!({"ice3":"invalid"}),
    ] {
        assert_eq!(
            edge_color_from_material(&invalid).unwrap_err().code(),
            "ICE3_STUDIO_ASSET_INVALID_GEOMETRY"
        );
    }
}

fn unit_quad() -> (Vec<[f32; 3]>, Vec<u32>) {
    (
        vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
        ],
        vec![0, 1, 2, 1, 3, 2],
    )
}

fn unit_cube() -> (Vec<[f32; 3]>, Vec<u32>) {
    (
        vec![
            [-1.0, -1.0, 1.0],
            [1.0, -1.0, 1.0],
            [1.0, 1.0, 1.0],
            [-1.0, 1.0, 1.0],
            [-1.0, -1.0, -1.0],
            [1.0, -1.0, -1.0],
            [1.0, 1.0, -1.0],
            [-1.0, 1.0, -1.0],
        ],
        vec![
            0, 1, 2, 0, 2, 3, 5, 4, 7, 5, 7, 6, 4, 0, 3, 4, 3, 7, 1, 5, 6, 1, 6, 2, 3, 2, 6, 3, 6,
            7, 4, 5, 1, 4, 1, 0,
        ],
    )
}
