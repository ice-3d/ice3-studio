use std::path::Path;

use oxideav_step::{
    ApSchema, Error as StepError, GeometryLimits, ReadOptions, StepLimits, Tolerance,
};

use super::mapping::project_model;
use crate::{
    ImportError, ImportRequest, NativeImportLimits, SourceCapabilities, SourceImporter, SourceScene,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct StepImporter;

impl StepImporter {
    pub const fn new() -> Self {
        Self
    }

    pub fn import_file(&self, path: &Path, asset_id: &str) -> Result<SourceScene, ImportError> {
        self.import_file_with_limits(path, asset_id, NativeImportLimits::default())
    }

    pub fn import_file_with_limits(
        &self,
        path: &Path,
        asset_id: &str,
        limits: NativeImportLimits,
    ) -> Result<SourceScene, ImportError> {
        self.import_with_limits(
            &ImportRequest {
                path: path.to_path_buf(),
                asset_id: asset_id.into(),
                requested_adapter: Some("step".into()),
            },
            limits,
        )
    }

    pub fn import_with_limits(
        &self,
        request: &ImportRequest,
        limits: NativeImportLimits,
    ) -> Result<SourceScene, ImportError> {
        let asset_id = request.asset_id.trim();
        if asset_id.is_empty() {
            return Err(ImportError::InvalidSource(
                "asset ID must not be empty".into(),
            ));
        }
        let bytes = read_bounded_source(&request.path, limits.max_memory_bytes)?;
        let model =
            oxideav_step::read_step_with(&bytes, &read_options(limits)?).map_err(map_step_error)?;
        if model.schema != ApSchema::Ap242 {
            return Err(ImportError::SourceDiagnostic {
                code: "ICE3_STUDIO_IMPORT_STEP_SCHEMA_UNSUPPORTED".into(),
                message: format!("unsupported STEP schema: {:?}", model.schema),
            });
        }
        project_model(&model, asset_id, limits)
    }
}

impl SourceImporter for StepImporter {
    fn id(&self) -> &'static str {
        "step"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &[".step", ".stp"]
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

    fn import(&self, request: &ImportRequest) -> Result<SourceScene, ImportError> {
        self.import_with_limits(request, NativeImportLimits::default())
    }
}

fn read_bounded_source(path: &Path, max_bytes: usize) -> Result<Vec<u8>, ImportError> {
    let metadata = std::fs::metadata(path)?;
    if metadata.len() > max_bytes as u64 {
        return Err(ImportError::LimitExceeded(format!(
            "STEP input has {} bytes; limit is {max_bytes}",
            metadata.len()
        )));
    }
    let bytes = std::fs::read(path)?;
    if bytes.len() > max_bytes {
        return Err(ImportError::LimitExceeded(format!(
            "STEP input has {} bytes; limit is {max_bytes}",
            bytes.len()
        )));
    }
    Ok(bytes)
}

fn read_options(limits: NativeImportLimits) -> Result<ReadOptions, ImportError> {
    Ok(ReadOptions {
        parse_limits: StepLimits {
            max_input_len: limits.max_memory_bytes,
            max_instances: limits.max_nodes,
            ..Default::default()
        },
        geometry_limits: GeometryLimits {
            max_triangles: usize::try_from(limits.max_triangles)
                .map_err(|_| ImportError::LimitExceeded("STEP triangle budget".into()))?,
            ..Default::default()
        },
        tolerance: Tolerance {
            relative: 0.001,
            absolute: None,
            max_angle: 0.5,
        },
        keep_invisible: false,
    })
}

fn map_step_error(error: StepError) -> ImportError {
    match error {
        StepError::LimitExceeded(message) => ImportError::LimitExceeded(message),
        StepError::NoGeometry(_) => ImportError::EmptyGeometry,
        StepError::Parse(_) => ImportError::InvalidSource(error.to_string()),
    }
}
