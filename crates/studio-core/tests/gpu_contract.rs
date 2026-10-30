use studio_core::gpu_contract::{
    BackendCapabilities, GeometryBackendKind, GeometryBackendMode, GeometryJobPriority,
};

#[test]
fn serializes_stable_backend_contract_values() {
    assert_eq!(
        serde_json::to_string(&GeometryBackendKind::Gpu).unwrap(),
        "\"gpu\""
    );
    assert_eq!(
        serde_json::to_string(&GeometryBackendMode::Auto).unwrap(),
        "\"auto\""
    );
    assert_eq!(
        serde_json::to_string(&GeometryJobPriority::Interactive).unwrap(),
        "\"interactive\""
    );
}

#[test]
fn reports_safe_capability_defaults() {
    let capabilities = BackendCapabilities::cpu();
    assert_eq!(capabilities.kind, GeometryBackendKind::Cpu);
    assert_eq!(capabilities.health, "ready");
    assert!(!capabilities.accelerated);
    assert_eq!(capabilities.memory_budget_bytes, 0);
}
