use studio_core::document::NodeContent;
use studio_core::geometry::{TriangleMesh, evaluate_node_content};
use studio_core::gpu_contract::{GeometryEvaluationMode, ResidentGeometryHandle};
use studio_gpu::NativeGpuBackend;

#[test]
fn repeated_digest_handle_lookup_does_not_reserialize_mesh() {
    let Some(backend) = NativeGpuBackend::for_test_gpu() else {
        return;
    };
    let mesh = synthetic_mesh(50_000);
    let digests = backend
        .prepare_geometry_digests(&NodeContent::CustomMesh(mesh.clone()))
        .unwrap();
    let first = backend
        .evaluate_with_digests(&mesh, digests, "component/body")
        .unwrap();
    let before = backend.resident_phase_counters().unwrap();

    for _ in 0..16 {
        backend.evaluate_resident_handle(&first.id).unwrap();
    }

    let after = backend.resident_phase_counters().unwrap();
    assert_eq!(after.full_hashes, before.full_hashes);
    assert_eq!(after.upload_bytes, before.upload_bytes);
    assert_eq!(after.fence_waits, before.fence_waits);
}

#[test]
fn stale_device_generation_rejects_the_handle() {
    let Some(backend) = NativeGpuBackend::for_test_gpu() else {
        return;
    };
    let mesh = synthetic_mesh(2);
    let digests = backend
        .prepare_geometry_digests(&NodeContent::CustomMesh(mesh.clone()))
        .unwrap();
    let resident = backend
        .evaluate_with_digests(&mesh, digests, "component/body")
        .unwrap();
    let handle = backend.resident_handle(&resident.id).unwrap();
    let stale = ResidentGeometryHandle {
        device_generation: handle.device_generation.saturating_sub(1),
        ..handle
    };
    assert!(backend.evaluate_resident_handle_checked(&stale).is_err());
}

#[test]
fn resident_hit_reports_zero_gpu_and_transfer_phases() {
    let Some(backend) = NativeGpuBackend::for_test_gpu() else {
        return;
    };
    let mesh = synthetic_mesh(2);
    let digests = backend
        .prepare_geometry_digests(&NodeContent::CustomMesh(mesh.clone()))
        .unwrap();
    let cold = backend
        .evaluate_with_digests(&mesh, digests, "component/body")
        .unwrap();
    assert!(cold.evidence.phases.validation_ms > 0.0);
    assert!(cold.evidence.phases.encode_ms > 0.0);
    assert!(cold.evidence.phases.submit_ms > 0.0);
    assert!(cold.evidence.phases.wait_ms > 0.0);

    let before = backend.resident_phase_counters().unwrap();
    let resident = backend.evaluate_resident_handle(&cold.id).unwrap();
    let after = backend.resident_phase_counters().unwrap();
    assert_eq!(after.upload_bytes, before.upload_bytes);
    assert_eq!(after.fence_waits, before.fence_waits);
    assert!(resident.evidence.phases.lookup_ms > 0.0);
    assert_eq!(resident.evidence.phases.gpu_kernel_ms, None);
}

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
