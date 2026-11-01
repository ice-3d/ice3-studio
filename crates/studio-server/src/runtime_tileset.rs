use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use studio_core::document::Document;
use studio_core::geometry::Bounds;
use studio_core::large_scene::evaluate::{EvaluatedScene, evaluated_scene_bounds};
use studio_pipeline::{AssetProfile, PartitionArtifact, build_with_profile};
use thiserror::Error;

use crate::preview::{PreviewError, export_source_with_scene};

#[derive(Debug, Error)]
pub enum RuntimeTilesetError {
    #[error(transparent)]
    Preview(#[from] PreviewError),
    #[error(transparent)]
    Asset(#[from] studio_assets::AssetError),
    #[error(transparent)]
    Pipeline(#[from] studio_pipeline::PipelineError),
    #[error("runtime output already exists")]
    OutputExists,
    #[error("runtime output contains an unsafe path: {0}")]
    InvalidPath(String),
    #[error("runtime output I/O failed")]
    Io(#[from] std::io::Error),
    #[error("runtime manifest serialization failed")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BindingManifest {
    pub kind: String,
    pub schema_version: String,
    pub asset_id: String,
    pub revision: u64,
    pub bindings: Vec<EntityBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EntityBinding {
    pub selection_id: String,
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub equipment_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area: Option<String>,
    pub node_selection_ids: Vec<String>,
    pub bounds: BindingBounds,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BindingBounds {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl BindingManifest {
    pub fn from_evaluated_scene(asset_id: &str, revision: u64, scene: &EvaluatedScene) -> Self {
        let node_bounds = evaluated_scene_bounds(scene);
        let mut groups =
            BTreeMap::<String, Vec<(&str, Option<String>, String, Option<String>, Bounds)>>::new();
        for node in &scene.nodes {
            if node.mesh_key.is_none() {
                continue;
            }
            let raw_tag = node
                .metadata
                .get("tag")
                .or_else(|| node.metadata.get("equipmentTag"))
                .filter(|value| !value.trim().is_empty())
                .cloned();
            let tag = raw_tag.clone();
            let selection_id = raw_tag
                .map(|tag| {
                    if tag.starts_with("plant/equipment/") {
                        tag
                    } else {
                        format!("plant/equipment/{tag}")
                    }
                })
                .unwrap_or_else(|| node.owner_id.clone());
            let display_name = node
                .metadata
                .get("displayName")
                .filter(|value| !value.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| node.name.clone());
            let area = node
                .metadata
                .get("train")
                .or_else(|| node.metadata.get("stage"))
                .filter(|value| !value.trim().is_empty())
                .cloned();
            let bounds = node_bounds
                .get(&node.id)
                .copied()
                .unwrap_or_else(zero_bounds);
            groups.entry(selection_id).or_default().push((
                node.id.as_str(),
                tag,
                display_name,
                area,
                bounds,
            ));
        }
        let bindings = groups
            .into_iter()
            .map(|(selection_id, mut nodes)| {
                nodes.sort_by(|left, right| left.0.cmp(right.0));
                let equipment_tag = nodes.iter().find_map(|node| node.1.clone());
                let display_name = nodes
                    .iter()
                    .map(|node| node.2.as_str())
                    .min()
                    .unwrap_or(&selection_id)
                    .to_owned();
                let area = nodes
                    .iter()
                    .filter_map(|node| node.3.as_deref())
                    .min()
                    .map(str::to_owned);
                let bounds = nodes
                    .iter()
                    .map(|node| node.4)
                    .reduce(union_bounds)
                    .unwrap_or_else(zero_bounds);
                EntityBinding {
                    selection_id,
                    display_name,
                    equipment_tag,
                    area,
                    node_selection_ids: nodes.iter().map(|node| node.0.to_owned()).collect(),
                    bounds: BindingBounds {
                        min: [bounds.min.x, bounds.min.y, bounds.min.z],
                        max: [bounds.max.x, bounds.max.y, bounds.max.z],
                    },
                }
            })
            .collect();
        Self {
            kind: "ice3-runtime-bindings".into(),
            schema_version: "1.0".into(),
            asset_id: asset_id.into(),
            revision,
            bindings,
        }
    }
}

fn union_bounds(left: Bounds, right: Bounds) -> Bounds {
    Bounds {
        min: studio_core::math::Vec3 {
            x: left.min.x.min(right.min.x),
            y: left.min.y.min(right.min.y),
            z: left.min.z.min(right.min.z),
        },
        max: studio_core::math::Vec3 {
            x: left.max.x.max(right.max.x),
            y: left.max.y.max(right.max.y),
            z: left.max.z.max(right.max.z),
        },
    }
}

fn zero_bounds() -> Bounds {
    Bounds {
        min: studio_core::math::Vec3::default(),
        max: studio_core::math::Vec3::default(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeTilesetExport {
    pub kind: String,
    pub schema_version: String,
    pub revision: u64,
    pub profile: String,
    pub source_digest: String,
    pub tileset_digest: String,
    pub bindings_digest: String,
    pub bundle_digest: String,
    pub output_dir: PathBuf,
    pub files: Vec<ExportedFile>,
    pub stats: ExportedStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportedFile {
    pub path: String,
    pub byte_length: usize,
    pub digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportedStats {
    pub triangle_count: u64,
    pub partition_count: usize,
    pub coarse_count: usize,
    pub binding_count: usize,
}

pub struct RuntimeTilesetExporter {
    output_dir: PathBuf,
    profile: AssetProfile,
    force: bool,
}

impl RuntimeTilesetExporter {
    pub fn new(output_dir: PathBuf, force: bool) -> Self {
        Self {
            output_dir,
            profile: AssetProfile::agent_console_v1(),
            force,
        }
    }

    pub fn with_profile(mut self, profile: AssetProfile) -> Self {
        self.profile = profile;
        self
    }

    pub fn export(
        &self,
        document: &Document,
        project_dir: &Path,
    ) -> Result<RuntimeTilesetExport, RuntimeTilesetError> {
        if self.output_dir.exists() && !self.force {
            return Err(RuntimeTilesetError::OutputExists);
        }
        let (source, scene) = export_source_with_scene(document, project_dir, None)?;
        let source_glb = studio_assets::read_glb(&source.source_path)?;
        let artifact = build_with_profile(&source_glb, &document.project_id, &self.profile)?;
        let bindings = BindingManifest::from_evaluated_scene(
            &document.project_id,
            document.revision.as_u64(),
            &scene,
        );
        self.publish(source, artifact, bindings)
    }

    fn publish(
        &self,
        source: crate::preview::SourceExport,
        artifact: PartitionArtifact,
        bindings: BindingManifest,
    ) -> Result<RuntimeTilesetExport, RuntimeTilesetError> {
        let parent = self
            .output_dir
            .parent()
            .ok_or_else(|| RuntimeTilesetError::InvalidPath("output has no parent".into()))?;
        std::fs::create_dir_all(parent)?;
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let base_name = self
            .output_dir
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| RuntimeTilesetError::InvalidPath("output name is invalid".into()))?;
        let staging = parent.join(format!(".{base_name}.staging-{nonce}"));
        let backup = parent.join(format!(".{base_name}.backup-{nonce}"));
        let cleanup_staging = StagingGuard(staging.clone());
        std::fs::create_dir_all(&staging)?;
        std::fs::create_dir_all(staging.join("coarse"))?;

        let tileset = artifact.tileset_json().into_bytes();
        let bindings = pretty_json(&bindings)?;
        let file_digest = bundle_digest(&artifact, &tileset, &bindings);
        let mut exported_files = artifact
            .files
            .iter()
            .map(|file| ExportedFile {
                path: file.relative_path.clone(),
                byte_length: file.byte_length,
                digest: file.content_hash.clone(),
            })
            .collect::<Vec<_>>();
        exported_files.push(ExportedFile {
            path: "tileset.json".into(),
            byte_length: tileset.len(),
            digest: format!("{:x}", Sha256::digest(&tileset)),
        });
        exported_files.push(ExportedFile {
            path: "bindings.json".into(),
            byte_length: bindings.len(),
            digest: format!("{:x}", Sha256::digest(&bindings)),
        });
        exported_files.sort_by(|left, right| left.path.cmp(&right.path));
        let export = RuntimeTilesetExport {
            kind: "ice3-runtime-asset".into(),
            schema_version: "1.0".into(),
            revision: source.revision,
            profile: self.profile.id.clone(),
            source_digest: source.source_digest,
            tileset_digest: format!("{:x}", Sha256::digest(&tileset)),
            bindings_digest: format!("{:x}", Sha256::digest(&bindings)),
            bundle_digest: file_digest,
            output_dir: self.output_dir.clone(),
            files: exported_files,
            stats: ExportedStats {
                triangle_count: artifact.stats.triangle_count,
                partition_count: artifact.stats.partition_count,
                coarse_count: artifact.stats.coarse_count,
                binding_count: bindings_count(&bindings)?,
            },
        };
        write_file(&staging, "tileset.json", &tileset)?;
        write_file(&staging, "bindings.json", &bindings)?;
        for file in &artifact.files {
            write_relative(&staging, &file.relative_path, &file.bytes)?;
        }
        write_file(&staging, "export.json", &pretty_json(&export)?)?;
        publish_directory(&staging, &self.output_dir, &backup, self.force)?;
        std::mem::forget(cleanup_staging);
        Ok(export)
    }
}

struct StagingGuard(PathBuf);

impl Drop for StagingGuard {
    fn drop(&mut self) {
        if self.0.exists() {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

fn publish_directory(
    staging: &Path,
    output: &Path,
    backup: &Path,
    force: bool,
) -> Result<(), RuntimeTilesetError> {
    if !output.exists() {
        return std::fs::rename(staging, output).map_err(Into::into);
    }
    if !force {
        return Err(RuntimeTilesetError::OutputExists);
    }
    std::fs::rename(output, backup)?;
    if let Err(error) = std::fs::rename(staging, output) {
        let _ = std::fs::rename(backup, output);
        return Err(error.into());
    }
    // The new bundle is already live. A backup cleanup failure must not turn a
    // successful atomic swap into a reported export failure.
    let _ = std::fs::remove_dir_all(backup);
    Ok(())
}

fn write_relative(root: &Path, relative: &str, bytes: &[u8]) -> Result<(), RuntimeTilesetError> {
    let relative = Path::new(relative);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(RuntimeTilesetError::InvalidPath(
            relative.display().to_string(),
        ));
    }
    write_file(root, relative.to_string_lossy().as_ref(), bytes)
}

fn write_file(root: &Path, relative: &str, bytes: &[u8]) -> Result<(), RuntimeTilesetError> {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, bytes)?;
    Ok(())
}

fn bundle_digest(artifact: &PartitionArtifact, tileset: &[u8], bindings: &[u8]) -> String {
    let mut digest = Sha256::new();
    let mut files = artifact
        .files
        .iter()
        .map(|file| (file.relative_path.as_str(), file.content_hash.as_str()))
        .collect::<Vec<_>>();
    files.sort_unstable();
    for (path, content) in files {
        digest.update(path.as_bytes());
        digest.update([0]);
        digest.update(content.as_bytes());
        digest.update([0]);
    }
    digest.update(b"tileset.json");
    digest.update([0]);
    digest.update(Sha256::digest(tileset));
    digest.update([0]);
    digest.update(b"bindings.json");
    digest.update([0]);
    digest.update(Sha256::digest(bindings));
    format!("{:x}", digest.finalize())
}

fn pretty_json<T: Serialize>(value: &T) -> Result<Vec<u8>, RuntimeTilesetError> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn bindings_count(bytes: &[u8]) -> Result<usize, RuntimeTilesetError> {
    let manifest: BindingManifest = serde_json::from_slice(bytes)?;
    Ok(manifest.bindings.len())
}
