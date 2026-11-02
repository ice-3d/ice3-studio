use std::path::{Path, PathBuf};
use std::sync::Arc;

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

#[derive(Clone)]
pub struct ScenePreviewContext {
    pub scene: Arc<EvaluatedScene>,
    pub patch: ScenePatch,
}

/// 判断 toolchain 的失败是否只意味着"没有可用的优化器"。
/// 未配置与版本不匹配时，Studio 用自己的产物即可；其余错误仍然暴露，
/// 因为它们往往说明输入本身有问题，静默降级会掩盖真实故障。
fn toolchain_is_optional(error: &studio_toolchain_client::ToolchainClientError) -> bool {
    use studio_toolchain_client::ToolchainClientError;
    matches!(
        error,
        ToolchainClientError::Unavailable { .. } | ToolchainClientError::Incompatible { .. }
    )
}

pub struct PreviewAdapter {
    toolchain: Arc<ToolchainService>,
    preview_packets_enabled: bool,
    compute: Option<Arc<ComputeService>>,
    scene_preview: Option<ScenePreviewContext>,
}

impl PreviewAdapter {
    pub fn new(workspace_root: &Path, toolchain_cli: PathBuf) -> Result<Self, PreviewError> {
        Ok(Self {
            toolchain: Arc::new(ToolchainService::new(workspace_root, Some(toolchain_cli))),
            preview_packets_enabled: false,
            compute: None,
            scene_preview: None,
        })
    }

    pub fn from_service(toolchain: Arc<ToolchainService>) -> Self {
        Self {
            toolchain,
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
        let preview_dir = project_dir.join("cache/toolchain/preview");
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
        // 内核本身就能产出 GLB；toolchain 只负责额外的压缩与分块优化。
        // 未配置或版本不匹配时直接使用内核产物，外部进程因此不再是必需依赖。
        let (resolved_asset_id, content_hash) =
            match self
                .toolchain
                .preview(&source_path, &preview_path, asset_id)
            {
                Ok(asset) => (asset.asset_id, asset.content_hash),
                Err(error) if toolchain_is_optional(&error) => {
                    std::fs::copy(&source_path, &preview_path).map_err(PreviewError::Io)?;
                    (asset_id.to_string(), source_digest.clone())
                }
                Err(error) => return Err(PreviewError::Toolchain(error)),
            };
        Ok(RuntimePreview {
            revision,
            asset_id: resolved_asset_id,
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
        let source_digest = format!("{:x}", Sha256::digest(&source));
        let source_path = candidate_dir.join(format!("{digest}-source.glb"));
        std::fs::write(&source_path, source).map_err(PreviewError::Io)?;
        // 与主预览一致：toolchain 只是可选优化器，缺失时使用内核产物。
        let content_hash = match self.toolchain.preview(
            &source_path,
            &preview_path,
            &format!("candidate-{source_digest}"),
        ) {
            Ok(asset) => asset.content_hash,
            Err(error) if toolchain_is_optional(&error) => {
                std::fs::copy(&source_path, &preview_path).map_err(PreviewError::Io)?;
                source_digest.clone()
            }
            Err(error) => return Err(PreviewError::Toolchain(error)),
        };
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
    std::fs::read(path)
        .map(|bytes| bytes.len() >= 12 && &bytes[0..4] == b"glTF")
        .unwrap_or(false)
}
