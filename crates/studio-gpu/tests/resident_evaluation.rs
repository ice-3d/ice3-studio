use studio_core::document::NodeContent;
use studio_core::geometry::{TriangleMesh, evaluate_node_content};
use studio_core::gpu_contract::GeometryEvaluationMode;
use studio_gpu::NativeGpuBackend;

#[test]
fn second_evaluation_reuses_topology_without_readback() {
    let Some(backend) = NativeGpuBackend::for_test_gpu() else {
        return;
    };
    let mesh = synthetic_mesh(500_000);
    let first = backend.evaluate_custom_mesh_resident(&mesh).unwrap();
    let second = backend.evaluate_custom_mesh_resident(&mesh).unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(second.mode, GeometryEvaluationMode::Resident);
    assert_eq!(second.evidence.mode, GeometryEvaluationMode::Resident);
    assert_eq!(second.evidence.stats.readback_bytes, 0);
    assert!(second.evidence.resident_hits >= 1);
}

#[test]
fn snapshot_is_the_only_custom_mesh_path_that_reads_back() {
    let Some(backend) = NativeGpuBackend::for_test_gpu() else {
        return;
    };
    let mesh = synthetic_mesh(50_000);
    let resident = backend.evaluate_custom_mesh_resident(&mesh).unwrap();
    let before = backend.resident_stats().unwrap().stats.readback_bytes;
    let snapshot = backend.snapshot_geometry(&resident.id).unwrap();
    let cpu = evaluate_node_content(&NodeContent::CustomMesh(mesh.clone())).unwrap();
    assert_eq!(snapshot.indices, cpu.indices);
    assert_eq!(snapshot.vertices.len(), cpu.vertices.len());
    assert!(
        backend.resident_stats().unwrap().stats.readback_bytes - before
            >= snapshot.vertices.len() as u64 * 16
    );
    backend.pin_resident(&resident.id).unwrap();
    backend.unpin_resident(&resident.id);
}

fn synthetic_mesh(triangle_count: u32) -> TriangleMesh {
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
