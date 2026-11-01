use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::json;

use crate::obj::read_obj_resources;
use crate::{
    ImportError, ImportRequest, ImportWarning, SourceCapabilities, SourceFormat, SourceImporter,
    SourceMaterial, SourceMesh, SourceNode, SourcePrimitive, SourceScene,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct ObjImporter;

impl ObjImporter {
    pub const fn new() -> Self {
        Self
    }

    pub fn import_file(&self, path: &Path, asset_id: &str) -> Result<SourceScene, ImportError> {
        self.import(&ImportRequest {
            path: path.to_path_buf(),
            asset_id: asset_id.into(),
            requested_adapter: Some("obj".into()),
        })
    }
}

impl SourceImporter for ObjImporter {
    fn id(&self) -> &'static str {
        "obj"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &[".obj"]
    }

    fn capabilities(&self) -> SourceCapabilities {
        SourceCapabilities {
            hierarchy: true,
            vertex_colors: false,
            material_colors: true,
            instances: false,
            semantic_metadata: false,
        }
    }

    fn import(&self, request: &ImportRequest) -> Result<SourceScene, ImportError> {
        let asset_id = request.asset_id.trim();
        if asset_id.is_empty() {
            return Err(ImportError::InvalidSource(
                "asset ID must not be empty".into(),
            ));
        }
        let resources = read_obj_resources(&request.path)?;
        parse_obj(&resources, asset_id)
    }
}

#[derive(Default)]
struct NodeDraft {
    name: String,
    parent: Option<usize>,
    mesh: Option<MeshDraft>,
    material_names: BTreeSet<String>,
}

#[derive(Default)]
struct MeshDraft {
    primitives: BTreeMap<String, PrimitiveDraft>,
}

struct PrimitiveDraft {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    all_normals: bool,
    indices: Vec<u32>,
    vertex_map: BTreeMap<(usize, Option<usize>, Option<usize>), u32>,
}

struct Parser {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    texture_coordinate_count: usize,
    nodes: Vec<NodeDraft>,
    current_object: Option<usize>,
    current_node: Option<usize>,
    current_material: Option<String>,
    warnings: BTreeSet<ImportWarning>,
}

fn parse_obj(
    resources: &crate::obj::ObjResources,
    asset_id: &str,
) -> Result<SourceScene, ImportError> {
    let text = std::str::from_utf8(&resources.obj)
        .map_err(|_| ImportError::InvalidSource("OBJ is not UTF-8".into()))?;
    let mut parser = Parser {
        positions: Vec::new(),
        normals: Vec::new(),
        texture_coordinate_count: 0,
        nodes: Vec::new(),
        current_object: None,
        current_node: None,
        current_material: None,
        warnings: resources.warnings.iter().cloned().collect(),
    };
    parser.warnings.insert(warning(
        "ICE3_STUDIO_IMPORT_OBJ_UNIT_ASSUMED_METERS",
        "OBJ units are interpreted as meters",
        None,
    ));
    for (line_index, raw_line) in text.lines().enumerate() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut fields = line.split_whitespace();
        let statement = fields.next().unwrap_or("");
        match statement {
            "v" => {
                let values = fields.collect::<Vec<_>>();
                if values.len() >= 7 {
                    parser.warnings.insert(warning(
                        "ICE3_STUDIO_IMPORT_OBJ_VERTEX_COLORS_UNSUPPORTED",
                        "OBJ vertex colors are not imported",
                        None,
                    ));
                }
                parser.positions.push([
                    parse_float(values.first().copied(), line_index)?,
                    parse_float(values.get(1).copied(), line_index)?,
                    parse_float(values.get(2).copied(), line_index)?,
                ]);
            }
            "vn" => {
                let values = fields.collect::<Vec<_>>();
                parser.normals.push([
                    parse_float(values.first().copied(), line_index)?,
                    parse_float(values.get(1).copied(), line_index)?,
                    parse_float(values.get(2).copied(), line_index)?,
                ]);
            }
            "vt" => {
                parser.texture_coordinate_count += 1;
            }
            "o" => {
                let name = fields.collect::<Vec<_>>().join(" ");
                let name = if name.is_empty() {
                    format!("OBJ Object {}", parser.nodes.len())
                } else {
                    name
                };
                let index = parser.nodes.len();
                parser.nodes.push(NodeDraft {
                    name,
                    ..NodeDraft::default()
                });
                parser.current_object = Some(index);
                parser.current_node = Some(index);
            }
            "g" => {
                let object = parser.ensure_object();
                let name = fields.collect::<Vec<_>>().join(" ");
                let name = if name.is_empty() {
                    "OBJ Group".into()
                } else {
                    name
                };
                let index = parser.nodes.len();
                parser.nodes.push(NodeDraft {
                    name,
                    parent: Some(object),
                    ..NodeDraft::default()
                });
                parser.current_node = Some(index);
            }
            "usemtl" => {
                let name = fields.collect::<Vec<_>>().join(" ");
                parser.current_material = (!name.is_empty()).then_some(name);
            }
            "f" => {
                let corners = fields
                    .map(|value| parser.face_corner(value, line_index))
                    .collect::<Result<Vec<_>, _>>()?;
                if corners.len() < 3 {
                    return Err(invalid(line_index, "face needs at least three corners"));
                }
                let node = parser.ensure_current_node();
                let material = parser
                    .current_material
                    .clone()
                    .unwrap_or_else(|| "__default".into());
                if material != "__default" && !resources.materials.contains_key(&material) {
                    parser.warnings.insert(warning(
                        "ICE3_STUDIO_IMPORT_OBJ_MATERIAL_UNAVAILABLE",
                        &format!("OBJ material {material} is unavailable"),
                        None,
                    ));
                }
                parser.nodes[node].material_names.insert(material.clone());
                let mesh = parser.nodes[node]
                    .mesh
                    .get_or_insert_with(MeshDraft::default);
                let primitive = mesh
                    .primitives
                    .entry(material)
                    .or_insert_with(|| PrimitiveDraft {
                        positions: Vec::new(),
                        normals: Vec::new(),
                        all_normals: true,
                        indices: Vec::new(),
                        vertex_map: BTreeMap::new(),
                    });
                for triangle in corners.windows(3) {
                    let [a, b, c] = [triangle[0], triangle[1], triangle[2]];
                    if a.0 == b.0 && b.0 == c.0 {
                        return Err(invalid(line_index, "face is degenerate"));
                    }
                    for corner in [a, b, c] {
                        let index =
                            append_corner(&parser.positions, &parser.normals, primitive, corner);
                        primitive.indices.push(index);
                    }
                }
            }
            _ => {}
        }
    }

    let mut min_x = f32::INFINITY;
    let mut min_z = f32::INFINITY;
    for node in &parser.nodes {
        if let Some(mesh) = &node.mesh {
            for primitive in mesh.primitives.values() {
                for point in &primitive.positions {
                    min_x = min_x.min(point[0]);
                    min_z = min_z.min(point[2]);
                }
            }
        }
    }
    if !min_x.is_finite() || !min_z.is_finite() {
        return Err(ImportError::EmptyGeometry);
    }
    let origin = [min_x, 0.0, min_z];
    let mut materials_by_name = resources.materials.clone();
    for node in &parser.nodes {
        if let Some(mesh) = &node.mesh {
            for name in mesh.primitives.keys() {
                if name != "__default" && !materials_by_name.contains_key(name) {
                    materials_by_name.insert(
                        name.clone(),
                        SourceMaterial {
                            id: name.clone(),
                            name: name.clone(),
                            base_color: [0.5; 3],
                            opacity: 1.0,
                            extras: json!({"ice3": {"id": name}}),
                        },
                    );
                }
            }
        }
    }
    let material_names = materials_by_name.keys().cloned().collect::<Vec<_>>();
    let material_indices = material_names
        .iter()
        .enumerate()
        .map(|(index, name)| (name.clone(), index))
        .collect::<BTreeMap<_, _>>();
    let materials = material_names
        .iter()
        .filter_map(|name| materials_by_name.remove(name))
        .collect::<Vec<_>>();
    let mut output_nodes = Vec::new();
    let mut output_meshes = Vec::new();
    for (node_index, node) in parser.nodes.iter().enumerate() {
        let id = format!("node-{node_index:06}");
        let mesh = node.mesh.as_ref().map(|mesh| {
            let mesh_index = output_meshes.len();
            let primitives = mesh
                .primitives
                .iter()
                .map(|(name, primitive)| {
                    let mut positions = primitive.positions.clone();
                    for position in &mut positions {
                        position[0] -= origin[0];
                        position[2] -= origin[2];
                    }
                    SourcePrimitive {
                        positions,
                        normals: primitive.all_normals.then(|| primitive.normals.clone()),
                        colors: None,
                        indices: primitive.indices.clone(),
                        material: (name != "__default").then(|| material_indices[name]),
                    }
                })
                .collect::<Vec<_>>();
            output_meshes.push(SourceMesh {
                id: format!("{asset_id}/obj/mesh/{node_index}"),
                primitives,
                extras: json!({
                    "ice3": {
                        "id": format!("{asset_id}/obj/mesh/{node_index}")
                    }
                }),
            });
            mesh_index
        });
        let material_names = node
            .material_names
            .iter()
            .filter(|name| name.as_str() != "__default")
            .cloned()
            .collect::<Vec<_>>();
        output_nodes.push(SourceNode {
            id: id.clone(),
            name: node.name.clone(),
            parent: node.parent.map(|parent| format!("node-{parent:06}")),
            matrix: identity(),
            mesh,
            material: None,
            extras: json!({
                "ice3": {
                    "id": id,
                    "selectionId": format!("{asset_id}/obj/node/{node_index}"),
                    "displayName": node.name,
                    "obj": {
                        "objectType": if mesh.is_some() { "MESH" } else { "EMPTY" },
                        "sourceName": node.name,
                        "materialNames": material_names,
                        "sourceOrigin": origin
                    }
                }
            }),
        });
    }
    let triangle_count = output_meshes
        .iter()
        .flat_map(|mesh| &mesh.primitives)
        .map(|primitive| primitive.indices.len() / 3)
        .sum::<usize>();
    if triangle_count == 0 {
        return Err(ImportError::EmptyGeometry);
    }
    Ok(SourceScene {
        asset_id: asset_id.into(),
        source_format: SourceFormat::Obj,
        nodes: output_nodes,
        meshes: output_meshes,
        materials,
        warnings: parser.warnings.into_iter().collect(),
        asset_extras: json!({
            "coordinateSystem": {
                "handedness": "right",
                "upAxis": "y",
                "linearUnit": "meter"
            },
            "obj": {
                "converter": "native-rust",
                "unitAssumption": "meter",
                "sourceOrigin": origin
            }
        }),
    })
}

impl Parser {
    fn ensure_object(&mut self) -> usize {
        if let Some(index) = self.current_object {
            return index;
        }
        let index = self.nodes.len();
        self.nodes.push(NodeDraft {
            name: "OBJ Root".into(),
            ..NodeDraft::default()
        });
        self.current_object = Some(index);
        self.current_node = Some(index);
        index
    }

    fn ensure_current_node(&mut self) -> usize {
        if let Some(index) = self.current_node {
            return index;
        }
        self.ensure_object()
    }

    fn face_corner(
        &self,
        value: &str,
        line_index: usize,
    ) -> Result<(usize, Option<usize>, Option<usize>), ImportError> {
        let parts = value.split('/').collect::<Vec<_>>();
        let vertex = resolve_index(parts.first().copied(), self.positions.len(), line_index)?;
        let texture = if parts.get(1).is_some_and(|value| !value.is_empty()) {
            Some(resolve_index(
                parts.get(1).copied(),
                self.texture_coordinate_count,
                line_index,
            )?)
        } else {
            None
        };
        let normal = if parts.get(2).is_some_and(|value| !value.is_empty()) {
            Some(resolve_index(
                parts.get(2).copied(),
                self.normals.len(),
                line_index,
            )?)
        } else {
            None
        };
        Ok((vertex, texture, normal))
    }
}

fn append_corner(
    source_positions: &[[f32; 3]],
    source_normals: &[[f32; 3]],
    primitive: &mut PrimitiveDraft,
    corner: (usize, Option<usize>, Option<usize>),
) -> u32 {
    if let Some(index) = primitive.vertex_map.get(&corner) {
        return *index;
    }
    let index = primitive.positions.len() as u32;
    primitive.positions.push(source_positions[corner.0]);
    if let Some(normal) = corner.2 {
        primitive.normals.push(source_normals[normal]);
    } else {
        primitive.all_normals = false;
    }
    primitive.vertex_map.insert(corner, index);
    index
}

fn resolve_index(value: Option<&str>, count: usize, line: usize) -> Result<usize, ImportError> {
    let parsed = value
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| *value != 0)
        .ok_or_else(|| invalid(line, "face index is invalid"))?;
    let index = if parsed > 0 {
        parsed as usize - 1
    } else {
        count
            .checked_sub(parsed.unsigned_abs() as usize)
            .ok_or_else(|| invalid(line, "face index is out of range"))?
    };
    if index >= count {
        return Err(invalid(line, "face index is out of range"));
    }
    Ok(index)
}

fn parse_float(value: Option<&str>, line: usize) -> Result<f32, ImportError> {
    value
        .and_then(|value| value.parse::<f32>().ok())
        .filter(|value| value.is_finite())
        .ok_or_else(|| invalid(line, "number is invalid"))
}

fn warning(code: &str, message: &str, path: Option<&str>) -> ImportWarning {
    ImportWarning {
        code: code.into(),
        message: message.into(),
        path: path.map(Into::into),
    }
}

fn invalid(line: usize, message: &str) -> ImportError {
    ImportError::InvalidSource(format!("OBJ line {}: {message}", line + 1))
}

fn identity() -> [f32; 16] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}
