use std::collections::BTreeSet;

use serde_json::Value;

use crate::{AssetError, AssetMetrics, AssetResult, BudgetResult, PreparationMetadata, SourceGlb};

#[derive(Debug, Clone, PartialEq)]
pub struct PreparedBounds {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreparedProfile {
    pub asset_id: String,
    pub profile_version: String,
    pub project_id: Option<String>,
    pub revision: Option<u64>,
    pub bounds: Option<PreparedBounds>,
    pub node_count: usize,
    pub mesh_count: usize,
    pub material_count: usize,
    pub metrics: AssetMetrics,
    pub preparation: Option<PreparationMetadata>,
    pub budget: Option<BudgetResult>,
}

pub fn validate_prepared_profile(source: &SourceGlb) -> AssetResult<PreparedProfile> {
    let buffers = source.buffers.iter().map(Vec::as_slice).collect::<Vec<_>>();
    validate_prepared_parts(&source.json, &buffers, &source.source_bytes)
}

/// Validates a Prepared GLB directly from its bytes without copying geometry buffers.
pub fn validate_prepared_glb_bytes(bytes: &[u8]) -> AssetResult<PreparedProfile> {
    let (json_bytes, binary) = crate::glb::parse_glb_slices(bytes)?;
    let json: Value = serde_json::from_slice(json_bytes)?;
    if !json.is_object() {
        return Err(AssetError::InvalidGlb("JSON root must be an object".into()));
    }
    let buffer = match json.get("buffers").and_then(Value::as_array) {
        None => None,
        Some(buffers) if buffers.len() == 1 => {
            let buffer = &buffers[0];
            if buffer.get("uri").is_some() {
                return Err(AssetError::InvalidGlb(
                    "prepared GLB cannot reference an external buffer".into(),
                ));
            }
            let declared = buffer
                .get("byteLength")
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(|| AssetError::InvalidGlb("buffer byteLength is invalid".into()))?;
            Some(
                binary
                    .get(..declared)
                    .ok_or_else(|| AssetError::InvalidGlb("buffer data is truncated".into()))?,
            )
        }
        Some(_) => {
            return Err(AssetError::InvalidGlb(
                "prepared GLB must contain at most one embedded buffer".into(),
            ));
        }
    };
    let buffers = buffer.as_slice();
    validate_prepared_parts(&json, buffers, bytes)
}

fn validate_prepared_parts(
    json: &Value,
    buffers: &[&[u8]],
    source_bytes: &[u8],
) -> AssetResult<PreparedProfile> {
    let asset = object(json.get("asset"), "asset")?;
    if asset.get("version").and_then(Value::as_str) != Some("2.0") {
        return Err(profile_invalid("asset version must be 2.0"));
    }
    let profile = asset
        .get("extras")
        .and_then(Value::as_object)
        .and_then(|extras| extras.get("ice3"))
        .and_then(Value::as_object)
        .ok_or(AssetError::ProfileUnsupported)?;
    if profile.get("profile").and_then(Value::as_str) != Some("ice3-prepared") {
        return Err(AssetError::ProfileUnsupported);
    }
    let profile_version = profile
        .get("profileVersion")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or(AssetError::ProfileUnsupported)?
        .to_owned();
    let asset_id = profile
        .get("assetId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or(AssetError::ProfileUnsupported)?
        .to_owned();

    if json.get("scene").and_then(Value::as_u64) != Some(0) {
        return Err(profile_invalid("default scene must be 0"));
    }
    let scenes = json
        .get("scenes")
        .and_then(Value::as_array)
        .ok_or_else(|| profile_invalid("scenes must contain exactly one scene"))?;
    if scenes.len() != 1 {
        return Err(profile_invalid("scenes must contain exactly one scene"));
    }
    for (field, code) in [
        ("animations", "animations are unsupported"),
        ("skins", "skins are unsupported"),
        ("cameras", "cameras are unsupported"),
    ] {
        if json
            .get(field)
            .and_then(Value::as_array)
            .is_some_and(|values| !values.is_empty())
        {
            return Err(profile_invalid(code));
        }
    }
    validate_textures(json)?;
    validate_unique_ids(json, "nodes")?;
    validate_unique_ids(json, "meshes")?;
    validate_unique_ids(json, "materials")?;

    let bounds = profile.get("bounds").map(parse_bounds).transpose()?;
    let metrics = metrics_for(json, buffers, source_bytes)?;
    if let Some(stats) = profile.get("stats").filter(|value| !value.is_null()) {
        let stats = stats
            .as_object()
            .ok_or_else(|| profile_invalid("stats must be an object"))?;
        validate_stats(stats, metrics)?;
    }
    let preparation = profile
        .get("preparation")
        .filter(|value| !value.is_null())
        .map(|value| {
            let mut value = value.clone();
            if value.get("creaseAngleDegrees").is_none()
                && let Some(object) = value.as_object_mut()
            {
                object.insert("creaseAngleDegrees".into(), Value::from(30.0));
            }
            serde_json::from_value::<PreparationMetadata>(value)
                .map_err(|error| profile_invalid(&format!("preparation is invalid: {error}")))
        })
        .transpose()?;
    if metrics.edge_segment_count > 0 && preparation.is_none() {
        return Err(profile_invalid(
            "preparation metadata is required when edge primitives exist",
        ));
    }
    if let Some(preparation) = &preparation {
        validate_preparation(
            profile.get("preparation"),
            preparation,
            metrics.edge_segment_count,
        )?;
    }
    let budget = profile
        .get("budget")
        .filter(|value| !value.is_null())
        .map(|value| {
            let mut value = value.clone();
            if value.get("checks").is_none()
                && let Some(object) = value.as_object_mut()
            {
                object.insert("checks".into(), Value::Array(Vec::new()));
            }
            serde_json::from_value::<BudgetResult>(value)
                .map_err(|error| profile_invalid(&format!("budget is invalid: {error}")))
        })
        .transpose()?;
    let project = json
        .get("extras")
        .and_then(Value::as_object)
        .and_then(|extras| extras.get("ice3"))
        .and_then(Value::as_object);
    let project_id = project
        .and_then(|value| value.get("projectId"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let revision = project
        .and_then(|value| value.get("revision"))
        .and_then(Value::as_u64);

    Ok(PreparedProfile {
        asset_id,
        profile_version,
        project_id,
        revision,
        bounds,
        node_count: array_len(json, "nodes"),
        mesh_count: array_len(json, "meshes"),
        material_count: array_len(json, "materials"),
        metrics,
        preparation,
        budget,
    })
}

fn metrics_for(json: &Value, buffers: &[&[u8]], source_bytes: &[u8]) -> AssetResult<AssetMetrics> {
    let nodes = json
        .get("nodes")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let meshes = json
        .get("meshes")
        .and_then(Value::as_array)
        .ok_or_else(|| profile_invalid("meshes must be an array"))?;
    let accessors = json
        .get("accessors")
        .and_then(Value::as_array)
        .ok_or_else(|| profile_invalid("accessors must be an array"))?;
    let buffer_views = json
        .get("bufferViews")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let materials = json
        .get("materials")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let mut primitive_count = 0_u64;
    let mut triangle_count = 0_u64;
    let mut edge_segment_count = 0_u64;
    for mesh in meshes {
        let primitives = mesh
            .get("primitives")
            .and_then(Value::as_array)
            .ok_or_else(|| profile_invalid("mesh primitives must be an array"))?;
        primitive_count += primitives.len() as u64;
        for primitive in primitives {
            let mode = primitive.get("mode").and_then(Value::as_u64).unwrap_or(4);
            if mode == 1 {
                let role = primitive
                    .pointer("/extras/ice3/role")
                    .and_then(Value::as_str);
                if role != Some("edges") {
                    return Err(profile_invalid("line primitive must have the edge role"));
                }
                let width = primitive
                    .pointer("/extras/ice3/widthPx")
                    .and_then(Value::as_u64);
                if width != Some(3) {
                    return Err(profile_invalid("edge primitive widthPx must be 3"));
                }
            } else if mode != 4 {
                return Err(profile_invalid("unsupported primitive mode"));
            }
            let position = validate_accessor(
                primitive,
                "POSITION",
                accessors,
                buffer_views,
                buffers,
                "VEC3",
                &[5126],
            )?;
            if position.normalized {
                return Err(profile_invalid("POSITION accessor must not be normalized"));
            }
            let position_count = position.count;
            let indices = validate_accessor(
                primitive,
                "indices",
                accessors,
                buffer_views,
                buffers,
                "SCALAR",
                &[5123, 5125],
            )?;
            if indices.normalized {
                return Err(profile_invalid("index accessor must not be normalized"));
            }
            if indices_in_range(buffers, &indices, position_count)? {
                return Err(profile_invalid(
                    "primitive index reaches beyond POSITION data",
                ));
            }
            let count = indices.count as u64;
            let material = primitive
                .get("material")
                .and_then(Value::as_u64)
                .and_then(|index| usize::try_from(index).ok());
            if material.is_some_and(|index| index >= materials.len()) {
                return Err(profile_invalid("primitive material index is out of range"));
            }
            if mode == 4 {
                if count == 0 || !count.is_multiple_of(3) {
                    return Err(profile_invalid(
                        "triangle index count must be a positive multiple of 3",
                    ));
                }
                triangle_count = triangle_count
                    .checked_add(count / 3)
                    .ok_or_else(|| profile_invalid("triangle count overflowed"))?;
            } else {
                if count == 0 || !count.is_multiple_of(2) {
                    return Err(profile_invalid(
                        "edge index count must be a positive multiple of 2",
                    ));
                }
                let material = material
                    .ok_or_else(|| profile_invalid("edge primitive material is missing"))?;
                let material = materials
                    .get(material)
                    .ok_or_else(|| profile_invalid("edge material index is out of range"))?;
                if !material
                    .pointer("/extras/ice3/id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| id.ends_with("/edges"))
                {
                    return Err(profile_invalid(
                        "edge primitive must reference a material with edge identity",
                    ));
                }
                edge_segment_count = edge_segment_count
                    .checked_add(count / 2)
                    .ok_or_else(|| profile_invalid("edge count overflowed"))?;
            }
        }
    }
    let selection_group_count = nodes
        .iter()
        .filter_map(|node| node.pointer("/extras/ice3/selectionId"))
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>()
        .len() as u64;
    let (json_chunk_bytes, bin_chunk_bytes) = chunk_lengths(source_bytes);
    Ok(AssetMetrics {
        encoded_bytes: source_bytes.len() as u64,
        json_chunk_bytes,
        bin_chunk_bytes,
        node_count: nodes.len() as u64,
        max_node_depth: json_depth(json)? as u64,
        mesh_count: meshes.len() as u64,
        primitive_count,
        triangle_count,
        edge_segment_count,
        material_count: array_len(json, "materials") as u64,
        selection_group_count,
    })
}

struct AccessorLayout {
    buffer_index: usize,
    component_type: u64,
    count: usize,
    start: usize,
    stride: usize,
    normalized: bool,
}

fn validate_accessor(
    primitive: &Value,
    field: &str,
    accessors: &[Value],
    buffer_views: &[Value],
    buffers: &[&[u8]],
    expected_type: &str,
    component_types: &[u64],
) -> AssetResult<AccessorLayout> {
    let accessor_index = if field == "indices" {
        primitive.get("indices")
    } else {
        primitive.pointer(&format!("/attributes/{field}"))
    };
    let accessor_index = accessor_index
        .and_then(Value::as_u64)
        .and_then(|index| usize::try_from(index).ok())
        .ok_or_else(|| profile_invalid(&format!("primitive {field} accessor is missing")))?;
    let accessor = accessors
        .get(accessor_index)
        .ok_or_else(|| profile_invalid(&format!("primitive {field} accessor is missing")))?;
    if accessor.get("type").and_then(Value::as_str) != Some(expected_type) {
        return Err(profile_invalid(&format!(
            "primitive {field} accessor type must be {expected_type}"
        )));
    }
    if accessor.get("sparse").is_some() {
        return Err(profile_invalid("sparse accessors are unsupported"));
    }
    let component_type = accessor
        .get("componentType")
        .and_then(Value::as_u64)
        .ok_or_else(|| profile_invalid(&format!("primitive {field} componentType is missing")))?;
    if !component_types.contains(&component_type) {
        return Err(profile_invalid(&format!(
            "primitive {field} componentType is unsupported"
        )));
    }
    let components = match expected_type {
        "SCALAR" => 1,
        "VEC3" => 3,
        _ => return Err(profile_invalid("primitive field type is unsupported")),
    };
    let component_bytes = match component_type {
        5123 => 2,
        5125 | 5126 => 4,
        _ => {
            return Err(profile_invalid(
                "primitive field componentType is unsupported",
            ));
        }
    };
    let count = accessor
        .get("count")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| profile_invalid(&format!("primitive {field} count is missing")))?;
    let view_index = accessor
        .get("bufferView")
        .and_then(Value::as_u64)
        .and_then(|index| usize::try_from(index).ok())
        .ok_or_else(|| profile_invalid(&format!("primitive {field} bufferView is missing")))?;
    let view = buffer_views
        .get(view_index)
        .and_then(Value::as_object)
        .ok_or_else(|| profile_invalid(&format!("primitive {field} bufferView is invalid")))?;
    let buffer_index = view
        .get("buffer")
        .and_then(Value::as_u64)
        .and_then(|index| usize::try_from(index).ok())
        .ok_or_else(|| profile_invalid("bufferView buffer index is invalid"))?;
    let buffer = buffers
        .get(buffer_index)
        .ok_or_else(|| profile_invalid("bufferView buffer index is out of range"))?;
    let view_offset: usize = view
        .get("byteOffset")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .try_into()
        .map_err(|_| profile_invalid("bufferView byteOffset is invalid"))?;
    let view_length = view
        .get("byteLength")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| profile_invalid("bufferView byteLength is invalid"))?;
    let accessor_offset: usize = accessor
        .get("byteOffset")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .try_into()
        .map_err(|_| profile_invalid("accessor byteOffset is invalid"))?;
    let element_bytes = component_bytes * components;
    let stride =
        view.get("byteStride")
            .and_then(Value::as_u64)
            .map_or(Ok(element_bytes), |value| {
                usize::try_from(value)
                    .map_err(|_| profile_invalid("bufferView byteStride is invalid"))
            })?;
    if stride < element_bytes {
        return Err(profile_invalid(
            "bufferView byteStride is smaller than one element",
        ));
    }
    let accessor_bytes = if count == 0 {
        0
    } else {
        (count - 1)
            .checked_mul(stride)
            .and_then(|value| value.checked_add(element_bytes))
            .ok_or_else(|| profile_invalid("accessor byte range overflows"))?
    };
    if view_offset
        .checked_add(view_length)
        .is_none_or(|end| end > buffer.len())
        || accessor_offset
            .checked_add(accessor_bytes)
            .is_none_or(|end| end > view_length)
    {
        return Err(profile_invalid("accessor byte range is out of bounds"));
    }
    Ok(AccessorLayout {
        buffer_index,
        component_type,
        count,
        start: view_offset + accessor_offset,
        stride,
        normalized: accessor.get("normalized").and_then(Value::as_bool) == Some(true),
    })
}

fn indices_in_range(
    buffers: &[&[u8]],
    layout: &AccessorLayout,
    position_count: usize,
) -> AssetResult<bool> {
    let buffer = buffers
        .get(layout.buffer_index)
        .ok_or_else(|| profile_invalid("bufferView buffer index is out of range"))?;
    for element in 0..layout.count {
        let offset = layout.start + element * layout.stride;
        let index = match layout.component_type {
            5123 => u16::from_le_bytes(
                buffer
                    .get(offset..offset + 2)
                    .ok_or_else(|| profile_invalid("index accessor is out of bounds"))?
                    .try_into()
                    .expect("two bytes"),
            ) as u32,
            5125 => u32::from_le_bytes(
                buffer
                    .get(offset..offset + 4)
                    .ok_or_else(|| profile_invalid("index accessor is out of bounds"))?
                    .try_into()
                    .expect("four bytes"),
            ),
            _ => return Err(profile_invalid("index componentType is unsupported")),
        };
        if index as usize >= position_count {
            return Ok(true);
        }
    }
    Ok(false)
}

fn validate_preparation(
    value: Option<&Value>,
    preparation: &PreparationMetadata,
    edge_segment_count: u64,
) -> AssetResult<()> {
    if !preparation.crease_angle_degrees.is_finite()
        || !(0.0..=180.0).contains(&preparation.crease_angle_degrees)
    {
        return Err(profile_invalid(
            "preparation creaseAngleDegrees must be between 0 and 180",
        ));
    }
    if !preparation.edges_prepared && edge_segment_count > 0 {
        return Err(profile_invalid(
            "preparation declares no edges but edge primitives exist",
        ));
    }
    let Some(options) = value.and_then(|value| value.get("options")) else {
        return Ok(());
    };
    let options = options
        .as_object()
        .ok_or_else(|| profile_invalid("preparation options must be an object"))?;
    if let Some(edges) = options.get("edges") {
        let edges = edges
            .as_bool()
            .ok_or_else(|| profile_invalid("preparation options.edges must be a boolean"))?;
        if edges != preparation.edges_prepared {
            return Err(profile_invalid(
                "preparation options.edges is inconsistent with edgesPrepared",
            ));
        }
    }
    if let Some(crease) = options.get("creaseAngleDegrees") {
        let crease = crease
            .as_f64()
            .filter(|value| value.is_finite() && (0.0..=180.0).contains(value))
            .ok_or_else(|| {
                profile_invalid("preparation options.creaseAngleDegrees must be between 0 and 180")
            })?;
        if (crease as f32) != preparation.crease_angle_degrees {
            return Err(profile_invalid(
                "preparation options.creaseAngleDegrees is inconsistent with creaseAngleDegrees",
            ));
        }
    }
    Ok(())
}

fn validate_stats(
    stats: &serde_json::Map<String, Value>,
    metrics: AssetMetrics,
) -> AssetResult<()> {
    for (field, expected) in [
        ("nodeCount", metrics.node_count),
        ("maxNodeDepth", metrics.max_node_depth),
        ("meshCount", metrics.mesh_count),
        ("primitiveCount", metrics.primitive_count),
        ("triangleCount", metrics.triangle_count),
        ("edgeSegmentCount", metrics.edge_segment_count),
        ("materialCount", metrics.material_count),
        ("selectionGroupCount", metrics.selection_group_count),
    ] {
        let actual = stats
            .get(field)
            .and_then(Value::as_u64)
            .ok_or_else(|| profile_invalid(&format!("stats.{field} is missing")))?;
        if actual != expected {
            return Err(profile_invalid(&format!(
                "stats.{field} is {actual}, expected {expected}"
            )));
        }
    }
    Ok(())
}

fn chunk_lengths(bytes: &[u8]) -> (u64, u64) {
    if bytes.len() < 20 || &bytes[..4] != b"glTF" {
        return (0, 0);
    }
    let json = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as u64;
    let bin_header = 20 + json as usize;
    let bin = bytes
        .get(bin_header..bin_header + 4)
        .map(|value| u32::from_le_bytes(value.try_into().unwrap()) as u64)
        .unwrap_or(0);
    (json, bin)
}

fn json_depth(json: &Value) -> AssetResult<usize> {
    fn visit(nodes: &[Value], index: usize, active: &mut BTreeSet<usize>) -> AssetResult<usize> {
        if !active.insert(index) {
            return Err(profile_invalid("node hierarchy contains a cycle"));
        }
        let depth = nodes
            .get(index)
            .and_then(|node| node.get("children"))
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[])
            .iter()
            .map(|child| {
                let child = child
                    .as_u64()
                    .and_then(|value| usize::try_from(value).ok())
                    .ok_or_else(|| profile_invalid("node child index is invalid"))?;
                Ok(1 + visit(nodes, child, active)?)
            })
            .collect::<AssetResult<Vec<_>>>()?
            .into_iter()
            .max()
            .unwrap_or(0);
        active.remove(&index);
        Ok(depth)
    }
    let nodes = json
        .get("nodes")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let roots = json
        .get("scenes")
        .and_then(Value::as_array)
        .and_then(|scenes| scenes.first())
        .and_then(|scene| scene.get("nodes"))
        .and_then(Value::as_array)
        .ok_or_else(|| profile_invalid("default scene roots are missing"))?;
    roots
        .iter()
        .map(|root| {
            let root = root
                .as_u64()
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(|| profile_invalid("scene root index is invalid"))?;
            visit(nodes, root, &mut BTreeSet::new())
        })
        .collect::<AssetResult<Vec<_>>>()?
        .into_iter()
        .max()
        .map_or(Ok(0), Ok)
}

fn validate_unique_ids(json: &Value, field: &str) -> AssetResult<()> {
    let Some(values) = json.get(field) else {
        return Ok(());
    };
    let values = values
        .as_array()
        .ok_or_else(|| profile_invalid(&format!("{field} must be an array")))?;
    let mut ids = BTreeSet::new();
    for value in values {
        let id = value
            .get("extras")
            .and_then(Value::as_object)
            .and_then(|extras| extras.get("ice3"))
            .and_then(Value::as_object)
            .and_then(|profile| profile.get("id"))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| profile_invalid(&format!("{field} contains a missing profile id")))?;
        if !ids.insert(id) {
            return Err(profile_invalid(&format!(
                "{field} contains a duplicate profile id"
            )));
        }
    }
    Ok(())
}

fn parse_bounds(value: &Value) -> AssetResult<PreparedBounds> {
    let object = value
        .as_object()
        .ok_or_else(|| profile_invalid("bounds must be an object"))?;
    Ok(PreparedBounds {
        min: vector3(object.get("min"), "bounds.min")?,
        max: vector3(object.get("max"), "bounds.max")?,
    })
}

fn vector3(value: Option<&Value>, field: &str) -> AssetResult<[f32; 3]> {
    let values = value
        .and_then(Value::as_array)
        .filter(|values| values.len() == 3)
        .ok_or_else(|| profile_invalid(&format!("{field} must contain three numbers")))?;
    let mut result = [0.0; 3];
    for (index, value) in values.iter().enumerate() {
        result[index] = value
            .as_f64()
            .filter(|value| value.is_finite())
            .map(|value| value as f32)
            .ok_or_else(|| profile_invalid(&format!("{field} must contain finite numbers")))?;
    }
    Ok(result)
}

fn object<'a>(
    value: Option<&'a Value>,
    field: &str,
) -> AssetResult<&'a serde_json::Map<String, Value>> {
    value
        .and_then(Value::as_object)
        .ok_or_else(|| profile_invalid(&format!("{field} must be an object")))
}

fn array_len(json: &Value, field: &str) -> usize {
    json.get(field)
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
}

fn profile_invalid(message: &str) -> AssetError {
    AssetError::ProfileInvalid(message.to_owned())
}

/// Base-colour textures are allowed, but only as relative files next to the asset.
///
/// The profile keeps exactly one buffer, so an embedded `bufferView` image would
/// hide the image inside the binary where the renderer cannot address it; a
/// relative `uri` keeps the asset streamable and portable.
fn validate_textures(json: &Value) -> AssetResult<()> {
    let images = array(json.get("images"));
    for image in &images {
        let uri = image.get("uri").and_then(Value::as_str);
        let valid = uri.is_some_and(|value| {
            !value.is_empty()
                && !value.starts_with('/')
                && !value.contains("..")
                && !value.contains("://")
        }) && image.get("bufferView").is_none();
        if !valid {
            return Err(profile_invalid(
                "texture images must be relative file paths",
            ));
        }
        if let (Some(uri), Some(mime_type)) = (uri, image.get("mimeType").and_then(Value::as_str)) {
            let expected = match mime_type {
                "image/png" => "png",
                "image/jpeg" => "jpg",
                "image/webp" => "webp",
                _ => {
                    return Err(profile_invalid("texture image mime type is invalid"));
                }
            };
            if !uri.to_ascii_lowercase().ends_with(&format!(".{expected}")) {
                return Err(profile_invalid(
                    "texture image mime type does not match its path",
                ));
            }
        }
    }
    let textures = array(json.get("textures"));
    let samplers = array(json.get("samplers"));
    for sampler in &samplers {
        if sampler
            .get("magFilter")
            .and_then(Value::as_u64)
            .is_some_and(|value| !matches!(value, 9728 | 9729))
            || sampler
                .get("minFilter")
                .and_then(Value::as_u64)
                .is_some_and(|value| !matches!(value, 9728 | 9729 | 9984..=9987))
            || sampler
                .get("wrapS")
                .and_then(Value::as_u64)
                .is_some_and(|value| !matches!(value, 33071 | 33648 | 10497))
            || sampler
                .get("wrapT")
                .and_then(Value::as_u64)
                .is_some_and(|value| !matches!(value, 33071 | 33648 | 10497))
        {
            return Err(profile_invalid("texture sampler value is invalid"));
        }
    }
    for texture in &textures {
        let source = texture.get("source").and_then(Value::as_u64);
        if source.is_none_or(|index| index as usize >= images.len()) {
            return Err(profile_invalid("texture source is out of range"));
        }
        if let Some(sampler) = texture.get("sampler").and_then(Value::as_u64)
            && sampler as usize >= samplers.len()
        {
            return Err(profile_invalid("texture sampler is out of range"));
        }
    }
    for material in array(json.get("materials")) {
        for pointer in [
            "/pbrMetallicRoughness/baseColorTexture/index",
            "/pbrMetallicRoughness/metallicRoughnessTexture/index",
            "/normalTexture/index",
            "/occlusionTexture/index",
        ] {
            let index = material.pointer(pointer).and_then(Value::as_u64);
            if let Some(index) = index
                && index as usize >= textures.len()
            {
                return Err(profile_invalid("texture index is out of range"));
            }
        }
        for pointer in [
            "/pbrMetallicRoughness/baseColorTexture",
            "/pbrMetallicRoughness/metallicRoughnessTexture",
            "/normalTexture",
            "/occlusionTexture",
        ] {
            let Some(info) = material.pointer(pointer).and_then(Value::as_object) else {
                continue;
            };
            if info
                .get("texCoord")
                .and_then(Value::as_u64)
                .is_some_and(|value| value != 0)
            {
                return Err(profile_invalid("texture coordinates must be TEXCOORD_0"));
            }
            if let Some(transform) = info
                .get("extensions")
                .and_then(Value::as_object)
                .and_then(|value| value.get("KHR_texture_transform"))
                .and_then(Value::as_object)
                && (transform
                    .get("texCoord")
                    .and_then(Value::as_u64)
                    .is_some_and(|value| value != 0)
                    || transform
                        .get("offset")
                        .is_some_and(|value| !valid_vec2(value, false))
                    || transform
                        .get("rotation")
                        .is_some_and(|value| !value.as_f64().is_some_and(f64::is_finite))
                    || transform
                        .get("scale")
                        .is_some_and(|value| !valid_vec2(value, true)))
            {
                return Err(profile_invalid("texture transform is invalid"));
            }
            if pointer == "/normalTexture"
                && info.get("scale").is_some_and(|value| {
                    !value
                        .as_f64()
                        .is_some_and(|value| value.is_finite() && (0.0..=2.0).contains(&value))
                })
            {
                return Err(profile_invalid("normal texture scale is invalid"));
            }
        }
    }
    Ok(())
}

fn valid_vec2(value: &Value, positive: bool) -> bool {
    value
        .as_array()
        .filter(|values| values.len() == 2)
        .is_some_and(|values| {
            values.iter().all(|value| {
                value
                    .as_f64()
                    .is_some_and(|value| value.is_finite() && (!positive || value > 0.0))
            })
        })
}

fn array(value: Option<&Value>) -> Vec<Value> {
    value
        .and_then(Value::as_array)
        .map(|values| values.to_vec())
        .unwrap_or_default()
}
