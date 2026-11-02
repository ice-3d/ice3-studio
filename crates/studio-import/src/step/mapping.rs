use std::collections::{BTreeMap, BTreeSet};

use glam::{DMat4, DVec3, DVec4};
use oxideav_step::{Occurrence, Shape, StepModel};
use serde_json::json;

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
    used_names: BTreeSet<String>,
    warnings: BTreeSet<ImportWarning>,
    visited_parts: BTreeSet<usize>,
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
    };

    for occurrence in &model.roots {
        projection.visit_occurrence(occurrence, None, 0)?;
    }
    for part_index in 0..model.parts.len() {
        if !projection.visited_parts.contains(&part_index) {
            projection.visit_part(part_index, None, None, 0)?;
        }
    }

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

    let source_origin = apply_shared_origin(&mut projection.nodes, &projection.meshes)?;
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
        depth: usize,
    ) -> Result<(), ImportError> {
        if depth > self.model.parts.len() {
            return Err(ImportError::InvalidSource(
                "STEP occurrence hierarchy is cyclic".into(),
            ));
        }
        self.visit_part(occurrence.part, Some(occurrence), parent, depth)
    }

    fn visit_part(
        &mut self,
        part_index: usize,
        occurrence: Option<&Occurrence>,
        parent: Option<&str>,
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
        let matrix = occurrence.map_or_else(identity_matrix, |value| {
            converted_transform(value, self.model.length_unit_metres)
        });
        self.nodes.push(SourceNode {
            id: node_id.clone(),
            name,
            parent: parent.map(str::to_owned),
            matrix,
            mesh,
            material: None,
            extras: json!({
                "ice3": {
                    "step": {
                        "schema": "ap242",
                        "productId": product_id,
                        "definitionId": definition,
                        "representationId": representation,
                        "occurrenceId": occurrence.and_then(|value| value.id),
                        "itemId": item_ids.first(),
                        "itemIds": item_ids,
                        "layers": layers
                    }
                }
            }),
        });
        self.visited_parts.insert(part_index);
        for child in children {
            self.visit_occurrence(&child, Some(&node_id), depth + 1)?;
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
            for (colour, indices) in grouped_triangles(shape)? {
                let material = self.material_for(colour);
                primitives.push(SourcePrimitive {
                    positions: positions.clone(),
                    // The AP242 reader exposes no authored normal buffer. The
                    // canonical writer generates normals from the triangles.
                    normals: None,
                    colors: None,
                    indices,
                    material: Some(material),
                });
            }
            shape_metadata.push(json!({
                "itemId": shape.item,
                "name": shape.name,
                "layers": shape.layers
            }));
        }
        if primitives.is_empty() {
            return Ok(None);
        }
        let mesh_index = self.meshes.len();
        self.meshes.push(SourceMesh {
            id: format!("{}/step/mesh/{part_index}", self.asset_id),
            primitives,
            extras: json!({
                "ice3": {
                    "step": {
                        "representationId": part.representation,
                        "shapes": shape_metadata
                    }
                }
            }),
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

type TriangleGroup = ([f32; 4], Vec<u32>);

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

fn grouped_triangles(shape: &Shape) -> Result<Vec<TriangleGroup>, ImportError> {
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
    Ok(groups
        .into_iter()
        .map(|(colour, indices)| (colour.rgba(), indices))
        .collect())
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
) -> Result<[f64; 3], ImportError> {
    let worlds = world_matrices(nodes)?;
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
        let local = matrix_to_dmat4(node.matrix);
        node.matrix = (translation * local)
            .to_cols_array()
            .map(|value| value as f32);
    }
    Ok(origin)
}

fn world_matrices(nodes: &[SourceNode]) -> Result<BTreeMap<String, DMat4>, ImportError> {
    let mut worlds = BTreeMap::new();
    for node in nodes {
        let local = matrix_to_dmat4(node.matrix);
        let world =
            match node.parent.as_deref() {
                Some(parent) => {
                    worlds.get(parent).copied().ok_or_else(|| {
                        ImportError::InvalidSource("STEP parent node missing".into())
                    })? * local
                }
                None => local,
            };
        worlds.insert(node.id.clone(), world);
    }
    Ok(worlds)
}

fn matrix_to_dmat4(matrix: [f32; 16]) -> DMat4 {
    DMat4::from_cols_array(&matrix.map(f64::from))
}

fn converted_transform(occurrence: &Occurrence, unit_scale: f64) -> [f32; 16] {
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
    (direction * local * direction.transpose())
        .to_cols_array()
        .map(|value| value as f32)
}

const fn identity_matrix() -> [f32; 16] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

#[cfg(test)]
mod tests {
    use oxideav_step::{ApSchema, Part, Shape, Transform, TriMesh};

    use super::*;

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
    }
}
