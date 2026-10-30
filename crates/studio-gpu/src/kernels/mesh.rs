use std::sync::mpsc;
use std::time::Duration;

use bytemuck::{Pod, Zeroable};
use studio_core::geometry::{Bounds, EvaluatedMesh, TriangleMesh, Vertex};
use studio_core::gpu_contract::BackendError;
use studio_core::math::Vec3;
use wgpu::{
    BindGroupDescriptor, BindGroupEntry, BindGroupLayoutDescriptor, BindGroupLayoutEntry,
    BindingType, Buffer, BufferAsyncError, BufferBindingType, BufferDescriptor, BufferUsages,
    CommandEncoderDescriptor, ComputePassDescriptor, ComputePipelineDescriptor, Device, MapMode,
    PipelineLayoutDescriptor, PollType, Queue, ShaderModuleDescriptor, ShaderSource, ShaderStages,
};

const MAX_MESH_BYTES: u64 = 512 * 1024 * 1024;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct MeshParams {
    vertex_count: u32,
    triangle_count: u32,
    workgroup_count: u32,
    reserved: u32,
}

pub fn evaluate_custom_mesh(
    device: &Device,
    queue: &Queue,
    mesh: &TriangleMesh,
) -> Result<EvaluatedMesh, BackendError> {
    let geometry = validate_mesh_input(mesh)?;
    let adjacency = build_adjacency(mesh, geometry.vertex_count)?;
    validate_memory_budget(&geometry, adjacency.offsets.len())?;

    let positions = mesh
        .positions
        .iter()
        .map(|position| [position[0], position[1], position[2], 1.0])
        .collect::<Vec<_>>();
    let params = MeshParams {
        vertex_count: geometry.vertex_count,
        triangle_count: geometry.triangle_count,
        workgroup_count: geometry.workgroup_count,
        reserved: 0,
    };

    let params_buffer = create_mapped_buffer(
        device,
        "ICE3 mesh parameters",
        BufferUsages::UNIFORM,
        bytemuck::bytes_of(&params),
    )?;
    let position_buffer = create_mapped_buffer(
        device,
        "ICE3 mesh positions",
        BufferUsages::STORAGE,
        bytemuck::cast_slice(&positions),
    )?;
    let index_buffer = create_mapped_buffer(
        device,
        "ICE3 mesh indices",
        BufferUsages::STORAGE,
        bytemuck::cast_slice(&mesh.indices),
    )?;
    let offset_buffer = create_mapped_buffer(
        device,
        "ICE3 mesh adjacency offsets",
        BufferUsages::STORAGE,
        bytemuck::cast_slice(&adjacency.offsets),
    )?;
    let incidence_buffer = create_mapped_buffer(
        device,
        "ICE3 mesh adjacency incidence",
        BufferUsages::STORAGE,
        bytemuck::cast_slice(&adjacency.incidence),
    )?;
    let face_normal_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("ICE3 mesh face normals"),
        size: geometry.triangle_count as u64 * std::mem::size_of::<[f32; 4]>() as u64,
        usage: BufferUsages::STORAGE,
        mapped_at_creation: false,
    });
    let normal_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("ICE3 mesh vertex normals"),
        size: geometry.vertex_count as u64 * std::mem::size_of::<[f32; 4]>() as u64,
        usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let bounds_buffer = device.create_buffer(&BufferDescriptor {
        label: Some("ICE3 mesh bounds partials"),
        size: geometry.workgroup_count as u64 * 2 * std::mem::size_of::<[f32; 4]>() as u64,
        usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("ICE3 mesh shaders"),
        source: ShaderSource::Wgsl(include_str!("mesh.wgsl").into()),
    });
    let layout = create_pipeline_layout(device);
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("ICE3 mesh pipeline layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let face_pipeline = create_pipeline(
        device,
        &pipeline_layout,
        &shader,
        "ICE3 mesh face normal pipeline",
        "generate_face_normals",
    );
    let normal_pipeline = create_pipeline(
        device,
        &pipeline_layout,
        &shader,
        "ICE3 mesh normal pipeline",
        "accumulate_normals",
    );
    let bounds_pipeline = create_pipeline(
        device,
        &pipeline_layout,
        &shader,
        "ICE3 mesh bounds pipeline",
        "reduce_bounds",
    );
    let bind_group = device.create_bind_group(&BindGroupDescriptor {
        label: Some("ICE3 mesh bindings"),
        layout: &layout,
        entries: &[
            binding(0, &params_buffer),
            binding(1, &position_buffer),
            binding(2, &index_buffer),
            binding(3, &offset_buffer),
            binding(4, &incidence_buffer),
            binding(5, &face_normal_buffer),
            binding(6, &normal_buffer),
            binding(7, &bounds_buffer),
        ],
    });

    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("ICE3 mesh encoder"),
    });
    {
        let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
            label: Some("ICE3 mesh compute pass"),
            timestamp_writes: None,
        });
        pass.set_bind_group(0, &bind_group, &[]);
        pass.set_pipeline(&face_pipeline);
        pass.dispatch_workgroups(geometry.triangle_count.div_ceil(64), 1, 1);
        pass.set_pipeline(&normal_pipeline);
        pass.dispatch_workgroups(geometry.workgroup_count, 1, 1);
        pass.set_pipeline(&bounds_pipeline);
        pass.dispatch_workgroups(geometry.workgroup_count, 1, 1);
    }

    let (normals, bounds) = readback_pair(
        device,
        queue,
        encoder,
        &normal_buffer,
        &bounds_buffer,
        geometry.vertex_count as u64 * std::mem::size_of::<[f32; 4]>() as u64,
        geometry.workgroup_count as u64 * 2 * std::mem::size_of::<[f32; 4]>() as u64,
    )?;

    let normals: &[[f32; 4]] = bytemuck::cast_slice(&normals);
    if normals
        .iter()
        .flatten()
        .any(|component| !component.is_finite())
    {
        return Err(BackendError::Parity);
    }
    let bounds: &[[f32; 4]] = bytemuck::cast_slice(&bounds);
    let bounds = reduce_partial_bounds(bounds)?;
    let vertices = mesh
        .positions
        .iter()
        .copied()
        .zip(
            normals
                .iter()
                .map(|normal| [normal[0], normal[1], normal[2]]),
        )
        .map(|(position, normal)| Vertex { position, normal })
        .collect();
    Ok(EvaluatedMesh {
        vertices,
        indices: mesh.indices.clone(),
        bounds,
    })
}

#[derive(Debug)]
struct MeshGeometry {
    vertex_count: u32,
    triangle_count: u32,
    workgroup_count: u32,
}

fn validate_mesh_input(mesh: &TriangleMesh) -> Result<MeshGeometry, BackendError> {
    if mesh.positions.is_empty() || mesh.indices.is_empty() || !mesh.indices.len().is_multiple_of(3)
    {
        return Err(BackendError::Kernel {
            message: "custom mesh must contain indexed triangles".into(),
        });
    }
    if mesh
        .positions
        .iter()
        .flatten()
        .any(|component| !component.is_finite())
        || mesh
            .indices
            .iter()
            .any(|index| *index as usize >= mesh.positions.len())
    {
        return Err(BackendError::Kernel {
            message: "custom mesh positions or indices are invalid".into(),
        });
    }
    let vertex_count =
        u32::try_from(mesh.positions.len()).map_err(|_| BackendError::BudgetExceeded)?;
    let triangle_count =
        u32::try_from(mesh.indices.len() / 3).map_err(|_| BackendError::BudgetExceeded)?;
    let workgroup_count = vertex_count.div_ceil(64);
    Ok(MeshGeometry {
        vertex_count,
        triangle_count,
        workgroup_count,
    })
}

#[derive(Debug)]
struct MeshAdjacency {
    offsets: Vec<u32>,
    incidence: Vec<u32>,
}

fn build_adjacency(mesh: &TriangleMesh, vertex_count: u32) -> Result<MeshAdjacency, BackendError> {
    let mut offsets = vec![0u32; vertex_count as usize + 1];
    for triangle in mesh.indices.as_chunks::<3>().0 {
        for vertex in triangle {
            let slot =
                offsets
                    .get_mut(*vertex as usize + 1)
                    .ok_or_else(|| BackendError::Kernel {
                        message: "custom mesh index is out of range".into(),
                    })?;
            *slot = slot.checked_add(1).ok_or(BackendError::BudgetExceeded)?;
        }
    }
    for index in 1..offsets.len() {
        offsets[index] = offsets[index - 1]
            .checked_add(offsets[index])
            .ok_or(BackendError::BudgetExceeded)?;
    }

    let mut cursor = offsets[..vertex_count as usize].to_vec();
    let mut incidence = vec![0u32; mesh.indices.len()];
    for (triangle_index, triangle) in mesh.indices.as_chunks::<3>().0.iter().enumerate() {
        for vertex in triangle {
            let cursor = cursor
                .get_mut(*vertex as usize)
                .ok_or_else(|| BackendError::Kernel {
                    message: "custom mesh index is out of range".into(),
                })?;
            let incidence = incidence
                .get_mut(*cursor as usize)
                .ok_or(BackendError::BudgetExceeded)?;
            *incidence = triangle_index as u32;
            *cursor += 1;
        }
    }
    Ok(MeshAdjacency { offsets, incidence })
}

fn validate_memory_budget(
    geometry: &MeshGeometry,
    offset_count: usize,
) -> Result<(), BackendError> {
    let bytes = [
        geometry.vertex_count as u64 * 16,
        geometry.triangle_count as u64 * 12,
        offset_count as u64 * 4,
        geometry.triangle_count as u64 * 12,
        geometry.triangle_count as u64 * 16,
        geometry.vertex_count as u64 * 16,
        geometry.workgroup_count as u64 * 32,
    ]
    .into_iter()
    .try_fold(0u64, u64::checked_add)
    .ok_or(BackendError::BudgetExceeded)?;
    if bytes > MAX_MESH_BYTES {
        Err(BackendError::BudgetExceeded)
    } else {
        Ok(())
    }
}

fn create_mapped_buffer(
    device: &Device,
    label: &str,
    usage: BufferUsages,
    contents: &[u8],
) -> Result<Buffer, BackendError> {
    let buffer = device.create_buffer(&BufferDescriptor {
        label: Some(label),
        size: contents.len() as u64,
        usage,
        mapped_at_creation: true,
    });
    let mut mapped =
        buffer
            .slice(..)
            .get_mapped_range_mut()
            .map_err(|error| BackendError::Kernel {
                message: format!("{label} mapping failed: {error}"),
            })?;
    mapped.copy_from_slice(contents);
    drop(mapped);
    buffer.unmap();
    Ok(buffer)
}

fn create_pipeline_layout(device: &Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("ICE3 mesh binding layout"),
        entries: &[
            entry(0, BufferBindingType::Uniform),
            entry(1, BufferBindingType::Storage { read_only: true }),
            entry(2, BufferBindingType::Storage { read_only: true }),
            entry(3, BufferBindingType::Storage { read_only: true }),
            entry(4, BufferBindingType::Storage { read_only: true }),
            entry(5, BufferBindingType::Storage { read_only: false }),
            entry(6, BufferBindingType::Storage { read_only: false }),
            entry(7, BufferBindingType::Storage { read_only: false }),
        ],
    })
}

fn entry(binding: u32, ty: BufferBindingType) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding,
        visibility: ShaderStages::COMPUTE,
        ty: BindingType::Buffer {
            ty,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn binding<'a>(binding: u32, buffer: &'a Buffer) -> BindGroupEntry<'a> {
    BindGroupEntry {
        binding,
        resource: buffer.as_entire_binding(),
    }
}

fn create_pipeline(
    device: &Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    label: &str,
    entry_point: &str,
) -> wgpu::ComputePipeline {
    device.create_compute_pipeline(&ComputePipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        module: shader,
        entry_point: Some(entry_point),
        compilation_options: Default::default(),
        cache: None,
    })
}

fn readback_pair(
    device: &Device,
    queue: &Queue,
    mut encoder: wgpu::CommandEncoder,
    normal_source: &Buffer,
    bounds_source: &Buffer,
    normal_size: u64,
    bounds_size: u64,
) -> Result<(Vec<u8>, Vec<u8>), BackendError> {
    let normal_target = device.create_buffer(&BufferDescriptor {
        label: Some("ICE3 mesh normal readback"),
        size: normal_size,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let bounds_target = device.create_buffer(&BufferDescriptor {
        label: Some("ICE3 mesh bounds readback"),
        size: bounds_size,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_buffer_to_buffer(normal_source, 0, &normal_target, 0, normal_size);
    encoder.copy_buffer_to_buffer(bounds_source, 0, &bounds_target, 0, bounds_size);
    queue.submit([encoder.finish()]);
    let normal_receiver = map_async(&normal_target);
    let bounds_receiver = map_async(&bounds_target);
    device
        .poll(PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(30)),
        })
        .map_err(|error| BackendError::Kernel {
            message: format!("mesh GPU poll failed: {error}"),
        })?;
    wait_for_mapping(&normal_receiver, "normal")?;
    wait_for_mapping(&bounds_receiver, "bounds")?;
    let normals = {
        let mapped = normal_target
            .slice(..)
            .get_mapped_range()
            .map_err(|error| BackendError::Kernel {
                message: format!("mesh normal readback mapping failed: {error}"),
            })?;
        mapped.to_vec()
    };
    normal_target.unmap();
    let bounds = {
        let mapped = bounds_target
            .slice(..)
            .get_mapped_range()
            .map_err(|error| BackendError::Kernel {
                message: format!("mesh bounds readback mapping failed: {error}"),
            })?;
        mapped.to_vec()
    };
    bounds_target.unmap();
    Ok((normals, bounds))
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
        .recv_timeout(Duration::from_secs(30))
        .map_err(|error| BackendError::Kernel {
            message: format!("mesh {label} readback did not complete: {error}"),
        })?
        .map_err(|error| BackendError::Kernel {
            message: format!("mesh {label} readback failed: {error}"),
        })
}

fn reduce_partial_bounds(partials: &[[f32; 4]]) -> Result<Bounds, BackendError> {
    if partials.len() < 2 || !partials.len().is_multiple_of(2) {
        return Err(BackendError::Parity);
    }
    let mut minimum = [f32::INFINITY; 3];
    let mut maximum = [f32::NEG_INFINITY; 3];
    for pair in partials.as_chunks::<2>().0 {
        for axis in 0..3 {
            minimum[axis] = minimum[axis].min(pair[0][axis]);
            maximum[axis] = maximum[axis].max(pair[1][axis]);
        }
    }
    if minimum
        .iter()
        .chain(maximum.iter())
        .any(|component| !component.is_finite())
    {
        return Err(BackendError::Parity);
    }
    Ok(Bounds {
        min: Vec3 {
            x: minimum[0],
            y: minimum[1],
            z: minimum[2],
        },
        max: Vec3 {
            x: maximum[0],
            y: maximum[1],
            z: maximum[2],
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_custom_meshes_before_gpu_upload() {
        let mesh = TriangleMesh {
            positions: vec![[0.0, 0.0, 0.0]],
            indices: vec![0, 1, 0],
        };
        assert!(validate_mesh_input(&mesh).is_err());
    }
}
