//! Native source import registry and format-neutral source scene.

mod gltf;
mod registry;
mod source;

pub use gltf::GltfImporter;
pub use registry::SourceImporterRegistry;
pub use source::{
    ImportError, ImportRequest, ImportWarning, SourceCapabilities, SourceFormat, SourceImporter,
    SourceMaterial, SourceMesh, SourceNode, SourcePrimitive, SourceScene,
};
