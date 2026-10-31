use studio_core::document::NodeContent;
use studio_core::geometry::{Primitive, TriangleMesh};
use studio_core::gpu_contract::{
    GeometryBackend, GeometryBatch, GeometryJobNode, GeometryJobPriority, ResidentGeometryState,
};
use studio_core::ids::NodeId;
use studio_gpu::NativeGpuBackend;

#[test]
fn compatible_nodes_share_one_submission_and_one_completion() {
    let Some(backend) = NativeGpuBackend::for_test_gpu() else {
        return;
    };
    let result = backend
        .evaluate_resident(box_batch("box-batch", 32))
        .unwrap();
    assert_eq!(result.entries.len(), 32);
    assert_eq!(result.evidence.submission_count, 1);
    assert_eq!(result.evidence.stats.readback_bytes, 0);
    assert!(backend.poll_resident("box-batch").is_some());
    assert!(backend.wait_resident("box-batch").is_err());
}

#[test]
fn distinct_compatible_meshes_share_one_submission() {
    let Some(backend) = NativeGpuBackend::for_test_gpu() else {
        return;
    };
    let result = backend
        .evaluate_resident(three_node_batch(
            "mixed-batch",
            triangle_mesh_with_offset(0.0),
            triangle_mesh_with_offset(1.0),
        ))
        .unwrap();
    assert_eq!(result.entries.len(), 3);
    assert_eq!(result.evidence.submission_count, 1);
    assert_eq!(result.evidence.phases.wait_ms, 0.0);
    let handle = backend.resident_handle(&result.entries[0].id).unwrap();
    assert_eq!(handle.state, ResidentGeometryState::Pending);
    let ready = backend.evaluate_resident_handle(&handle.id).unwrap();
    assert_eq!(
        ready.mode,
        studio_core::gpu_contract::GeometryEvaluationMode::Resident
    );
    assert_eq!(
        backend.resident_handle(&handle.id).unwrap().state,
        ResidentGeometryState::Ready
    );
}

#[test]
fn cancelled_background_batch_never_publishes_entries() {
    let Some(backend) = NativeGpuBackend::for_test_gpu() else {
        return;
    };
    pollster::block_on(backend.cancel("background-batch")).unwrap();
    let result = backend.evaluate_resident(box_batch("background-batch", 2));
    assert!(result.is_err());
    assert!(backend.poll_resident("background-batch").is_none());
}

fn box_batch(job_id: &str, count: u32) -> GeometryBatch {
    GeometryBatch {
        job_id: job_id.into(),
        priority: GeometryJobPriority::Background,
        budget_bytes: 128 * 1024 * 1024,
        nodes: (0..count)
            .map(|ordinal| GeometryJobNode {
                node_id: NodeId::from(format!("node-{ordinal:06}")),
                content: NodeContent::Primitive(Primitive::Box {
                    width: 1.0,
                    height: 1.0,
                    depth: 1.0,
                }),
                content_digest: String::new(),
                dependency_digest: String::new(),
                cache_key: String::new(),
                digests: None,
            })
            .collect(),
    }
}

fn three_node_batch(job_id: &str, first: TriangleMesh, second: TriangleMesh) -> GeometryBatch {
    GeometryBatch {
        job_id: job_id.into(),
        priority: GeometryJobPriority::Interactive,
        budget_bytes: 128 * 1024 * 1024,
        nodes: [first.clone(), second, first]
            .into_iter()
            .enumerate()
            .map(|(index, mesh)| GeometryJobNode {
                node_id: NodeId::from(format!("mixed-{index}")),
                content: NodeContent::CustomMesh(mesh),
                content_digest: String::new(),
                dependency_digest: String::new(),
                cache_key: String::new(),
                digests: None,
            })
            .collect(),
    }
}

fn triangle_mesh_with_offset(offset: f32) -> TriangleMesh {
    TriangleMesh {
        positions: vec![
            [offset, 0.0, 0.0],
            [offset + 1.0, 0.0, 0.0],
            [offset, 1.0, 0.0],
        ],
        indices: vec![0, 1, 2],
    }
}
