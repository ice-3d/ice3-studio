use std::collections::BTreeMap;

use studio_core::gpu_contract::BackendError;
use wgpu::{
    BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType,
    BufferBindingType, ComputePipeline, ComputePipelineDescriptor, Device,
    PipelineLayoutDescriptor, ShaderModuleDescriptor, ShaderSource, ShaderStages,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PipelineKey {
    Box,
    Plane,
    Cylinder,
    Sphere,
    Sweep,
    MeshFaceNormals,
    MeshNormals,
    MeshBounds,
    InstanceVisibility,
}

pub struct PipelineCache {
    pipelines: BTreeMap<PipelineKey, ComputePipeline>,
    radial_layout: BindGroupLayout,
    mesh_layout: BindGroupLayout,
    visibility_layout: BindGroupLayout,
}

impl PipelineCache {
    pub(crate) fn new(device: &Device) -> Result<Self, BackendError> {
        let box_plane_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("ICE3 resident box/plane shader"),
            source: ShaderSource::Wgsl(include_str!("../kernels/box_plane.wgsl").into()),
        });
        let radial_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("ICE3 resident radial shader"),
            source: ShaderSource::Wgsl(include_str!("../kernels/radial.wgsl").into()),
        });
        let mesh_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("ICE3 resident mesh shader"),
            source: ShaderSource::Wgsl(include_str!("../kernels/mesh.wgsl").into()),
        });
        let visibility_shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("ICE3 instance visibility shader"),
            source: ShaderSource::Wgsl(include_str!("../scene/visibility.wgsl").into()),
        });

        let radial_layout = radial_bind_group_layout(device);
        let mesh_layout = mesh_bind_group_layout(device);
        let visibility_layout = visibility_bind_group_layout(device);
        let radial_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("ICE3 resident radial pipeline layout"),
            bind_group_layouts: &[Some(&radial_layout)],
            immediate_size: 0,
        });
        let mesh_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("ICE3 resident mesh pipeline layout"),
            bind_group_layouts: &[Some(&mesh_layout)],
            immediate_size: 0,
        });
        let visibility_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("ICE3 instance visibility pipeline layout"),
            bind_group_layouts: &[Some(&visibility_layout)],
            immediate_size: 0,
        });

        let mut pipelines = BTreeMap::new();
        pipelines.insert(
            PipelineKey::Box,
            create_pipeline(device, None, &box_plane_shader, "Box", "generate_box"),
        );
        pipelines.insert(
            PipelineKey::Plane,
            create_pipeline(device, None, &box_plane_shader, "Plane", "generate_plane"),
        );
        pipelines.insert(
            PipelineKey::Cylinder,
            create_pipeline(
                device,
                Some(&radial_pipeline_layout),
                &radial_shader,
                "Cylinder",
                "generate_cylinder",
            ),
        );
        pipelines.insert(
            PipelineKey::InstanceVisibility,
            create_pipeline(
                device,
                Some(&visibility_pipeline_layout),
                &visibility_shader,
                "Instance visibility",
                "cull_instances",
            ),
        );
        pipelines.insert(
            PipelineKey::Sphere,
            create_pipeline(
                device,
                Some(&radial_pipeline_layout),
                &radial_shader,
                "Sphere",
                "generate_sphere",
            ),
        );
        pipelines.insert(
            PipelineKey::Sweep,
            create_pipeline(
                device,
                Some(&radial_pipeline_layout),
                &radial_shader,
                "Sweep",
                "generate_sweep",
            ),
        );
        pipelines.insert(
            PipelineKey::MeshFaceNormals,
            create_pipeline(
                device,
                Some(&mesh_pipeline_layout),
                &mesh_shader,
                "Mesh face normals",
                "generate_face_normals",
            ),
        );
        pipelines.insert(
            PipelineKey::MeshNormals,
            create_pipeline(
                device,
                Some(&mesh_pipeline_layout),
                &mesh_shader,
                "Mesh normals",
                "accumulate_normals",
            ),
        );
        pipelines.insert(
            PipelineKey::MeshBounds,
            create_pipeline(
                device,
                Some(&mesh_pipeline_layout),
                &mesh_shader,
                "Mesh bounds",
                "reduce_bounds",
            ),
        );

        Ok(Self {
            pipelines,
            radial_layout,
            mesh_layout,
            visibility_layout,
        })
    }

    pub(crate) fn get(&self, key: PipelineKey) -> Result<&ComputePipeline, BackendError> {
        self.pipelines
            .get(&key)
            .ok_or_else(|| BackendError::Kernel {
                message: format!("resident pipeline {key:?} is unavailable"),
            })
    }

    pub(crate) fn radial_layout(&self) -> &BindGroupLayout {
        &self.radial_layout
    }

    pub(crate) fn mesh_layout(&self) -> &BindGroupLayout {
        &self.mesh_layout
    }

    pub(crate) fn visibility_layout(&self) -> &BindGroupLayout {
        &self.visibility_layout
    }

    pub(crate) fn count(&self) -> usize {
        self.pipelines.len()
    }
}

fn create_pipeline(
    device: &Device,
    layout: Option<&wgpu::PipelineLayout>,
    shader: &wgpu::ShaderModule,
    label: &str,
    entry_point: &str,
) -> ComputePipeline {
    device.create_compute_pipeline(&ComputePipelineDescriptor {
        label: Some(label),
        layout,
        module: shader,
        entry_point: Some(entry_point),
        compilation_options: Default::default(),
        cache: None,
    })
}

fn radial_bind_group_layout(device: &Device) -> BindGroupLayout {
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("ICE3 resident radial binding layout"),
        entries: &[
            storage_entry(
                0,
                BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
            ),
            storage_entry(1, writable_buffer()),
            storage_entry(2, writable_buffer()),
            storage_entry(3, readonly_buffer()),
        ],
    })
}

fn mesh_bind_group_layout(device: &Device) -> BindGroupLayout {
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("ICE3 resident mesh binding layout"),
        entries: &[
            storage_entry(
                0,
                BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
            ),
            storage_entry(1, readonly_buffer()),
            storage_entry(2, readonly_buffer()),
            storage_entry(3, readonly_buffer()),
            storage_entry(4, readonly_buffer()),
            storage_entry(5, writable_buffer()),
            storage_entry(6, writable_buffer()),
            storage_entry(7, writable_buffer()),
        ],
    })
}

fn visibility_bind_group_layout(device: &Device) -> BindGroupLayout {
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("ICE3 instance visibility binding layout"),
        entries: &[
            storage_entry(0, readonly_buffer()),
            storage_entry(1, readonly_buffer()),
            storage_entry(
                2,
                BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
            ),
            storage_entry(3, writable_buffer()),
            storage_entry(4, writable_buffer()),
        ],
    })
}

fn storage_entry(binding: u32, ty: BindingType) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding,
        visibility: ShaderStages::COMPUTE,
        ty,
        count: None,
    }
}

fn readonly_buffer() -> BindingType {
    BindingType::Buffer {
        ty: BufferBindingType::Storage { read_only: true },
        has_dynamic_offset: false,
        min_binding_size: None,
    }
}

fn writable_buffer() -> BindingType {
    BindingType::Buffer {
        ty: BufferBindingType::Storage { read_only: false },
        has_dynamic_offset: false,
        min_binding_size: None,
    }
}
