use studio_core::document::NodeContent;
use studio_core::geometry::Primitive;
use studio_core::gpu_contract::{
    GeometryBackend, GeometryBackendKind, GeometryBackendMode, GeometryBatch, GeometryJobNode,
    GeometryJobPriority,
};
use studio_core::ids::NodeId;
use studio_gpu::NativeGpuBackend;

#[test]
fn cpu_mode_never_requires_a_gpu_adapter() {
    let _probe_type = std::any::type_name::<studio_gpu::CapabilityProbe>();
    let backend = NativeGpuBackend::new(GeometryBackendMode::Cpu);
    let capabilities = pollster::block_on(backend.capabilities()).unwrap();
    assert_eq!(capabilities.kind, GeometryBackendKind::Cpu);
    assert!(!capabilities.accelerated);
}

#[test]
fn gpu_discovery_is_bounded_and_returns_structured_state() {
    let backend = NativeGpuBackend::new(GeometryBackendMode::Gpu);
    match pollster::block_on(backend.initialize()) {
        Ok(capabilities) => {
            assert_eq!(capabilities.kind, GeometryBackendKind::Gpu);
            assert_eq!(capabilities.health, "ready");
        }
        Err(error) => assert!(
            error.to_string().contains("unavailable") || error.to_string().contains("incompatible"),
            "{error}"
        ),
    }
}

#[test]
fn batch_evaluation_uses_the_native_gpu_kernels() {
    let Some(backend) = NativeGpuBackend::for_test_gpu() else {
        return;
    };
    let node_id = NodeId::from("gpu-box");
    let result = pollster::block_on(backend.evaluate(GeometryBatch {
        job_id: "gpu-box-job".into(),
        priority: GeometryJobPriority::Interactive,
        budget_bytes: 64 * 1024 * 1024,
        nodes: vec![GeometryJobNode {
            node_id: node_id.clone(),
            content: NodeContent::Primitive(Primitive::Box {
                width: 2.0,
                height: 3.0,
                depth: 4.0,
            }),
            content_digest: String::new(),
            dependency_digest: String::new(),
            cache_key: String::new(),
        }],
    }))
    .unwrap();

    assert_eq!(result.backend, GeometryBackendKind::Gpu);
    assert_eq!(result.meshes[&node_id].vertices.len(), 24);
    assert!(result.diagnostics.is_empty());
}
