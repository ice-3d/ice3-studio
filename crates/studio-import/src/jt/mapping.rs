use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value, json};

use crate::{ImportError, SourceFormat, SourceScene};

const MAX_PROPERTY_DEPTH: usize = 8;
const MAX_PROPERTY_ARRAY_ITEMS: usize = 256;

pub(crate) fn prepare_source_identities(
    json: &mut Value,
    asset_id: &str,
) -> Result<(), ImportError> {
    let node_count = json
        .get("nodes")
        .and_then(Value::as_array)
        .map(Vec::len)
        .ok_or_else(geometry_invalid)?;
    let active = active_nodes(json, node_count)?
        .into_iter()
        .collect::<Vec<_>>();
    let nodes = json
        .get_mut("nodes")
        .and_then(Value::as_array_mut)
        .ok_or_else(geometry_invalid)?;
    for (ordinal, index) in active.into_iter().enumerate() {
        let node = &mut nodes[index];
        let node = node.as_object_mut().ok_or_else(geometry_invalid)?;
        let source_name = clean_name(node.get("name").and_then(Value::as_str), ordinal);
        let selection_id = format!("{asset_id}/jt/node/{ordinal}");
        let extras = node
            .entry("extras")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or_else(geometry_invalid)?;
        let ice3 = extras
            .entry("ice3")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or_else(geometry_invalid)?;
        ice3.insert("id".into(), Value::from(selection_id.clone()));
        ice3.insert("selectionId".into(), Value::from(selection_id.clone()));
        ice3.insert("displayName".into(), Value::from(source_name));
    }
    Ok(())
}

pub(crate) fn apply_jt_semantics(
    scene: &mut SourceScene,
    source_json: &Value,
    origin: [f64; 3],
) -> Result<(), ImportError> {
    let nodes = source_json
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or_else(geometry_invalid)?;
    let node_objects = nodes
        .iter()
        .map(|node| node.as_object().ok_or_else(geometry_invalid))
        .collect::<Result<Vec<_>, _>>()?;
    let meshes = source_json
        .get("meshes")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let materials = source_json
        .get("materials")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let active = active_nodes(source_json, nodes.len())?;
    let ordinals = active
        .iter()
        .enumerate()
        .map(|(ordinal, index)| (*index, ordinal))
        .collect::<BTreeMap<_, _>>();
    let instance_groups = instance_groups(&node_objects, &active);
    let ids = active
        .iter()
        .map(|index| {
            let node = node_objects[*index];
            let id = node
                .get("extras")
                .and_then(Value::as_object)
                .and_then(|extras| extras.get("ice3"))
                .and_then(Value::as_object)
                .and_then(|ice3| ice3.get("id"))
                .and_then(Value::as_str)
                .ok_or_else(geometry_invalid)?;
            Ok((id.to_owned(), *index))
        })
        .collect::<Result<BTreeMap<_, _>, ImportError>>()?;

    for output in &mut scene.nodes {
        let source_index = *ids.get(&output.id).ok_or_else(geometry_invalid)?;
        let ordinal = *ordinals.get(&source_index).ok_or_else(geometry_invalid)?;
        let source = node_objects[source_index];
        let display_name = clean_name(source.get("name").and_then(Value::as_str), ordinal);
        let selection_id = format!("{}/jt/node/{ordinal}", scene.asset_id);
        let source_jt = source
            .get("extras")
            .and_then(Value::as_object)
            .and_then(|extras| extras.get("jt"))
            .and_then(Value::as_object);

        let object = output.extras.as_object_mut().ok_or_else(geometry_invalid)?;
        object.remove("jt");
        let ice3 = object
            .entry("ice3")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or_else(geometry_invalid)?;
        ice3.insert("id".into(), Value::from(selection_id.clone()));
        ice3.insert("selectionId".into(), Value::from(selection_id.clone()));
        ice3.insert("displayName".into(), Value::from(display_name.clone()));
        ice3.insert(
            "jt".into(),
            jt_metadata(
                source,
                source_jt,
                display_name,
                meshes,
                materials,
                &instance_groups,
            ),
        );
        output.id = selection_id;
        output.name = clean_name(source.get("name").and_then(Value::as_str), ordinal);
    }

    scene.source_format = SourceFormat::Jt;
    scene.asset_extras = json!({
        "ice3": {
            "coordinateSystem": {
                "handedness": "right",
                "upAxis": "y",
                "linearUnit": "meter"
            },
            "jt": {
                "converterContractVersion": "1.0",
                "sourceOrigin": [origin[0], 0.0, origin[2]]
            }
        }
    });
    Ok(())
}

fn jt_metadata(
    source: &Map<String, Value>,
    source_jt: Option<&Map<String, Value>>,
    source_name: String,
    meshes: &[Value],
    materials: &[Value],
    instance_groups: &BTreeMap<usize, String>,
) -> Value {
    let mesh = source
        .get("mesh")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok());
    let material_names = mesh
        .and_then(|mesh| meshes.get(mesh))
        .and_then(Value::as_object)
        .and_then(|mesh| mesh.get("primitives"))
        .and_then(Value::as_array)
        .map(|primitives| {
            primitives
                .iter()
                .filter_map(|primitive| primitive.get("material"))
                .filter_map(Value::as_u64)
                .filter_map(|index| usize::try_from(index).ok())
                .filter_map(|index| materials.get(index))
                .filter_map(|material| material.get("name"))
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default()
        .into_iter()
        .collect::<Vec<_>>();
    let custom_properties = source_jt
        .and_then(|jt| jt.get("customProperties"))
        .and_then(Value::as_object)
        .map(sanitize_properties);
    let mut metadata = Map::new();
    metadata.insert(
        "objectType".into(),
        Value::from(if mesh.is_some() { "MESH" } else { "EMPTY" }),
    );
    metadata.insert("sourceName".into(), Value::from(source_name));
    metadata.insert("materialNames".into(), json!(material_names));
    if let Some(value) = source_jt
        .and_then(|jt| jt.get("objectId"))
        .and_then(Value::as_str)
    {
        metadata.insert("objectId".into(), Value::from(value));
    }
    if let Some(value) = source_jt
        .and_then(|jt| jt.get("assemblyPath"))
        .and_then(Value::as_str)
    {
        metadata.insert("assemblyPath".into(), Value::from(value));
    }
    if let Some(group) = mesh.and_then(|mesh| instance_groups.get(&mesh)) {
        metadata.insert("instanceGroup".into(), Value::from(group.as_str()));
    }
    if let Some(properties) = custom_properties {
        metadata.insert("customProperties".into(), Value::Object(properties));
    }
    Value::Object(metadata)
}

fn instance_groups(
    nodes: &[&Map<String, Value>],
    active: &BTreeSet<usize>,
) -> BTreeMap<usize, String> {
    let mut uses = BTreeMap::<usize, usize>::new();
    for index in active {
        if let Some(mesh) = nodes[*index]
            .get("mesh")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
        {
            *uses.entry(mesh).or_default() += 1;
        }
    }
    let mut groups = BTreeMap::new();
    let mut ordinal = 0;
    for (mesh, uses) in uses {
        if uses > 1 {
            groups.insert(mesh, format!("jt/instance/{ordinal}"));
            ordinal += 1;
        }
    }
    groups
}

fn active_nodes(json: &Value, node_count: usize) -> Result<BTreeSet<usize>, ImportError> {
    let scene_index = json.get("scene").and_then(Value::as_u64).unwrap_or(0) as usize;
    let roots = json
        .get("scenes")
        .and_then(Value::as_array)
        .and_then(|scenes| scenes.get(scene_index))
        .and_then(Value::as_object)
        .and_then(|scene| scene.get("nodes"))
        .and_then(Value::as_array)
        .ok_or_else(geometry_invalid)?;
    let nodes = json
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or_else(geometry_invalid)?;
    let mut active = BTreeSet::new();
    let mut stack = roots
        .iter()
        .map(|root| {
            root.as_u64()
                .and_then(|value| usize::try_from(value).ok())
                .filter(|value| *value < node_count)
                .ok_or_else(geometry_invalid)
        })
        .collect::<Result<Vec<_>, _>>()?;
    while let Some(index) = stack.pop() {
        if !active.insert(index) {
            return Err(geometry_invalid());
        }
        for child in nodes[index]
            .get("children")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            stack.push(
                child
                    .as_u64()
                    .and_then(|value| usize::try_from(value).ok())
                    .filter(|value| *value < node_count)
                    .ok_or_else(geometry_invalid)?,
            );
        }
    }
    Ok(active)
}

fn sanitize_properties(input: &Map<String, Value>) -> Map<String, Value> {
    let mut output = Map::new();
    for (key, value) in input {
        if let Some(value) = sanitize_value(value, 0) {
            output.insert(key.clone(), value);
        }
    }
    output
}

fn sanitize_value(value: &Value, depth: usize) -> Option<Value> {
    if depth > MAX_PROPERTY_DEPTH {
        return None;
    }
    match value {
        Value::String(_) | Value::Bool(_) => Some(value.clone()),
        Value::Number(value) if value.as_f64().is_some_and(f64::is_finite) => {
            Some(Value::Number(value.clone()))
        }
        Value::Array(values) if values.len() <= MAX_PROPERTY_ARRAY_ITEMS => values
            .iter()
            .map(|value| sanitize_value(value, depth + 1))
            .collect::<Option<Vec<_>>>()
            .map(Value::Array),
        _ => None,
    }
}

fn clean_name(value: Option<&str>, ordinal: usize) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("jt/node/{ordinal}"))
}

fn geometry_invalid() -> ImportError {
    ImportError::SourceDiagnostic {
        code: "ICE3_STUDIO_IMPORT_JT_GEOMETRY_INVALID".into(),
        message: "JT geometry, bounds or transform is invalid".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_properties_are_bounded_and_deterministic() {
        let input = json!({
            "z": [1, true, "value"],
            "a": 12.5,
            "nested": {"ignored": true},
            "tooDeep": [[[[[[[[[1]]]]]]]]]
        })
        .as_object()
        .unwrap()
        .clone();

        let output = sanitize_properties(&input);

        assert_eq!(output.len(), 2);
        assert_eq!(output["a"], 12.5);
        assert_eq!(output["z"], json!([1, true, "value"]));
    }
}
