use std::path::{Path, PathBuf};
use std::sync::Arc;
#[cfg(debug_assertions)]
use std::sync::atomic::{AtomicU64, Ordering};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use studio_core::document::Document;
use studio_core::export::glb::export_glb_from_scene;
use studio_core::gpu_contract::{GeometryBackendKind, GeometryEvaluationMode};
use studio_core::large_scene::cache::GeometryEvaluator;
use studio_core::large_scene::evaluate::{
    EvaluatedScene, LargeSceneError, evaluate_large_scene_with_cache_and_evaluator,
};
use studio_core::scene_contract::{SceneGeometrySnapshotRequest, ScenePatch};
use studio_project::geometry_cache::{GeometryCacheError, GeometryDiskCache};
use thiserror::Error;
use ts_rs::TS;

use crate::compute::{ComputeEvaluator, ComputeService};
use crate::preview_packet::{
    PreviewGeometryNode, build_full_preview_packet, build_scene_preview_delta,
    decode_preview_packet, encode_preview_packet,
};

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
    /// 该预览所属的资产 ID。
    pub asset_id: String,
    /// 候选变更集的摘要，与审批时使用的 digest 一致。
    pub digest: String,
    /// 预览文件字节的 SHA-256，用于校验内容未被替换。
    pub content_hash: String,
    /// 候选 GLB 的读取地址。
    pub url: String,
    /// 增量 preview packet 地址；仅在 host 显式开启时非 null。
    pub packet_url: Option<String>,
}

pub fn export_source(
    document: &Document,
    project_dir: &Path,
) -> Result<SourceExport, PreviewError> {
    export_source_with_scene(document, project_dir, None).map(|(source, _)| source)
}

pub(crate) fn export_source_with_scene(
    document: &Document,
    project_dir: &Path,
    evaluator: Option<&dyn GeometryEvaluator>,
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
    let evaluated = evaluate_large_scene_with_cache_and_evaluator(document, &mut cache, evaluator)?;
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
    #[error("source GLB export failed")]
    Export(#[from] studio_core::export::glb::ExportError),
    #[error("preview I/O failed")]
    Io(#[source] std::io::Error),
    #[error("preview adapter did not produce a valid GLB")]
    InvalidOutput,
    #[error("geometry cache failed: {0}")]
    GeometryCache(#[from] GeometryCacheError),
    #[error("derived scene evaluation failed: {0}")]
    LargeScene(#[from] LargeSceneError),
}

#[derive(Clone)]
pub struct ScenePreviewContext {
    pub scene: Arc<EvaluatedScene>,
    pub patch: ScenePatch,
}

pub struct PreviewAdapter {
    preview_packets_enabled: bool,
    compute: Option<Arc<ComputeService>>,
    scene_preview: Option<ScenePreviewContext>,
}

impl PreviewAdapter {
    pub fn new() -> Self {
        Self {
            preview_packets_enabled: false,
            compute: None,
            scene_preview: None,
        }
    }

    pub fn with_preview_packets(mut self, enabled: bool) -> Self {
        self.preview_packets_enabled = enabled;
        self
    }

    pub fn with_compute(mut self, compute: Arc<ComputeService>) -> Self {
        self.compute = Some(compute);
        self
    }

    pub fn with_scene_preview(mut self, context: ScenePreviewContext) -> Self {
        self.scene_preview = Some(context);
        self
    }

    pub fn generate(
        &self,
        document: &Document,
        project_dir: &Path,
        asset_id: &str,
    ) -> Result<RuntimePreview, PreviewError> {
        apply_preview_test_delay();
        let preview_dir = project_dir.join("cache/preview");
        std::fs::create_dir_all(&preview_dir).map_err(PreviewError::Io)?;
        let revision = document.revision.as_u64();
        let evaluator = self.compute.as_deref().map(ComputeEvaluator::new);
        let (source_export, evaluated) = if let Some(context) = &self.scene_preview {
            (
                export_source_from_scene(document, project_dir, &context.scene)?,
                context.scene.as_ref().clone(),
            )
        } else {
            export_source_with_scene(
                document,
                project_dir,
                evaluator
                    .as_ref()
                    .map(|value| value as &dyn GeometryEvaluator),
            )?
        };
        let (backend, mode) = self.packet_evidence();
        if self.preview_packets_enabled {
            self.write_runtime_packet(document, &evaluated, project_dir, revision, backend, mode)?;
        }
        let source_path = source_export.source_path;
        let source_digest = source_export.source_digest;
        let preview_path = preview_dir.join(format!("{revision:08}.glb"));
        publish_preview(&source_path, &preview_path)?;
        let content_hash = preview_content_hash(&preview_path)?;
        Ok(RuntimePreview {
            revision,
            asset_id: asset_id.to_string(),
            source_digest,
            content_hash,
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
        apply_preview_test_delay();
        let candidate_dir = project_dir.join("cache/candidates");
        let preview_dir = project_dir.join("cache/preview/candidates");
        std::fs::create_dir_all(&candidate_dir).map_err(PreviewError::Io)?;
        std::fs::create_dir_all(&preview_dir).map_err(PreviewError::Io)?;
        let preview_path = preview_dir.join(format!("{digest}.glb"));
        if valid_glb(&preview_path) {
            return self.cached_candidate_preview(digest, &preview_path);
        }
        let mut cache = GeometryDiskCache::new(project_dir)?;
        let evaluator = self.compute.as_deref().map(ComputeEvaluator::new);
        let evaluated = evaluate_large_scene_with_cache_and_evaluator(
            document,
            &mut cache,
            evaluator
                .as_ref()
                .map(|value| value as &dyn GeometryEvaluator),
        )?;
        let (backend, mode) = self.packet_evidence();
        if self.preview_packets_enabled {
            write_preview_packet(
                document,
                &evaluated,
                project_dir,
                &format!("{digest}.ice3pgp"),
                backend,
                mode,
            )?;
        }
        let source = export_glb_from_scene(document, &evaluated)?;
        let source_path = candidate_dir.join(format!("{digest}-source.glb"));
        std::fs::write(&source_path, source).map_err(PreviewError::Io)?;
        publish_preview(&source_path, &preview_path)?;
        let content_hash = preview_content_hash(&preview_path)?;
        Ok(CandidatePreview {
            asset_id: digest.into(),
            digest: digest.into(),
            content_hash,
            url: format!("/previews/candidates/{digest}"),
            packet_url: self
                .preview_packets_enabled
                .then(|| format!("/preview-packets/candidates/{digest}")),
        })
    }

    fn packet_evidence(&self) -> (GeometryBackendKind, GeometryEvaluationMode) {
        let Some(compute) = &self.compute else {
            return (GeometryBackendKind::Cpu, GeometryEvaluationMode::Cpu);
        };
        let status = compute.status();
        let mode = match status.last_evaluation_mode.as_str() {
            "cold" => GeometryEvaluationMode::Cold,
            "warm" => GeometryEvaluationMode::Warm,
            "resident" => GeometryEvaluationMode::Resident,
            "snapshot" => GeometryEvaluationMode::Snapshot,
            _ => GeometryEvaluationMode::Cpu,
        };
        (status.selected_backend, mode)
    }

    fn cached_candidate_preview(
        &self,
        digest: &str,
        path: &Path,
    ) -> Result<CandidatePreview, PreviewError> {
        Ok(CandidatePreview {
            asset_id: digest.into(),
            digest: digest.into(),
            content_hash: preview_content_hash(path)?,
            url: format!("/previews/candidates/{digest}"),
            packet_url: self
                .preview_packets_enabled
                .then(|| format!("/preview-packets/candidates/{digest}")),
        })
    }

    fn write_runtime_packet(
        &self,
        document: &Document,
        evaluated: &EvaluatedScene,
        project_dir: &Path,
        revision: u64,
        backend: GeometryBackendKind,
        mode: GeometryEvaluationMode,
    ) -> Result<(), PreviewError> {
        let Some(context) = &self.scene_preview else {
            return write_preview_packet(
                document,
                evaluated,
                project_dir,
                &format!("{revision:08}.ice3pgp"),
                backend,
                mode,
            );
        };
        let directory = project_dir.join("cache/preview-packets");
        std::fs::create_dir_all(&directory).map_err(PreviewError::Io)?;
        let base_path = directory.join(format!("{:08}.ice3pgp", revision.saturating_sub(1)));
        let base = match std::fs::read(base_path) {
            Ok(bytes) => decode_preview_packet(&bytes).ok(),
            Err(_) => None,
        };
        let Some(base) = base else {
            return write_preview_packet(
                document,
                evaluated,
                project_dir,
                &format!("{revision:08}.ice3pgp"),
                backend,
                mode,
            );
        };
        let nodes = context
            .patch
            .instances
            .iter()
            .map(|instance| {
                let source = evaluated
                    .nodes
                    .iter()
                    .find(|node| node.id == instance.node_id);
                PreviewGeometryNode {
                    node_id: instance.node_id.clone(),
                    parent_id: source.and_then(|node| node.parent_id.clone()),
                    name: source
                        .map(|node| node.name.clone())
                        .unwrap_or_else(|| instance.node_id.clone()),
                    matrix: instance.matrix,
                    material_id: instance.material_id.clone(),
                    geometry_id: instance.geometry_id.clone(),
                    visible: instance.visible,
                }
            })
            .collect();
        let mut readbacks = Vec::new();
        if let Some(compute) = &self.compute {
            let geometry_ids = context
                .patch
                .geometry
                .iter()
                .filter(|change| {
                    change
                        .dirty
                        .contains(studio_core::gpu_contract::GeometryDirtyChannel::Positions)
                        || change
                            .dirty
                            .contains(studio_core::gpu_contract::GeometryDirtyChannel::Topology)
                })
                .map(|change| change.geometry_id.clone())
                .collect::<Vec<_>>();
            if !geometry_ids.is_empty() {
                let loaded = pollster::block_on(compute.snapshot_scene_geometries(
                    SceneGeometrySnapshotRequest {
                        target_revision: revision,
                        geometry_ids,
                    },
                ));
                let Ok(mut loaded) = loaded else {
                    return write_preview_packet(
                        document,
                        evaluated,
                        project_dir,
                        &format!("{revision:08}.ice3pgp"),
                        backend,
                        mode,
                    );
                };
                for readback in &mut loaded {
                    readback.topology_changed = context.patch.geometry.iter().any(|change| {
                        change.geometry_id == readback.geometry_id
                            && change
                                .dirty
                                .contains(studio_core::gpu_contract::GeometryDirtyChannel::Topology)
                    });
                }
                readbacks = loaded;
            }
        }
        let packet = build_scene_preview_delta(
            &base,
            document,
            nodes,
            readbacks,
            context.patch.removed_node_ids.clone(),
            context.patch.removed_geometry_ids.clone(),
        );
        let bytes = encode_preview_packet(&packet);
        decode_preview_packet(&bytes).map_err(|_| PreviewError::InvalidOutput)?;
        std::fs::write(directory.join(format!("{revision:08}.ice3pgp")), bytes)
            .map_err(PreviewError::Io)
    }
}

impl Default for PreviewAdapter {
    fn default() -> Self {
        Self::new()
    }
}

fn export_source_from_scene(
    document: &Document,
    project_dir: &Path,
    evaluated: &EvaluatedScene,
) -> Result<SourceExport, PreviewError> {
    let preview_dir = project_dir.join("cache/preview");
    std::fs::create_dir_all(&preview_dir).map_err(PreviewError::Io)?;
    let source = export_glb_from_scene(document, evaluated)?;
    let source_digest = format!("{:x}", Sha256::digest(&source));
    let source_path = preview_dir.join(format!("{:08}-source.glb", document.revision.as_u64()));
    std::fs::write(&source_path, source).map_err(PreviewError::Io)?;
    Ok(SourceExport {
        revision: document.revision.as_u64(),
        source_digest,
        source_path,
    })
}

fn write_preview_packet(
    document: &Document,
    evaluated: &studio_core::large_scene::evaluate::EvaluatedScene,
    project_dir: &Path,
    file_name: &str,
    backend: GeometryBackendKind,
    mode: GeometryEvaluationMode,
) -> Result<(), PreviewError> {
    let directory = project_dir.join("cache/preview-packets");
    std::fs::create_dir_all(&directory).map_err(PreviewError::Io)?;
    let packet = build_full_preview_packet(document, evaluated, backend, mode);
    let bytes = encode_preview_packet(&packet);
    decode_preview_packet(&bytes).map_err(|_| PreviewError::InvalidOutput)?;
    std::fs::write(directory.join(file_name), bytes).map_err(PreviewError::Io)
}

fn valid_glb(path: &Path) -> bool {
    std::fs::read(path).is_ok_and(|bytes| {
        bytes.len() >= 12
            && &bytes[0..4] == b"glTF"
            && u32::from_le_bytes(bytes[4..8].try_into().expect("four bytes")) == 2
            && u32::from_le_bytes(bytes[8..12].try_into().expect("four bytes")) as usize
                == bytes.len()
    })
}

fn preview_content_hash(path: &Path) -> Result<String, PreviewError> {
    Ok(format!(
        "{:x}",
        Sha256::digest(std::fs::read(path).map_err(PreviewError::Io)?)
    ))
}

fn publish_preview(source: &Path, destination: &Path) -> Result<(), PreviewError> {
    let parent = destination.parent().ok_or_else(|| {
        PreviewError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "preview destination has no parent directory",
        ))
    })?;
    std::fs::create_dir_all(parent).map_err(PreviewError::Io)?;
    let mut input = std::fs::File::open(source).map_err(PreviewError::Io)?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".preview-")
        .suffix(".tmp")
        .tempfile_in(parent)
        .map_err(PreviewError::Io)?;
    std::io::copy(&mut input, temporary.as_file_mut()).map_err(PreviewError::Io)?;
    temporary.as_file().sync_all().map_err(PreviewError::Io)?;
    temporary
        .persist(destination)
        .map_err(|error| PreviewError::Io(error.error))?;
    Ok(())
}

#[cfg(debug_assertions)]
static PREVIEW_TEST_DELAY_MS: AtomicU64 = AtomicU64::new(0);

#[cfg(debug_assertions)]
#[doc(hidden)]
pub struct PreviewTestDelayGuard {
    previous: u64,
}

#[cfg(debug_assertions)]
impl Drop for PreviewTestDelayGuard {
    fn drop(&mut self) {
        PREVIEW_TEST_DELAY_MS.store(self.previous, Ordering::Release);
    }
}

/// Deterministically holds preview evaluation for integration tests without
/// mutating process-global environment variables.
#[cfg(debug_assertions)]
#[doc(hidden)]
pub fn hold_preview_for_tests(duration: std::time::Duration) -> PreviewTestDelayGuard {
    let milliseconds = u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);
    let previous = PREVIEW_TEST_DELAY_MS.swap(milliseconds, Ordering::AcqRel);
    PreviewTestDelayGuard { previous }
}

#[cfg(debug_assertions)]
fn apply_preview_test_delay() {
    let milliseconds = PREVIEW_TEST_DELAY_MS.load(Ordering::Acquire);
    if milliseconds > 0 {
        std::thread::sleep(std::time::Duration::from_millis(milliseconds));
    }
}

#[cfg(not(debug_assertions))]
fn apply_preview_test_delay() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_publication_preserves_the_previous_public_bytes() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("preview.glb");
        std::fs::write(&destination, b"previous-valid-preview").unwrap();
        let missing_source = root.path().join("missing-source.glb");

        let error = publish_preview(&missing_source, &destination).unwrap_err();

        assert!(matches!(error, PreviewError::Io(_)));
        assert_eq!(
            std::fs::read(&destination).unwrap(),
            b"previous-valid-preview"
        );
    }

    #[test]
    fn publication_replaces_the_destination_only_after_the_copy_completes() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source.glb");
        let destination = root.path().join("preview.glb");
        std::fs::write(&source, b"new-preview").unwrap();
        std::fs::write(&destination, b"old-preview").unwrap();

        publish_preview(&source, &destination).unwrap();

        assert_eq!(std::fs::read(&destination).unwrap(), b"new-preview");
        assert_eq!(std::fs::read(&source).unwrap(), b"new-preview");
    }
}
