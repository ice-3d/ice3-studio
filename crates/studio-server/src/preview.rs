use std::path::{Path, PathBuf};
use std::process::Stdio;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use studio_core::document::Document;
use studio_core::export::glb::export_glb;
use thiserror::Error;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RuntimePreview {
    pub revision: u64,
    pub source_digest: String,
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
}

pub struct PreviewAdapter {
    node_binary: PathBuf,
    toolchain_cli: PathBuf,
}

impl PreviewAdapter {
    pub fn new(workspace_root: &Path, toolchain_cli: PathBuf) -> Result<Self, PreviewError> {
        let root = workspace_root.canonicalize().map_err(PreviewError::Io)?;
        let toolchain_cli = toolchain_cli.canonicalize().map_err(PreviewError::Io)?;
        if !toolchain_cli.is_file() || !toolchain_cli.starts_with(&root) {
            return Err(PreviewError::InvalidToolchain);
        }
        Ok(Self {
            node_binary: std::env::var_os("STUDIO_NODE")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("node")),
            toolchain_cli,
        })
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
        let temporary = preview_dir.join(format!("{revision:08}.glb.tmp"));
        let status = std::process::Command::new(&self.node_binary)
            .arg(&self.toolchain_cli)
            .arg("build")
            .arg(&source_path)
            .arg("--output")
            .arg(&temporary)
            .arg("--asset-id")
            .arg(asset_id)
            .arg("--budget")
            .arg("unrestricted")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .status()
            .map_err(PreviewError::Io)?;
        if !status.success() {
            let _ = std::fs::remove_file(&temporary);
            return Err(PreviewError::AdapterFailed(status));
        }
        let bytes = match std::fs::read(&temporary) {
            Ok(bytes) => bytes,
            Err(error) => {
                let _ = std::fs::remove_file(&temporary);
                return Err(PreviewError::Io(error));
            }
        };
        if bytes.len() < 12 || &bytes[0..4] != b"glTF" {
            let _ = std::fs::remove_file(&temporary);
            return Err(PreviewError::InvalidOutput);
        }
        std::fs::rename(&temporary, &preview_path).map_err(PreviewError::Io)?;
        Ok(RuntimePreview {
            revision,
            source_digest: source_export.source_digest,
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
                url: format!("/previews/candidates/{digest}"),
            });
        }
        let source = export_glb(document)?;
        let source_digest = format!("{:x}", Sha256::digest(&source));
        let source_path = candidate_dir.join(format!("{digest}-source.glb"));
        std::fs::write(&source_path, source).map_err(PreviewError::Io)?;
        let temporary = preview_dir.join(format!("{digest}.glb.tmp"));
        let status = std::process::Command::new(&self.node_binary)
            .arg(&self.toolchain_cli)
            .arg("build")
            .arg(&source_path)
            .arg("--output")
            .arg(&temporary)
            .arg("--asset-id")
            .arg(format!("candidate-{source_digest}"))
            .arg("--budget")
            .arg("unrestricted")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .status()
            .map_err(PreviewError::Io)?;
        if !status.success() {
            let _ = std::fs::remove_file(&temporary);
            return Err(PreviewError::AdapterFailed(status));
        }
        if !valid_glb(&temporary) {
            let _ = std::fs::remove_file(&temporary);
            return Err(PreviewError::InvalidOutput);
        }
        std::fs::rename(&temporary, &preview_path).map_err(PreviewError::Io)?;
        Ok(CandidatePreview {
            asset_id: digest.into(),
            digest: digest.into(),
            url: format!("/previews/candidates/{digest}"),
        })
    }
}

fn valid_glb(path: &Path) -> bool {
    std::fs::read(path)
        .map(|bytes| bytes.len() >= 12 && &bytes[0..4] == b"glTF")
        .unwrap_or(false)
}
