use std::path::{Component, Path, PathBuf};

use base64::Engine;
use percent_encoding::percent_decode_str;
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

const GLB_MAGIC: u32 = 0x4654_6C67;
const JSON_CHUNK: u32 = 0x4E4F_534A;
const BIN_CHUNK: u32 = 0x004E_4942;

pub type AssetResult<T> = Result<T, AssetError>;

#[derive(Debug, Error)]
pub enum AssetError {
    #[error("invalid GLB or glTF source: {0}")]
    InvalidGlb(String),
    #[error("unsupported asset profile")]
    ProfileUnsupported,
    #[error("invalid prepared profile: {0}")]
    ProfileInvalid(String),
    #[error("unsupported accessor: {0}")]
    AccessorUnsupported(String),
    #[error("accessor exceeds its buffer view")]
    AccessorOutOfRange,
    #[error("source I/O failed")]
    Io(#[from] std::io::Error),
    #[error("GLB JSON is invalid")]
    Json(#[from] serde_json::Error),
    #[error("invalid asset budget: {0}")]
    InvalidBudget(String),
    #[error("asset budget exceeded")]
    BudgetExceeded,
    #[error("asset report serialization failed: {0}")]
    Report(serde_json::Error),
}

impl AssetError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidGlb(_) | Self::Io(_) | Self::Json(_) => "ICE3_ASSET_INVALID_GLB",
            Self::ProfileUnsupported => "ICE3_ASSET_PROFILE_UNSUPPORTED",
            Self::ProfileInvalid(_) => "ICE3_ASSET_PROFILE_INVALID",
            Self::AccessorUnsupported(_) => "ICE3_ASSET_ACCESSOR_UNSUPPORTED",
            Self::AccessorOutOfRange => "ICE3_ASSET_ACCESSOR_OUT_OF_RANGE",
            Self::InvalidBudget(_) => "ICE3_STUDIO_ASSET_INVALID_BUDGET",
            Self::BudgetExceeded => "ICE3_STUDIO_ASSET_BUDGET_EXCEEDED",
            Self::Report(_) => "ICE3_STUDIO_ASSET_REPORT_WRITE_FAILED",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SourceGlb {
    pub json: Value,
    pub binary: Vec<u8>,
    pub buffers: Vec<Vec<u8>>,
    pub base_dir: PathBuf,
    pub digest: String,
    pub source_bytes: Vec<u8>,
}

pub fn read_glb(path: &Path) -> AssetResult<SourceGlb> {
    let bytes = std::fs::read(path)?;
    let base_dir = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    read_glb_bytes(&bytes, base_dir)
}

pub fn read_glb_bytes(bytes: &[u8], base_dir: &Path) -> AssetResult<SourceGlb> {
    let base_dir = std::fs::canonicalize(base_dir).unwrap_or_else(|_| base_dir.to_path_buf());
    let source_bytes = if requires_draco(bytes)? {
        decode_draco(bytes, &base_dir)?
    } else {
        bytes.to_vec()
    };
    let digest = format!("{:x}", Sha256::digest(&source_bytes));
    let (json_bytes, binary) = if is_glb(&source_bytes) {
        parse_glb(&source_bytes)?
    } else {
        (source_bytes.clone(), Vec::new())
    };
    let json: Value = serde_json::from_slice(&json_bytes)?;
    if !json.is_object() {
        return Err(invalid("JSON root must be an object"));
    }
    if is_glb(&source_bytes) {
        gltf::Gltf::from_slice(&source_bytes).map_err(|error| invalid(&error.to_string()))?;
    }
    let buffers = read_buffers(&json, &base_dir, &binary)?;
    let binary = buffers.first().cloned().unwrap_or(binary);
    Ok(SourceGlb {
        json,
        binary,
        buffers,
        base_dir,
        digest,
        source_bytes,
    })
}

fn requires_draco(bytes: &[u8]) -> AssetResult<bool> {
    let json_bytes = if is_glb(bytes) {
        parse_glb(bytes)?.0
    } else {
        bytes.to_vec()
    };
    let json: Value = serde_json::from_slice(&json_bytes)?;
    Ok(json
        .get("extensionsRequired")
        .and_then(Value::as_array)
        .is_some_and(|extensions| {
            extensions
                .iter()
                .any(|extension| extension.as_str() == Some("KHR_draco_mesh_compression"))
        }))
}

fn decode_draco(bytes: &[u8], base_dir: &Path) -> AssetResult<Vec<u8>> {
    let resolver = draco_gltf::FileResourceResolver::new(
        base_dir,
        draco_gltf::ExternalFilePolicy::ConfineToBase,
    );
    let mut import = draco_gltf::parse_with_options(
        bytes,
        Some(base_dir),
        Some(&resolver),
        &draco_gltf::ResourceLimits::default(),
        &draco_gltf::DracoDecodeOptions::default(),
        draco_gltf::ValidationProfile::Gltf20,
        &draco_gltf::ExtensionRegistry::default(),
    )
    .map_err(|error| invalid(&format!("Draco read failed: {error}")))?;
    import
        .decompress_in_place()
        .map_err(|error| invalid(&format!("Draco decode failed: {error}")))?;
    let normalized = import
        .to_bytes(draco_gltf::OutputFormat::GlbV2)
        .map_err(|error| invalid(&format!("normalized GLB write failed: {error}")))?;
    repair_orphaned_draco_accessors(&normalized)
}

fn repair_orphaned_draco_accessors(bytes: &[u8]) -> AssetResult<Vec<u8>> {
    let (json_bytes, binary) = parse_glb(bytes)?;
    let mut json: Value = serde_json::from_slice(&json_bytes)?;
    let Some(accessors) = json.get_mut("accessors").and_then(Value::as_array_mut) else {
        return Ok(bytes.to_vec());
    };
    let mut changed = false;
    for accessor in accessors {
        let Some(accessor) = accessor.as_object_mut() else {
            continue;
        };
        if !accessor.contains_key("bufferView") {
            // The decoder retains old Draco-only accessors as provenance but
            // materializes geometry into appended raw accessors.
            accessor.insert("bufferView".into(), Value::from(0));
            accessor.insert("byteOffset".into(), Value::from(0));
            changed = true;
        }
    }
    if !changed {
        return Ok(bytes.to_vec());
    }
    let json = serde_json::to_vec(&json)?;
    Ok(pack_glb(&json, &binary))
}

fn pack_glb(json: &[u8], binary: &[u8]) -> Vec<u8> {
    let mut json_chunk = json.to_vec();
    while !json_chunk.len().is_multiple_of(4) {
        json_chunk.push(b' ');
    }
    let mut binary_chunk = binary.to_vec();
    while !binary_chunk.len().is_multiple_of(4) {
        binary_chunk.push(0);
    }
    let total = 12 + 8 + json_chunk.len() + 8 + binary_chunk.len();
    let mut glb = Vec::with_capacity(total);
    glb.extend_from_slice(&GLB_MAGIC.to_le_bytes());
    glb.extend_from_slice(&2_u32.to_le_bytes());
    glb.extend_from_slice(&(total as u32).to_le_bytes());
    glb.extend_from_slice(&(json_chunk.len() as u32).to_le_bytes());
    glb.extend_from_slice(&JSON_CHUNK.to_le_bytes());
    glb.extend_from_slice(&json_chunk);
    glb.extend_from_slice(&(binary_chunk.len() as u32).to_le_bytes());
    glb.extend_from_slice(&BIN_CHUNK.to_le_bytes());
    glb.extend_from_slice(&binary_chunk);
    glb
}

pub fn read_bounded_uri(uri: &str, base_dir: &Path) -> AssetResult<Vec<u8>> {
    if let Some(payload) = uri.strip_prefix("data:") {
        let (metadata, data) = payload
            .split_once(',')
            .ok_or_else(|| invalid("data URI is invalid"))?;
        if metadata.ends_with(";base64") {
            return base64::engine::general_purpose::STANDARD
                .decode(data)
                .map_err(|_| invalid("data URI payload is invalid"));
        }
        return percent_decode_str(data)
            .decode_utf8()
            .map(|value| value.into_owned().into_bytes())
            .map_err(|_| invalid("data URI payload is invalid"));
    }
    if Path::new(uri).is_absolute() || looks_like_scheme(uri) {
        return Err(invalid("external resource URI must be relative"));
    }
    let decoded = percent_decode_str(uri)
        .decode_utf8()
        .map_err(|_| invalid("URI is not UTF-8"))?;
    let decoded = decoded.as_ref();
    let relative = Path::new(decoded);
    if relative
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(invalid(
            "external resource URI escapes the source directory",
        ));
    }
    if relative
        .components()
        .any(|component| matches!(component, Component::Prefix(_) | Component::RootDir))
    {
        return Err(invalid("external resource URI must be relative"));
    }
    let base =
        std::fs::canonicalize(base_dir).map_err(|_| invalid("source directory is missing"))?;
    let target = base.join(relative);
    let canonical = std::fs::canonicalize(&target)
        .map_err(|_| invalid(&format!("external resource is missing: {uri}")))?;
    if !canonical.starts_with(&base) {
        return Err(invalid(
            "external resource URI escapes the source directory",
        ));
    }
    std::fs::read(canonical)
        .map_err(|_| invalid(&format!("external resource is unreadable: {uri}")))
}

fn parse_glb(bytes: &[u8]) -> AssetResult<(Vec<u8>, Vec<u8>)> {
    if bytes.len() < 12 {
        return Err(invalid("GLB header is truncated"));
    }
    let version = u32::from_le_bytes(bytes[4..8].try_into().expect("four bytes"));
    let declared = u32::from_le_bytes(bytes[8..12].try_into().expect("four bytes")) as usize;
    if version != 2 {
        return Err(invalid("GLB version must be 2"));
    }
    if declared != bytes.len() {
        return Err(invalid("GLB declared length does not match the file"));
    }
    let mut offset = 12;
    let mut chunk_index = 0;
    let mut json = None;
    let mut binary = None;
    while offset < bytes.len() {
        let header = bytes
            .get(offset..offset + 8)
            .ok_or_else(|| invalid("GLB chunk header is truncated"))?;
        let length = u32::from_le_bytes(header[0..4].try_into().expect("four bytes")) as usize;
        let kind = u32::from_le_bytes(header[4..8].try_into().expect("four bytes"));
        if !length.is_multiple_of(4) {
            return Err(invalid("GLB chunk length is not aligned"));
        }
        let end = offset
            .checked_add(8)
            .and_then(|value| value.checked_add(length))
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| invalid("GLB chunk length is invalid"))?;
        let data = &bytes[offset + 8..end];
        match (chunk_index, kind) {
            (0, JSON_CHUNK) => json = Some(data.to_vec()),
            (0, _) => return Err(invalid("GLB JSON chunk must be first")),
            (1, BIN_CHUNK) => binary = Some(data.to_vec()),
            (_, JSON_CHUNK) => return Err(invalid("GLB must contain exactly one JSON chunk")),
            (_, BIN_CHUNK) => return Err(invalid("GLB BIN chunk must be second and unique")),
            _ => {}
        }
        chunk_index += 1;
        offset = end;
    }
    let json = json.ok_or_else(|| invalid("GLB JSON chunk is missing"))?;
    Ok((json, binary.unwrap_or_default()))
}

fn read_buffers(json: &Value, base_dir: &Path, binary: &[u8]) -> AssetResult<Vec<Vec<u8>>> {
    let Some(buffers) = json.get("buffers") else {
        return Ok(Vec::new());
    };
    let buffers = buffers
        .as_array()
        .ok_or_else(|| invalid("buffers must be an array"))?;
    buffers
        .iter()
        .enumerate()
        .map(|(index, buffer)| {
            let byte_length = buffer
                .get("byteLength")
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(|| invalid(&format!("buffer {index} byteLength is invalid")))?;
            let data = match buffer.get("uri") {
                None => {
                    if index != 0 || binary.len() < byte_length {
                        return Err(invalid(&format!("buffer {index} has no data source")));
                    }
                    binary[..byte_length].to_vec()
                }
                Some(uri) => {
                    let uri = uri
                        .as_str()
                        .ok_or_else(|| invalid(&format!("buffer {index} uri is invalid")))?;
                    let data = read_bounded_uri(uri, base_dir)?;
                    if data.len() < byte_length {
                        return Err(invalid(&format!(
                            "buffer {index} is shorter than byteLength"
                        )));
                    }
                    data[..byte_length].to_vec()
                }
            };
            Ok(data)
        })
        .collect()
}

fn is_glb(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && u32::from_le_bytes(bytes[..4].try_into().expect("four bytes")) == GLB_MAGIC
}

fn looks_like_scheme(uri: &str) -> bool {
    let mut chars = uri.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    chars
        .take_while(|character| *character != ':')
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.'))
        && uri.contains(':')
}

fn invalid(message: &str) -> AssetError {
    AssetError::InvalidGlb(message.to_owned())
}
