use std::path::{Path, PathBuf};
use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use studio_core::document::Document;
use studio_core::export::glb::export_glb;
use thiserror::Error;
use ts_rs::TS;

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
}

pub fn export_source(
    document: &Document,
    project_dir: &Path,
) -> Result<SourceExport, PreviewError> {
    let preview_dir = project_dir.join("cache/preview");
    std::fs::create_dir_all(&preview_dir).map_err(PreviewError::Io)?;
    let source = export_glb(document)?;
    let digest = format!("{:x}", Sha256::digest(&source));
    let source_path = preview_dir.join(format!("{:08}-source.glb", document.revision.as_u64()));
    std::fs::write(&source_path, source).map_err(PreviewError::Io)?;
    Ok(SourceExport {
        revision: document.revision.as_u64(),
        source_digest: digest,
        source_path,
    })
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
}

pub struct PreviewAdapter {
    toolchain: Arc<ToolchainService>,
}

impl PreviewAdapter {
    pub fn new(workspace_root: &Path, toolchain_cli: PathBuf) -> Result<Self, PreviewError> {
        Ok(Self {
            toolchain: Arc::new(ToolchainService::new(workspace_root, Some(toolchain_cli))),
        })
    }

    pub fn from_service(toolchain: Arc<ToolchainService>) -> Self {
        Self { toolchain }
    }

    pub fn generate(
        &self,
        document: &Document,
        project_dir: &Path,
        asset_id: &str,
    ) -> Result<RuntimePreview, PreviewError> {
        let preview_dir = project_dir.join("cache/preview");
        std::fs::create_dir_all(&preview_dir).map_err(PreviewError::Io)?;
        let revision = document.revision.as_u64();
        let source_export = export_source(document, project_dir)?;
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
        })
    }

    pub fn generate_candidate(
        &self,
        document: &Document,
        project_dir: &Path,
        digest: &str,
    ) -> Result<CandidatePreview, PreviewError> {
        let candidate_dir = project_dir.join("cache/candidates");
        let preview_dir = project_dir.join("cache/preview");
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
            });
        }
        let source = export_glb(document)?;
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
        })
    }
}

fn valid_glb(path: &Path) -> bool {
    std::fs::read(path)
        .map(|bytes| bytes.len() >= 12 && &bytes[0..4] == b"glTF")
        .unwrap_or(false)
}
