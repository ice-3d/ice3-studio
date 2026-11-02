//! Native source import registry and format-neutral source scene.

#[cfg(feature = "fbx")]
mod fbx;
mod gltf;
mod limits;
pub mod obj;
mod registry;
mod source;
#[cfg(feature = "usd")]
mod usd;
mod worker;

#[cfg(feature = "fbx")]
pub use fbx::FbxImporter;
pub use gltf::GltfImporter;
pub use limits::{NativeImportLimits, WorkerLimits, validate_scene_limits};
pub use obj::ObjImporter;
pub use registry::SourceImporterRegistry;
pub use source::{
    ImportError, ImportRequest, ImportWarning, SourceCapabilities, SourceFormat, SourceImporter,
    SourceMaterial, SourceMesh, SourceNode, SourcePrimitive, SourceScene,
};
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
                extras: material.extras.clone(),
            })
            .collect(),
        asset_extras: source.asset_extras.clone(),
    }
}
