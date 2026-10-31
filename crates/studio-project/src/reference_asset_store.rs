use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use studio_core::geometry::Bounds;
use studio_core::math::{Transform, Vec3};
use studio_core::reference_asset::{
    ReferenceAssetDescriptor, ReferenceAssetEquipment, ReferenceAssetStats,
};
use thiserror::Error;

const MAX_MANIFEST_BYTES: u64 = 32 * 1024 * 1024;
const MAX_MANIFEST_ENTRIES: usize = 100_000;
const MAX_GLB_JSON_BYTES: u32 = 64 * 1024 * 1024;
const GLB_MAGIC: &[u8; 4] = b"glTF";
const GLB_JSON_CHUNK: u32 = 0x4E4F_534A;
const GLB_BINARY_CHUNK: u32 = 0x004E_4942;

static TEMP_NONCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
pub struct ReferenceAssetSource {
    pub source_path: PathBuf,
    pub prepared_path: Option<PathBuf>,
    pub tileset_path: Option<PathBuf>,
    pub equipment_manifest_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReferenceAssetRegistration {
    pub asset_id: String,
    pub descriptor: ReferenceAssetDescriptor,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RegisteredReferenceAsset {
    pub descriptor: ReferenceAssetDescriptor,
    pub root: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReferenceBindingPage {
    pub items: Vec<ReferenceAssetEquipment>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Error)]
pub enum ReferenceAssetError {
    #[error("reference asset path escapes the configured workspace")]
    PathEscape,
    #[error("reference asset file is missing or unreadable")]
    Missing,
    #[error("reference asset digest does not match its declared content")]
    DigestMismatch,
    #[error("reference GLB is malformed or unsupported")]
    InvalidGlb,
    #[error("reference equipment manifest is malformed or unsupported")]
    InvalidManifest,
    #[error("reference asset does not exist")]
    NotFound,
    #[error("reference asset storage operation failed")]
    Io(#[source] std::io::Error),
}

pub struct ReferenceAssetStore {
    project_dir: PathBuf,
    workspace_root: PathBuf,
}

impl ReferenceAssetStore {
    pub fn new(project_dir: impl AsRef<Path>) -> Self {
        let project_dir = project_dir.as_ref().to_path_buf();
        let workspace_root = project_dir
            .parent()
            .map(|path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf()))
            .unwrap_or_else(|| project_dir.clone());
        Self {
            project_dir,
            workspace_root,
        }
    }

    pub fn register(
        &self,
        source: &ReferenceAssetSource,
    ) -> Result<ReferenceAssetRegistration, ReferenceAssetError> {
        let source_path = self.resolve_path(&source.source_path)?;
        let prepared_path = source
            .prepared_path
            .as_deref()
            .map(|path| self.resolve_path(path))
            .transpose()?;
        let tileset_path = source
            .tileset_path
            .as_deref()
            .map(|path| self.resolve_path(path))
            .transpose()?;
        let manifest_path = self.resolve_path(&source.equipment_manifest_path)?;

        let source_digest = hash_file(&source_path)?;
        let prepared_digest = prepared_path.as_deref().map(hash_file).transpose()?;
        let tileset_digest = tileset_path.as_deref().map(hash_directory).transpose()?;
        let manifest_digest = hash_file(&manifest_path)?;
        let source_glb = parse_glb(&source_path)?;
        let prepared_glb = prepared_path.as_deref().map(parse_glb).transpose()?;
        let glb = prepared_glb.as_ref().unwrap_or(&source_glb);
        let manifest = parse_manifest(&manifest_path)?;

        let references = self.project_dir.join("references");
        fs::create_dir_all(&references).map_err(ReferenceAssetError::Io)?;
        if let Some(existing) = self.find_identical(
            &references,
            &source_digest,
            prepared_digest.as_deref(),
            tileset_digest.as_deref(),
            &manifest_digest,
        )? {
            return Ok(existing);
        }

        let asset_id = next_asset_id(&references)?;
        let descriptor = ReferenceAssetDescriptor {
            asset_id: asset_id.clone(),
            source_digest,
            prepared_digest,
            tileset_digest,
            bounds: glb.bounds.ok_or(ReferenceAssetError::InvalidGlb)?,
            stats: ReferenceAssetStats {
                source_nodes: glb.source_nodes,
                effective_triangles: glb.effective_triangles,
                unique_meshes: glb.unique_meshes,
                materials: glb.materials,
                equipment: manifest.len(),
            },
            imported_at: timestamp_now(),
        };

        let temporary = references.join(format!(".tmp-{asset_id}-{}", nonce()));
        let final_root = references.join(&asset_id);
        let publish = (|| -> Result<(), ReferenceAssetError> {
            fs::create_dir(&temporary).map_err(ReferenceAssetError::Io)?;
            copy_and_verify(
                &source_path,
                &temporary.join("source.glb"),
                &descriptor.source_digest,
            )?;
            if let (Some(path), Some(expected)) = (
                prepared_path.as_deref(),
                descriptor.prepared_digest.as_deref(),
            ) {
                copy_and_verify(path, &temporary.join("prepared.glb"), expected)?;
            }
            if let Some(path) = tileset_path.as_deref() {
                let target = temporary.join("tiled");
                copy_directory(path, &target)?;
                if hash_directory(&target)? != descriptor.tileset_digest.clone().unwrap_or_default()
                {
                    return Err(ReferenceAssetError::DigestMismatch);
                }
            }
            copy_and_verify(
                &manifest_path,
                &temporary.join("equipment.json"),
                &manifest_digest,
            )?;
            let descriptor_bytes = serde_json::to_vec_pretty(&descriptor)
                .map_err(|_| ReferenceAssetError::InvalidGlb)?;
            fs::write(temporary.join("asset.json"), descriptor_bytes)
                .map_err(ReferenceAssetError::Io)?;
            if final_root.exists() {
                return Err(ReferenceAssetError::Io(std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    "reference asset target already exists",
                )));
            }
            mark_read_only(&temporary)?;
            fs::rename(&temporary, &final_root).map_err(ReferenceAssetError::Io)?;
            Ok(())
        })();
        if publish.is_err() {
            let _ = fs::remove_dir_all(&temporary);
        }
        publish?;
        Ok(ReferenceAssetRegistration {
            asset_id,
            descriptor,
        })
    }

    pub fn get(&self, asset_id: &str) -> Result<RegisteredReferenceAsset, ReferenceAssetError> {
        validate_asset_id(asset_id)?;
        let root = self.project_dir.join("references").join(asset_id);
        let bytes = fs::read(root.join("asset.json")).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                ReferenceAssetError::NotFound
            } else {
                ReferenceAssetError::Io(error)
            }
        })?;
        let descriptor =
            serde_json::from_slice(&bytes).map_err(|_| ReferenceAssetError::InvalidManifest)?;
        Ok(RegisteredReferenceAsset { descriptor, root })
    }

    pub fn bindings(
        &self,
        asset_id: &str,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<ReferenceBindingPage, ReferenceAssetError> {
        let registered = self.get(asset_id)?;
        let manifest = parse_manifest(&registered.root.join("equipment.json"))?;
        let start = cursor
            .and_then(|cursor| {
                manifest
                    .iter()
                    .position(|entry| entry.selection_id == cursor)
                    .map(|index| index + 1)
            })
            .unwrap_or(0);
        let end = start.saturating_add(limit).min(manifest.len());
        let items = manifest[start..end].to_vec();
        let next_cursor = (end < manifest.len())
            .then(|| items.last().map(|entry| entry.selection_id.clone()))
            .flatten();
        Ok(ReferenceBindingPage { items, next_cursor })
    }

    fn resolve_path(&self, path: &Path) -> Result<PathBuf, ReferenceAssetError> {
        if path.is_absolute()
            || path.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err(ReferenceAssetError::PathEscape);
        }
        let mut current = self.workspace_root.clone();
        for component in path.components() {
            let Component::Normal(part) = component else {
                continue;
            };
            current.push(part);
            let metadata = fs::symlink_metadata(&current).map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    ReferenceAssetError::Missing
                } else {
                    ReferenceAssetError::Io(error)
                }
            })?;
            if metadata.file_type().is_symlink() {
                return Err(ReferenceAssetError::PathEscape);
            }
        }
        let canonical = current
            .canonicalize()
            .map_err(|_| ReferenceAssetError::Missing)?;
        if !canonical.starts_with(&self.workspace_root) {
            return Err(ReferenceAssetError::PathEscape);
        }
        Ok(canonical)
    }

    fn find_identical(
        &self,
        references: &Path,
        source_digest: &str,
        prepared_digest: Option<&str>,
        tileset_digest: Option<&str>,
        manifest_digest: &str,
    ) -> Result<Option<ReferenceAssetRegistration>, ReferenceAssetError> {
        for asset_id in asset_ids(references)? {
            let root = references.join(&asset_id);
            let bytes = match fs::read(root.join("asset.json")) {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(ReferenceAssetError::Io(error)),
            };
            let Ok(descriptor) = serde_json::from_slice::<ReferenceAssetDescriptor>(&bytes) else {
                continue;
            };
            if descriptor.source_digest == source_digest
                && descriptor.prepared_digest.as_deref() == prepared_digest
                && descriptor.tileset_digest.as_deref() == tileset_digest
                && hash_file(&root.join("equipment.json"))
                    .is_ok_and(|digest| digest == manifest_digest)
            {
                return Ok(Some(ReferenceAssetRegistration {
                    asset_id,
                    descriptor,
                }));
            }
        }
        Ok(None)
    }
}

#[derive(Debug)]
struct GlbMetadata {
    source_nodes: usize,
    unique_meshes: usize,
    materials: usize,
    effective_triangles: u64,
    bounds: Option<Bounds>,
}

fn parse_glb(path: &Path) -> Result<GlbMetadata, ReferenceAssetError> {
    let file = File::open(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            ReferenceAssetError::Missing
        } else {
            ReferenceAssetError::Io(error)
        }
    })?;
    let mut reader = BufReader::new(file);
    let mut header = [0u8; 12];
    reader
        .read_exact(&mut header)
        .map_err(|_| ReferenceAssetError::InvalidGlb)?;
    if &header[..4] != GLB_MAGIC || u32::from_le_bytes(header[4..8].try_into().unwrap()) != 2 {
        return Err(ReferenceAssetError::InvalidGlb);
    }
    let length = u32::from_le_bytes(header[8..12].try_into().unwrap()) as u64;
    if length < 20 || length != fs::metadata(path).map_err(ReferenceAssetError::Io)?.len() {
        return Err(ReferenceAssetError::InvalidGlb);
    }

    let mut json = None;
    let mut binary_digest = None;
    let mut consumed = 12u64;
    while consumed < length {
        let mut chunk_header = [0u8; 8];
        reader
            .read_exact(&mut chunk_header)
            .map_err(|_| ReferenceAssetError::InvalidGlb)?;
        consumed += 8;
        let chunk_length = u32::from_le_bytes(chunk_header[..4].try_into().unwrap());
        let chunk_type = u32::from_le_bytes(chunk_header[4..].try_into().unwrap());
        if consumed + u64::from(chunk_length) > length {
            return Err(ReferenceAssetError::InvalidGlb);
        }
        if chunk_type == GLB_JSON_CHUNK {
            if json.is_some() || chunk_length > MAX_GLB_JSON_BYTES {
                return Err(ReferenceAssetError::InvalidGlb);
            }
            let mut bytes = vec![0u8; chunk_length as usize];
            reader
                .read_exact(&mut bytes)
                .map_err(|_| ReferenceAssetError::InvalidGlb)?;
            json = Some(
                serde_json::from_slice::<Value>(&bytes)
                    .map_err(|_| ReferenceAssetError::InvalidGlb)?,
            );
        } else if chunk_type == GLB_BINARY_CHUNK {
            let mut hasher = Sha256::new();
            let mut remaining = u64::from(chunk_length);
            let mut buffer = [0u8; 64 * 1024];
            while remaining > 0 {
                let count = usize::try_from(remaining.min(buffer.len() as u64))
                    .map_err(|_| ReferenceAssetError::InvalidGlb)?;
                reader
                    .read_exact(&mut buffer[..count])
                    .map_err(|_| ReferenceAssetError::InvalidGlb)?;
                hasher.update(&buffer[..count]);
                remaining -= count as u64;
            }
            binary_digest = Some(format!("{:x}", hasher.finalize()));
        } else {
            let remaining = u64::from(chunk_length);
            let mut sink = std::io::sink();
            std::io::copy(&mut (&mut reader).take(remaining), &mut sink)
                .map_err(ReferenceAssetError::Io)?;
        }
        consumed += u64::from(chunk_length);
    }

    let json = json.ok_or(ReferenceAssetError::InvalidGlb)?;
    let ice3 = json.pointer("/asset/extras/ice3");
    if let Some(expected) = ice3
        .and_then(|value| value.pointer("/contentDigest/value"))
        .and_then(Value::as_str)
        && expected.to_ascii_lowercase()
            != binary_digest.as_deref().unwrap_or_else(|| empty_sha256())
    {
        return Err(ReferenceAssetError::DigestMismatch);
    }

    let stats = ice3.and_then(|value| value.get("stats"));
    let source_nodes = stats
        .and_then(|value| value.get("nodeCount"))
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .or_else(|| array_len(&json, "nodes"))
        .ok_or(ReferenceAssetError::InvalidGlb)?;
    let unique_meshes = stats
        .and_then(|value| value.get("meshCount"))
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .or_else(|| array_len(&json, "meshes"))
        .ok_or(ReferenceAssetError::InvalidGlb)?;
    let materials = stats
        .and_then(|value| value.get("materialCount"))
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .or_else(|| array_len(&json, "materials"))
        .unwrap_or(0);
    let bounds = ice3
        .and_then(|value| value.get("bounds"))
        .map(parse_bounds)
        .transpose()?;
    let effective_triangles = effective_triangle_count(&json)?;
    Ok(GlbMetadata {
        source_nodes,
        unique_meshes,
        materials,
        effective_triangles,
        bounds,
    })
}

fn empty_sha256() -> &'static str {
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
}

fn array_len(value: &Value, field: &str) -> Option<usize> {
    value.get(field)?.as_array().map(Vec::len)
}

fn effective_triangle_count(json: &Value) -> Result<u64, ReferenceAssetError> {
    let accessors = json
        .get("accessors")
        .and_then(Value::as_array)
        .ok_or(ReferenceAssetError::InvalidGlb)?;
    let meshes = json
        .get("meshes")
        .and_then(Value::as_array)
        .ok_or(ReferenceAssetError::InvalidGlb)?;
    let nodes = json
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or(ReferenceAssetError::InvalidGlb)?;
    let mesh_triangles = meshes
        .iter()
        .map(|mesh| {
            mesh.get("primitives")
                .and_then(Value::as_array)
                .ok_or(ReferenceAssetError::InvalidGlb)?
                .iter()
                .try_fold(0u64, |total, primitive| {
                    if primitive.get("mode").and_then(Value::as_u64).unwrap_or(4) != 4 {
                        return Ok(total);
                    }
                    let accessor_index = primitive
                        .get("indices")
                        .or_else(|| primitive.pointer("/attributes/POSITION"))
                        .and_then(Value::as_u64)
                        .and_then(|index| usize::try_from(index).ok())
                        .ok_or(ReferenceAssetError::InvalidGlb)?;
                    let count = accessors
                        .get(accessor_index)
                        .and_then(|accessor| accessor.get("count"))
                        .and_then(Value::as_u64)
                        .ok_or(ReferenceAssetError::InvalidGlb)?;
                    Ok(total + count / 3)
                })
        })
        .collect::<Result<Vec<_>, ReferenceAssetError>>()?;
    nodes.iter().try_fold(0u64, |total, node| {
        let Some(mesh_index) = node.get("mesh").and_then(Value::as_u64) else {
            return Ok(total);
        };
        let mesh_index =
            usize::try_from(mesh_index).map_err(|_| ReferenceAssetError::InvalidGlb)?;
        let triangles = mesh_triangles
            .get(mesh_index)
            .ok_or(ReferenceAssetError::InvalidGlb)?;
        total
            .checked_add(*triangles)
            .ok_or(ReferenceAssetError::InvalidGlb)
    })
}

fn parse_bounds(value: &Value) -> Result<Bounds, ReferenceAssetError> {
    let min = value
        .get("min")
        .and_then(Value::as_array)
        .filter(|values| values.len() == 3)
        .ok_or(ReferenceAssetError::InvalidGlb)?;
    let max = value
        .get("max")
        .and_then(Value::as_array)
        .filter(|values| values.len() == 3)
        .ok_or(ReferenceAssetError::InvalidGlb)?;
    let vector = |values: &[Value]| -> Result<Vec3, ReferenceAssetError> {
        let coordinate = |index: usize| {
            values[index]
                .as_f64()
                .map(|value| value as f32)
                .filter(|value| value.is_finite())
                .ok_or(ReferenceAssetError::InvalidGlb)
        };
        Ok(Vec3 {
            x: coordinate(0)?,
            y: coordinate(1)?,
            z: coordinate(2)?,
        })
    };
    let bounds = Bounds {
        min: vector(min)?,
        max: vector(max)?,
    };
    if bounds.min.x > bounds.max.x || bounds.min.y > bounds.max.y || bounds.min.z > bounds.max.z {
        return Err(ReferenceAssetError::InvalidGlb);
    }
    Ok(bounds)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EquipmentManifest {
    asset_id: String,
    equipment: Vec<EquipmentManifestEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EquipmentManifestEntry {
    selection_id: String,
    name: String,
    tag: Option<String>,
    stage: Option<String>,
    area: Option<String>,
    bounds: ManifestBounds,
}

#[derive(Debug, Deserialize)]
struct ManifestBounds {
    min: [f32; 3],
    max: [f32; 3],
}

fn parse_manifest(path: &Path) -> Result<Vec<ReferenceAssetEquipment>, ReferenceAssetError> {
    let metadata = fs::metadata(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            ReferenceAssetError::Missing
        } else {
            ReferenceAssetError::Io(error)
        }
    })?;
    if metadata.len() > MAX_MANIFEST_BYTES {
        return Err(ReferenceAssetError::InvalidManifest);
    }
    let bytes = fs::read(path).map_err(ReferenceAssetError::Io)?;
    let manifest: EquipmentManifest =
        serde_json::from_slice(&bytes).map_err(|_| ReferenceAssetError::InvalidManifest)?;
    if manifest.asset_id.trim().is_empty()
        || manifest.equipment.is_empty()
        || manifest.equipment.len() > MAX_MANIFEST_ENTRIES
    {
        return Err(ReferenceAssetError::InvalidManifest);
    }
    let mut entries = manifest
        .equipment
        .into_iter()
        .map(|entry| {
            if entry.selection_id.trim().is_empty() || entry.name.trim().is_empty() {
                return Err(ReferenceAssetError::InvalidManifest);
            }
            let bounds = Bounds {
                min: Vec3 {
                    x: entry.bounds.min[0],
                    y: entry.bounds.min[1],
                    z: entry.bounds.min[2],
                },
                max: Vec3 {
                    x: entry.bounds.max[0],
                    y: entry.bounds.max[1],
                    z: entry.bounds.max[2],
                },
            };
            let valid = [
                bounds.min.x,
                bounds.min.y,
                bounds.min.z,
                bounds.max.x,
                bounds.max.y,
                bounds.max.z,
            ]
            .into_iter()
            .all(f32::is_finite)
                && bounds.min.x <= bounds.max.x
                && bounds.min.y <= bounds.max.y
                && bounds.min.z <= bounds.max.z;
            if !valid {
                return Err(ReferenceAssetError::InvalidManifest);
            }
            Ok(ReferenceAssetEquipment {
                selection_id: entry.selection_id,
                name: entry.name,
                kind: entry.tag,
                process_stage: entry.stage,
                area: entry.area,
                bounds,
                transform: Transform::IDENTITY,
                material_ids: Vec::new(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort_by(|left, right| left.selection_id.cmp(&right.selection_id));
    if entries
        .windows(2)
        .any(|pair| pair[0].selection_id == pair[1].selection_id)
    {
        return Err(ReferenceAssetError::InvalidManifest);
    }
    Ok(entries)
}

fn hash_file(path: &Path) -> Result<String, ReferenceAssetError> {
    let file = File::open(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            ReferenceAssetError::Missing
        } else {
            ReferenceAssetError::Io(error)
        }
    })?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = reader.read(&mut buffer).map_err(ReferenceAssetError::Io)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn hash_directory(path: &Path) -> Result<String, ReferenceAssetError> {
    let files = collect_directory_files(path)?;
    let mut hasher = Sha256::new();
    for (relative, absolute) in files {
        let relative = relative
            .to_str()
            .ok_or(ReferenceAssetError::InvalidManifest)?;
        hasher.update((relative.len() as u64).to_le_bytes());
        hasher.update(relative.as_bytes());
        let digest = hash_file(&absolute)?;
        hasher.update(digest.as_bytes());
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn collect_directory_files(path: &Path) -> Result<BTreeMap<PathBuf, PathBuf>, ReferenceAssetError> {
    if !path.is_dir() {
        return Err(ReferenceAssetError::InvalidManifest);
    }
    let mut files = BTreeMap::new();
    collect_directory_files_inner(path, path, &mut files)?;
    if files.is_empty() {
        return Err(ReferenceAssetError::InvalidManifest);
    }
    Ok(files)
}

fn collect_directory_files_inner(
    root: &Path,
    directory: &Path,
    files: &mut BTreeMap<PathBuf, PathBuf>,
) -> Result<(), ReferenceAssetError> {
    let mut entries = fs::read_dir(directory)
        .map_err(ReferenceAssetError::Io)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(ReferenceAssetError::Io)?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let file_type = entry.file_type().map_err(ReferenceAssetError::Io)?;
        if file_type.is_symlink() {
            return Err(ReferenceAssetError::PathEscape);
        }
        if file_type.is_dir() {
            collect_directory_files_inner(root, &entry.path(), files)?;
        } else if file_type.is_file() {
            let relative = entry
                .path()
                .strip_prefix(root)
                .map_err(|_| ReferenceAssetError::PathEscape)?
                .to_path_buf();
            files.insert(relative, entry.path());
        }
    }
    Ok(())
}

fn copy_directory(source: &Path, target: &Path) -> Result<(), ReferenceAssetError> {
    for (relative, absolute) in collect_directory_files(source)? {
        let destination = target.join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(ReferenceAssetError::Io)?;
        }
        fs::copy(absolute, destination).map_err(ReferenceAssetError::Io)?;
    }
    Ok(())
}

fn copy_and_verify(
    source: &Path,
    target: &Path,
    expected: &str,
) -> Result<(), ReferenceAssetError> {
    fs::copy(source, target).map_err(ReferenceAssetError::Io)?;
    if hash_file(target)? != expected {
        return Err(ReferenceAssetError::DigestMismatch);
    }
    Ok(())
}

fn validate_asset_id(asset_id: &str) -> Result<(), ReferenceAssetError> {
    let Some((prefix, ordinal)) = asset_id.rsplit_once('-') else {
        return Err(ReferenceAssetError::NotFound);
    };
    if prefix != "asset" || ordinal.len() != 6 || !ordinal.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(ReferenceAssetError::NotFound);
    }
    Ok(())
}

fn asset_ids(references: &Path) -> Result<Vec<String>, ReferenceAssetError> {
    if !references.exists() {
        return Ok(Vec::new());
    }
    let mut ids = fs::read_dir(references)
        .map_err(ReferenceAssetError::Io)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| validate_asset_id(name).is_ok())
        .collect::<Vec<_>>();
    ids.sort();
    Ok(ids)
}

fn next_asset_id(references: &Path) -> Result<String, ReferenceAssetError> {
    let next = asset_ids(references)?
        .iter()
        .filter_map(|asset_id| asset_id.rsplit_once('-')?.1.parse::<u64>().ok())
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or(ReferenceAssetError::InvalidManifest)?;
    Ok(format!("asset-{next:06}"))
}

fn nonce() -> u64 {
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or_default();
    time ^ (u64::from(std::process::id()) << 32) ^ TEMP_NONCE.fetch_add(1, Ordering::Relaxed)
}

fn timestamp_now() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default();
    let days = seconds.div_euclid(86_400);
    let seconds_of_day = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3_600;
    let minute = seconds_of_day % 3_600 / 60;
    let second = seconds_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month, day)
}

fn mark_read_only(root: &Path) -> Result<(), ReferenceAssetError> {
    for entry in fs::read_dir(root).map_err(ReferenceAssetError::Io)? {
        let path = entry.map_err(ReferenceAssetError::Io)?.path();
        if path.is_dir() {
            mark_read_only(&path)?;
        } else {
            let mut permissions = fs::metadata(&path)
                .map_err(ReferenceAssetError::Io)?
                .permissions();
            permissions.set_readonly(true);
            fs::set_permissions(path, permissions).map_err(ReferenceAssetError::Io)?;
        }
    }
    Ok(())
}
