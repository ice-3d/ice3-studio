//! Native source import registry and format-neutral source scene.

mod gltf;
pub mod obj;
mod registry;
mod source;

pub use gltf::GltfImporter;
pub use obj::ObjImporter;
pub use registry::SourceImporterRegistry;
pub use source::{
    ImportError, ImportRequest, ImportWarning, SourceCapabilities, SourceFormat, SourceImporter,
    SourceMaterial, SourceMesh, SourceNode, SourcePrimitive, SourceScene,
};
