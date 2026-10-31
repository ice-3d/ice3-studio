use studio_core::document::NodeContent;
use studio_core::geometry::Primitive;
use studio_core::gpu_contract::{
    GeometryBackend, GeometryBatch, GeometryJobNode, GeometryJobPriority,
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
