use studio_core::gpu_contract::{GeometryBackend, GeometryBackendKind, GeometryBackendMode};
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
