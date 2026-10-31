use studio_core::gpu_contract::{
    BackendCapabilities, BackendError, GeometryBackend, GeometryBatch, GeometryBatchResult,
    GeometryDirtyChannel, GeometryDirtyMask, GeometryEvaluationMode, GeometryExecutionStats,
    GeometryJobPriority,
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
