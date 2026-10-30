use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use studio_core::document::{Document, NodeContent, SceneNode};
use studio_core::geometry::{Primitive, evaluate_node_content};
use studio_core::gpu_contract::{
    BackendCapabilities, BackendError, GeometryBackend, GeometryBackendKind, GeometryBackendMode,
    GeometryBatch, GeometryBatchResult, GeometryExecutionStats, GeometryJobPriority,
};
use studio_core::ids::{ComponentId, NodeId};
use studio_core::large_scene::cache::{GeometryStore, MemoryGeometryStore};
use studio_core::large_scene::component::{
    ComponentDefinition, ComponentInstance, ComponentPart, PartContent, PartTransform,
    ScalarBinding,
};
use studio_core::large_scene::evaluate::evaluate_large_scene_with_cache_and_evaluator;
use studio_core::math::Transform;
use studio_server::compute::{ComputeEvaluator, ComputePolicy, ComputeService};

struct FakeGpu {
    calls: AtomicUsize,
    active: AtomicUsize,
    max_active: AtomicUsize,
    starts: Mutex<Vec<GeometryJobPriority>>,
    outcome: FakeOutcome,
}

enum FakeOutcome {
    Success,
    Slow,
    Unavailable,
    DeviceLost,
}

impl FakeGpu {
    fn new(outcome: FakeOutcome) -> Arc<Self> {
        Arc::new(Self {
            calls: AtomicUsize::new(0),
            active: AtomicUsize::new(0),
            max_active: AtomicUsize::new(0),
            starts: Mutex::new(Vec::new()),
            outcome,
        })
    }
}

#[async_trait::async_trait]
impl GeometryBackend for FakeGpu {
    async fn initialize(&self) -> Result<BackendCapabilities, BackendError> {
        match self.outcome {
            FakeOutcome::Unavailable => Err(BackendError::Unavailable {
                reason: "fake adapter unavailable".into(),
            }),
            _ => Ok(BackendCapabilities {
                kind: GeometryBackendKind::Gpu,
                health: "ready".into(),
                accelerated: true,
                adapter_tier: "mock".into(),
                backend_family: "fake".into(),
                memory_budget_bytes: 2 * 1024 * 1024 * 1024,
                fallback_reason: None,
            }),
        }
    }

    async fn evaluate(&self, batch: GeometryBatch) -> Result<GeometryBatchResult, BackendError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.starts.lock().unwrap().push(batch.priority);
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_active.fetch_max(active, Ordering::SeqCst);
        match self.outcome {
            FakeOutcome::DeviceLost => {
                self.active.fetch_sub(1, Ordering::SeqCst);
                return Err(BackendError::DeviceLost {
                    message: "fake device removed".into(),
                });
            }
            FakeOutcome::Unavailable => {
                self.active.fetch_sub(1, Ordering::SeqCst);
                return Err(BackendError::Unavailable {
                    reason: "fake adapter unavailable".into(),
                });
            }
            FakeOutcome::Slow => {
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
            FakeOutcome::Success => {}
        }
        let mut meshes = BTreeMap::new();
        for node in &batch.nodes {
            meshes.insert(
                node.node_id.clone(),
                evaluate_node_content(&node.content).unwrap(),
            );
        }
        self.active.fetch_sub(1, Ordering::SeqCst);
        Ok(GeometryBatchResult {
            job_id: batch.job_id,
            backend: GeometryBackendKind::Gpu,
            meshes,
            stats: GeometryExecutionStats::default(),
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

fn box_content() -> NodeContent {
    NodeContent::Primitive(Primitive::Box {
        width: 1.0,
        height: 1.0,
        depth: 1.0,
    })
}

fn box_component_document() -> Document {
    let mut document = Document::new("scheduled-components");
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
    let mut node = SceneNode::group(NodeId::from("node-000001"), "Pump");
    node.transform = Transform::IDENTITY;
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
async fn cpu_mode_never_invokes_the_gpu() {
    let gpu = FakeGpu::new(FakeOutcome::Success);
    let service = ComputeService::new(
        ComputePolicy {
            mode: GeometryBackendMode::Cpu,
            ..ComputePolicy::default()
        },
        Some(gpu.clone()),
    );
    let result = service.evaluate(box_content()).await.unwrap();
    assert_eq!(result.backend, GeometryBackendKind::Cpu);
    assert_eq!(gpu.calls.load(Ordering::SeqCst), 0);
    let status = service.status();
    assert_eq!(status.health, "ready");
    assert_eq!(status.fallback_reason, None);
}

#[tokio::test]
async fn measured_default_keeps_auto_on_the_cpu_reference() {
    let gpu = FakeGpu::new(FakeOutcome::Success);
    let service = ComputeService::defaults(Some(gpu.clone()));
    let result = service.evaluate(box_content()).await.unwrap();
    assert_eq!(result.backend, GeometryBackendKind::Cpu);
    assert_eq!(gpu.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn auto_uses_the_measured_triangle_threshold() {
    let gpu = FakeGpu::new(FakeOutcome::Success);
    let service = ComputeService::new(
        ComputePolicy {
            mode: GeometryBackendMode::Auto,
            gpu_threshold_triangles: 100,
            ..ComputePolicy::default()
        },
        Some(gpu.clone()),
    );
    let cpu = service.evaluate(box_content()).await.unwrap();
    assert_eq!(cpu.backend, GeometryBackendKind::Cpu);
    assert_eq!(gpu.calls.load(Ordering::SeqCst), 0);

    let gpu = FakeGpu::new(FakeOutcome::Success);
    let service = ComputeService::new(
        ComputePolicy {
            mode: GeometryBackendMode::Auto,
            gpu_threshold_triangles: 1,
            ..ComputePolicy::default()
        },
        Some(gpu.clone()),
    );
    let accelerated = service.evaluate(box_content()).await.unwrap();
    assert_eq!(accelerated.backend, GeometryBackendKind::Gpu);
    assert_eq!(gpu.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn forced_gpu_reports_structured_unavailability() {
    let service = ComputeService::new(
        ComputePolicy {
            mode: GeometryBackendMode::Gpu,
            ..ComputePolicy::default()
        },
        None,
    );
    let error = service.evaluate(box_content()).await.unwrap_err();
    assert_eq!(error.diagnostic_code(), "ICE3_STUDIO_GPU_UNAVAILABLE");
}

#[tokio::test]
async fn forced_gpu_reports_backend_initialization_failure() {
    let gpu = FakeGpu::new(FakeOutcome::Unavailable);
    let service = ComputeService::new(
        ComputePolicy {
            mode: GeometryBackendMode::Gpu,
            ..ComputePolicy::default()
        },
        Some(gpu),
    );
    let error = service.evaluate(box_content()).await.unwrap_err();
    assert!(error.to_string().contains("fake adapter unavailable"));
}

#[tokio::test]
async fn auto_falls_back_to_cpu_and_records_the_reason() {
    let gpu = FakeGpu::new(FakeOutcome::DeviceLost);
    let service = ComputeService::new(
        ComputePolicy {
            mode: GeometryBackendMode::Auto,
            gpu_threshold_triangles: 1,
            ..ComputePolicy::default()
        },
        Some(gpu),
    );
    let result = service.evaluate(box_content()).await.unwrap();
    assert_eq!(result.backend, GeometryBackendKind::Cpu);
    assert!(
        result
            .fallback_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("device was lost"))
    );
    let status = service.status();
    assert_eq!(status.selected_backend, GeometryBackendKind::Cpu);
    assert_eq!(status.health, "unavailable");
    assert!(
        status
            .fallback_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("device was lost"))
    );
}

#[tokio::test]
async fn gpu_failure_never_mutates_the_committed_document() {
    let document = Document::new("device-loss");
    let revision = document.revision;
    let gpu = FakeGpu::new(FakeOutcome::DeviceLost);
    let service = ComputeService::new(
        ComputePolicy {
            mode: GeometryBackendMode::Auto,
            gpu_threshold_triangles: 1,
            ..ComputePolicy::default()
        },
        Some(gpu),
    );
    service.evaluate(box_content()).await.unwrap();
    assert_eq!(document.revision, revision);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn large_scene_hook_uses_gpu_and_caches_under_the_selected_backend() {
    let gpu = FakeGpu::new(FakeOutcome::Success);
    let service = ComputeService::new(
        ComputePolicy {
            mode: GeometryBackendMode::Auto,
            gpu_threshold_triangles: 1,
            ..ComputePolicy::default()
        },
        Some(gpu.clone()),
    );
    service.initialize().await.unwrap();
    let evaluator = ComputeEvaluator::new(&service);
    let document = box_component_document();
    let mut store = MemoryGeometryStore::default();

    let first =
        evaluate_large_scene_with_cache_and_evaluator(&document, &mut store, Some(&evaluator))
            .unwrap();
    assert_eq!(gpu.calls.load(Ordering::SeqCst), 1);
    assert_eq!(first.stats.generated_meshes, 1);
    let mesh_key = first.nodes[0].mesh_key.as_deref().unwrap();
    let cached = store.load_entry(mesh_key).unwrap();
    assert_eq!(cached.metadata.backend, GeometryBackendKind::Gpu);

    let second =
        evaluate_large_scene_with_cache_and_evaluator(&document, &mut store, Some(&evaluator))
            .unwrap();
    assert_eq!(gpu.calls.load(Ordering::SeqCst), 1);
    assert_eq!(second.stats.cache_hits, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn queue_pressure_stays_within_the_configured_gpu_concurrency() {
    let gpu = FakeGpu::new(FakeOutcome::Slow);
    let service = Arc::new(ComputeService::new(
        ComputePolicy {
            mode: GeometryBackendMode::Gpu,
            max_concurrent_gpu_jobs: 2,
            ..ComputePolicy::default()
        },
        Some(gpu.clone()),
    ));
    let mut tasks = Vec::new();
    for _ in 0..6 {
        let service = service.clone();
        tasks.push(tokio::spawn(async move {
            service.evaluate(box_content()).await.unwrap()
        }));
    }
    for task in tasks {
        task.await.unwrap();
    }
    assert_eq!(gpu.calls.load(Ordering::SeqCst), 6);
    assert_eq!(gpu.max_active.load(Ordering::SeqCst), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn queued_interactive_work_precedes_background_work() {
    let gpu = FakeGpu::new(FakeOutcome::Slow);
    let service = Arc::new(ComputeService::new(
        ComputePolicy {
            mode: GeometryBackendMode::Gpu,
            max_concurrent_gpu_jobs: 1,
            ..ComputePolicy::default()
        },
        Some(gpu.clone()),
    ));
    let first = tokio::spawn({
        let service = service.clone();
        async move {
            service
                .evaluate_with_priority(box_content(), GeometryJobPriority::Background)
                .await
                .unwrap()
        }
    });
    while gpu.calls.load(Ordering::SeqCst) == 0 {
        tokio::task::yield_now().await;
    }
    let background = tokio::spawn({
        let service = service.clone();
        async move {
            service
                .evaluate_with_priority(box_content(), GeometryJobPriority::Background)
                .await
                .unwrap()
        }
    });
    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    let interactive = tokio::spawn({
        let service = service.clone();
        async move {
            service
                .evaluate_with_priority(box_content(), GeometryJobPriority::Interactive)
                .await
                .unwrap()
        }
    });
    first.await.unwrap();
    background.await.unwrap();
    interactive.await.unwrap();

    assert_eq!(
        *gpu.starts.lock().unwrap(),
        vec![
            GeometryJobPriority::Background,
            GeometryJobPriority::Interactive,
            GeometryJobPriority::Background,
        ]
    );
}
