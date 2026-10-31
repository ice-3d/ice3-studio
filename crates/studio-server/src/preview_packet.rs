use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use studio_core::document::Document;
use studio_core::gpu_contract::{GeometryBackendKind, GeometryEvaluationMode, ResidentGeometryId};
use studio_core::large_scene::evaluate::EvaluatedScene;
use studio_core::math::Transform;
use thiserror::Error;
use ts_rs::TS;

pub const PACKET_MEDIA_TYPE: &str = "application/vnd.ice3.preview-geometry; version=1";
pub const PACKET_MAGIC: &[u8; 8] = b"ICE3PGP1";
pub const PACKET_SCHEMA_VERSION: u16 = 1;
pub const PACKET_TRANSPORT_VERSION: u16 = 1;
pub const DEFAULT_MAX_PACKET_BYTES: usize = 256 * 1024 * 1024;

const HEADER_LENGTH: usize = 120;
const MANIFEST_LENGTH_OFFSET: usize = 40;
const BINARY_LENGTH_OFFSET: usize = 48;
const MANIFEST_DIGEST_OFFSET: usize = 56;
const CONTENT_DIGEST_OFFSET: usize = 88;
const MANIFEST_ALIGNMENT: usize = 8;
const MAX_NODES: usize = 1_000_000;
const MAX_GEOMETRIES: usize = 250_000;
const MAX_RANGES_PER_GEOMETRY: usize = 64;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PreviewGeometryPacket {
    pub manifest: PreviewGeometryManifest,
    #[serde(skip)]
    #[ts(type = "Uint8Array")]
    pub binary: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PreviewGeometryManifest {
    pub schema_version: u16,
    pub transport_version: u16,
    pub revision: u64,
    pub parent_revision: Option<u64>,
    pub source_digest: String,
    pub backend: GeometryBackendKind,
    pub residency_mode: GeometryEvaluationMode,
    pub nodes: Vec<PreviewGeometryNode>,
    pub geometries: Vec<PreviewGeometryRecord>,
    pub materials: Vec<PreviewGeometryMaterial>,
    pub removed_node_ids: Vec<String>,
    pub removed_geometry_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PreviewGeometryNode {
    pub node_id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub matrix: [f32; 16],
    pub material_id: Option<String>,
    pub geometry_id: Option<String>,
    pub visible: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PreviewGeometryRecord {
    pub geometry_id: String,
    pub resident_geometry_id: Option<ResidentGeometryId>,
    pub topology_digest: String,
    pub attribute_digest: String,
    pub vertex_count: u32,
    pub index_count: u32,
    pub index_width: u8,
    pub ranges: Vec<PreviewGeometryRange>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PreviewGeometryRange {
    pub byte_offset: u64,
    pub byte_length: u64,
    pub usage: PreviewGeometryRangeUsage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PreviewGeometryRangeUsage {
    VertexAttributes,
    Indices,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PreviewGeometryMaterial {
    pub material_id: String,
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub opacity: f32,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PreviewPacketError {
    #[error("preview packet is too large")]
    TooLarge,
    #[error("preview packet header is invalid")]
    InvalidHeader,
    #[error("preview packet manifest is invalid")]
    InvalidManifest,
    #[error("preview packet digest does not match its payload")]
    DigestMismatch,
}

pub fn encode_preview_packet(packet: &PreviewGeometryPacket) -> Vec<u8> {
    let Ok(mut manifest) = serde_json::to_vec(&packet.manifest) else {
        return Vec::new();
    };
    while manifest.len() % MANIFEST_ALIGNMENT != 0 {
        manifest.push(b' ');
    }
    let Some(total_length) = HEADER_LENGTH
        .checked_add(manifest.len())
        .and_then(|value| value.checked_add(packet.binary.len()))
    else {
        return Vec::new();
    };
    if total_length > DEFAULT_MAX_PACKET_BYTES {
        return Vec::new();
    }

    let manifest_digest = Sha256::digest(&manifest);
    let mut content_hasher = Sha256::new();
    content_hasher.update(&manifest);
    content_hasher.update(&packet.binary);
    let content_digest = content_hasher.finalize();

    let mut bytes = Vec::with_capacity(total_length);
    bytes.extend_from_slice(PACKET_MAGIC);
    bytes.extend_from_slice(&PACKET_SCHEMA_VERSION.to_le_bytes());
    bytes.extend_from_slice(&PACKET_TRANSPORT_VERSION.to_le_bytes());
    bytes.extend_from_slice(&(HEADER_LENGTH as u32).to_le_bytes());
    bytes.resize(MANIFEST_LENGTH_OFFSET, 0);
    bytes.extend_from_slice(&(manifest.len() as u64).to_le_bytes());
    bytes.extend_from_slice(&(packet.binary.len() as u64).to_le_bytes());
    bytes.extend_from_slice(&manifest_digest);
    bytes.extend_from_slice(&content_digest);
    debug_assert_eq!(bytes.len(), HEADER_LENGTH);
    bytes.extend_from_slice(&manifest);
    bytes.extend_from_slice(&packet.binary);
    bytes
}

pub fn decode_preview_packet(bytes: &[u8]) -> Result<PreviewGeometryPacket, PreviewPacketError> {
    decode_preview_packet_with_limit(bytes, DEFAULT_MAX_PACKET_BYTES)
}

pub fn decode_preview_packet_with_limit(
    bytes: &[u8],
    max_bytes: usize,
) -> Result<PreviewGeometryPacket, PreviewPacketError> {
    if bytes.len() > max_bytes || bytes.len() < HEADER_LENGTH {
        return Err(PreviewPacketError::TooLarge);
    }
    if &bytes[0..8] != PACKET_MAGIC {
        return Err(PreviewPacketError::InvalidHeader);
    }
    let schema_version = u16::from_le_bytes(bytes[8..10].try_into().unwrap());
    let transport_version = u16::from_le_bytes(bytes[10..12].try_into().unwrap());
    let header_length = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    if schema_version != PACKET_SCHEMA_VERSION
        || transport_version != PACKET_TRANSPORT_VERSION
        || header_length != HEADER_LENGTH
    {
        return Err(PreviewPacketError::InvalidHeader);
    }
    let manifest_length = usize::try_from(read_u64(bytes, MANIFEST_LENGTH_OFFSET))
        .map_err(|_| PreviewPacketError::TooLarge)?;
    let binary_length = usize::try_from(read_u64(bytes, BINARY_LENGTH_OFFSET))
        .map_err(|_| PreviewPacketError::TooLarge)?;
    if manifest_length == 0
        || manifest_length % MANIFEST_ALIGNMENT != 0
        || HEADER_LENGTH
            .checked_add(manifest_length)
            .and_then(|value| value.checked_add(binary_length))
            != Some(bytes.len())
    {
        return Err(PreviewPacketError::InvalidHeader);
    }
    let manifest_end = HEADER_LENGTH + manifest_length;
    let manifest_bytes = &bytes[HEADER_LENGTH..manifest_end];
    let binary = &bytes[manifest_end..];
    if Sha256::digest(manifest_bytes).as_slice()
        != &bytes[MANIFEST_DIGEST_OFFSET..MANIFEST_DIGEST_OFFSET + 32]
    {
        return Err(PreviewPacketError::DigestMismatch);
    }
    let mut content_hasher = Sha256::new();
    content_hasher.update(manifest_bytes);
    content_hasher.update(binary);
    if content_hasher.finalize().as_slice()
        != &bytes[CONTENT_DIGEST_OFFSET..CONTENT_DIGEST_OFFSET + 32]
    {
        return Err(PreviewPacketError::DigestMismatch);
    }

    let manifest: PreviewGeometryManifest =
        serde_json::from_slice(manifest_bytes).map_err(|_| PreviewPacketError::InvalidManifest)?;
    validate_manifest(&manifest, binary.len())?;
    Ok(PreviewGeometryPacket {
        manifest,
        binary: binary.to_vec(),
    })
}

pub fn build_preview_delta(
    base: &PreviewGeometryPacket,
    next: &PreviewGeometryPacket,
) -> PreviewGeometryPacket {
    let base_nodes = base
        .manifest
        .nodes
        .iter()
        .map(|node| (node.node_id.as_str(), node))
        .collect::<std::collections::BTreeMap<_, _>>();
    let next_node_ids = next
        .manifest
        .nodes
        .iter()
        .map(|node| node.node_id.as_str())
        .collect::<BTreeSet<_>>();
    let nodes = next
        .manifest
        .nodes
        .iter()
        .filter(|node| base_nodes.get(node.node_id.as_str()) != Some(node))
        .cloned()
        .collect::<Vec<_>>();
    let removed_node_ids = base
        .manifest
        .nodes
        .iter()
        .filter(|node| !next_node_ids.contains(node.node_id.as_str()))
        .map(|node| node.node_id.clone())
        .collect();

    let base_geometries = base
        .manifest
        .geometries
        .iter()
        .map(|geometry| (geometry.geometry_id.as_str(), geometry))
        .collect::<std::collections::BTreeMap<_, _>>();
    let next_geometry_ids = next
        .manifest
        .geometries
        .iter()
        .map(|geometry| geometry.geometry_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut geometries = Vec::new();
    let mut binary = Vec::new();
    for geometry in &next.manifest.geometries {
        let base_geometry = base_geometries.get(geometry.geometry_id.as_str()).copied();
        if base_geometry == Some(geometry) {
            continue;
        }
        let mut delta_geometry = geometry.clone();
        let topology_changed =
            base_geometry.is_none_or(|base| base.topology_digest != geometry.topology_digest);
        let attributes_changed =
            base_geometry.is_none_or(|base| base.attribute_digest != geometry.attribute_digest);
        delta_geometry.ranges.retain(|range| match range.usage {
            PreviewGeometryRangeUsage::VertexAttributes => attributes_changed,
            PreviewGeometryRangeUsage::Indices => topology_changed,
        });
        for range in &mut delta_geometry.ranges {
            let start = range.byte_offset as usize;
            let Some(end) = start.checked_add(range.byte_length as usize) else {
                continue;
            };
            let Some(chunk) = next.binary.get(start..end) else {
                continue;
            };
            range.byte_offset = binary.len() as u64;
            binary.extend_from_slice(chunk);
        }
        geometries.push(delta_geometry);
    }
    let removed_geometry_ids = base
        .manifest
        .geometries
        .iter()
        .filter(|geometry| !next_geometry_ids.contains(geometry.geometry_id.as_str()))
        .map(|geometry| geometry.geometry_id.clone())
        .collect();
    let base_materials = base
        .manifest
        .materials
        .iter()
        .map(|material| (material.material_id.as_str(), material))
        .collect::<std::collections::BTreeMap<_, _>>();
    let materials = next
        .manifest
        .materials
        .iter()
        .filter(|material| base_materials.get(material.material_id.as_str()) != Some(material))
        .cloned()
        .collect();

    PreviewGeometryPacket {
        manifest: PreviewGeometryManifest {
            schema_version: next.manifest.schema_version,
            transport_version: next.manifest.transport_version,
            revision: next.manifest.revision,
            parent_revision: Some(base.manifest.revision),
            source_digest: next.manifest.source_digest.clone(),
            backend: next.manifest.backend,
            residency_mode: next.manifest.residency_mode,
            nodes,
            geometries,
            materials,
            removed_node_ids,
            removed_geometry_ids,
        },
        binary,
    }
}

pub fn build_full_preview_packet(
    document: &Document,
    scene: &EvaluatedScene,
    backend: GeometryBackendKind,
    residency_mode: GeometryEvaluationMode,
) -> PreviewGeometryPacket {
    let nodes = scene
        .nodes
        .iter()
        .map(|node| PreviewGeometryNode {
            node_id: node.id.clone(),
            parent_id: node.parent_id.clone(),
            name: node.name.clone(),
            matrix: transform_matrix(node.transform),
            material_id: node.material_id.as_ref().map(ToString::to_string),
            geometry_id: node
                .resident_geometry_id
                .as_ref()
                .map(|id| id.0.clone())
                .or_else(|| node.mesh_key.clone()),
            visible: node.visible,
        })
        .collect();

    let mut geometries = Vec::new();
    let mut binary = Vec::new();
    let mut encoded_geometry_ids = BTreeSet::new();
    for node in &scene.nodes {
        let Some(mesh_key) = &node.mesh_key else {
            continue;
        };
        let Some(mesh) = scene.meshes.get(mesh_key) else {
            continue;
        };
        let geometry_id = node
            .resident_geometry_id
            .as_ref()
            .map(|id| id.0.clone())
            .unwrap_or_else(|| mesh_key.clone());
        if !encoded_geometry_ids.insert(geometry_id.clone()) {
            continue;
        }

        let mut attribute_bytes = Vec::with_capacity(mesh.vertices.len() * 24);
        for vertex in &mesh.vertices {
            for value in vertex.position.into_iter().chain(vertex.normal) {
                attribute_bytes.extend_from_slice(&value.to_le_bytes());
            }
        }
        let mut topology_bytes = Vec::with_capacity(mesh.indices.len() * 4);
        for index in &mesh.indices {
            topology_bytes.extend_from_slice(&index.to_le_bytes());
        }
        let mut ranges = Vec::new();
        if !attribute_bytes.is_empty() {
            let byte_offset = binary.len() as u64;
            binary.extend_from_slice(&attribute_bytes);
            ranges.push(PreviewGeometryRange {
                byte_offset,
                byte_length: attribute_bytes.len() as u64,
                usage: PreviewGeometryRangeUsage::VertexAttributes,
            });
        }
        if !topology_bytes.is_empty() {
            let byte_offset = binary.len() as u64;
            binary.extend_from_slice(&topology_bytes);
            ranges.push(PreviewGeometryRange {
                byte_offset,
                byte_length: topology_bytes.len() as u64,
                usage: PreviewGeometryRangeUsage::Indices,
            });
        }
        geometries.push(PreviewGeometryRecord {
            geometry_id,
            resident_geometry_id: node.resident_geometry_id.clone(),
            topology_digest: format!("{:x}", Sha256::digest(&topology_bytes)),
            attribute_digest: format!("{:x}", Sha256::digest(&attribute_bytes)),
            vertex_count: u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX),
            index_count: u32::try_from(mesh.indices.len()).unwrap_or(u32::MAX),
            index_width: 4,
            ranges,
        });
    }
    let materials = document
        .materials
        .iter()
        .map(|material| PreviewGeometryMaterial {
            material_id: material.id.to_string(),
            base_color: material.base_color,
            metallic: 0.0,
            roughness: 0.75,
            opacity: material.opacity,
        })
        .collect();
    let source_bytes = serde_json::to_vec(document).unwrap_or_default();

    PreviewGeometryPacket {
        manifest: PreviewGeometryManifest {
            schema_version: PACKET_SCHEMA_VERSION,
            transport_version: PACKET_TRANSPORT_VERSION,
            revision: document.revision.as_u64(),
            parent_revision: None,
            source_digest: format!("{:x}", Sha256::digest(source_bytes)),
            backend,
            residency_mode,
            nodes,
            geometries,
            materials,
            removed_node_ids: Vec::new(),
            removed_geometry_ids: Vec::new(),
        },
        binary,
    }
}

fn validate_manifest(
    manifest: &PreviewGeometryManifest,
    binary_length: usize,
) -> Result<(), PreviewPacketError> {
    if manifest.schema_version != PACKET_SCHEMA_VERSION
        || manifest.transport_version != PACKET_TRANSPORT_VERSION
        || !is_sha256(&manifest.source_digest)
        || manifest.nodes.len() > MAX_NODES
        || manifest.geometries.len() > MAX_GEOMETRIES
        || manifest.materials.len() > MAX_GEOMETRIES
        || manifest
            .parent_revision
            .is_some_and(|parent| parent >= manifest.revision)
    {
        return Err(PreviewPacketError::InvalidManifest);
    }

    let mut node_ids = BTreeSet::new();
    for node in &manifest.nodes {
        if node.node_id.is_empty()
            || !node_ids.insert(&node.node_id)
            || !node.matrix.iter().all(|value| value.is_finite())
        {
            return Err(PreviewPacketError::InvalidManifest);
        }
    }
    let mut geometry_ids = BTreeSet::new();
    for geometry in &manifest.geometries {
        if geometry.geometry_id.is_empty()
            || !geometry_ids.insert(&geometry.geometry_id)
            || !is_sha256(&geometry.topology_digest)
            || !is_sha256(&geometry.attribute_digest)
            || !matches!(geometry.index_width, 2 | 4)
            || geometry.ranges.len() > MAX_RANGES_PER_GEOMETRY
        {
            return Err(PreviewPacketError::InvalidManifest);
        }
        for range in &geometry.ranges {
            let range_end = range
                .byte_offset
                .checked_add(range.byte_length)
                .ok_or(PreviewPacketError::InvalidManifest)?;
            if range.byte_offset % 4 != 0
                || range.byte_length == 0
                || range_end > binary_length as u64
            {
                return Err(PreviewPacketError::InvalidManifest);
            }
        }
    }
    for material in &manifest.materials {
        if material.material_id.is_empty()
            || !material.base_color.iter().all(|value| value.is_finite())
            || !material.metallic.is_finite()
            || !material.roughness.is_finite()
            || !material.opacity.is_finite()
        {
            return Err(PreviewPacketError::InvalidManifest);
        }
    }
    Ok(())
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn transform_matrix(transform: Transform) -> [f32; 16] {
    let [x, y, z, w] = transform.rotation;
    let x2 = x + x;
    let y2 = y + y;
    let z2 = z + z;
    let xx = x * x2;
    let xy = x * y2;
    let xz = x * z2;
    let yy = y * y2;
    let yz = y * z2;
    let zz = z * z2;
    let wx = w * x2;
    let wy = w * y2;
    let wz = w * z2;
    let [sx, sy, sz] = [transform.scale.x, transform.scale.y, transform.scale.z];
    [
        (1.0 - (yy + zz)) * sx,
        (xy + wz) * sx,
        (xz - wy) * sx,
        0.0,
        (xy - wz) * sy,
        (1.0 - (xx + zz)) * sy,
        (yz + wx) * sy,
        0.0,
        (xz + wy) * sz,
        (yz - wx) * sz,
        (1.0 - (xx + yy)) * sz,
        0.0,
        transform.translation.x,
        transform.translation.y,
        transform.translation.z,
        1.0,
    ]
}
