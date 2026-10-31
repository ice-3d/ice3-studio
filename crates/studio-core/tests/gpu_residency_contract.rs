use studio_core::gpu_contract::{
    BackendCapabilities, BackendError, GeometryBackend, GeometryBatch, GeometryBatchResult,
    GeometryDigestSet, GeometryDirtyChannel, GeometryDirtyMask, GeometryEvaluationMode,
    GeometryExecutionStats, GeometryJobPriority, ResidentGeometryHandle, ResidentGeometryId,
    ResidentGeometryState,
};

#[test]
fn dirty_channels_round_trip_as_a_stable_mask() {
    let mut mask = GeometryDirtyMask::empty();
    mask.insert(GeometryDirtyChannel::Positions);
    mask.insert(GeometryDirtyChannel::Normals);
    mask.insert(GeometryDirtyChannel::Bounds);
    assert_eq!(mask.bits(), 0b0001_1010);
    assert!(mask.contains(GeometryDirtyChannel::Normals));
    assert!(!mask.contains(GeometryDirtyChannel::Topology));
    assert_eq!(
        serde_json::to_string(&GeometryEvaluationMode::Resident).unwrap(),
        "\"resident\""
    );
}

#[test]
fn digest_set_round_trips_and_does_not_contain_payload() {
    let digests = sample_digests();
    let encoded = serde_json::to_value(&digests).unwrap();
    assert_eq!(encoded["topology"], "a".repeat(64));
    assert_eq!(encoded["kernelVersion"], "geometry-kernel-v1");
    assert!(encoded.get("positions").is_none());
}

#[test]
fn resident_handle_serializes_device_generation_and_state() {
    let handle = ResidentGeometryHandle {
        id: ResidentGeometryId("geometry-1".into()),
        geometry_id: "component/body".into(),
        digests: sample_digests(),
        vertex_count: 3,
        triangle_count: 1,
        device_generation: 4,
        state: ResidentGeometryState::Ready,
    };
    let encoded = serde_json::to_value(handle).unwrap();
    assert_eq!(encoded["deviceGeneration"], 4);
    assert_eq!(encoded["state"], "ready");
}

fn sample_digests() -> GeometryDigestSet {
    GeometryDigestSet {
        topology: "a".repeat(64),
        attribute: "b".repeat(64),
        dependency: "c".repeat(64),
        kernel_version: "geometry-kernel-v1".into(),
        parity_version: "parity-1e-5-v1".into(),
    }
}

#[test]
fn resident_backend_defaults_are_explicitly_unsupported() {
    struct Reference;

    #[async_trait::async_trait]
    impl GeometryBackend for Reference {
        async fn initialize(&self) -> Result<BackendCapabilities, BackendError> {
            panic!("unused")
        }

        async fn evaluate(
            &self,
            _batch: GeometryBatch,
        ) -> Result<GeometryBatchResult, BackendError> {
            panic!("unused")
        }

        async fn cancel(&self, _job_id: &str) -> Result<(), BackendError> {
            Ok(())
        }

        fn stats(&self) -> GeometryExecutionStats {
            GeometryExecutionStats::default()
        }
    }

    let error = pollster::block_on(Reference.evaluate_resident(GeometryBatch {
        job_id: "resident".into(),
        priority: GeometryJobPriority::Interactive,
        budget_bytes: 1024,
        nodes: Vec::new(),
    }))
    .unwrap_err();
    assert!(error.to_string().contains("resident"));
}
