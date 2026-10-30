use studio_core::document::NodeContent;
use studio_core::geometry::{Bounds, TriangleMesh, evaluate_node_content};
use studio_gpu::NativeGpuBackend;

fn synthetic_mesh(triangle_count: u32) -> TriangleMesh {
    assert!(triangle_count.is_multiple_of(2));
    let quad_count = triangle_count / 2;
    let mut columns = (quad_count as f64).sqrt().floor() as u32;
    while !quad_count.is_multiple_of(columns) {
        columns -= 1;
    }
    let rows = quad_count / columns;
    let stride = columns + 1;
    let mut positions = Vec::with_capacity(stride as usize * (rows + 1) as usize);
    for row in 0..=rows {
        for column in 0..=columns {
            positions.push([
                column as f32 * 0.01,
                row as f32 * 0.01,
                (column % 7) as f32 * 0.01 + (row % 5) as f32 * 0.02,
            ]);
        }
    }
    let mut indices = Vec::with_capacity(triangle_count as usize * 3);
    for row in 0..rows {
        for column in 0..columns {
            let a = row * stride + column;
            let b = a + stride;
            indices.extend([a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    TriangleMesh { positions, indices }
}

fn assert_bounds_within_relative_error(actual: Bounds, expected: Bounds, tolerance: f32) {
    let pairs = [
        (actual.min.x, expected.min.x),
        (actual.min.y, expected.min.y),
        (actual.min.z, expected.min.z),
        (actual.max.x, expected.max.x),
        (actual.max.y, expected.max.y),
        (actual.max.z, expected.max.z),
    ];
    for (actual, expected) in pairs {
        let scale = expected.abs().max(1.0);
        assert!((actual - expected).abs() <= tolerance * scale);
    }
}

fn assert_normals_within(
    actual: &[studio_core::geometry::Vertex],
    expected: &[studio_core::geometry::Vertex],
    tolerance: f32,
) {
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected) {
        for axis in 0..3 {
            assert!((actual.position[axis] - expected.position[axis]).abs() <= tolerance);
            assert!((actual.normal[axis] - expected.normal[axis]).abs() <= tolerance);
        }
    }
}

fn assert_mesh_parity(triangle_count: u32) {
    let mesh = synthetic_mesh(triangle_count);
    let Some(backend) = NativeGpuBackend::for_test_gpu() else {
        eprintln!("SKIP: native GPU unavailable for {triangle_count} triangles");
        return;
    };
    let gpu = backend.evaluate_custom_mesh(&mesh).unwrap();
    let cpu = evaluate_node_content(&NodeContent::CustomMesh(mesh)).unwrap();
    assert_eq!(gpu.indices, cpu.indices);
    assert_bounds_within_relative_error(gpu.bounds, cpu.bounds, 1e-5);
    assert_normals_within(&gpu.vertices, &cpu.vertices, 1e-5);
}

#[test]
fn custom_mesh_normals_and_bounds_match_cpu_reference() {
    for triangle_count in [50_000, 500_000, 2_000_000] {
        assert_mesh_parity(triangle_count);
    }
}
