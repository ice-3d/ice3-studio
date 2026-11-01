use std::collections::BTreeSet;

use serde_json::Value;

use crate::{AssetError, AssetResult, SourceGlb};

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
    })
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
