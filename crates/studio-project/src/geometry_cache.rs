use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use studio_core::geometry::{Bounds, EvaluatedMesh, Vertex};
use studio_core::large_scene::cache::{GeometryCacheEntry, GeometryCacheMetadata, GeometryStore};
use studio_core::math::Vec3;
use thiserror::Error;

const MAGIC: &[u8; 8] = b"ICE3GEO2";
const HEADER_BYTES: u64 = 28;
const VERTEX_BYTES: u64 = 6 * 4;
const BOUNDS_BYTES: u64 = 6 * 4;
const DEFAULT_MAX_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum GeometryCacheError {
    #[error("geometry cache I/O failed")]
    Io(#[source] std::io::Error),
}

pub struct GeometryDiskCache {
    directory: PathBuf,
    max_bytes: u64,
}

impl GeometryDiskCache {
    pub fn new(project_dir: impl AsRef<Path>) -> Result<Self, GeometryCacheError> {
        Self::with_max_bytes(project_dir, DEFAULT_MAX_BYTES)
    }

    pub fn with_max_bytes(
        project_dir: impl AsRef<Path>,
        max_bytes: u64,
    ) -> Result<Self, GeometryCacheError> {
        let directory = project_dir.as_ref().join("cache/geometry");
        fs::create_dir_all(&directory).map_err(GeometryCacheError::Io)?;
        Ok(Self {
            directory,
            max_bytes,
        })
    }

    fn path_for(&self, key: &str) -> Option<PathBuf> {
        if key.is_empty()
            || key.len() > 160
            || !key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return None;
        }
        Some(self.directory.join(format!("{key}.mesh")))
    }
}

impl GeometryStore for GeometryDiskCache {
    fn load(&self, key: &str) -> Option<EvaluatedMesh> {
        self.load_entry(key).map(|entry| entry.mesh)
    }

    fn load_entry(&self, key: &str) -> Option<GeometryCacheEntry> {
        let path = self.path_for(key)?;
        let metadata = fs::metadata(&path).ok()?;
        if metadata.len() < HEADER_BYTES + BOUNDS_BYTES || metadata.len() > self.max_bytes {
            let _ = fs::remove_file(path);
            return None;
        }
        let mut bytes = Vec::new();
        File::open(&path)
            .ok()?
            .take(self.max_bytes + 1)
            .read_to_end(&mut bytes)
            .ok()?;
        match decode(&bytes) {
            Some(entry) => Some(entry),
            None => {
                let _ = fs::remove_file(path);
                None
            }
        }
    }

    fn store(&mut self, key: String, mesh: EvaluatedMesh) {
        self.store_entry(
            key,
            GeometryCacheEntry {
                mesh,
                metadata: GeometryCacheMetadata::cpu(),
                resident_geometry_id: None,
            },
        );
    }

    fn store_entry(&mut self, key: String, entry: GeometryCacheEntry) {
        let Some(path) = self.path_for(&key) else {
            return;
        };
        let Some(bytes) = encode(&entry) else {
            return;
        };
        if bytes.len() as u64 > self.max_bytes {
            return;
        }
        let temporary = path.with_extension("mesh.tmp");
        let result = (|| -> std::io::Result<()> {
            let mut file = File::create(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::rename(&temporary, &path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(temporary);
        }
    }
}

fn encode(entry: &GeometryCacheEntry) -> Option<Vec<u8>> {
    let mesh = &entry.mesh;
    let vertex_count = u64::try_from(mesh.vertices.len()).ok()?;
    let index_count = u64::try_from(mesh.indices.len()).ok()?;
    let metadata = serde_json::to_vec(&entry.metadata).ok()?;
    let metadata_length = u32::try_from(metadata.len()).ok()?;
    let byte_length = HEADER_BYTES
        .checked_add(vertex_count.checked_mul(VERTEX_BYTES)?)?
        .checked_add(index_count.checked_mul(4)?)?
        .checked_add(BOUNDS_BYTES)?
        .checked_add(metadata.len() as u64)?;
    let mut output = Vec::with_capacity(usize::try_from(byte_length).ok()?);
    output.extend_from_slice(MAGIC);
    output.extend_from_slice(&vertex_count.to_le_bytes());
    output.extend_from_slice(&index_count.to_le_bytes());
    output.extend_from_slice(&metadata_length.to_le_bytes());
    output.extend_from_slice(&metadata);
    for vertex in &mesh.vertices {
        for value in vertex.position.into_iter().chain(vertex.normal) {
            output.extend_from_slice(&value.to_le_bytes());
        }
    }
    for index in &mesh.indices {
        output.extend_from_slice(&index.to_le_bytes());
    }
    for value in [
        mesh.bounds.min.x,
        mesh.bounds.min.y,
        mesh.bounds.min.z,
        mesh.bounds.max.x,
        mesh.bounds.max.y,
        mesh.bounds.max.z,
    ] {
        output.extend_from_slice(&value.to_le_bytes());
    }
    Some(output)
}

fn decode(bytes: &[u8]) -> Option<GeometryCacheEntry> {
    if bytes.len() < HEADER_BYTES as usize || &bytes[0..8] != MAGIC {
        return None;
    }
    let vertex_count = usize::try_from(u64::from_le_bytes(bytes[8..16].try_into().ok()?)).ok()?;
    let index_count = usize::try_from(u64::from_le_bytes(bytes[16..24].try_into().ok()?)).ok()?;
    let metadata_length =
        usize::try_from(u32::from_le_bytes(bytes[24..28].try_into().ok()?)).ok()?;
    if metadata_length > 4096 {
        return None;
    }
    if vertex_count == 0 || index_count == 0 || !index_count.is_multiple_of(3) {
        return None;
    }
    let metadata_start = HEADER_BYTES as usize;
    let metadata_end = metadata_start.checked_add(metadata_length)?;
    let metadata: GeometryCacheMetadata =
        serde_json::from_slice(bytes.get(metadata_start..metadata_end)?).ok()?;
    let expected = metadata_end
        .checked_add(vertex_count.checked_mul(VERTEX_BYTES as usize)?)?
        .checked_add(index_count.checked_mul(4)?)?
        .checked_add(BOUNDS_BYTES as usize)?;
    if expected != bytes.len() {
        return None;
    }
    let mut offset = metadata_end;
    let mut vertices = Vec::with_capacity(vertex_count);
    for _ in 0..vertex_count {
        let mut values = [0.0_f32; 6];
        for value in &mut values {
            *value = f32::from_le_bytes(bytes[offset..offset + 4].try_into().ok()?);
            offset += 4;
        }
        if !values.iter().all(|value| value.is_finite()) {
            return None;
        }
        vertices.push(Vertex {
            position: [values[0], values[1], values[2]],
            normal: [values[3], values[4], values[5]],
        });
    }
    let mut indices = Vec::with_capacity(index_count);
    for _ in 0..index_count {
        let index = u32::from_le_bytes(bytes[offset..offset + 4].try_into().ok()?);
        if index as usize >= vertex_count {
            return None;
        }
        indices.push(index);
        offset += 4;
    }
    let mut bounds = [0.0_f32; 6];
    for value in &mut bounds {
        *value = f32::from_le_bytes(bytes[offset..offset + 4].try_into().ok()?);
        offset += 4;
    }
    if !bounds.iter().all(|value| value.is_finite())
        || bounds[0] > bounds[3]
        || bounds[1] > bounds[4]
        || bounds[2] > bounds[5]
    {
        return None;
    }
    Some(GeometryCacheEntry {
        mesh: EvaluatedMesh {
            vertices,
            indices,
            bounds: Bounds {
                min: Vec3 {
                    x: bounds[0],
                    y: bounds[1],
                    z: bounds[2],
                },
                max: Vec3 {
                    x: bounds[3],
                    y: bounds[4],
                    z: bounds[5],
                },
            },
        },
        metadata,
        resident_geometry_id: None,
    })
}
