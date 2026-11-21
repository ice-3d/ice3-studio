//! Native source import registry and format-neutral source scene.

#[cfg(feature = "fbx")]
mod fbx;
mod gltf;
#[cfg(feature = "ifc")]
mod ifc;
#[cfg(feature = "jt")]
pub mod jt;
mod limits;
pub mod obj;
mod registry;
mod source;
#[cfg(feature = "step")]
mod step;
#[cfg(feature = "usd")]
mod usd;
mod worker;

#[cfg(feature = "fbx")]
pub use fbx::FbxImporter;
pub use gltf::GltfImporter;
#[cfg(feature = "ifc")]
pub use ifc::{
    IfcImporter, IfcMetadata, IfcNodeMetadata, IfcProductMetadata, IfcPropertySetMetadata,
    IfcQuantitySetMetadata, IfcSystemMetadata, IfcSystemReference, extract_ifc_metadata,
};
#[cfg(feature = "jt")]
pub use jt::JtImporter;
pub use limits::{NativeImportLimits, WorkerLimits, validate_scene_limits};
pub use obj::ObjImporter;
pub use registry::SourceImporterRegistry;
pub use source::{
    ImportError, ImportRequest, ImportWarning, SourceCapabilities, SourceFormat, SourceImporter,
    SourceMaterial, SourceMesh, SourceNode, SourceNormalTexture, SourcePrimitive, SourceResource,
    SourceScene, SourceSurface, SourceTextureBinding,
};
#[cfg(feature = "step")]
pub use step::StepImporter;
#[cfg(feature = "usd")]
pub use usd::UsdImporter;
pub use worker::{
    WORKER_PROTOCOL_VERSION, WorkerRequest, WorkerResponse, decode_worker_response,
    execute_worker_request, run_isolated_import,
};

pub fn to_canonical(source: &SourceScene) -> studio_assets::canonical::CanonicalScene {
    studio_assets::canonical::CanonicalScene {
        asset_id: source.asset_id.clone(),
        source_format: source.source_format.as_str().into(),
        nodes: source
            .nodes
            .iter()
            .map(|node| studio_assets::canonical::CanonicalNode {
                id: node.id.clone(),
                name: node.name.clone(),
                parent: node.parent.clone(),
                matrix: node.matrix,
                mesh: node.mesh,
                material: node.material,
                extras: node.extras.clone(),
            })
            .collect(),
        meshes: source
            .meshes
            .iter()
            .map(|mesh| studio_assets::canonical::CanonicalMesh {
                id: mesh.id.clone(),
                primitives: mesh
                    .primitives
                    .iter()
                    .map(|primitive| studio_assets::canonical::CanonicalPrimitive {
                        positions: primitive.positions.clone(),
                        normals: primitive.normals.clone(),
                        colors: primitive.colors.clone(),
                        uvs: primitive.uvs.clone(),
                        indices: primitive.indices.clone(),
                        material: primitive.material,
                    })
                    .collect(),
                extras: mesh.extras.clone(),
            })
            .collect(),
        materials: source
            .materials
            .iter()
            .map(|material| studio_assets::canonical::CanonicalMaterial {
                id: material.id.clone(),
                name: material.name.clone(),
                base_color: material.base_color,
                opacity: material.opacity,
                alpha_mode: material.alpha_mode,
                alpha_cutoff: material.alpha_cutoff,
                double_sided: material.double_sided,
                surface: material.surface.as_ref().map(map_surface),
                extras: material.extras.clone(),
                texture: None,
            })
            .collect(),
        resources: source
            .resources
            .iter()
            .map(|resource| studio_assets::canonical::CanonicalResource {
                digest: resource.digest.clone(),
                media_type: resource.media_type.clone(),
                bytes: resource.bytes.clone(),
            })
            .collect(),
        asset_extras: source.asset_extras.clone(),
    }
}

fn map_surface(surface: &SourceSurface) -> studio_assets::canonical::CanonicalSurface {
    studio_assets::canonical::CanonicalSurface {
        metallic: surface.metallic,
        roughness: surface.roughness,
        emissive: surface.emissive,
        base_color_texture: surface.base_color_texture.as_ref().map(map_binding),
        normal_texture: surface.normal_texture.as_ref().map(|normal| {
            studio_assets::canonical::CanonicalNormalTexture {
                binding: map_binding(&normal.binding),
                scale: normal.scale,
            }
        }),
        occlusion_texture: surface.occlusion_texture.as_ref().map(map_binding),
        metallic_roughness_texture: surface.metallic_roughness_texture.as_ref().map(map_binding),
    }
}

fn map_binding(
    binding: &SourceTextureBinding,
) -> studio_assets::canonical::CanonicalTextureBinding {
    studio_assets::canonical::CanonicalTextureBinding {
        resource: binding.resource.clone(),
        tex_coord: binding.tex_coord,
        transform: binding.transform,
        sampler: binding.sampler,
    }
}
