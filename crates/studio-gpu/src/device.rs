use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use studio_core::gpu_contract::{
    BackendCapabilities, BackendError, GeometryBackend, GeometryBackendKind, GeometryBackendMode,
    GeometryBatch, GeometryBatchResult, GeometryExecutionStats,
};
use wgpu::{
    Adapter, BindGroupDescriptor, BindGroupEntry, BufferDescriptor, BufferUsages,
    CommandEncoderDescriptor, ComputePassDescriptor, ComputePipelineDescriptor, Device,
    DeviceDescriptor, ExperimentalFeatures, Features, Instance, InstanceDescriptor, Limits,
    MapMode, MemoryHints, PollType, PowerPreference, Queue, RequestAdapterOptions,
    ShaderModuleDescriptor, ShaderSource, Trace,
};

#[derive(Debug, Clone)]
pub struct GpuHostLimits {
    pub max_buffer_bytes: u64,
    pub max_job_memory_bytes: u64,
    pub max_concurrent_jobs: usize,
}

impl Default for GpuHostLimits {
    fn default() -> Self {
        Self {
            max_buffer_bytes: 512 * 1024 * 1024,
            max_job_memory_bytes: 512 * 1024 * 1024,
            max_concurrent_jobs: 2,
        }
    }
}

struct DeviceState {
    capabilities: Option<BackendCapabilities>,
    _adapter: Option<Adapter>,
    _device: Option<Arc<Device>>,
    _queue: Option<Arc<Queue>>,
    stats: GeometryExecutionStats,
}

pub struct NativeGpuBackend {
    mode: GeometryBackendMode,
    state: Arc<Mutex<DeviceState>>,
    limits: GpuHostLimits,
}

impl NativeGpuBackend {
    pub fn new(mode: GeometryBackendMode) -> Self {
        Self {
            mode,
            state: Arc::new(Mutex::new(DeviceState {
                capabilities: None,
                _adapter: None,
                _device: None,
                _queue: None,
                stats: GeometryExecutionStats::default(),
            })),
            limits: GpuHostLimits::default(),
        }
    }

    pub fn for_test_gpu() -> Option<Self> {
        let backend = Self::new(GeometryBackendMode::Gpu);
        pollster::block_on(backend.initialize()).ok()?;
        Some(backend)
    }

    pub async fn discover(&self) -> Result<CapabilityProbe, BackendError> {
        let instance = Instance::new(InstanceDescriptor::new_without_display_handle());
        let adapter = instance
            .request_adapter(&RequestAdapterOptions {
                power_preference: PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: None,
                apply_limit_buckets: true,
            })
            .await
            .map_err(|error| BackendError::Unavailable {
                reason: error.to_string(),
            })?;
        let info = adapter.get_info();
        if adapter.limits().max_buffer_size < self.limits.max_job_memory_bytes {
            return Err(BackendError::Incompatible {
                reason: "adapter buffer limit is below the host per-job budget".into(),
            });
        }
        let adapter_limits = adapter.limits();
        let mut limits = Limits::default().using_resolution(adapter_limits.clone());
        limits.max_buffer_size = self
            .limits
            .max_buffer_bytes
            .min(adapter_limits.max_buffer_size);
        let required_features = adapter.features() & Features::TIMESTAMP_QUERY;
        let (device, queue) = adapter
            .request_device(&DeviceDescriptor {
                label: Some("ICE3 Studio Native Compute"),
                required_features,
                required_limits: limits,
                experimental_features: ExperimentalFeatures::disabled(),
                memory_hints: MemoryHints::Performance,
                trace: Trace::Off,
            })
            .await
            .map_err(|error| BackendError::Unavailable {
                reason: error.to_string(),
            })?;
        warmup_one_triangle(&device, &queue)?;
        let state = self.state.clone();
        device.set_device_lost_callback(move |reason, message| {
            record_device_loss(&state, format!("{reason:?}: {message}"));
        });
        Ok(CapabilityProbe {
            capabilities: BackendCapabilities {
                kind: GeometryBackendKind::Gpu,
                health: "ready".into(),
                accelerated: true,
                adapter_tier: format!("{:?}", info.device_type).to_ascii_lowercase(),
                backend_family: format!("{:?}", info.backend).to_ascii_lowercase(),
                memory_budget_bytes: self.limits.max_buffer_bytes,
                fallback_reason: None,
            },
            adapter,
            device,
            queue,
        })
    }

    pub async fn capabilities(&self) -> Result<BackendCapabilities, BackendError> {
        if self.mode == GeometryBackendMode::Cpu {
            return Ok(BackendCapabilities::cpu());
        }
        if let Some(capabilities) = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .capabilities
            .clone()
        {
            return Ok(capabilities);
        }
        self.initialize().await?;
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .capabilities
            .clone()
            .ok_or_else(|| BackendError::Unavailable {
                reason: "device initialization did not publish capabilities".into(),
            })
    }

    pub fn evaluate_primitive(
        &self,
        job: crate::PrimitiveJob,
    ) -> Result<crate::MeshReadback, BackendError> {
        let (device, queue) = self.ready_device()?;
        match job.primitive {
            studio_core::geometry::Primitive::Box {
                width,
                height,
                depth,
            } => {
                validate_positive(&[width, height, depth])?;
                crate::evaluate_box(
                    &device,
                    &queue,
                    crate::PrimitiveGpuParams::for_box(width, height, depth),
                )
            }
            studio_core::geometry::Primitive::Plane { width, depth } => {
                validate_positive(&[width, depth, 1.0])?;
                crate::evaluate_plane(
                    &device,
                    &queue,
                    crate::PrimitiveGpuParams::for_plane(width, depth),
                )
            }
            studio_core::geometry::Primitive::Cylinder { .. }
            | studio_core::geometry::Primitive::Sphere { .. } => Err(BackendError::Kernel {
                message: "this primitive kernel is not implemented yet".into(),
            }),
        }
    }

    fn ready_device(&self) -> Result<(Arc<Device>, Arc<Queue>), BackendError> {
        if self.mode == GeometryBackendMode::Cpu {
            return Err(BackendError::Unavailable {
                reason: "GPU mode is disabled".into(),
            });
        }
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(capabilities) = &state.capabilities
            && capabilities.health != "ready"
        {
            return Err(BackendError::DeviceLost {
                message: capabilities
                    .fallback_reason
                    .clone()
                    .unwrap_or_else(|| "GPU device is no longer ready".into()),
            });
        }
        match (&state._device, &state._queue) {
            (Some(device), Some(queue)) => Ok((device.clone(), queue.clone())),
            _ => Err(BackendError::Unavailable {
                reason: "GPU device has not been initialized".into(),
            }),
        }
    }
}

pub struct CapabilityProbe {
    pub capabilities: BackendCapabilities,
    adapter: Adapter,
    device: Device,
    queue: Queue,
}

#[async_trait::async_trait]
impl GeometryBackend for NativeGpuBackend {
    async fn initialize(&self) -> Result<BackendCapabilities, BackendError> {
        if self.mode == GeometryBackendMode::Cpu {
            return Ok(BackendCapabilities::cpu());
        }
        if let Some(capabilities) = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .capabilities
            .clone()
        {
            if capabilities.health != "ready" {
                return Err(BackendError::DeviceLost {
                    message: capabilities
                        .fallback_reason
                        .unwrap_or_else(|| "GPU device is no longer ready".into()),
                });
            }
            return Ok(capabilities);
        }
        let probe = self.discover().await?;
        let capabilities = probe.capabilities.clone();
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.capabilities = Some(capabilities.clone());
        state._adapter = Some(probe.adapter);
        state._device = Some(Arc::new(probe.device));
        state._queue = Some(Arc::new(probe.queue));
        Ok(capabilities)
    }

    async fn evaluate(&self, batch: GeometryBatch) -> Result<GeometryBatchResult, BackendError> {
        let mut meshes = std::collections::BTreeMap::new();
        let mut diagnostics = Vec::new();
        for node in &batch.nodes {
            match studio_core::geometry::evaluate_node_content(&node.content) {
                Ok(mesh) => {
                    meshes.insert(node.node_id.clone(), mesh);
                }
                Err(error) => diagnostics.push(studio_core::validation::Diagnostic {
                    code: studio_core::validation::DiagnosticCode::GeometryInvalid,
                    operation_index: None,
                    object_id: Some(node.node_id.as_str().into()),
                    pointer: format!("/nodes/{}/content", node.node_id.as_str()),
                    message: error.to_string(),
                }),
            }
        }
        Ok(GeometryBatchResult {
            job_id: batch.job_id,
            backend: GeometryBackendKind::Cpu,
            meshes,
            stats: GeometryExecutionStats::default(),
            diagnostics,
        })
    }

    async fn cancel(&self, _job_id: &str) -> Result<(), BackendError> {
        Ok(())
    }

    fn stats(&self) -> GeometryExecutionStats {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .stats
            .clone()
    }
}

fn record_device_loss(state: &Mutex<DeviceState>, reason: String) {
    let mut state = state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(capabilities) = state.capabilities.as_mut() {
        capabilities.health = "unavailable".into();
        capabilities.accelerated = false;
        capabilities.fallback_reason = Some(reason);
    }
}

fn validate_positive(values: &[f32]) -> Result<(), BackendError> {
    if values.iter().all(|value| value.is_finite() && *value > 0.0) {
        Ok(())
    } else {
        Err(BackendError::Kernel {
            message: "primitive dimensions must be finite and positive".into(),
        })
    }
}

const WARMUP_SHADER: &str = r#"
@group(0) @binding(0) var<storage, read> input_values: array<f32>;
@group(0) @binding(1) var<storage, read_write> output_values: array<f32>;

@compute @workgroup_size(1)
fn main(@builtin(global_invocation_id) invocation: vec3<u32>) {
    if (invocation.x >= 3u) {
        return;
    }
    output_values[invocation.x] = input_values[invocation.x] * 2.0 + 1.0;
}
"#;

fn warmup_one_triangle(device: &Device, queue: &Queue) -> Result<(), BackendError> {
    let input_values = [0.0f32, 1.0, 2.0];
    let input = device.create_buffer(&BufferDescriptor {
        label: Some("ICE3 GPU warm-up input"),
        size: std::mem::size_of_val(&input_values) as u64,
        usage: BufferUsages::STORAGE,
        mapped_at_creation: true,
    });
    let mut mapped_input =
        input
            .slice(..)
            .get_mapped_range_mut()
            .map_err(|error| BackendError::Kernel {
                message: format!("GPU warm-up input mapping failed: {error}"),
            })?;
    mapped_input.copy_from_slice(bytemuck::cast_slice(&input_values));
    drop(mapped_input);
    input.unmap();

    let output = device.create_buffer(&BufferDescriptor {
        label: Some("ICE3 GPU warm-up output"),
        size: std::mem::size_of_val(&input_values) as u64,
        usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("ICE3 GPU warm-up readback"),
        size: std::mem::size_of_val(&input_values) as u64,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("ICE3 GPU warm-up shader"),
        source: ShaderSource::Wgsl(WARMUP_SHADER.into()),
    });
    let pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
        label: Some("ICE3 GPU warm-up pipeline"),
        layout: None,
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("ICE3 GPU warm-up bindings"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: input.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 1,
                resource: output.as_entire_binding(),
            },
        ],
    });
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("ICE3 GPU warm-up encoder"),
    });
    {
        let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
            label: Some("ICE3 GPU warm-up pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(3, 1, 1);
    }
    encoder.copy_buffer_to_buffer(
        &output,
        0,
        &readback,
        0,
        std::mem::size_of_val(&input_values) as u64,
    );
    queue.submit([encoder.finish()]);

    let slice = readback.slice(..);
    let (sender, receiver) = mpsc::channel();
    slice.map_async(MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    device
        .poll(PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(5)),
        })
        .map_err(|error| BackendError::Kernel {
            message: format!("GPU warm-up poll failed: {error}"),
        })?;
    receiver
        .recv_timeout(Duration::from_secs(5))
        .map_err(|error| BackendError::Kernel {
            message: format!("GPU warm-up readback did not complete: {error}"),
        })?
        .map_err(|error| BackendError::Kernel {
            message: format!("GPU warm-up readback failed: {error}"),
        })?;
    let mapped = slice
        .get_mapped_range()
        .map_err(|error| BackendError::Kernel {
            message: format!("GPU warm-up readback mapping failed: {error}"),
        })?;
    let actual: &[f32] = bytemuck::cast_slice(&mapped);
    let expected = [1.0f32, 3.0, 5.0];
    let matches = actual.len() == expected.len()
        && actual
            .iter()
            .zip(expected)
            .all(|(actual, expected)| (*actual - expected).abs() <= f32::EPSILON);
    drop(mapped);
    readback.unmap();
    if !matches {
        return Err(BackendError::Parity);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_loss_marks_cached_capabilities_unavailable() {
        let state = Mutex::new(DeviceState {
            capabilities: Some(BackendCapabilities {
                kind: GeometryBackendKind::Gpu,
                health: "ready".into(),
                accelerated: true,
                adapter_tier: "integrated".into(),
                backend_family: "metal".into(),
                memory_budget_bytes: 512 * 1024 * 1024,
                fallback_reason: None,
            }),
            _adapter: None,
            _device: None,
            _queue: None,
            stats: GeometryExecutionStats::default(),
        });

        record_device_loss(&state, "adapter removed".into());

        let capabilities = state
            .lock()
            .unwrap()
            .capabilities
            .clone()
            .expect("capabilities remain observable after loss");
        assert_eq!(capabilities.health, "unavailable");
        assert!(!capabilities.accelerated);
        assert_eq!(
            capabilities.fallback_reason.as_deref(),
            Some("adapter removed")
        );
    }
}
