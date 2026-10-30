use std::sync::mpsc;
use std::time::Duration;

use bytemuck::{Pod, Zeroable};
use studio_core::geometry::{Primitive, Vertex};
use studio_core::gpu_contract::BackendError;
use wgpu::{
    BindGroupDescriptor, BindGroupEntry, BindGroupLayoutDescriptor, BindGroupLayoutEntry,
    BindingType, Buffer, BufferAsyncError, BufferBindingType, BufferDescriptor, BufferUsages,
    CommandEncoder, CommandEncoderDescriptor, ComputePassDescriptor, ComputePipelineDescriptor,
    Device, MapMode, PipelineLayoutDescriptor, PollType, Queue, ShaderModuleDescriptor,
    ShaderSource, ShaderStages,
};

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
            index_count: rings * segments * 6,
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
    params: PrimitiveGpuParams,
) -> Result<MeshReadback, BackendError> {
    run_kernel(device, queue, params, "generate_box")
}

pub fn evaluate_plane(
    device: &Device,
    queue: &Queue,
    params: PrimitiveGpuParams,
) -> Result<MeshReadback, BackendError> {
    run_kernel(device, queue, params, "generate_plane")
}

pub fn evaluate_cylinder(
    device: &Device,
    queue: &Queue,
    params: PrimitiveGpuParams,
) -> Result<MeshReadback, BackendError> {
    run_radial_kernel(device, queue, params, "generate_cylinder", None)
}

pub fn evaluate_sphere(
    device: &Device,
    queue: &Queue,
    params: PrimitiveGpuParams,
) -> Result<MeshReadback, BackendError> {
    run_radial_kernel(device, queue, params, "generate_sphere", None)
}

pub fn evaluate_sweep(
    device: &Device,
    queue: &Queue,
    params: PrimitiveGpuParams,
    points: &[[f32; 3]],
) -> Result<MeshReadback, BackendError> {
    run_radial_kernel(device, queue, params, "generate_sweep", Some(points))
}

fn run_kernel(
    device: &Device,
    queue: &Queue,
    params: PrimitiveGpuParams,
    entry_point: &str,
) -> Result<MeshReadback, BackendError> {
    validate_gpu_params(&params, entry_point)?;

    let vertex_bytes = params.vertex_count as u64 * std::mem::size_of::<GpuVertex>() as u64;
    let index_bytes = params.index_count as u64 * std::mem::size_of::<u32>() as u64;
    let params_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("ICE3 primitive parameters"),
        size: std::mem::size_of::<PrimitiveGpuParams>() as u64,
        usage: BufferUsages::UNIFORM,
        mapped_at_creation: true,
    });
    let mut mapped_params = params_buffer
        .slice(..)
        .get_mapped_range_mut()
        .map_err(|error| BackendError::Kernel {
            message: format!("primitive parameter mapping failed: {error}"),
        })?;
    mapped_params.copy_from_slice(bytemuck::bytes_of(&params));
    drop(mapped_params);
    params_buffer.unmap();

    let vertex_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("ICE3 primitive vertices"),
        size: vertex_bytes,
        usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let index_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("ICE3 primitive indices"),
        size: index_bytes,
        usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("ICE3 primitive shaders"),
        source: ShaderSource::Wgsl(include_str!("box_plane.wgsl").into()),
    });
    let pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
        label: Some("ICE3 primitive pipeline"),
        layout: None,
        module: &shader,
        entry_point: Some(entry_point),
        compilation_options: Default::default(),
        cache: None,
    });
    let bind_group_layout = pipeline.get_bind_group_layout(0);
    let bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("ICE3 primitive bindings"),
        layout: &bind_group_layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: params_buffer.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 1,
                resource: vertex_buffer.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 2,
                resource: index_buffer.as_entire_binding(),
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
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(workgroups, 1, 1);
    }
    readback_mesh(
        device,
        queue,
        encoder,
        &vertex_buffer,
        &index_buffer,
        params.vertex_count,
        params.index_count,
    )
}

fn run_radial_kernel(
    device: &Device,
    queue: &Queue,
    params: PrimitiveGpuParams,
    entry_point: &str,
    points: Option<&[[f32; 3]]>,
) -> Result<MeshReadback, BackendError> {
    validate_radial_params(&params, entry_point, points)?;

    let vertex_bytes = params.vertex_count as u64 * std::mem::size_of::<GpuVertex>() as u64;
    let index_bytes = params.index_count as u64 * std::mem::size_of::<u32>() as u64;
    let params_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("ICE3 radial parameters"),
        size: std::mem::size_of::<PrimitiveGpuParams>() as u64,
        usage: BufferUsages::UNIFORM,
        mapped_at_creation: true,
    });
    let mut mapped_params = params_buffer
        .slice(..)
        .get_mapped_range_mut()
        .map_err(|error| BackendError::Kernel {
            message: format!("radial parameter mapping failed: {error}"),
        })?;
    mapped_params.copy_from_slice(bytemuck::bytes_of(&params));
    drop(mapped_params);
    params_buffer.unmap();

    let point_values = points
        .map(|points| {
            points
                .iter()
                .map(|point| [point[0], point[1], point[2], 1.0])
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| vec![[0.0, 0.0, 0.0, 1.0]]);
    let point_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("ICE3 radial input points"),
        size: (point_values.len() * std::mem::size_of::<[f32; 4]>()) as u64,
        usage: BufferUsages::STORAGE,
        mapped_at_creation: true,
    });
    let mut mapped_points = point_buffer
        .slice(..)
        .get_mapped_range_mut()
        .map_err(|error| BackendError::Kernel {
            message: format!("radial input mapping failed: {error}"),
        })?;
    mapped_points.copy_from_slice(bytemuck::cast_slice(&point_values));
    drop(mapped_points);
    point_buffer.unmap();

    let vertex_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("ICE3 radial vertices"),
        size: vertex_bytes,
        usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let index_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("ICE3 radial indices"),
        size: index_bytes,
        usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("ICE3 radial shaders"),
        source: ShaderSource::Wgsl(include_str!("radial.wgsl").into()),
    });
    let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("ICE3 radial binding layout"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 2,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 3,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("ICE3 radial pipeline layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
        label: Some("ICE3 radial pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some(entry_point),
        compilation_options: Default::default(),
        cache: None,
    });
    let bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("ICE3 radial bindings"),
        layout: &bind_group_layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: params_buffer.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 1,
                resource: vertex_buffer.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 2,
                resource: index_buffer.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 3,
                resource: point_buffer.as_entire_binding(),
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
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(workgroups, 1, 1);
    }

    readback_mesh(
        device,
        queue,
        encoder,
        &vertex_buffer,
        &index_buffer,
        params.vertex_count,
        params.index_count,
    )
}

fn readback_mesh(
    device: &Device,
    queue: &Queue,
    mut encoder: CommandEncoder,
    vertex_buffer: &Buffer,
    index_buffer: &Buffer,
    vertex_count: u32,
    index_count: u32,
) -> Result<MeshReadback, BackendError> {
    let vertex_bytes = vertex_count as u64 * std::mem::size_of::<GpuVertex>() as u64;
    let index_bytes = index_count as u64 * std::mem::size_of::<u32>() as u64;
    let vertex_readback = create_readback(device, "primitive vertex readback", vertex_bytes);
    let index_readback = create_readback(device, "primitive index readback", index_bytes);
    encoder.copy_buffer_to_buffer(vertex_buffer, 0, &vertex_readback, 0, vertex_bytes);
    encoder.copy_buffer_to_buffer(index_buffer, 0, &index_readback, 0, index_bytes);
    queue.submit([encoder.finish()]);

    let vertex_mapping = map_async(&vertex_readback);
    let index_mapping = map_async(&index_readback);
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
            .slice(..)
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
    vertex_readback.unmap();

    let indices = {
        let mapped = index_readback
            .slice(..)
            .get_mapped_range()
            .map_err(|error| BackendError::Kernel {
                message: format!("primitive index readback mapping failed: {error}"),
            })?;
        let indices = bytemuck::cast_slice(&mapped).to_vec();
        drop(mapped);
        indices
    };
    index_readback.unmap();

    validate_readback(&vertices, &indices)?;
    Ok(MeshReadback { vertices, indices })
}

fn validate_readback(vertices: &[Vertex], indices: &[u32]) -> Result<(), BackendError> {
    if !vertices.is_empty()
        && indices.len().is_multiple_of(3)
        && !indices
            .iter()
            .any(|index| *index as usize >= vertices.len())
        && !vertices.iter().any(|vertex| {
            vertex
                .position
                .iter()
                .chain(vertex.normal.iter())
                .any(|value| !value.is_finite())
        })
    {
        Ok(())
    } else {
        Err(BackendError::Parity)
    }
}

fn create_readback(device: &Device, label: &str, size: u64) -> Buffer {
    device.create_buffer(&BufferDescriptor {
        label: Some(label),
        size,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    })
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
            ((rings + 1) * (segments + 1), rings * segments * 6)
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
