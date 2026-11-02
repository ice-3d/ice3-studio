use std::path::Path;
use std::time::Duration;

use crate::jt::converter::{JtConverterConfig, run_jt_converter};
use crate::jt::mapping::apply_jt_semantics;
use crate::jt::mapping::prepare_source_identities;
use crate::jt::rebase::rebase_json;
use crate::{ImportError, ImportRequest, SourceCapabilities, SourceImporter, SourceScene};

#[derive(Debug, Clone, Default)]
pub struct JtImporter {
    converter: Option<JtConverterConfig>,
}

impl JtImporter {
    pub const fn new() -> Self {
        Self { converter: None }
    }

    pub fn with_converter(binary: impl Into<std::path::PathBuf>, timeout: Duration) -> Self {
        Self {
            converter: Some(JtConverterConfig::new(binary, timeout)),
        }
    }

    pub fn with_converter_and_supervisor(
        binary: impl Into<std::path::PathBuf>,
        supervisor: impl Into<std::path::PathBuf>,
        timeout: Duration,
    ) -> Self {
        Self {
            converter: Some(JtConverterConfig::new(binary, timeout).with_supervisor(supervisor)),
        }
    }

    pub fn import_file(&self, path: &Path, asset_id: &str) -> Result<SourceScene, ImportError> {
        self.import(&ImportRequest {
            path: path.to_path_buf(),
            asset_id: asset_id.into(),
            requested_adapter: Some("jt".into()),
        })
    }

    fn converter_config(&self) -> Result<JtConverterConfig, ImportError> {
        self.converter
            .clone()
            .map(Ok)
            .unwrap_or_else(JtConverterConfig::from_env)
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

    fn import(&self, request: &ImportRequest) -> Result<SourceScene, ImportError> {
        let asset_id = request.asset_id.trim();
        if asset_id.is_empty() {
            return Err(ImportError::InvalidSource(
                "asset ID must not be empty".into(),
            ));
        }
        let temporary = tempfile::Builder::new()
            .prefix("ice3-studio-jt-")
            .tempdir()?;
        let raw = temporary.path().join("raw.glb");
        let config = self.converter_config()?;
        run_jt_converter(&request.path, &raw, &config)?;
        let mut source = studio_assets::read_glb(&raw).map_err(|_| {
            jt_diagnostic(
                "ICE3_STUDIO_IMPORT_JT_EXPORT_INVALID",
                "JT converter did not produce a valid GLB",
            )
        })?;
        let mut json = source.json.clone();
        let origin = rebase_json(&mut json, &source)?;
        prepare_source_identities(&mut json, asset_id)?;
        source.json = json;
        let mut scene =
            crate::gltf::import_source_glb(&source, asset_id).map_err(map_gltf_error)?;
        apply_jt_semantics(&mut scene, &source.json, origin.0)?;
        Ok(scene)
    }
}

fn map_gltf_error(error: ImportError) -> ImportError {
    match error {
        ImportError::EmptyGeometry => ImportError::EmptyGeometry,
        ImportError::WorkerTimeout
        | ImportError::WorkerProtocolInvalid(_)
        | ImportError::WorkerReported { .. }
        | ImportError::WorkerFailed(_)
        | ImportError::WorkerOutputTooLarge(_) => error,
        ImportError::InvalidSource(_)
        | ImportError::LimitExceeded(_)
        | ImportError::ResourceIgnored(_)
        | ImportError::SourceDiagnostic { .. } => jt_diagnostic(
            "ICE3_STUDIO_IMPORT_JT_GEOMETRY_INVALID",
            "JT geometry, bounds or transform is invalid",
        ),
        other => other,
    }
}

fn jt_diagnostic(code: &str, message: &str) -> ImportError {
    ImportError::SourceDiagnostic {
        code: code.into(),
        message: message.into(),
    }
}
