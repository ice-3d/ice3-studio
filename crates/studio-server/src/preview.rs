use std::path::{Path, PathBuf};
use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use studio_core::document::Document;
use studio_core::export::glb::export_glb_from_scene;
use studio_core::large_scene::evaluate::{LargeSceneError, evaluate_large_scene_with_cache};
use studio_project::geometry_cache::{GeometryCacheError, GeometryDiskCache};
use thiserror::Error;
use ts_rs::TS;

use crate::preview_packet::{
    build_full_preview_packet, decode_preview_packet, encode_preview_packet,
};
use crate::toolchain::ToolchainService;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RuntimePreview {
    pub revision: u64,
    pub asset_id: String,
    pub source_digest: String,
    pub content_hash: String,
    pub source_path: PathBuf,
    pub preview_path: PathBuf,
    pub packet_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SourceExport {
    pub revision: u64,
    pub source_digest: String,
    pub source_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CandidatePreview {
    pub asset_id: String,
    pub digest: String,
    pub content_hash: String,
    pub url: String,
    pub packet_url: Option<String>,
}

pub fn export_source(
    document: &Document,
    project_dir: &Path,
) -> Result<SourceExport, PreviewError> {
    export_source_with_scene(document, project_dir).map(|(source, _)| source)
}

fn export_source_with_scene(
    document: &Document,
    project_dir: &Path,
) -> Result<
    (
        SourceExport,
        studio_core::large_scene::evaluate::EvaluatedScene,
    ),
    PreviewError,
> {
    let preview_dir = project_dir.join("cache/preview");
    std::fs::create_dir_all(&preview_dir).map_err(PreviewError::Io)?;
    let mut cache = GeometryDiskCache::new(project_dir)?;
    let evaluated = evaluate_large_scene_with_cache(document, &mut cache)?;
    let source = export_glb_from_scene(document, &evaluated)?;
    let digest = format!("{:x}", Sha256::digest(&source));
    let source_path = preview_dir.join(format!("{:08}-source.glb", document.revision.as_u64()));
    std::fs::write(&source_path, source).map_err(PreviewError::Io)?;
    Ok((
        SourceExport {
            revision: document.revision.as_u64(),
            source_digest: digest,
            source_path,
        },
        evaluated,
    ))
}

#[derive(Debug, Error)]
pub enum PreviewError {
    #[error("toolchain adapter must be a file inside the configured workspace")]
    InvalidToolchain,
    #[error("source GLB export failed")]
    Export(#[from] studio_core::export::glb::ExportError),
    #[error("preview I/O failed")]
    Io(#[source] std::io::Error),
    #[error("preview adapter exited with status {0}")]
    AdapterFailed(std::process::ExitStatus),
    #[error("preview adapter did not produce a valid GLB")]
    InvalidOutput,
    #[error("Toolchain sidecar failed: {0}")]
    Toolchain(#[from] studio_toolchain_client::ToolchainClientError),
    #[error("geometry cache failed: {0}")]
    GeometryCache(#[from] GeometryCacheError),
    #[error("derived scene evaluation failed: {0}")]
    LargeScene(#[from] LargeSceneError),
}

pub struct PreviewAdapter {
    toolchain: Arc<ToolchainService>,
    preview_packets_enabled: bool,
}

impl PreviewAdapter {
    pub fn new(workspace_root: &Path, toolchain_cli: PathBuf) -> Result<Self, PreviewError> {
        Ok(Self {
            toolchain: Arc::new(ToolchainService::new(workspace_root, Some(toolchain_cli))),
            preview_packets_enabled: false,
        })
    }

    pub fn from_service(toolchain: Arc<ToolchainService>) -> Self {
        Self {
            toolchain,
            preview_packets_enabled: false,
        }
    }

    pub fn with_preview_packets(mut self, enabled: bool) -> Self {
        self.preview_packets_enabled = enabled;
        self
    }

    pub fn generate(
        &self,
        document: &Document,
        project_dir: &Path,
        asset_id: &str,
    ) -> Result<RuntimePreview, PreviewError> {
        let preview_dir = project_dir.join("cache/toolchain/preview");
        std::fs::create_dir_all(&preview_dir).map_err(PreviewError::Io)?;
        let revision = document.revision.as_u64();
        let (source_export, evaluated) = export_source_with_scene(document, project_dir)?;
        if self.preview_packets_enabled {
            write_preview_packet(
                document,
                &evaluated,
                project_dir,
                &format!("{revision:08}.ice3pgp"),
            )?;
        }
        let source_path = source_export.source_path;
        let preview_path = preview_dir.join(format!("{revision:08}.glb"));
        let asset = self
            .toolchain
            .preview(&source_path, &preview_path, asset_id)?;
        Ok(RuntimePreview {
            revision,
            asset_id: asset.asset_id,
            source_digest: source_export.source_digest,
            content_hash: asset.content_hash,
            source_path,
            preview_path,
            packet_url: self
                .preview_packets_enabled
                .then(|| format!("/preview-packets/{revision}")),
        })
    }

    pub fn generate_candidate(
        &self,
        document: &Document,
        project_dir: &Path,
        digest: &str,
    ) -> Result<CandidatePreview, PreviewError> {
        let candidate_dir = project_dir.join("cache/candidates");
        let preview_dir = project_dir.join("cache/toolchain/candidates");
        std::fs::create_dir_all(&candidate_dir).map_err(PreviewError::Io)?;
        std::fs::create_dir_all(&preview_dir).map_err(PreviewError::Io)?;
        let preview_path = preview_dir.join(format!("{digest}.glb"));
        if valid_glb(&preview_path) {
            return Ok(CandidatePreview {
                asset_id: digest.into(),
                digest: digest.into(),
                content_hash: format!(
                    "{:x}",
                    Sha256::digest(std::fs::read(&preview_path).map_err(PreviewError::Io)?)
                ),
                url: format!("/previews/candidates/{digest}"),
                packet_url: self
                    .preview_packets_enabled
                    .then(|| format!("/preview-packets/candidates/{digest}")),
            });
        }
        let mut cache = GeometryDiskCache::new(project_dir)?;
        let evaluated = evaluate_large_scene_with_cache(document, &mut cache)?;
        if self.preview_packets_enabled {
            write_preview_packet(
                document,
                &evaluated,
                project_dir,
                &format!("{digest}.ice3pgp"),
            )?;
        }
        let source = export_glb_from_scene(document, &evaluated)?;
        let source_digest = format!("{:x}", Sha256::digest(&source));
        let source_path = candidate_dir.join(format!("{digest}-source.glb"));
        std::fs::write(&source_path, source).map_err(PreviewError::Io)?;
        let asset = self.toolchain.preview(
            &source_path,
            &preview_path,
            &format!("candidate-{source_digest}"),
        )?;
        Ok(CandidatePreview {
            asset_id: digest.into(),
            digest: digest.into(),
            content_hash: asset.content_hash,
            url: format!("/previews/candidates/{digest}"),
            packet_url: self
                .preview_packets_enabled
                .then(|| format!("/preview-packets/candidates/{digest}")),
        })
    }
}

fn write_preview_packet(
    document: &Document,
    evaluated: &studio_core::large_scene::evaluate::EvaluatedScene,
    project_dir: &Path,
    file_name: &str,
) -> Result<(), PreviewError> {
    let directory = project_dir.join("cache/preview-packets");
    std::fs::create_dir_all(&directory).map_err(PreviewError::Io)?;
    let packet = build_full_preview_packet(
        document,
        evaluated,
        studio_core::gpu_contract::GeometryBackendKind::Cpu,
        studio_core::gpu_contract::GeometryEvaluationMode::Cpu,
    );
    let bytes = encode_preview_packet(&packet);
    decode_preview_packet(&bytes).map_err(|_| PreviewError::InvalidOutput)?;
    std::fs::write(directory.join(file_name), bytes).map_err(PreviewError::Io)
}

fn valid_glb(path: &Path) -> bool {
    std::fs::read(path)
        .map(|bytes| bytes.len() >= 12 && &bytes[0..4] == b"glTF")
        .unwrap_or(false)
}
