use std::sync::mpsc;
use std::time::Duration;

use bytemuck::{Pod, Zeroable};
use studio_core::geometry::{Primitive, Vertex};
use studio_core::gpu_contract::BackendError;
use wgpu::{
    BindGroupDescriptor, BindGroupEntry, Buffer, BufferAsyncError, BufferDescriptor, BufferUsages,
    CommandEncoderDescriptor, ComputePassDescriptor, ComputePipelineDescriptor, Device, MapMode,
    PollType, Queue, ShaderModuleDescriptor, ShaderSource,
};

#[derive(Debug, Clone, PartialEq)]
pub struct PrimitiveJob {
    pub primitive: Primitive,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct PrimitiveGpuParams {
    pub dimensions: [f32; 4],
    pub vertex_count: u32,
    pub index_count: u32,
    pub reserved_0: u32,
    pub reserved_1: u32,
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
            reserved_0: 0,
            reserved_1: 0,
        }
    }

    pub fn for_plane(width: f32, depth: f32) -> Self {
        Self {
            dimensions: [width, 0.0, depth, 0.0],
            vertex_count: 4,
            index_count: 6,
            reserved_0: 0,
            reserved_1: 0,
        }
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
    let vertex_readback = create_readback(device, "primitive vertex readback", vertex_bytes);
    let index_readback = create_readback(device, "primitive index readback", index_bytes);
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
    encoder.copy_buffer_to_buffer(&vertex_buffer, 0, &vertex_readback, 0, vertex_bytes);
    encoder.copy_buffer_to_buffer(&index_buffer, 0, &index_readback, 0, index_bytes);
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

    if vertices.iter().any(|vertex| {
        vertex
            .position
            .iter()
            .chain(vertex.normal.iter())
            .any(|value| !value.is_finite())
    }) || indices
        .iter()
        .any(|index| *index as usize >= vertices.len())
    {
        return Err(BackendError::Parity);
    }

    Ok(MeshReadback { vertices, indices })
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
