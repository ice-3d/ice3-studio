use std::collections::{BTreeMap, BTreeSet};

use glam::{DMat4, DQuat, DVec3};
use serde_json::{Map, Value, json};
use studio_assets::{AccessorData, SourceGlb, read_accessor};

use crate::ImportError;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct JtOrigin(pub [f64; 3]);

type Bounds = ([f64; 3], [f64; 3]);

pub(crate) fn rebase_json(json: &mut Value, source: &SourceGlb) -> Result<JtOrigin, ImportError> {
    let nodes = json
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or_else(geometry_invalid)?;
    if nodes.is_empty() {
        return Err(ImportError::EmptyGeometry);
    }
    let node_objects = nodes
        .iter()
        .map(|node| node.as_object().ok_or_else(geometry_invalid))
        .collect::<Result<Vec<_>, _>>()?;
    let children = node_objects
        .iter()
        .map(|node| integer_array(node.get("children"), nodes.len()))
        .collect::<Result<Vec<_>, _>>()?;
    let parents = parent_map(&children)?;
    let roots = scene_roots(json, nodes.len())?;
    for root in &roots {
        if parents[*root].is_some() {
            return Err(geometry_invalid());
        }
    }
    let world = world_matrices(&node_objects, &children, &parents)?;
    let active = active_nodes(&roots, &children)?;
    let bounds = active_bounds(source, &node_objects, &world, &active, json)?;
    let origin = [bounds.0[0], 0.0, bounds.0[2]];

    apply_origin(json, &roots, origin)?;
    Ok(JtOrigin(origin))
}

fn world_matrices(
    nodes: &[&Map<String, Value>],
    children: &[Vec<usize>],
    parents: &[Option<usize>],
) -> Result<Vec<DMat4>, ImportError> {
    let mut output = BTreeMap::new();
    for root in parents
        .iter()
        .enumerate()
        .filter_map(|(index, parent)| parent.is_none().then_some(index))
    {
        let mut stack = vec![(root, DMat4::IDENTITY)];
        while let Some((index, parent_world)) = stack.pop() {
            if output.contains_key(&index) {
                return Err(geometry_invalid());
            }
            let world = parent_world * local_matrix(nodes[index])?;
            output.insert(index, world);
            for child in &children[index] {
                stack.push((*child, world));
            }
        }
    }
    if output.len() != nodes.len() {
        return Err(geometry_invalid());
    }
    (0..nodes.len())
        .map(|index| output.get(&index).copied().ok_or_else(geometry_invalid))
        .collect()
}

fn parent_map(children: &[Vec<usize>]) -> Result<Vec<Option<usize>>, ImportError> {
    let mut parents = vec![None; children.len()];
    for (parent, children) in children.iter().enumerate() {
        for child in children {
            if parents[*child].replace(parent).is_some() {
                return Err(geometry_invalid());
            }
        }
    }
    Ok(parents)
}

fn active_nodes(roots: &[usize], children: &[Vec<usize>]) -> Result<BTreeSet<usize>, ImportError> {
    let mut active = BTreeSet::new();
    let mut stack = roots.to_vec();
    while let Some(index) = stack.pop() {
        if !active.insert(index) {
            return Err(geometry_invalid());
        }
        stack.extend(children.get(index).ok_or_else(geometry_invalid)?);
    }
    Ok(active)
}

fn active_bounds(
    source: &SourceGlb,
    nodes: &[&Map<String, Value>],
    world: &[DMat4],
    active: &BTreeSet<usize>,
    json: &Value,
) -> Result<Bounds, ImportError> {
    let meshes = json
        .get("meshes")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    let mut has_geometry = false;

    for index in active {
        let mesh_index = match nodes[*index].get("mesh") {
            Some(value) => Some(
                value
                    .as_u64()
                    .and_then(|value| usize::try_from(value).ok())
                    .ok_or_else(geometry_invalid)?,
            ),
            None => None,
        };
        let Some(mesh_index) = mesh_index else {
            continue;
        };
        let mesh = meshes
            .get(mesh_index)
            .and_then(Value::as_object)
            .ok_or_else(geometry_invalid)?;
        let primitives = mesh
            .get("primitives")
            .and_then(Value::as_array)
            .ok_or_else(geometry_invalid)?;
        for primitive in primitives {
            let primitive = primitive.as_object().ok_or_else(geometry_invalid)?;
            if primitive.get("mode").and_then(Value::as_u64).unwrap_or(4) != 4 {
                continue;
            }
            let position = primitive
                .get("attributes")
                .and_then(Value::as_object)
                .and_then(|attributes| attributes.get("POSITION"))
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(geometry_invalid)?;
            let Some((local_min, local_max)) = accessor_bounds(source, position)? else {
                continue;
            };
            for corner in 0..8 {
                let local = DVec3::new(
                    if corner & 1 == 0 {
                        local_min[0]
                    } else {
                        local_max[0]
                    },
                    if corner & 2 == 0 {
                        local_min[1]
                    } else {
                        local_max[1]
                    },
                    if corner & 4 == 0 {
                        local_min[2]
                    } else {
                        local_max[2]
                    },
                );
                let point = world[*index].transform_point3(local);
                if !point.is_finite() {
                    return Err(geometry_invalid());
                }
                for axis in 0..3 {
                    min[axis] = min[axis].min(point[axis]);
                    max[axis] = max[axis].max(point[axis]);
                }
                has_geometry = true;
            }
        }
    }
    if !has_geometry || min.iter().chain(max.iter()).any(|value| !value.is_finite()) {
        return Err(ImportError::EmptyGeometry);
    }
    Ok((min, max))
}

fn accessor_bounds(
    source: &SourceGlb,
    accessor_index: usize,
) -> Result<Option<Bounds>, ImportError> {
    let accessor = source
        .json
        .get("accessors")
        .and_then(Value::as_array)
        .and_then(|accessors| accessors.get(accessor_index))
        .and_then(Value::as_object)
        .ok_or_else(geometry_invalid)?;
    if let (Some(min), Some(max)) = (accessor.get("min"), accessor.get("max"))
        && let (Some(min), Some(max)) = (vector3_f64(min), vector3_f64(max))
    {
        if (0..3).any(|axis| min[axis] > max[axis]) {
            return Err(geometry_invalid());
        }
        return Ok(Some((min, max)));
    }
    let AccessorData::F32 {
        values,
        components: 3,
    } = read_accessor(source, accessor_index).map_err(|_| geometry_invalid())?
    else {
        return Err(geometry_invalid());
    };
    if values.is_empty() {
        return Ok(None);
    }
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for position in values.as_chunks::<3>().0 {
        for axis in 0..3 {
            min[axis] = min[axis].min(f64::from(position[axis]));
            max[axis] = max[axis].max(f64::from(position[axis]));
        }
    }
    Ok(Some((min, max)))
}

fn scene_roots(json: &Value, node_count: usize) -> Result<Vec<usize>, ImportError> {
    let scene_index = json.get("scene").and_then(Value::as_u64).unwrap_or(0) as usize;
    let roots = json
        .get("scenes")
        .and_then(Value::as_array)
        .and_then(|scenes| scenes.get(scene_index))
        .and_then(Value::as_object)
        .and_then(|scene| scene.get("nodes"))
        .ok_or_else(geometry_invalid)
        .and_then(|value| integer_array(Some(value), node_count))?;
    if roots.is_empty() {
        return Err(ImportError::EmptyGeometry);
    }
    Ok(roots)
}

fn apply_origin(json: &mut Value, roots: &[usize], origin: [f64; 3]) -> Result<(), ImportError> {
    let translation = DMat4::from_translation(-DVec3::from_array(origin));
    if roots.len() == 1 {
        let root = roots[0];
        let nodes = json
            .get_mut("nodes")
            .and_then(Value::as_array_mut)
            .ok_or_else(geometry_invalid)?;
        let node = nodes
            .get_mut(root)
            .and_then(Value::as_object_mut)
            .ok_or_else(geometry_invalid)?;
        let matrix = translation * local_matrix(node)?;
        node.remove("translation");
        node.remove("rotation");
        node.remove("scale");
        node.insert("matrix".into(), json!(matrix.to_cols_array()));
        return Ok(());
    }

    let nodes = json
        .get_mut("nodes")
        .and_then(Value::as_array_mut)
        .ok_or_else(geometry_invalid)?;
    let root_index = nodes.len();
    nodes.push(json!({
        "name": "JT Root",
        "matrix": translation.to_cols_array(),
        "children": roots,
    }));
    let scene_index = json.get("scene").and_then(Value::as_u64).unwrap_or(0) as usize;
    let scene = json
        .get_mut("scenes")
        .and_then(Value::as_array_mut)
        .and_then(|scenes| scenes.get_mut(scene_index))
        .and_then(Value::as_object_mut)
        .ok_or_else(geometry_invalid)?;
    scene.insert("nodes".into(), json!([root_index]));
    Ok(())
}

fn local_matrix(node: &Map<String, Value>) -> Result<DMat4, ImportError> {
    if let Some(matrix) = node.get("matrix") {
        let values = matrix
            .as_array()
            .filter(|values| values.len() == 16)
            .ok_or_else(geometry_invalid)?
            .iter()
            .map(finite_f64)
            .collect::<Result<Vec<_>, _>>()?;
        return values
            .try_into()
            .map(|values| DMat4::from_cols_array(&values))
            .map_err(|_| geometry_invalid());
    }
    let translation = vector3_f64(node.get("translation").unwrap_or(&json!([0, 0, 0])))
        .ok_or_else(geometry_invalid)?;
    let rotation = vector4_f64(node.get("rotation").unwrap_or(&json!([0, 0, 0, 1])))
        .ok_or_else(geometry_invalid)?;
    let scale =
        vector3_f64(node.get("scale").unwrap_or(&json!([1, 1, 1]))).ok_or_else(geometry_invalid)?;
    let rotation = DQuat::from_xyzw(rotation[0], rotation[1], rotation[2], rotation[3]);
    if rotation.length_squared() <= f64::EPSILON || !rotation.is_finite() {
        return Err(geometry_invalid());
    }
    Ok(DMat4::from_scale_rotation_translation(
        DVec3::from_array(scale),
        rotation.normalize(),
        DVec3::from_array(translation),
    ))
}

fn integer_array(value: Option<&Value>, node_count: usize) -> Result<Vec<usize>, ImportError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    value
        .as_array()
        .ok_or_else(geometry_invalid)?
        .iter()
        .map(|value| {
            value
                .as_u64()
                .and_then(|value| usize::try_from(value).ok())
                .filter(|value| *value < node_count)
                .ok_or_else(geometry_invalid)
        })
        .collect()
}

fn vector3_f64(value: &Value) -> Option<[f64; 3]> {
    let values = value.as_array().filter(|values| values.len() == 3)?;
    Some([
        finite_f64(&values[0]).ok()?,
        finite_f64(&values[1]).ok()?,
        finite_f64(&values[2]).ok()?,
    ])
}

fn vector4_f64(value: &Value) -> Option<[f64; 4]> {
    let values = value.as_array().filter(|values| values.len() == 4)?;
    Some([
        finite_f64(&values[0]).ok()?,
        finite_f64(&values[1]).ok()?,
        finite_f64(&values[2]).ok()?,
        finite_f64(&values[3]).ok()?,
    ])
}

fn finite_f64(value: &Value) -> Result<f64, ImportError> {
    value
        .as_f64()
        .filter(|value| value.is_finite())
        .ok_or_else(geometry_invalid)
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
    use std::path::PathBuf;

    #[test]
    fn rebases_a_single_matrix_root_to_the_shared_origin() {
        let source = source_with(
            json!([
                {
                    "name": "Root",
                    "matrix": [
                        1.0, 0.0, 0.0, 0.0,
                        0.0, 1.0, 0.0, 0.0,
                        0.0, 0.0, 1.0, 0.0,
                        10.0, 0.0, 20.0, 1.0
                    ],
                    "mesh": 0
                }
            ]),
            json!([0]),
        );
        let mut json = source.json.clone();

        let origin = rebase_json(&mut json, &source).unwrap();

        assert_eq!(origin.0, [9.0, 0.0, 19.0]);
        assert_eq!(json["nodes"][0]["matrix"][12], 1.0);
        assert_eq!(json["nodes"][0]["matrix"][14], 1.0);
    }

    #[test]
    fn wraps_multiple_roots_with_one_origin_node() {
        let source = source_with(
            json!([
                {"name": "A", "translation": [2, 0, 3], "mesh": 0},
                {"name": "B", "translation": [6, 0, 8], "mesh": 0}
            ]),
            json!([0, 1]),
        );
        let mut json = source.json.clone();

        let origin = rebase_json(&mut json, &source).unwrap();

        assert_eq!(origin.0, [1.0, 0.0, 2.0]);
        assert_eq!(json["nodes"].as_array().unwrap().len(), 3);
        assert_eq!(json["nodes"][2]["name"], "JT Root");
        assert_eq!(json["nodes"][2]["children"], json!([0, 1]));
        assert_eq!(json["scenes"][0]["nodes"], json!([2]));
    }

    #[test]
    fn accepts_trs_roots_and_keeps_f64_origin_precision() {
        let source = source_with(
            json!([
                {
                    "name": "Root",
                    "translation": [16777218.0, 0.0, 33554434.0],
                    "rotation": [0.0, 0.0, 0.0, 1.0],
                    "scale": [1.0, 1.0, 1.0],
                    "children": [1]
                },
                {
                    "name": "Pump",
                    "translation": [1.0, 0.0, -1.0],
                    "mesh": 0
                }
            ]),
            json!([0]),
        );
        let mut json = source.json.clone();

        let origin = rebase_json(&mut json, &source).unwrap();

        assert!(origin.0[0] > 16_777_216.0);
        assert!(origin.0[2] >= 33_554_432.0);
        let root_matrix = json["nodes"][0]["matrix"].as_array().unwrap();
        assert!(root_matrix[12].as_f64().unwrap().abs() < 5.0);
        assert!(root_matrix[14].as_f64().unwrap().abs() < 5.0);
        assert!(json["nodes"][1].get("translation").is_some());
    }

    #[test]
    fn rejects_a_node_cycle_before_mutating_the_source() {
        let source = source_with(
            json!([
                {"name": "A", "children": [1]},
                {"name": "B", "children": [0]}
            ]),
            json!([0]),
        );
        let mut json = source.json.clone();
        let before = json.clone();

        let error = rebase_json(&mut json, &source).unwrap_err();

        assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_JT_GEOMETRY_INVALID");
        assert_eq!(json, before);
    }

    #[test]
    fn rejects_inverted_accessor_bounds() {
        let mut source = source_with(json!([{"name": "Root", "mesh": 0}]), json!([0]));
        source.json["accessors"][0]["min"] = json!([2, 0, -1]);
        source.json["accessors"][0]["max"] = json!([1, 1, 1]);
        let mut json = source.json.clone();

        let error = rebase_json(&mut json, &source).unwrap_err();

        assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_JT_GEOMETRY_INVALID");
    }

    fn source_with(nodes: Value, roots: Value) -> SourceGlb {
        SourceGlb {
            json: json!({
                "asset": {"version": "2.0"},
                "scene": 0,
                "scenes": [{"nodes": roots}],
                "nodes": nodes,
                "meshes": [{
                    "primitives": [{"attributes": {"POSITION": 0}, "mode": 4}]
                }],
                "accessors": [{
                    "type": "VEC3",
                    "componentType": 5126,
                    "count": 8,
                    "min": [-1, 0, -1],
                    "max": [1, 1, 1]
                }]
            }),
            binary: Vec::new(),
            buffers: Vec::new(),
            base_dir: PathBuf::new(),
            digest: String::new(),
            source_bytes: Vec::new(),
        }
    }
}
