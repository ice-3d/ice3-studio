use std::path::Path;

use ifc_lite_processing::ProcessingRequest;

use super::{extract_ifc_metadata, geometry::project_ifc_scene};
use crate::{
    ImportError, ImportRequest, NativeImportLimits, SourceCapabilities, SourceImporter, SourceScene,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct IfcImporter;

impl IfcImporter {
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
                asset_id: asset_id.to_owned(),
                requested_adapter: Some("ifc".into()),
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
        let metadata = extract_ifc_metadata(&bytes, asset_id, limits)?;
        let processing = ProcessingRequest::full_pass()
            .retain_meshes(true)
            .instancing(true);
        let result = ifc_lite_processing::process(&bytes, &processing, &mut ());
        if result.metadata.schema_version != metadata.schema {
            return Err(ImportError::SourceDiagnostic {
                code: "ICE3_STUDIO_IMPORT_IFC_SCHEMA_UNSUPPORTED".into(),
                message: format!(
                    "schema changed during geometry processing: {} -> {}",
                    metadata.schema, result.metadata.schema_version
                ),
            });
        }
        let unit_scale =
            result
                .metadata
                .length_unit_scale
                .ok_or_else(|| ImportError::SourceDiagnostic {
                    code: "ICE3_STUDIO_IMPORT_IFC_UNIT_UNSUPPORTED".into(),
                    message: "missing IFC length unit".into(),
                })?;
        if !unit_scale.is_finite() || unit_scale <= 0.0 {
            return Err(ImportError::SourceDiagnostic {
                code: "ICE3_STUDIO_IMPORT_IFC_UNIT_UNSUPPORTED".into(),
                message: format!("invalid IFC length unit scale: {unit_scale}"),
            });
        }
        project_ifc_scene(&metadata, &result, asset_id, limits)
    }
}

impl SourceImporter for IfcImporter {
    fn id(&self) -> &'static str {
        "ifc"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &[".ifc"]
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

fn read_bounded_source(path: &Path, limit: usize) -> Result<Vec<u8>, ImportError> {
    let metadata = std::fs::metadata(path)?;
    if metadata.len() > limit as u64 {
        return Err(ImportError::LimitExceeded(format!(
            "IFC input has {} bytes; limit is {limit}",
            metadata.len()
        )));
    }
    let bytes = std::fs::read(path)?;
    if bytes.len() > limit {
        return Err(ImportError::LimitExceeded(format!(
            "IFC input has {} bytes; limit is {limit}",
            bytes.len()
        )));
    }
    Ok(bytes)
}
