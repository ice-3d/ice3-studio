use crate::{ImportError, ImportRequest, SourceCapabilities, SourceImporter, SourceScene};

#[derive(Debug, Clone, Copy, Default)]
pub struct JtImporter;

impl JtImporter {
    pub const fn new() -> Self {
        Self
    }
}

impl SourceImporter for JtImporter {
    fn id(&self) -> &'static str {
        "jt"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &[".jt"]
    }

    fn capabilities(&self) -> SourceCapabilities {
        SourceCapabilities {
            hierarchy: true,
            vertex_colors: false,
            material_colors: true,
            instances: true,
            semantic_metadata: true,
        }
    }

    fn import(&self, _request: &ImportRequest) -> Result<SourceScene, ImportError> {
        Err(ImportError::AdapterUnavailable(
            "JT converter implementation is not wired".into(),
        ))
    }
}
