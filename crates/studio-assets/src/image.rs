use std::path::Path;

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{AssetError, AssetResult, SourceGlb, read_bounded_uri_with_limit};

pub const MAX_TEXTURE_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_TEXTURE_TOTAL_BYTES: usize = 256 * 1024 * 1024;
pub const MAX_TEXTURE_COUNT: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageResource {
    pub digest: String,
    pub media_type: String,
    pub bytes: Vec<u8>,
}

pub fn validate_image_bytes(
    bytes: Vec<u8>,
    declared_media_type: Option<&str>,
    max_bytes: usize,
) -> AssetResult<ImageResource> {
    if bytes.len() > max_bytes {
        return Err(invalid("external resource exceeds its size limit"));
    }
    let media_type = if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        "image/png"
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        "image/jpeg"
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else {
        return Err(invalid("unsupported image format"));
    };
    if declared_media_type.is_some_and(|declared| declared != media_type) {
        return Err(invalid("image media type does not match its bytes"));
    }
    Ok(ImageResource {
        digest: format!("{:x}", Sha256::digest(&bytes)),
        media_type: media_type.into(),
        bytes,
    })
}

pub fn read_image_uri(
    uri: &str,
    base_dir: &Path,
    declared_media_type: Option<&str>,
    max_bytes: usize,
) -> AssetResult<ImageResource> {
    let bytes = read_bounded_uri_with_limit(uri, base_dir, max_bytes)?;
    validate_image_bytes(bytes, declared_media_type, max_bytes)
}

pub fn read_image_buffer_view(
    source: &SourceGlb,
    image: &Value,
    max_bytes: usize,
) -> AssetResult<ImageResource> {
    let image = image
        .as_object()
        .ok_or_else(|| invalid("image is invalid"))?;
    let view_index = image
        .get("bufferView")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| invalid("image bufferView is missing"))?;
    let view = source
        .json
        .get("bufferViews")
        .and_then(Value::as_array)
        .and_then(|views| views.get(view_index))
        .and_then(Value::as_object)
        .ok_or_else(|| invalid("image buffer view is out of range"))?;
    let buffer_index = view
        .get("buffer")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| invalid("image buffer is missing"))?;
    let buffer = source
        .buffers
        .get(buffer_index)
        .ok_or_else(|| invalid("image buffer is out of range"))?;
    let offset: usize = view
        .get("byteOffset")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .try_into()
        .map_err(|_| invalid("image buffer offset is invalid"))?;
    let length = view
        .get("byteLength")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| invalid("image buffer length is missing"))?;
    if length > max_bytes {
        return Err(invalid("external resource exceeds its size limit"));
    }
    let end = offset
        .checked_add(length)
        .filter(|end| *end <= buffer.len())
        .ok_or_else(|| invalid("image buffer view is out of range"))?;
    let declared = image.get("mimeType").and_then(Value::as_str);
    validate_image_bytes(buffer[offset..end].to_vec(), declared, max_bytes)
}

fn invalid(message: &str) -> AssetError {
    AssetError::InvalidGlb(message.into())
}
