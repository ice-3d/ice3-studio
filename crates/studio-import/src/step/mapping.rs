use std::collections::{BTreeMap, BTreeSet};

use glam::{DMat4, DVec3, DVec4};
use oxideav_step::{Occurrence, Shape, StepModel};
use serde_json::{Map, Value, json};

use crate::{
    ImportError, ImportWarning, NativeImportLimits, SourceFormat, SourceMaterial, SourceMesh,
    SourceNode, SourcePrimitive, SourceScene,
};

const DEFAULT_COLOR: [f32; 4] = [0.72, 0.74, 0.76, 1.0];

struct Projection<'a> {
    model: &'a StepModel,
    asset_id: &'a str,
    nodes: Vec<SourceNode>,
    meshes: Vec<SourceMesh>,
    materials: Vec<SourceMaterial>,
    part_meshes: BTreeMap<usize, usize>,
    node_worlds: BTreeMap<String, DMat4>,
    used_names: BTreeSet<String>,
    warnings: BTreeSet<ImportWarning>,
    visited_parts: BTreeSet<usize>,
    pending_mesh_nodes: Vec<PendingMeshNode>,
}

struct PendingMeshNode {
    parent_id: String,
    name: String,
    world: DMat4,
    mesh: usize,
    product_id: Option<String>,
    definition_id: Option<u64>,
    representation_id: Option<u64>,
    occurrence_id: Option<u64>,
    item_ids: Vec<u64>,
    layers: Vec<String>,
}

pub(super) fn project_model(
    model: &StepModel,
    asset_id: &str,
    limits: NativeImportLimits,
) -> Result<SourceScene, ImportError> {
    if model.triangle_count() == 0 {
        return Err(ImportError::EmptyGeometry);
    }
    let triangle_count = u64::try_from(model.triangle_count())
        .map_err(|_| ImportError::LimitExceeded("STEP triangle count overflow".into()))?;
    if triangle_count > limits.max_triangles {
        return Err(ImportError::LimitExceeded(format!(
            "STEP has {triangle_count} triangles; limit is {}",
            limits.max_triangles
        )));
    }

    let mut projection = Projection {
        model,
        asset_id,
        nodes: Vec::new(),
        meshes: Vec::new(),
        materials: Vec::new(),
        part_meshes: BTreeMap::new(),
        node_worlds: BTreeMap::new(),
        used_names: BTreeSet::new(),
        warnings: model
            .warnings
            .iter()
            .map(|message| ImportWarning {
                code: "ICE3_STUDIO_IMPORT_STEP_UNSUPPORTED_ENTITY".into(),
                message: message.clone(),
                path: None,
            })
            .collect(),
        visited_parts: BTreeSet::new(),
        pending_mesh_nodes: Vec::new(),
    };

    let root_id = format!("{asset_id}/step/node/0");
    projection.used_names.insert("step/node/0".into());
    projection.nodes.push(SourceNode {
        id: root_id.clone(),
        name: "step/node/0".into(),
        parent: None,
        matrix: DMat4::IDENTITY.to_cols_array().map(|value| value as f32),
        mesh: None,
        material: None,
        extras: step_node_extras(None, None, None, None, &[], &[]),
    });
    projection
        .node_worlds
        .insert(root_id.clone(), DMat4::IDENTITY);

    for occurrence in &model.roots {
        projection.visit_occurrence(occurrence, Some(&root_id), Some(DMat4::IDENTITY), 0)?;
    }
    for part_index in 0..model.parts.len() {
        if !projection.visited_parts.contains(&part_index) {
            projection.visit_part(part_index, None, Some(&root_id), Some(DMat4::IDENTITY), 0)?;
        }
    }
    projection.append_pending_mesh_nodes()?;

    if projection.nodes.len() > limits.max_nodes {
        return Err(ImportError::LimitExceeded(format!(
            "STEP has {} nodes; limit is {}",
            projection.nodes.len(),
            limits.max_nodes
        )));
    }
    if projection.meshes.is_empty() {
        return Err(ImportError::EmptyGeometry);
    }

    let source_origin = apply_shared_origin(
        &mut projection.nodes,
        &projection.meshes,
        &projection.node_worlds,
    )?;
    Ok(SourceScene {
        asset_id: asset_id.into(),
        source_format: SourceFormat::Step,
        nodes: projection.nodes,
        meshes: projection.meshes,
        materials: projection.materials,
        warnings: projection.warnings.into_iter().collect(),
        asset_extras: json!({
            "ice3": {
                "coordinateSystem": {
                    "handedness": "right",
                    "upAxis": "y",
                    "linearUnit": "meter"
                },
                "step": {
                    "schema": "ap242",
                    "unitAssumption": "meter",
                    "originalUnitMeters": model.length_unit_metres,
                    "sourceOrigin": source_origin
                }
            }
        }),
    })
}

impl Projection<'_> {
    fn visit_occurrence(
        &mut self,
        occurrence: &Occurrence,
        parent: Option<&str>,
        parent_world: Option<DMat4>,
        depth: usize,
    ) -> Result<(), ImportError> {
        if depth > self.model.parts.len() {
            return Err(ImportError::InvalidSource(
                "STEP occurrence hierarchy is cyclic".into(),
            ));
        }
        self.visit_part(
            occurrence.part,
            Some(occurrence),
            parent,
            parent_world,
            depth,
        )
    }

    fn visit_part(
        &mut self,
        part_index: usize,
        occurrence: Option<&Occurrence>,
        parent: Option<&str>,
        parent_world: Option<DMat4>,
        depth: usize,
    ) -> Result<(), ImportError> {
        let (raw_name, product_id, definition, representation, children, item_ids, layers) = {
            let part = self.model.parts.get(part_index).ok_or_else(|| {
                ImportError::InvalidSource(format!("STEP part {part_index} is missing"))
            })?;
            (
                occurrence
                    .and_then(|_| part.name.clone())
                    .or_else(|| occurrence.and_then(|value| value.name.clone()))
                    .filter(|name| !name.is_empty())
                    .unwrap_or_else(|| format!("step/node/{part_index}")),
                part.product_id.clone(),
                part.definition,
                part.representation,
                part.children.clone(),
                part.shapes
                    .iter()
                    .map(|shape| shape.item)
                    .collect::<Vec<_>>(),
                part.shapes
                    .iter()
                    .flat_map(|shape| shape.layers.iter().cloned())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>(),
            )
        };
        let name = self.stable_name(&raw_name);
        let ordinal = self.nodes.len();
        let node_id = format!("{}/step/node/{ordinal}", self.asset_id);
        let mesh = self.ensure_part_mesh(part_index)?;
        let local_world = occurrence.map_or(DMat4::IDENTITY, |value| {
            converted_transform(value, self.model.length_unit_metres)
        });
        let world = parent_world.map_or(local_world, |parent| parent * local_world);
        let matrix = local_world.to_cols_array().map(|value| value as f32);
        self.node_worlds.insert(node_id.clone(), world);
        self.nodes.push(SourceNode {
            id: node_id.clone(),
            name,
            parent: parent.map(str::to_owned),
            matrix,
            mesh: None,
            material: None,
            extras: step_node_extras(
                product_id.clone(),
                definition,
                representation,
                occurrence.and_then(|value| value.id),
                &item_ids,
                &layers,
            ),
        });
        self.visited_parts.insert(part_index);
        if let Some(mesh) = mesh {
            let mesh_name = self.stable_name(&raw_name);
            self.pending_mesh_nodes.push(PendingMeshNode {
                parent_id: node_id.clone(),
                name: mesh_name,
                world,
                mesh,
                product_id,
                definition_id: definition,
                representation_id: representation,
                occurrence_id: occurrence.and_then(|value| value.id),
                item_ids,
                layers,
            });
        }
        for child in children {
            self.visit_occurrence(&child, Some(&node_id), Some(world), depth + 1)?;
        }
        Ok(())
    }

    fn append_pending_mesh_nodes(&mut self) -> Result<(), ImportError> {
        for pending in std::mem::take(&mut self.pending_mesh_nodes) {
            let ordinal = self.nodes.len();
            let node_id = format!("{}/step/node/{ordinal}", self.asset_id);
            self.node_worlds.insert(node_id.clone(), pending.world);
            self.nodes.push(SourceNode {
                id: node_id,
                name: pending.name,
                parent: Some(pending.parent_id),
                matrix: DMat4::IDENTITY.to_cols_array().map(|value| value as f32),
                mesh: Some(pending.mesh),
                material: None,
                extras: step_node_extras(
                    pending.product_id,
                    pending.definition_id,
                    pending.representation_id,
                    pending.occurrence_id,
                    &pending.item_ids,
                    &pending.layers,
                ),
            });
        }
        Ok(())
    }

    fn ensure_part_mesh(&mut self, part_index: usize) -> Result<Option<usize>, ImportError> {
        if let Some(mesh) = self.part_meshes.get(&part_index) {
            return Ok(Some(*mesh));
        }
        let part = self.model.parts.get(part_index).ok_or_else(|| {
            ImportError::InvalidSource(format!("STEP part {part_index} is missing"))
        })?;
        if part.shapes.is_empty() {
            return Ok(None);
        }
        let mut primitives = Vec::new();
        let mut shape_metadata = Vec::new();
        for shape in &part.shapes {
            if shape.mesh.triangles.is_empty() {
                continue;
            }
            let positions = convert_positions(shape, self.model.length_unit_metres)?;
            for (colour, group_positions, indices) in grouped_triangle_positions(shape, &positions)?
            {
                let material = self.material_for(colour);
                primitives.push(SourcePrimitive {
                    positions: group_positions,
                    // The AP242 reader exposes no authored normal buffer. The
                    // canonical writer generates normals from the triangles.
                    normals: None,
                    colors: None,
                    indices,
                    material: Some(material),
                });
            }
            shape_metadata.push(step_shape_metadata(shape));
        }
        if primitives.is_empty() {
            return Ok(None);
        }
        let mesh_index = self.meshes.len();
        self.meshes.push(SourceMesh {
            id: format!("{}/step/mesh/{part_index}", self.asset_id),
            primitives,
            extras: step_mesh_extras(part.representation, shape_metadata),
        });
        self.part_meshes.insert(part_index, mesh_index);
        Ok(Some(mesh_index))
    }

    fn material_for(&mut self, rgba: [f32; 4]) -> usize {
        let color = [rgba[0], rgba[1], rgba[2]];
        if let Some(index) = self
            .materials
            .iter()
            .position(|material| material.base_color == color && material.opacity == rgba[3])
        {
            return index;
        }
        let index = self.materials.len();
        self.materials.push(SourceMaterial {
            id: format!("{}/step/material/{index}", self.asset_id),
            name: format!("step/material/{index}"),
            base_color: color,
            opacity: rgba[3],
            extras: json!({ "ice3": { "step": true } }),
        });
        index
    }

    fn stable_name(&mut self, raw: &str) -> String {
        let base = raw.trim();
        let mut candidate = base.to_owned();
        let mut suffix = 1;
        while !self.used_names.insert(candidate.clone()) {
            candidate = format!("{base}[{suffix}]");
            suffix += 1;
        }
        candidate
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct TriangleGroupKey([u32; 4]);

type CompactTriangleGroup = ([f32; 4], Vec<[f32; 3]>, Vec<u32>);

impl From<[f32; 4]> for TriangleGroupKey {
    fn from(value: [f32; 4]) -> Self {
        let mut normalized = value.map(|component| {
            if component.is_finite() {
                component.clamp(0.0, 1.0)
            } else {
                0.0
            }
        });
        for component in &mut normalized {
            if *component == -0.0 {
                *component = 0.0;
            }
        }
        Self(normalized.map(f32::to_bits))
    }
}

impl TriangleGroupKey {
    fn rgba(self) -> [f32; 4] {
        self.0.map(f32::from_bits)
    }
}

fn grouped_triangle_positions(
    shape: &Shape,
    positions: &[[f32; 3]],
) -> Result<Vec<CompactTriangleGroup>, ImportError> {
    let mut groups = BTreeMap::<TriangleGroupKey, Vec<u32>>::new();
    let vertex_count = shape.mesh.positions.len();
    for (triangle_index, triangle) in shape.mesh.triangles.iter().enumerate() {
        let colour = shape
            .triangle_colours
            .get(triangle_index)
            .copied()
            .flatten()
            .or(shape.colour)
            .unwrap_or(DEFAULT_COLOR);
        let indices = groups.entry(colour.into()).or_default();
        for index in triangle {
            if usize::try_from(*index)
                .ok()
                .filter(|index| *index < vertex_count)
                .is_none()
            {
                return Err(ImportError::InvalidSource(
                    "STEP index is outside the vertex buffer".into(),
                ));
            }
            indices.push(*index);
        }
    }
    groups
        .into_iter()
        .map(|(colour, indices)| {
            let mut remap = BTreeMap::<u32, u32>::new();
            let mut compact_positions = Vec::new();
            let mut compact_indices = Vec::with_capacity(indices.len());
            for index in indices {
                let local = *remap.entry(index).or_insert_with(|| {
                    let local = u32::try_from(compact_positions.len())
                        .expect("STEP compact primitive index overflow");
                    compact_positions.push(positions[index as usize]);
                    local
                });
                compact_indices.push(local);
            }
            Ok((colour.rgba(), compact_positions, compact_indices))
        })
        .collect()
}

fn convert_positions(shape: &Shape, unit_scale: f64) -> Result<Vec<[f32; 3]>, ImportError> {
    shape
        .mesh
        .positions
        .iter()
        .map(|point| {
            let converted = [
                (point[0] * unit_scale) as f32,
                (point[2] * unit_scale) as f32,
                (-point[1] * unit_scale) as f32,
            ];
            if converted.iter().all(|value| value.is_finite()) {
                Ok(converted)
            } else {
                Err(ImportError::InvalidSource(
                    "STEP mesh contains non-finite positions".into(),
                ))
            }
        })
        .collect()
}

fn apply_shared_origin(
    nodes: &mut [SourceNode],
    meshes: &[SourceMesh],
    worlds: &BTreeMap<String, DMat4>,
) -> Result<[f64; 3], ImportError> {
    let mut min = DVec3::splat(f64::INFINITY);
    let mut max = DVec3::splat(f64::NEG_INFINITY);
    for node in nodes.iter() {
        let Some(mesh_index) = node.mesh else {
            continue;
        };
        let world = worlds
            .get(&node.id)
            .copied()
            .ok_or_else(|| ImportError::InvalidSource("STEP node world matrix missing".into()))?;
        let Some(mesh) = meshes.get(mesh_index) else {
            return Err(ImportError::InvalidSource(
                "STEP node references a missing mesh".into(),
            ));
        };
        for primitive in &mesh.primitives {
            for position in &primitive.positions {
                let point = world.transform_point3(DVec3::from_array(position.map(f64::from)));
                if !point.is_finite() {
                    return Err(ImportError::InvalidSource(
                        "STEP world bounds contain non-finite values".into(),
                    ));
                }
                min = min.min(point);
                max = max.max(point);
            }
        }
    }
    if !min.is_finite() || !max.is_finite() {
        return Ok([0.0, 0.0, 0.0]);
    }

    let origin = [min.x, 0.0, min.z];
    let translation = DMat4::from_translation(DVec3::new(-origin[0], 0.0, -origin[2]));
    for node in nodes.iter_mut().filter(|node| node.parent.is_none()) {
        let local = worlds
            .get(&node.id)
            .copied()
            .ok_or_else(|| ImportError::InvalidSource("STEP node world matrix missing".into()))?;
        node.matrix = (translation * local)
            .to_cols_array()
            .map(|value| value as f32);
    }
    Ok(origin)
}

fn converted_transform(occurrence: &Occurrence, unit_scale: f64) -> DMat4 {
    let transform = occurrence.transform;
    let direction = DMat4::from_cols(
        DVec4::new(1.0, 0.0, 0.0, 0.0),
        DVec4::new(0.0, 0.0, 1.0, 0.0),
        DVec4::new(0.0, -1.0, 0.0, 0.0),
        DVec4::new(0.0, 0.0, 0.0, 1.0),
    );
    let local = DMat4::from_cols(
        DVec4::new(
            transform.cols[0][0],
            transform.cols[0][1],
            transform.cols[0][2],
            0.0,
        ),
        DVec4::new(
            transform.cols[1][0],
            transform.cols[1][1],
            transform.cols[1][2],
            0.0,
        ),
        DVec4::new(
            transform.cols[2][0],
            transform.cols[2][1],
            transform.cols[2][2],
            0.0,
        ),
        DVec4::new(
            transform.translation[0] * unit_scale,
            transform.translation[1] * unit_scale,
            transform.translation[2] * unit_scale,
            1.0,
        ),
    );
    direction * local * direction.transpose()
}

fn step_node_extras(
    product_id: Option<String>,
    definition_id: Option<u64>,
    representation_id: Option<u64>,
    occurrence_id: Option<u64>,
    item_ids: &[u64],
    layers: &[String],
) -> Value {
    let mut metadata = Map::new();
    metadata.insert("schema".into(), json!("ap242"));
    if let Some(product_id) = product_id {
        metadata.insert("productId".into(), json!(product_id));
    }
    if let Some(definition_id) = definition_id {
        metadata.insert("definitionId".into(), json!(definition_id));
    }
    if let Some(representation_id) = representation_id {
        metadata.insert("representationId".into(), json!(representation_id));
    }
    if let Some(occurrence_id) = occurrence_id {
        metadata.insert("occurrenceId".into(), json!(occurrence_id));
    }
    if let Some(item_id) = item_ids.first() {
        metadata.insert("itemId".into(), json!(item_id));
        metadata.insert("itemIds".into(), json!(item_ids));
    }
    if !layers.is_empty() {
        metadata.insert("layers".into(), json!(layers));
    }
    json!({ "ice3": { "step": Value::Object(metadata) } })
}

fn step_shape_metadata(shape: &Shape) -> Value {
    let mut metadata = Map::new();
    metadata.insert("itemId".into(), json!(shape.item));
    if let Some(name) = &shape.name {
        metadata.insert("name".into(), json!(name));
    }
    if !shape.layers.is_empty() {
        metadata.insert("layers".into(), json!(shape.layers));
    }
    Value::Object(metadata)
}

fn step_mesh_extras(representation_id: Option<u64>, shapes: Vec<Value>) -> Value {
    let mut metadata = Map::new();
    if let Some(representation_id) = representation_id {
        metadata.insert("representationId".into(), json!(representation_id));
    }
    if !shapes.is_empty() {
        metadata.insert("shapes".into(), Value::Array(shapes));
    }
    json!({ "ice3": { "step": Value::Object(metadata) } })
}

#[cfg(test)]
mod tests {
    use oxideav_step::{ApSchema, Part, Shape, Transform, TriMesh};

    use super::*;

    fn triangle_shape(item: u64, a: [f64; 3], b: [f64; 3]) -> Shape {
        Shape {
            item,
            name: Some("Triangle".into()),
            mesh: TriMesh {
                positions: vec![a, b, [a[0], a[1], a[2] + 1.0]],
                triangles: vec![[0, 1, 2]],
            },
            colour: Some([0.2, 0.4, 0.6, 1.0]),
            triangle_colours: Vec::new(),
            triangle_faces: vec![item],
            layers: Vec::new(),
        }
    }

    #[test]
    fn repeated_occurrences_share_one_mesh() {
        let mut child = Part {
            name: Some("Pump".into()),
            ..Part::default()
        };
        child.shapes.push(Shape {
            item: 1,
            name: Some("Pump body".into()),
            mesh: TriMesh {
                positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                triangles: vec![[0, 1, 2]],
            },
            colour: Some([0.2, 0.4, 0.6, 1.0]),
            triangle_colours: Vec::new(),
            triangle_faces: vec![1],
            layers: Vec::new(),
        });
        let mut root = Part {
            name: Some("Root".into()),
            children: vec![
                Occurrence {
                    part: 1,
                    id: Some(10),
                    name: Some("Pump A".into()),
                    transform: Transform::IDENTITY,
                    placed_by: None,
                },
                Occurrence {
                    part: 1,
                    id: Some(11),
                    name: Some("Pump B".into()),
                    transform: Transform::IDENTITY,
                    placed_by: None,
                },
            ],
            ..Part::default()
        };
        root.product_id = Some("Root".into());
        child.product_id = Some("Pump".into());
        let model = StepModel {
            schema: ApSchema::Ap242,
            header: Default::default(),
            length_unit_metres: 1.0,
            parts: vec![root, child],
            roots: vec![Occurrence {
                part: 0,
                id: None,
                name: None,
                transform: Transform::IDENTITY,
                placed_by: None,
            }],
            occurrence_colours: Vec::new(),
            warnings: Vec::new(),
        };

        let scene = project_model(&model, "step-instances", NativeImportLimits::default()).unwrap();
        let pump_nodes = scene
            .nodes
            .iter()
            .filter(|node| node.name.starts_with("Pump"))
            .filter(|node| node.mesh.is_some())
            .collect::<Vec<_>>();
        assert_eq!(pump_nodes.len(), 2);
        assert_eq!(pump_nodes[0].mesh, pump_nodes[1].mesh);
        assert_eq!(scene.meshes.len(), 1);
        assert!(
            pump_nodes[0].extras["ice3"]["step"]
                .get("representationId")
                .is_none()
        );
        assert!(pump_nodes[0].extras["ice3"]["step"].get("layers").is_none());
    }

    #[test]
    fn computes_large_origin_bounds_before_narrowing_transforms() {
        let large_origin = 16_777_217.0;
        let mut child = Part {
            name: Some("Offsite Pump".into()),
            product_id: Some("Offsite Pump".into()),
            ..Part::default()
        };
        child
            .shapes
            .push(triangle_shape(1, [0.0, 0.0, 0.0], [1.0, 0.0, 0.0]));
        let mut root = Part {
            name: Some("Root".into()),
            children: vec![Occurrence {
                part: 1,
                id: Some(10),
                name: Some("Offsite Pump".into()),
                transform: Transform {
                    cols: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
                    translation: [large_origin, 0.0, 0.0],
                },
                placed_by: None,
            }],
            ..Part::default()
        };
        root.product_id = Some("Root".into());
        let model = StepModel {
            schema: ApSchema::Ap242,
            header: Default::default(),
            length_unit_metres: 1.0,
            parts: vec![root, child],
            roots: vec![Occurrence {
                part: 0,
                id: None,
                name: None,
                transform: Transform::IDENTITY,
                placed_by: None,
            }],
            occurrence_colours: Vec::new(),
            warnings: Vec::new(),
        };

        let scene =
            project_model(&model, "step-large-origin", NativeImportLimits::default()).unwrap();
        let origin = scene.asset_extras["ice3"]["step"]["sourceOrigin"]
            .as_array()
            .unwrap();
        assert_eq!(origin[0], large_origin);
        assert_eq!(origin[2], 0.0);
    }

    #[test]
    fn compacts_positions_for_each_face_colour_group() {
        let shape = Shape {
            item: 1,
            name: Some("Multicolour".into()),
            mesh: TriMesh {
                positions: vec![
                    [0.0, 0.0, 0.0],
                    [1.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0],
                    [10.0, 0.0, 0.0],
                    [11.0, 0.0, 0.0],
                    [10.0, 1.0, 0.0],
                    [100.0, 0.0, 0.0],
                ],
                triangles: vec![[0, 1, 2], [3, 4, 5]],
            },
            colour: Some([0.2, 0.2, 0.2, 1.0]),
            triangle_colours: vec![Some([1.0, 0.0, 0.0, 1.0]), Some([0.0, 1.0, 0.0, 1.0])],
            triangle_faces: vec![10, 11],
            layers: Vec::new(),
        };
        let positions = convert_positions(&shape, 1.0).unwrap();
        let groups = grouped_triangle_positions(&shape, &positions).unwrap();

        assert_eq!(groups.len(), 2);
        assert!(groups.iter().all(|(_, positions, indices)| {
            positions.len() == 3
                && indices
                    .iter()
                    .all(|index| (*index as usize) < positions.len())
        }));
    }
}
