use std::collections::BTreeSet;

use serde_json::Value;

use crate::{
    AccessorData, AssetError, AssetMetrics, AssetResult, BudgetResult, PreparationMetadata,
    SourceGlb, read_accessor,
};

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
    let asset = object(source.json.get("asset"), "asset")?;
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

    if source.json.get("scene").and_then(Value::as_u64) != Some(0) {
        return Err(profile_invalid("default scene must be 0"));
    }
    let scenes = source
        .json
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
        ("textures", "textures are unsupported"),
        ("images", "images are unsupported"),
        ("samplers", "samplers are unsupported"),
    ] {
        if source
            .json
            .get(field)
            .and_then(Value::as_array)
            .is_some_and(|values| !values.is_empty())
        {
            return Err(profile_invalid(code));
        }
    }
    validate_unique_ids(source, "nodes")?;
    validate_unique_ids(source, "meshes")?;
    validate_unique_ids(source, "materials")?;

    let bounds = profile.get("bounds").map(parse_bounds).transpose()?;
    let metrics = metrics_for(source)?;
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
    let project = source
        .json
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
        node_count: array_len(&source.json, "nodes"),
        mesh_count: array_len(&source.json, "meshes"),
        material_count: array_len(&source.json, "materials"),
        metrics,
        preparation,
        budget,
    })
}

fn metrics_for(source: &SourceGlb) -> AssetResult<AssetMetrics> {
    let nodes = source
        .json
        .get("nodes")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let meshes = source
        .json
        .get("meshes")
        .and_then(Value::as_array)
        .ok_or_else(|| profile_invalid("meshes must be an array"))?;
    let accessors = source
        .json
        .get("accessors")
        .and_then(Value::as_array)
        .ok_or_else(|| profile_invalid("accessors must be an array"))?;
    let buffer_views = source
        .json
        .get("bufferViews")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let materials = source
        .json
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
            let position_accessor = validate_accessor(
                primitive,
                "POSITION",
                accessors,
                buffer_views,
                source.buffers.len(),
                "VEC3",
                &[5126],
            )?;
            let position = read_accessor(source, position_accessor).map_err(|error| {
                profile_invalid(&format!("POSITION accessor is invalid: {error}"))
            })?;
            let position_count = match position {
                AccessorData::F32 {
                    values,
                    components: 3,
                } => values.len() / 3,
                _ => {
                    return Err(profile_invalid(
                        "POSITION accessor must decode as VEC3 float data",
                    ));
                }
            };
            let index_accessor = validate_accessor(
                primitive,
                "indices",
                accessors,
                buffer_views,
                source.buffers.len(),
                "SCALAR",
                &[5123, 5125],
            )?;
            let indices = read_accessor(source, index_accessor)
                .map_err(|error| profile_invalid(&format!("index accessor is invalid: {error}")))?;
            let decoded_indices = match indices {
                AccessorData::U16 {
                    values,
                    components: 1,
                } => values.into_iter().map(u32::from).collect::<Vec<_>>(),
                AccessorData::U32 {
                    values,
                    components: 1,
                } => values,
                _ => {
                    return Err(profile_invalid(
                        "index accessor must decode as SCALAR unsigned data",
                    ));
                }
            };
            if decoded_indices
                .iter()
                .any(|index| *index as usize >= position_count)
            {
                return Err(profile_invalid(
                    "primitive index reaches beyond POSITION data",
                ));
            }
            let count = decoded_indices.len() as u64;
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
    let (json_chunk_bytes, bin_chunk_bytes) = chunk_lengths(&source.source_bytes);
    Ok(AssetMetrics {
        encoded_bytes: source.source_bytes.len() as u64,
        json_chunk_bytes,
        bin_chunk_bytes,
        node_count: nodes.len() as u64,
        max_node_depth: json_depth(&source.json)? as u64,
        mesh_count: meshes.len() as u64,
        primitive_count,
        triangle_count,
        edge_segment_count,
        material_count: array_len(&source.json, "materials") as u64,
        selection_group_count,
    })
}

fn validate_accessor(
    primitive: &Value,
    field: &str,
    accessors: &[Value],
    buffer_views: &[Value],
    buffer_count: usize,
    expected_type: &str,
    component_types: &[u64],
) -> AssetResult<usize> {
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
    let component_type = accessor
        .get("componentType")
        .and_then(Value::as_u64)
        .ok_or_else(|| profile_invalid(&format!("primitive {field} componentType is missing")))?;
    if !component_types.contains(&component_type) {
        return Err(profile_invalid(&format!(
            "primitive {field} componentType is unsupported"
        )));
    }
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
    if buffer_index >= buffer_count {
        return Err(profile_invalid("bufferView buffer index is out of range"));
    }
    accessor
        .get("count")
        .and_then(Value::as_u64)
        .ok_or_else(|| profile_invalid(&format!("primitive {field} count is missing")))?;
    Ok(accessor_index)
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

fn validate_unique_ids(source: &SourceGlb, field: &str) -> AssetResult<()> {
    let Some(values) = source.json.get(field) else {
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
