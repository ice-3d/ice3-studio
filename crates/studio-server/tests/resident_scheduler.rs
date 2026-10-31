use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use studio_core::document::{Document, NodeContent, SceneNode};
use studio_core::geometry::{EvaluatedMesh, Primitive, evaluate_node_content};
use studio_core::gpu_contract::{
    BackendCapabilities, BackendError, GeometryBackend, GeometryBackendKind, GeometryBackendMode,
    GeometryBatch, GeometryBatchResult, GeometryEvaluationEvidence, GeometryEvaluationMode,
    GeometryExecutionStats, GeometryResidentBatchResult, GeometryResidentEntry,
    GeometrySnapshotRequest, ResidentGeometryHandle, ResidentGeometryId,
};
use studio_core::ids::{ComponentId, MaterialId, NodeId};
use studio_core::large_scene::cache::{
    GeometryCacheMetadata, GeometryEvaluation, GeometryEvaluator, MemoryGeometryStore,
};
use studio_core::large_scene::component::{
    ComponentDefinition, ComponentInstance, ComponentPart, PartContent, PartTransform,
    ScalarBinding,
};
use studio_core::large_scene::evaluate::{
    evaluate_large_scene, evaluate_large_scene_with_cache_and_evaluator, update_material_only,
    update_transform_only,
};
use studio_core::math::Transform;
use studio_server::compute::{
    ComputeEvaluator, ComputePolicy, ComputeService, GeometryEvaluationPurpose, SnapshotPolicy,
};
use studio_server::preview_packet::{PreviewGeometrySourceView, build_resident_preview_packet};

struct FakeGpu {
    evaluate_calls: AtomicUsize,
    snapshot_calls: AtomicUsize,
    lose_resident_device: std::sync::atomic::AtomicBool,
}

impl FakeGpu {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            evaluate_calls: AtomicUsize::new(0),
            snapshot_calls: AtomicUsize::new(0),
            lose_resident_device: std::sync::atomic::AtomicBool::new(false),
        })
    }

    fn evaluate_calls(&self) -> usize {
        self.evaluate_calls.load(Ordering::SeqCst)
    }

    fn snapshot_calls(&self) -> usize {
        self.snapshot_calls.load(Ordering::SeqCst)
    }

    fn lose_resident_device(&self) {
        self.lose_resident_device.store(true, Ordering::SeqCst);
    }
}

fn box_content() -> NodeContent {
    NodeContent::Primitive(Primitive::Box {
        width: 1.0,
        height: 1.0,
        depth: 1.0,
    })
}

fn oversized_content() -> NodeContent {
    NodeContent::CustomMesh(studio_core::geometry::TriangleMesh {
        positions: vec![[0.0, 0.0, 0.0]; 5_000],
        indices: vec![0, 0, 0],
    })
}

fn mesh(content: &NodeContent) -> EvaluatedMesh {
    evaluate_node_content(content).unwrap()
}

fn resident_evidence(mode: GeometryEvaluationMode) -> GeometryEvaluationEvidence {
    GeometryEvaluationEvidence {
        mode,
        stats: GeometryExecutionStats::default(),
        submission_count: 1,
        resident_geometries: 1,
        resident_bytes: 4096,
        peak_resident_bytes: 4096,
        topology_hits: usize::from(mode == GeometryEvaluationMode::Resident),
        resident_hits: usize::from(mode == GeometryEvaluationMode::Resident),
        evictions: 0,
        preview_packet_bytes: 0,
        browser_upload_bytes: 0,
        phases: Default::default(),
    }
}

#[async_trait::async_trait]
impl GeometryBackend for FakeGpu {
    async fn initialize(&self) -> Result<BackendCapabilities, BackendError> {
        Ok(BackendCapabilities {
            kind: GeometryBackendKind::Gpu,
            health: "ready".into(),
            accelerated: true,
            adapter_tier: "mock".into(),
            backend_family: "fake".into(),
            memory_budget_bytes: 2 * 1024 * 1024 * 1024,
            fallback_reason: None,
        })
    }

    async fn evaluate(&self, batch: GeometryBatch) -> Result<GeometryBatchResult, BackendError> {
        self.evaluate_calls.fetch_add(1, Ordering::SeqCst);
        let meshes = batch
            .nodes
            .into_iter()
            .map(|node| (node.node_id, mesh(&node.content)))
            .collect();
        Ok(GeometryBatchResult {
            job_id: batch.job_id,
            backend: GeometryBackendKind::Gpu,
            meshes,
            stats: GeometryExecutionStats::default(),
            diagnostics: Vec::new(),
        })
    }

    async fn evaluate_resident(
        &self,
        batch: GeometryBatch,
    ) -> Result<GeometryResidentBatchResult, BackendError> {
        if self.lose_resident_device.load(Ordering::SeqCst) {
            return Err(BackendError::DeviceLost {
                message: "fake resident device removed".into(),
            });
        }
        let first = batch.nodes.first().ok_or_else(|| BackendError::Kernel {
            message: "empty resident batch".into(),
        })?;
        self.evaluate_calls.fetch_add(1, Ordering::SeqCst);
        let evaluated = mesh(&first.content);
        let mode = if self.evaluate_calls() == 1 {
            GeometryEvaluationMode::Cold
        } else {
            GeometryEvaluationMode::Resident
        };
        let evidence = resident_evidence(mode);
        Ok(GeometryResidentBatchResult {
            job_id: batch.job_id,
            backend: GeometryBackendKind::Gpu,
            entries: vec![GeometryResidentEntry {
                id: ResidentGeometryId(if first.cache_key.is_empty() {
                    "resident-1".into()
                } else {
                    first.cache_key.clone()
                }),
                node_id: first.node_id.clone(),
                mode,
                dirty: Default::default(),
                bounds: evaluated.bounds,
                vertex_count: evaluated.vertices.len(),
                triangle_count: evaluated.indices.len() / 3,
                evidence: evidence.clone(),
            }],
            evidence,
        })
    }

    async fn snapshot(
        &self,
        request: GeometrySnapshotRequest,
    ) -> Result<GeometryBatchResult, BackendError> {
        self.snapshot_calls.fetch_add(1, Ordering::SeqCst);
        let content = box_content();
        let node_id = studio_core::ids::NodeId::from("compute-node-000001");
        if request.geometry_ids.is_empty() || request.geometry_ids[0].0.is_empty() {
            return Err(BackendError::Kernel {
                message: "unexpected geometry id".into(),
            });
        }
        let mut meshes = BTreeMap::new();
        meshes.insert(node_id, mesh(&content));
        Ok(GeometryBatchResult {
            job_id: "snapshot".into(),
            backend: GeometryBackendKind::Gpu,
            meshes,
            stats: GeometryExecutionStats {
                readback_bytes: 64,
                ..GeometryExecutionStats::default()
            },
            diagnostics: Vec::new(),
        })
    }

    async fn cancel(&self, _job_id: &str) -> Result<(), BackendError> {
        Ok(())
    }

    fn stats(&self) -> GeometryExecutionStats {
        GeometryExecutionStats::default()
    }
}

fn resident_service(gpu: Arc<FakeGpu>) -> ComputeService {
    ComputeService::new(
        ComputePolicy {
            mode: GeometryBackendMode::Gpu,
            resident_enabled: true,
            ..ComputePolicy::default()
        },
        Some(gpu),
    )
}

struct ResidentEvaluator {
    calls: AtomicUsize,
}

impl GeometryEvaluator for ResidentEvaluator {
    fn select_cache_metadata(&self, _content: &NodeContent) -> GeometryCacheMetadata {
        GeometryCacheMetadata {
            backend: GeometryBackendKind::Gpu,
            kernel_version: "geometry-kernel-v1".into(),
            parity_status: "passed".into(),
        }
    }

    fn evaluate(&self, content: &NodeContent) -> Result<GeometryEvaluation, BackendError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(GeometryEvaluation {
            mesh: mesh(content),
            metadata: self.select_cache_metadata(content),
            resident_geometry_id: Some(ResidentGeometryId("resident-part".into())),
        })
    }
}

fn component_document() -> Document {
    let mut document = Document::new("resident-scene");
    document.components.push(ComponentDefinition {
        id: ComponentId::from("component-000001"),
        name: "Pump".into(),
        parameters: Vec::new(),
        parts: vec![ComponentPart {
            id: "body".into(),
            parent_id: None,
            transform: PartTransform::default(),
            content: PartContent::Box {
                width: ScalarBinding::Literal(2.0),
                height: ScalarBinding::Literal(1.0),
                depth: ScalarBinding::Literal(1.0),
            },
            material_role: None,
            metadata: BTreeMap::new(),
            visible: true,
        }],
        ports: Vec::new(),
        metadata: BTreeMap::new(),
    });
    let mut node = SceneNode::group(NodeId::from("pump-1"), "Pump");
    node.content = NodeContent::ComponentInstance(ComponentInstance {
        definition_id: ComponentId::from("component-000001"),
        parameters: BTreeMap::new(),
        overrides: BTreeMap::new(),
        metadata: BTreeMap::new(),
    });
    document.scene.nodes_mut().push(node);
    document
}

#[tokio::test]
async fn transform_only_change_performs_no_geometry_work() {
    let gpu = FakeGpu::new();
    let service = resident_service(gpu.clone());
    service.evaluate(box_content()).await.unwrap();
    service
        .evaluate_transform_only("shared", Transform::IDENTITY)
        .await
        .unwrap();

    assert_eq!(gpu.evaluate_calls(), 1);
    assert_eq!(gpu.snapshot_calls(), 0);
    assert_eq!(service.status().last_evaluation_mode, "resident");
}

#[tokio::test]
async fn export_requests_snapshot_while_preview_uses_residency() {
    let gpu = FakeGpu::new();
    let service = resident_service(gpu.clone());
    service.evaluate(box_content()).await.unwrap();
    service
        .evaluate_for(GeometryEvaluationPurpose::Preview)
        .await
        .unwrap();
    assert_eq!(service.status().last_evaluation_mode, "resident");

    service
        .snapshot_for(GeometryEvaluationPurpose::SourceExport)
        .await
        .unwrap();
    assert_eq!(service.status().last_evaluation_mode, "snapshot");
    assert_eq!(gpu.snapshot_calls(), 1);
}

#[test]
fn experimental_resident_and_preview_packet_modes_default_to_off() {
    let policy = ComputePolicy::default();
    assert!(!policy.resident_enabled);
    assert!(!policy.preview_packet_enabled);
}

#[test]
fn only_export_and_diagnostics_require_cpu_snapshots() {
    assert_eq!(
        SnapshotPolicy::for_purpose(GeometryEvaluationPurpose::Preview),
        SnapshotPolicy::Resident
    );
    assert_eq!(
        SnapshotPolicy::for_purpose(GeometryEvaluationPurpose::Candidate),
        SnapshotPolicy::Resident
    );
    assert_eq!(
        SnapshotPolicy::for_purpose(GeometryEvaluationPurpose::SourceExport),
        SnapshotPolicy::CpuReadable
    );
    assert_eq!(
        SnapshotPolicy::for_purpose(GeometryEvaluationPurpose::Diagnostics),
        SnapshotPolicy::CpuReadable
    );
}

#[test]
fn large_scene_propagates_resident_ids_and_updates_without_re_evaluation() {
    let evaluator = ResidentEvaluator {
        calls: AtomicUsize::new(0),
    };
    let mut store = MemoryGeometryStore::default();
    let mut scene = evaluate_large_scene_with_cache_and_evaluator(
        &component_document(),
        &mut store,
        Some(&evaluator),
    )
    .unwrap();
    assert_eq!(
        scene.nodes[0].resident_geometry_id,
        Some(ResidentGeometryId("resident-part".into()))
    );
    assert_eq!(evaluator.calls.load(Ordering::SeqCst), 1);

    let node_id = scene.nodes[0].id.clone();
    update_transform_only(&mut scene, &node_id, Transform::IDENTITY).unwrap();
    update_material_only(&mut scene, &node_id, Some(MaterialId::from("steel"))).unwrap();
    assert_eq!(evaluator.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn compute_evaluator_propagates_resident_ids_to_large_scene_nodes() {
    let gpu = FakeGpu::new();
    let service = resident_service(gpu.clone());
    let evaluator = ComputeEvaluator::new(&service);
    let mut store = MemoryGeometryStore::default();
    let scene = evaluate_large_scene_with_cache_and_evaluator(
        &component_document(),
        &mut store,
        Some(&evaluator),
    )
    .unwrap();

    assert!(scene.nodes[0].resident_geometry_id.is_some());
    assert_eq!(gpu.snapshot_calls(), 1);

    let second = evaluate_large_scene_with_cache_and_evaluator(
        &component_document(),
        &mut store,
        Some(&evaluator),
    )
    .unwrap();
    assert!(second.nodes[0].resident_geometry_id.is_some());
    assert_eq!(gpu.snapshot_calls(), 1);
}

#[test]
fn compute_evaluator_reuses_registered_handle_without_rehashing() {
    let gpu = FakeGpu::new();
    let service = resident_service(gpu.clone());
    let evaluator = ComputeEvaluator::new(&service);
    let content = box_content();
    let first = GeometryEvaluator::evaluate(&evaluator, &content).unwrap();
    let hashes = service.status().resident_hot_path_hashes;
    let second = GeometryEvaluator::evaluate(&evaluator, &content).unwrap();

    assert_eq!(service.status().resident_hot_path_hashes, hashes);
    assert_eq!(first.resident_geometry_id, second.resident_geometry_id);
    assert_eq!(gpu.snapshot_calls(), 2);
}

#[tokio::test]
async fn resident_evidence_is_exposed_in_compute_status() {
    let service = resident_service(FakeGpu::new());
    service.evaluate(box_content()).await.unwrap();
    service
        .evaluate_for(GeometryEvaluationPurpose::Preview)
        .await
        .unwrap();
    let status = service.status();
    assert_eq!(status.last_evaluation_mode, "resident");
    assert_eq!(status.resident_geometry.geometry_count, 1);
    assert_eq!(status.resident_geometry.resident_bytes, 4096);
    assert_eq!(status.resident_geometry.topology_hits, 1);
    assert!(status.pipelines_warmed);
}

#[tokio::test]
async fn repeated_handle_evaluation_skips_content_hashing() {
    let gpu = FakeGpu::new();
    let service = resident_service(gpu.clone());
    let handle = service
        .register_geometry("shared".into(), box_content())
        .await
        .unwrap();
    let first = service.evaluate_handle(&handle).await.unwrap();
    let hashes = service.status().resident_hot_path_hashes;
    let second = service.evaluate_handle(&first.handle).await.unwrap();
    assert_eq!(service.status().resident_hot_path_hashes, hashes);
    assert_eq!(second.mode, "resident");
}

#[tokio::test]
async fn stale_handle_falls_back_without_mutating_revision() {
    let gpu = FakeGpu::new();
    let service = resident_service(gpu);
    let handle = service
        .register_geometry("shared".into(), box_content())
        .await
        .unwrap();
    let document = component_document();
    let revision = document.revision;
    let stale = ResidentGeometryHandle {
        device_generation: handle.device_generation + 1,
        ..handle
    };
    assert!(service.evaluate_handle(&stale).await.is_err());
    assert_eq!(document.revision, revision);
}

#[test]
fn transform_only_preview_reuses_resident_source_without_readback() {
    let gpu = FakeGpu::new();
    let service = resident_service(gpu.clone());
    let handle =
        pollster::block_on(service.register_geometry("pump-1".into(), box_content())).unwrap();
    let evaluated = pollster::block_on(service.evaluate_handle(&handle)).unwrap();
    let before = gpu.snapshot_calls();
    let source = service.preview_source_view(&evaluated.handle).unwrap();
    let document = component_document();
    let scene = evaluate_large_scene(&document).unwrap();
    let packet = build_resident_preview_packet(&document, &scene, &[source], false);
    assert_eq!(
        packet.manifest.residency_mode,
        GeometryEvaluationMode::Resident
    );
    assert!(packet.manifest.geometries.is_empty());
    assert!(packet.binary.is_empty());
    assert_eq!(gpu.snapshot_calls(), before);
}

#[test]
fn gpu_derived_attribute_requests_explicit_snapshot() {
    let source = PreviewGeometrySourceView {
        handle: ResidentGeometryHandle {
            id: ResidentGeometryId("geometry-1".into()),
            geometry_id: "pump-1".into(),
            digests: studio_core::gpu_contract::GeometryDigestSet {
                topology: "a".repeat(64),
                attribute: "b".repeat(64),
                dependency: "c".repeat(64),
                kernel_version: "geometry-kernel-v1".into(),
                parity_version: "parity-1e-5-v1".into(),
            },
            vertex_count: 3,
            triangle_count: 1,
            device_generation: 1,
            state: studio_core::gpu_contract::ResidentGeometryState::Ready,
        },
        positions: Arc::new(vec![[0.0, 0.0, 0.0]; 3]),
        indices: Arc::new(vec![0, 1, 2]),
        needs_gpu_normals: true,
    };
    assert!(source.requires_snapshot());
}

#[tokio::test]
async fn resident_device_loss_discards_handles_without_mutating_revision() {
    let gpu = FakeGpu::new();
    let service = resident_service(gpu.clone());
    service.evaluate(box_content()).await.unwrap();
    service
        .evaluate_for(GeometryEvaluationPurpose::Preview)
        .await
        .unwrap();
    let document = component_document();
    let revision = document.revision;
    gpu.lose_resident_device();

    let error = service
        .evaluate_for(GeometryEvaluationPurpose::Preview)
        .await
        .unwrap_err();
    assert_eq!(error.diagnostic_code(), "ICE3_STUDIO_GPU_DEVICE_LOST");
    assert_eq!(document.revision, revision);
    let status = service.status();
    assert_eq!(status.health, "unavailable");
    assert_eq!(status.resident_geometry.geometry_count, 0);

    let snapshot_error = service
        .snapshot_for(GeometryEvaluationPurpose::SourceExport)
        .await
        .unwrap_err();
    assert_eq!(
        snapshot_error.diagnostic_code(),
        "ICE3_STUDIO_GPU_UNAVAILABLE"
    );
}

#[tokio::test]
async fn resident_preview_respects_the_effective_gpu_budget() {
    let service = ComputeService::new(
        ComputePolicy {
            mode: GeometryBackendMode::Gpu,
            gpu_budget_bytes: 64,
            resident_enabled: true,
            ..ComputePolicy::default()
        },
        Some(FakeGpu::new()),
    );
    assert_eq!(
        service
            .evaluate(oversized_content())
            .await
            .unwrap_err()
            .diagnostic_code(),
        "ICE3_STUDIO_GPU_BUDGET_EXCEEDED"
    );
    assert_eq!(
        service
            .evaluate_for(GeometryEvaluationPurpose::Preview)
            .await
            .unwrap_err()
            .diagnostic_code(),
        "ICE3_STUDIO_GPU_BUDGET_EXCEEDED"
    );
}
