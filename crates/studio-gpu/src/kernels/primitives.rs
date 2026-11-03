use std::sync::mpsc;
use std::time::Duration;

use bytemuck::{Pod, Zeroable};
use studio_core::geometry::{Primitive, Vertex};
use studio_core::gpu_contract::BackendError;
use wgpu::{
    BindGroupDescriptor, BindGroupEntry, BindingResource, Buffer, BufferAsyncError, BufferBinding,
    BufferUsages, CommandEncoder, CommandEncoderDescriptor, ComputePassDescriptor, Device, MapMode,
    PollType, Queue,
};

use crate::resources::{BufferLease, BufferRole, DeviceResources, PipelineKey};

#[derive(Debug, Clone, PartialEq)]
pub struct PrimitiveJob {
    pub primitive: Primitive,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SweepJob {
    pub points: Vec<[f32; 3]>,
    pub radius: f32,
    pub radial_segments: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct PrimitiveGpuParams {
    pub dimensions: [f32; 4],
    pub vertex_count: u32,
    pub index_count: u32,
    pub item_count: u32,
    pub secondary_count: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct GpuVertex {
    position: [f32; 4],
    normal: [f32; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub struct MeshReadback {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl PrimitiveGpuParams {
    pub fn for_box(width: f32, height: f32, depth: f32) -> Self {
        Self {
            dimensions: [width, height, depth, 0.0],
            vertex_count: 24,
            index_count: 36,
            item_count: 0,
            secondary_count: 0,
        }
    }

    pub fn for_plane(width: f32, depth: f32) -> Self {
        Self {
            dimensions: [width, 0.0, depth, 0.0],
            vertex_count: 4,
            index_count: 6,
            item_count: 0,
            secondary_count: 0,
        }
    }

    pub fn for_cylinder(radius: f32, height: f32, segments: u32) -> Self {
        Self {
            dimensions: [radius, height, 0.0, 0.0],
            vertex_count: segments * 4 + 6,
            index_count: segments * 12,
            item_count: segments,
            secondary_count: 0,
        }
    }

    pub fn for_sphere(radius: f32, segments: u32, rings: u32) -> Self {
        Self {
            dimensions: [radius, 0.0, 0.0, 0.0],
            vertex_count: (rings + 1) * (segments + 1),
            index_count: (rings - 1) * segments * 6,
            item_count: segments,
            secondary_count: rings,
        }
    }

    pub fn for_sweep(points: u32, radius: f32, radial_segments: u32) -> Option<Self> {
        let vertex_count = points.checked_mul(radial_segments)?;
        let side_indices = points
            .checked_sub(1)?
            .checked_mul(radial_segments)?
            .checked_mul(6)?;
        let cap_indices = radial_segments.checked_sub(2)?.checked_mul(6)?;
        Some(Self {
            dimensions: [radius, 0.0, 0.0, 0.0],
            vertex_count,
            index_count: side_indices.checked_add(cap_indices)?,
            item_count: radial_segments,
            secondary_count: points,
        })
    }
}

pub fn evaluate_box(
    device: &Device,
    queue: &Queue,
    resources: &DeviceResources,
    params: PrimitiveGpuParams,
) -> Result<MeshReadback, BackendError> {
    run_kernel(device, queue, resources, params, PipelineKey::Box)
}

pub fn evaluate_plane(
    device: &Device,
    queue: &Queue,
    resources: &DeviceResources,
    params: PrimitiveGpuParams,
) -> Result<MeshReadback, BackendError> {
    run_kernel(device, queue, resources, params, PipelineKey::Plane)
}

pub fn evaluate_cylinder(
    device: &Device,
    queue: &Queue,
    resources: &DeviceResources,
    params: PrimitiveGpuParams,
) -> Result<MeshReadback, BackendError> {
    run_radial_kernel(
        device,
        queue,
        resources,
        params,
        PipelineKey::Cylinder,
        None,
    )
}

pub fn evaluate_sphere(
    device: &Device,
    queue: &Queue,
    resources: &DeviceResources,
    params: PrimitiveGpuParams,
) -> Result<MeshReadback, BackendError> {
    run_radial_kernel(device, queue, resources, params, PipelineKey::Sphere, None)
}

pub fn evaluate_sweep(
    device: &Device,
    queue: &Queue,
    resources: &DeviceResources,
    params: PrimitiveGpuParams,
    points: &[[f32; 3]],
) -> Result<MeshReadback, BackendError> {
    run_radial_kernel(
        device,
        queue,
        resources,
        params,
        PipelineKey::Sweep,
        Some(points),
    )
}

fn run_kernel(
    device: &Device,
    queue: &Queue,
    resources: &DeviceResources,
    params: PrimitiveGpuParams,
    pipeline_key: PipelineKey,
) -> Result<MeshReadback, BackendError> {
    let entry_point = match pipeline_key {
        PipelineKey::Box => "generate_box",
        PipelineKey::Plane => "generate_plane",
        _ => {
            return Err(BackendError::Kernel {
                message: "invalid primitive pipeline".into(),
            });
        }
    };
    validate_gpu_params(&params, entry_point)?;

    let vertex_bytes = params.vertex_count as u64 * std::mem::size_of::<GpuVertex>() as u64;
    let index_bytes = params.index_count as u64 * std::mem::size_of::<u32>() as u64;
    let params_lease = resources.buffers().acquire(
        device,
        BufferRole::DynamicAttribute,
        BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        std::mem::size_of::<PrimitiveGpuParams>() as u64,
        "ICE3 primitive parameters",
    )?;
    queue.write_buffer(&params_lease.buffer, 0, bytemuck::bytes_of(&params));
    let vertex_lease = resources.buffers().acquire(
        device,
        BufferRole::DeviceOutput,
        BufferUsages::STORAGE | BufferUsages::COPY_SRC,
        vertex_bytes,
        "ICE3 primitive vertices",
    )?;
    let index_lease = resources.buffers().acquire(
        device,
        BufferRole::DeviceOutput,
        BufferUsages::STORAGE | BufferUsages::COPY_SRC,
        index_bytes,
        "ICE3 primitive indices",
    )?;
    let pipeline = resources.pipelines().get(pipeline_key)?;
    let bind_group_layout = pipeline.get_bind_group_layout(0);
    let bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("ICE3 primitive bindings"),
        layout: &bind_group_layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: lease_binding(&params_lease),
            },
            BindGroupEntry {
                binding: 1,
                resource: lease_binding(&vertex_lease),
            },
            BindGroupEntry {
                binding: 2,
                resource: lease_binding(&index_lease),
            },
        ],
    });

    let work_items = params.vertex_count.max(params.index_count);
    let workgroups = work_items.div_ceil(64);
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("ICE3 primitive encoder"),
    });
    {
        let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
            label: Some("ICE3 primitive pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(workgroups, 1, 1);
    }
    let result = readback_mesh(
        device,
        queue,
        resources,
        encoder,
        ReadbackInputs {
            vertex_buffer: &vertex_lease.buffer,
            index_buffer: &index_lease.buffer,
            vertex_count: params.vertex_count,
            index_count: params.index_count,
        },
    );
    resources.buffers().release(params_lease);
    resources.buffers().release(vertex_lease);
    resources.buffers().release(index_lease);
    result
}

fn run_radial_kernel(
    device: &Device,
    queue: &Queue,
    resources: &DeviceResources,
    params: PrimitiveGpuParams,
    pipeline_key: PipelineKey,
    points: Option<&[[f32; 3]]>,
) -> Result<MeshReadback, BackendError> {
    let entry_point = match pipeline_key {
        PipelineKey::Cylinder => "generate_cylinder",
        PipelineKey::Sphere => "generate_sphere",
        PipelineKey::Sweep => "generate_sweep",
        _ => {
            return Err(BackendError::Kernel {
                message: "invalid radial pipeline".into(),
            });
        }
    };
    validate_radial_params(&params, entry_point, points)?;

    let vertex_bytes = params.vertex_count as u64 * std::mem::size_of::<GpuVertex>() as u64;
    let index_bytes = params.index_count as u64 * std::mem::size_of::<u32>() as u64;
    let params_lease = resources.buffers().acquire(
        device,
        BufferRole::DynamicAttribute,
        BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        std::mem::size_of::<PrimitiveGpuParams>() as u64,
        "ICE3 radial parameters",
    )?;
    queue.write_buffer(&params_lease.buffer, 0, bytemuck::bytes_of(&params));

    let point_values = points
        .map(|points| {
            points
                .iter()
                .map(|point| [point[0], point[1], point[2], 1.0])
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| vec![[0.0, 0.0, 0.0, 1.0]]);
    let point_lease = resources.buffers().acquire(
        device,
        BufferRole::DynamicAttribute,
        BufferUsages::STORAGE | BufferUsages::COPY_DST,
        (point_values.len() * std::mem::size_of::<[f32; 4]>()) as u64,
        "ICE3 radial input points",
    )?;
    queue.write_buffer(&point_lease.buffer, 0, bytemuck::cast_slice(&point_values));

    let vertex_lease = resources.buffers().acquire(
        device,
        BufferRole::DeviceOutput,
        BufferUsages::STORAGE | BufferUsages::COPY_SRC,
        vertex_bytes,
        "ICE3 radial vertices",
    )?;
    let index_lease = resources.buffers().acquire(
        device,
        BufferRole::DeviceOutput,
        BufferUsages::STORAGE | BufferUsages::COPY_SRC,
        index_bytes,
        "ICE3 radial indices",
    )?;
    let pipeline = resources.pipelines().get(pipeline_key)?;
    let bind_group_layout = resources.pipelines().radial_layout();
    let bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("ICE3 radial bindings"),
        layout: bind_group_layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: lease_binding(&params_lease),
            },
            BindGroupEntry {
                binding: 1,
                resource: lease_binding(&vertex_lease),
            },
            BindGroupEntry {
                binding: 2,
                resource: lease_binding(&index_lease),
            },
            BindGroupEntry {
                binding: 3,
                resource: lease_binding(&point_lease),
            },
        ],
    });

    let workgroups = params.vertex_count.max(params.index_count).div_ceil(64);
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("ICE3 radial encoder"),
    });
    {
        let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
            label: Some("ICE3 radial pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(workgroups, 1, 1);
    }

    let result = readback_mesh(
        device,
        queue,
        resources,
        encoder,
        ReadbackInputs {
            vertex_buffer: &vertex_lease.buffer,
            index_buffer: &index_lease.buffer,
            vertex_count: params.vertex_count,
            index_count: params.index_count,
        },
    );
    resources.buffers().release(params_lease);
    resources.buffers().release(point_lease);
    resources.buffers().release(vertex_lease);
    resources.buffers().release(index_lease);
    result
}

struct ReadbackInputs<'a> {
    vertex_buffer: &'a Buffer,
    index_buffer: &'a Buffer,
    vertex_count: u32,
    index_count: u32,
}

fn readback_mesh(
    device: &Device,
    queue: &Queue,
    resources: &DeviceResources,
    mut encoder: CommandEncoder,
    inputs: ReadbackInputs<'_>,
) -> Result<MeshReadback, BackendError> {
    let vertex_bytes = inputs.vertex_count as u64 * std::mem::size_of::<GpuVertex>() as u64;
    let index_bytes = inputs.index_count as u64 * std::mem::size_of::<u32>() as u64;
    let vertex_readback = resources.staging().acquire(device, vertex_bytes)?;
    let index_readback = resources.staging().acquire(device, index_bytes)?;
    debug_assert_ne!(vertex_readback.index, index_readback.index);
    encoder.copy_buffer_to_buffer(
        inputs.vertex_buffer,
        0,
        &vertex_readback.buffer,
        0,
        vertex_bytes,
    );
    encoder.copy_buffer_to_buffer(
        inputs.index_buffer,
        0,
        &index_readback.buffer,
        0,
        index_bytes,
    );
    queue.submit([encoder.finish()]);

    let vertex_mapping = map_async(&vertex_readback.buffer);
    let index_mapping = map_async(&index_readback.buffer);
    device
        .poll(PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(10)),
        })
        .map_err(|error| BackendError::Kernel {
            message: format!("primitive GPU poll failed: {error}"),
        })?;
    wait_for_mapping(&vertex_mapping, "vertex")?;
    wait_for_mapping(&index_mapping, "index")?;

    let vertices = {
        let mapped = vertex_readback
            .buffer
            .slice(0..vertex_bytes)
            .get_mapped_range()
            .map_err(|error| BackendError::Kernel {
                message: format!("primitive vertex readback mapping failed: {error}"),
            })?;
        let output: &[GpuVertex] = bytemuck::cast_slice(&mapped);
        let vertices = output
            .iter()
            .map(|vertex| Vertex {
                position: [vertex.position[0], vertex.position[1], vertex.position[2]],
                normal: [vertex.normal[0], vertex.normal[1], vertex.normal[2]],
            })
            .collect::<Vec<_>>();
        drop(mapped);
        vertices
    };
    vertex_readback.buffer.unmap();

    let indices = {
        let mapped = index_readback
            .buffer
            .slice(0..index_bytes)
            .get_mapped_range()
            .map_err(|error| BackendError::Kernel {
                message: format!("primitive index readback mapping failed: {error}"),
            })?;
        let indices = bytemuck::cast_slice(&mapped).to_vec();
        drop(mapped);
        indices
    };
    index_readback.buffer.unmap();
    resources.staging().complete(1);

    validate_readback(&vertices, &indices)?;
    Ok(MeshReadback { vertices, indices })
}

fn lease_binding(lease: &BufferLease) -> BindingResource<'_> {
    BindingResource::Buffer(BufferBinding {
        buffer: &lease.buffer,
        offset: 0,
        size: std::num::NonZeroU64::new(lease.requested_bytes),
    })
}

fn validate_readback(vertices: &[Vertex], indices: &[u32]) -> Result<(), BackendError> {
    if vertices.is_empty() || !indices.len().is_multiple_of(3) {
        return Err(BackendError::Kernel {
            message: "primitive readback has invalid topology counts".into(),
        });
    }
    if let Some((position, index)) = indices
        .iter()
        .enumerate()
        .find(|(_, index)| **index as usize >= vertices.len())
    {
        return Err(BackendError::Kernel {
            message: format!(
                "primitive readback index {index} at {position} exceeds {} vertices",
                vertices.len()
            ),
        });
    }
    if let Some((position, value)) = vertices.iter().enumerate().find_map(|(position, vertex)| {
        vertex
            .position
            .iter()
            .chain(vertex.normal.iter())
            .copied()
            .find(|value| !value.is_finite())
            .map(|value| (position, value))
    }) {
        return Err(BackendError::Kernel {
            message: format!("primitive readback value {value} at vertex {position} is not finite"),
        });
    }
    Ok(())
}

fn map_async(buffer: &Buffer) -> mpsc::Receiver<Result<(), BufferAsyncError>> {
    let (sender, receiver) = mpsc::channel();
    buffer.slice(..).map_async(MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    receiver
}

fn wait_for_mapping(
    receiver: &mpsc::Receiver<Result<(), BufferAsyncError>>,
    label: &str,
) -> Result<(), BackendError> {
    receiver
        .recv_timeout(Duration::from_secs(10))
        .map_err(|error| BackendError::Kernel {
            message: format!("primitive {label} readback did not complete: {error}"),
        })?
        .map_err(|error| BackendError::Kernel {
            message: format!("primitive {label} readback failed: {error}"),
        })
}

fn validate_gpu_params(params: &PrimitiveGpuParams, entry_point: &str) -> Result<(), BackendError> {
    if params.vertex_count == 0 || params.index_count == 0 {
        return Err(BackendError::Kernel {
            message: "refusing to dispatch an empty primitive".into(),
        });
    }
    if params.dimensions.iter().any(|value| !value.is_finite()) {
        return Err(BackendError::Kernel {
            message: "primitive dimensions must be finite".into(),
        });
    }
    let positive_dimensions = match entry_point {
        "generate_box" => params.dimensions[..3].iter().all(|value| *value > 0.0),
        "generate_plane" => params.dimensions[0] > 0.0 && params.dimensions[2] > 0.0,
        _ => false,
    };
    if !positive_dimensions {
        return Err(BackendError::Kernel {
            message: "primitive dimensions must be positive".into(),
        });
    }
    Ok(())
}

fn validate_radial_params(
    params: &PrimitiveGpuParams,
    entry_point: &str,
    points: Option<&[[f32; 3]]>,
) -> Result<(), BackendError> {
    if params.dimensions.iter().any(|value| !value.is_finite()) || params.dimensions[0] <= 0.0 {
        return Err(BackendError::Kernel {
            message: "radial dimensions must be finite and positive".into(),
        });
    }

    let expected = match entry_point {
        "generate_cylinder" => {
            let segments = params.item_count as u64;
            if !(3..=256).contains(&segments) || params.dimensions[1] <= 0.0 {
                return Err(BackendError::Kernel {
                    message: "cylinder parameters are outside the supported range".into(),
                });
            }
            (segments * 4 + 6, segments * 12)
        }
        "generate_sphere" => {
            let segments = params.item_count as u64;
            let rings = params.secondary_count as u64;
            if !(3..=256).contains(&segments) || !(2..=128).contains(&rings) {
                return Err(BackendError::Kernel {
                    message: "sphere parameters are outside the supported range".into(),
                });
            }
            ((rings + 1) * (segments + 1), (rings - 1) * segments * 6)
        }
        "generate_sweep" => {
            let segments = params.item_count as u64;
            let point_count = params.secondary_count as usize;
            let Some(points) = points else {
                return Err(BackendError::Kernel {
                    message: "sweep points are required".into(),
                });
            };
            if !(3..=128).contains(&segments)
                || points.len() < 2
                || points.len() != point_count
                || points.iter().flatten().any(|value| !value.is_finite())
            {
                return Err(BackendError::Kernel {
                    message: "sweep parameters are invalid".into(),
                });
            }
            if points.windows(2).any(|pair| {
                let delta = [
                    pair[1][0] - pair[0][0],
                    pair[1][1] - pair[0][1],
                    pair[1][2] - pair[0][2],
                ];
                (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt()
                    <= f32::EPSILON
            }) {
                return Err(BackendError::Kernel {
                    message: "sweep contains duplicate adjacent points".into(),
                });
            }
            let side_indices = (point_count as u64 - 1) * segments * 6;
            let cap_indices = (segments - 2) * 6;
            (point_count as u64 * segments, side_indices + cap_indices)
        }
        _ => {
            return Err(BackendError::Kernel {
                message: "unknown radial kernel".into(),
            });
        }
    };
    if (params.vertex_count as u64, params.index_count as u64) != expected {
        return Err(BackendError::Kernel {
            message: "radial output counts do not match the selected kernel".into(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_finite_or_empty_gpu_dispatch() {
        let non_finite = PrimitiveGpuParams::for_box(f32::NAN, 1.0, 1.0);
        assert!(validate_gpu_params(&non_finite, "generate_box").is_err());

        let empty = PrimitiveGpuParams {
            vertex_count: 0,
            index_count: 0,
            ..PrimitiveGpuParams::for_plane(1.0, 1.0)
        };
        assert!(validate_gpu_params(&empty, "generate_plane").is_err());

        assert!(
            validate_gpu_params(&PrimitiveGpuParams::for_box(0.0, 1.0, 1.0), "generate_box")
                .is_err()
        );
        assert!(
            validate_gpu_params(&PrimitiveGpuParams::for_plane(1.0, 0.0), "generate_plane")
                .is_err()
        );
    }
}
