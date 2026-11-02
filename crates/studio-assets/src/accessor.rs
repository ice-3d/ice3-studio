use crate::{AssetError, AssetResult, SourceGlb};

const COMPONENT_U16: u64 = 5123;
const COMPONENT_U32: u64 = 5125;
const COMPONENT_F32: u64 = 5126;

#[derive(Debug, Clone, PartialEq)]
pub enum AccessorData {
    F32 { values: Vec<f32>, components: usize },
    U16 { values: Vec<u16>, components: usize },
    U32 { values: Vec<u32>, components: usize },
}

impl AccessorData {
    pub fn components(&self) -> usize {
        match self {
            Self::F32 { components, .. }
            | Self::U16 { components, .. }
            | Self::U32 { components, .. } => *components,
        }
    }

    pub fn len(&self) -> usize {
        match self {
            Self::F32 { values, .. } => values.len(),
            Self::U16 { values, .. } => values.len(),
            Self::U32 { values, .. } => values.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn as_f32(&self) -> Option<&[f32]> {
        match self {
            Self::F32 { values, .. } => Some(values),
            _ => None,
        }
    }

    pub fn as_u16(&self) -> Option<&[u16]> {
        match self {
            Self::U16 { values, .. } => Some(values),
            _ => None,
        }
    }

    pub fn as_u32(&self) -> Option<&[u32]> {
        match self {
            Self::U32 { values, .. } => Some(values),
            _ => None,
        }
    }
}

pub fn read_accessor(source: &SourceGlb, accessor_index: usize) -> AssetResult<AccessorData> {
    let accessor = array(source, "accessors")?
        .get(accessor_index)
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| accessor_error("accessor index is missing"))?;
    if accessor.contains_key("sparse") {
        return Err(accessor_error("sparse accessors are unsupported"));
    }
    if accessor
        .get("normalized")
        .and_then(serde_json::Value::as_bool)
        == Some(true)
    {
        return Err(accessor_error("normalized accessors are unsupported"));
    }

    let component_type = unsigned(accessor.get("componentType"), "componentType")?;
    let component_bytes = match component_type {
        COMPONENT_U16 => 2,
        COMPONENT_U32 | COMPONENT_F32 => 4,
        other => {
            return Err(accessor_error(&format!(
                "unsupported component type {other}"
            )));
        }
    };
    let components = match accessor.get("type").and_then(serde_json::Value::as_str) {
        Some("SCALAR") => 1,
        Some("VEC2") => 2,
        Some("VEC3") => 3,
        Some("VEC4") => 4,
        Some(other) => {
            return Err(accessor_error(&format!(
                "unsupported accessor type {other}"
            )));
        }
        None => return Err(accessor_error("accessor type is missing")),
    };
    let count = unsigned(accessor.get("count"), "count")? as usize;
    let view_index = unsigned(accessor.get("bufferView"), "bufferView")? as usize;
    let view = array(source, "bufferViews")?
        .get(view_index)
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| accessor_error("bufferView index is missing"))?;
    let buffer_index = unsigned(view.get("buffer"), "buffer")? as usize;
    let buffer = source
        .buffers
        .get(buffer_index)
        .ok_or_else(|| accessor_error("buffer index is missing"))?;
    let view_offset = optional_unsigned(view.get("byteOffset"), "byteOffset")? as usize;
    let view_length = unsigned(view.get("byteLength"), "byteLength")? as usize;
    let accessor_offset = optional_unsigned(accessor.get("byteOffset"), "byteOffset")? as usize;
    let element_bytes = component_bytes * components;
    let stride = match view.get("byteStride") {
        Some(value) => unsigned(Some(value), "byteStride")? as usize,
        None => element_bytes,
    };
    if stride < element_bytes {
        return Err(accessor_error("byteStride is smaller than one element"));
    }
    let accessor_bytes = if count == 0 {
        0
    } else {
        (count - 1)
            .checked_mul(stride)
            .and_then(|value| value.checked_add(element_bytes))
            .ok_or_else(|| accessor_error("accessor byte range overflows"))?
    };
    if view_offset
        .checked_add(view_length)
        .is_none_or(|end| end > buffer.len())
        || accessor_offset
            .checked_add(accessor_bytes)
            .is_none_or(|end| end > view_length)
    {
        return Err(AssetError::AccessorOutOfRange);
    }

    let start = view_offset + accessor_offset;
    let mut values = Vec::with_capacity(
        count
            .checked_mul(components)
            .ok_or_else(|| accessor_error("accessor element count overflows"))?,
    );
    for element in 0..count {
        let element_start = start + element * stride;
        for component in 0..components {
            let offset = element_start + component * component_bytes;
            let value = match component_type {
                COMPONENT_F32 => f32::from_le_bytes(
                    buffer[offset..offset + 4]
                        .try_into()
                        .map_err(|_| AssetError::AccessorOutOfRange)?,
                ),
                COMPONENT_U16 => u16::from_le_bytes(
                    buffer[offset..offset + 2]
                        .try_into()
                        .map_err(|_| AssetError::AccessorOutOfRange)?,
                ) as f32,
                COMPONENT_U32 => u32::from_le_bytes(
                    buffer[offset..offset + 4]
                        .try_into()
                        .map_err(|_| AssetError::AccessorOutOfRange)?,
                ) as f32,
                _ => unreachable!(),
            };
            values.push(value);
        }
    }

    match component_type {
        COMPONENT_F32 => Ok(AccessorData::F32 { values, components }),
        COMPONENT_U16 => Ok(AccessorData::U16 {
            values: values.into_iter().map(|value| value as u16).collect(),
            components,
        }),
        COMPONENT_U32 => Ok(AccessorData::U32 {
            values: values.into_iter().map(|value| value as u32).collect(),
            components,
        }),
        _ => unreachable!(),
    }
}

fn array<'a>(source: &'a SourceGlb, field: &str) -> AssetResult<&'a Vec<serde_json::Value>> {
    source
        .json
        .get(field)
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| accessor_error(&format!("{field} must be an array")))
}

fn unsigned(value: Option<&serde_json::Value>, field: &str) -> AssetResult<u64> {
    value
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| accessor_error(&format!("{field} must be a non-negative integer")))
}

fn optional_unsigned(value: Option<&serde_json::Value>, field: &str) -> AssetResult<u64> {
    value.map_or(Ok(0), |value| unsigned(Some(value), field))
}

fn accessor_error(message: &str) -> AssetError {
    AssetError::AccessorUnsupported(message.to_owned())
}
