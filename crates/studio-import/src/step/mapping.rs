use std::collections::{BTreeMap, BTreeSet};
use std::mem::size_of;

use glam::{DMat4, DVec3, DVec4};
use oxideav_step::{Occurrence, Shape, StepModel};
use serde_json::{Map, Value, json};

use crate::{
    ImportError, ImportWarning, NativeImportLimits, SourceFormat, SourceMaterial, SourceMesh,
    SourceNode, SourcePrimitive, SourceScene, validate_scene_limits,
};

const DEFAULT_COLOR: [f32; 4] = [0.72, 0.74, 0.76, 1.0];
const STEP_NODE_ESTIMATE_BYTES: usize = 512;
const STEP_MESH_ESTIMATE_BYTES: usize = 1_024;
const STEP_PRIMITIVE_ESTIMATE_BYTES: usize = 256;
const STEP_MATERIAL_ESTIMATE_BYTES: usize = 384;
const STEP_SHAPE_METADATA_ESTIMATE_BYTES: usize = 1_024;
const STEP_WARNING_ESTIMATE_BYTES: usize = 512;
const STEP_SCENE_ESTIMATE_BYTES: usize = size_of::<SourceScene>() + 4_096;
// AP242 assemblies in the supported plant workflow are shallow. Cap before
// projection so pathological chains fail with a deterministic limit instead of
// overflowing the worker thread stack.
const MAX_STEP_HIERARCHY_DEPTH: usize = 128;

type OccurrencePath = [u64; 2];
type MeshVariantKey = (usize, Vec<(u64, TriangleGroupKey)>);

struct Projection<'a> {
    model: &'a StepModel,
    asset_id: &'a str,
    nodes: Vec<SourceNode>,
    meshes: Vec<SourceMesh>,
    mesh_positions: BTreeMap<usize, Vec<Vec<[f64; 3]>>>,
    materials: Vec<SourceMaterial>,
    part_meshes: BTreeMap<MeshVariantKey, usize>,
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
    item_id: Option<u64>,
    layers: Vec<String>,
}

pub(super) fn project_model(
    model: &StepModel,
    asset_id: &str,
    limits: NativeImportLimits,
) -> Result<SourceScene, ImportError> {
    preflight_model(model, limits)?;

    let mut projection = Projection {
        model,
        asset_id,
        nodes: Vec::new(),
        meshes: Vec::new(),
        mesh_positions: BTreeMap::new(),
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
        extras: step_node_extras(None, None, None, None, None, &[]),
    });
    projection
        .node_worlds
        .insert(root_id.clone(), DMat4::IDENTITY);

    for occurrence in &model.roots {
        projection.visit_occurrence(occurrence, Some(&root_id), Some(DMat4::IDENTITY), 0, &[])?;
    }
    for part_index in 0..model.parts.len() {
        if !projection.visited_parts.contains(&part_index) {
            projection.visit_part(
                part_index,
                None,
                Some(&root_id),
                Some(DMat4::IDENTITY),
                0,
                &[],
            )?;
        }
    }
    projection.append_pending_mesh_nodes()?;
    projection.annotate_shared_instances();

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
        &mut projection.meshes,
        &mut projection.mesh_positions,
        &projection.node_worlds,
    )?;
    let scene = SourceScene {
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
    };
    validate_scene_limits(&scene, &limits)?;
    Ok(scene)
}

fn preflight_model(model: &StepModel, limits: NativeImportLimits) -> Result<(), ImportError> {
    if model.triangle_count() == 0 {
        return Err(ImportError::EmptyGeometry);
    }

    let mut preflight = StepPreflight::new(limits, model.warnings.len())?;
    preflight.add_nodes(1)?;
    let mut visited_parts = BTreeSet::new();
    let mut variant_keys = BTreeSet::new();
    for occurrence in &model.roots {
        let path = occurrence_path(&[], occurrence);
        preflight.walk_part(
            model,
            occurrence.part,
            0,
            &path,
            &mut visited_parts,
            &mut variant_keys,
        )?;
    }
    for part_index in 0..model.parts.len() {
        if !visited_parts.contains(&part_index) {
            preflight.walk_part(
                model,
                part_index,
                0,
                &[],
                &mut visited_parts,
                &mut variant_keys,
            )?;
        }
    }
    Ok(())
}

struct StepPreflight {
    limits: NativeImportLimits,
    nodes: usize,
    triangles: u64,
    estimated_bytes: usize,
    material_keys: BTreeSet<TriangleGroupKey>,
}

impl StepPreflight {
    fn new(limits: NativeImportLimits, warning_count: usize) -> Result<Self, ImportError> {
        let mut estimated_bytes = STEP_SCENE_ESTIMATE_BYTES;
        estimated_bytes = checked_add(
            estimated_bytes,
            warning_count
                .checked_mul(STEP_WARNING_ESTIMATE_BYTES)
                .ok_or_else(|| limit_error("STEP warning estimate overflowed"))?,
        )?;
        Ok(Self {
            limits,
            nodes: 0,
            triangles: 0,
            estimated_bytes,
            material_keys: BTreeSet::new(),
        })
    }

    fn walk_part(
        &mut self,
        model: &StepModel,
        start_part: usize,
        start_depth: usize,
        start_path: &[OccurrencePath],
        visited_parts: &mut BTreeSet<usize>,
        variant_keys: &mut BTreeSet<MeshVariantKey>,
    ) -> Result<(), ImportError> {
        let mut stack = vec![(start_part, start_depth, start_path.to_vec())];
        while let Some((part_index, depth, path)) = stack.pop() {
            if depth > MAX_STEP_HIERARCHY_DEPTH {
                return Err(limit_error(format!(
                    "STEP hierarchy depth {} exceeds limit {}",
                    depth, MAX_STEP_HIERARCHY_DEPTH
                )));
            }
            if depth > model.parts.len() {
                return Err(ImportError::InvalidSource(
                    "STEP occurrence hierarchy is cyclic".into(),
                ));
            }
            let part = model.parts.get(part_index).ok_or_else(|| {
                ImportError::InvalidSource(format!("STEP part {part_index} is missing"))
            })?;
            self.add_nodes(1)?;
            let has_mesh = part
                .shapes
                .iter()
                .any(|shape| !shape.mesh.triangles.is_empty());
            if has_mesh {
                self.add_nodes(1)?;
            }
            let overrides = occurrence_overrides(model, part_index, &path);
            let key = (part_index, overrides);
            if variant_keys.insert(key.clone()) {
                self.add_mesh(part, &key.1)?;
            }
            visited_parts.insert(part_index);
            for child in part.children.iter().rev() {
                stack.push((child.part, depth + 1, occurrence_path(&path, child)));
            }
        }
        Ok(())
    }

    fn add_nodes(&mut self, count: usize) -> Result<(), ImportError> {
        self.nodes = checked_add(self.nodes, count)?;
        if self.nodes > self.limits.max_nodes {
            return Err(limit_error(format!(
                "STEP projects at least {} nodes; limit is {}",
                self.nodes, self.limits.max_nodes
            )));
        }
        self.add_bytes(
            count
                .checked_mul(STEP_NODE_ESTIMATE_BYTES)
                .ok_or_else(|| limit_error("STEP node estimate overflowed"))?,
        )
    }

    fn add_mesh(
        &mut self,
        part: &oxideav_step::Part,
        overrides: &[(u64, TriangleGroupKey)],
    ) -> Result<(), ImportError> {
        self.add_bytes(STEP_MESH_ESTIMATE_BYTES)?;
        let mut part_triangles = 0_usize;
        for shape in &part.shapes {
            let triangles = shape.mesh.triangles.len();
            if triangles == 0 {
                continue;
            }
            part_triangles = checked_add(part_triangles, triangles)?;
            self.add_triangles(triangles)?;
            let colour_groups = triangle_group_keys_with_overrides(shape, overrides);
            self.add_bytes(
                colour_groups
                    .len()
                    .checked_mul(size_of::<SourcePrimitive>() + STEP_PRIMITIVE_ESTIMATE_BYTES)
                    .ok_or_else(|| limit_error("STEP primitive estimate overflowed"))?,
            )?;
            for colour in colour_groups {
                self.add_material(colour)?;
            }
            self.add_bytes(STEP_SHAPE_METADATA_ESTIMATE_BYTES)?;
        }
        let corners = part_triangles
            .checked_mul(3)
            .ok_or_else(|| limit_error("STEP vertex estimate overflowed"))?;
        self.add_bytes(
            corners
                .checked_mul(size_of::<[f32; 3]>() + size_of::<u32>())
                .ok_or_else(|| limit_error("STEP geometry estimate overflowed"))?,
        )?;
        Ok(())
    }

    fn add_triangles(&mut self, count: usize) -> Result<(), ImportError> {
        let count =
            u64::try_from(count).map_err(|_| limit_error("STEP triangle count overflowed"))?;
        self.triangles = self
            .triangles
            .checked_add(count)
            .ok_or_else(|| limit_error("STEP triangle count overflowed"))?;
        if self.triangles > self.limits.max_triangles {
            return Err(limit_error(format!(
                "STEP projects at least {} triangles; limit is {}",
                self.triangles, self.limits.max_triangles
            )));
        }
        Ok(())
    }

    fn add_material(&mut self, colour: TriangleGroupKey) -> Result<(), ImportError> {
        if self.material_keys.insert(colour) {
            self.add_bytes(size_of::<SourceMaterial>() + STEP_MATERIAL_ESTIMATE_BYTES)?;
        }
        Ok(())
    }

    fn add_bytes(&mut self, bytes: usize) -> Result<(), ImportError> {
        self.estimated_bytes = checked_add(self.estimated_bytes, bytes)?;
        if self.estimated_bytes > self.limits.max_memory_bytes {
            return Err(limit_error(format!(
                "STEP projects approximately {} bytes; limit is {}",
                self.estimated_bytes, self.limits.max_memory_bytes
            )));
        }
        Ok(())
    }
}

fn checked_add(left: usize, right: usize) -> Result<usize, ImportError> {
    left.checked_add(right)
        .ok_or_else(|| limit_error("STEP import budget overflowed"))
}

fn limit_error(message: impl Into<String>) -> ImportError {
    ImportError::LimitExceeded(message.into())
}

impl Projection<'_> {
    fn visit_occurrence(
        &mut self,
        occurrence: &Occurrence,
        parent: Option<&str>,
        parent_world: Option<DMat4>,
        depth: usize,
        path: &[OccurrencePath],
    ) -> Result<(), ImportError> {
        if depth > self.model.parts.len() {
            return Err(ImportError::InvalidSource(
                "STEP occurrence hierarchy is cyclic".into(),
            ));
        }
        let child_path = occurrence_path(path, occurrence);
        self.visit_part(
            occurrence.part,
            Some(occurrence),
            parent,
            parent_world,
            depth,
            &child_path,
        )
    }

    fn visit_part(
        &mut self,
        part_index: usize,
        occurrence: Option<&Occurrence>,
        parent: Option<&str>,
        parent_world: Option<DMat4>,
        depth: usize,
        path: &[OccurrencePath],
    ) -> Result<(), ImportError> {
        let (raw_name, product_id, definition, representation, children, item_id, layers) = {
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
                part.shapes.first().map(|shape| shape.item),
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
        let mesh = self.ensure_part_mesh(part_index, path)?;
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
                item_id,
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
                item_id,
                layers,
            });
        }
        for child in children {
            self.visit_occurrence(&child, Some(&node_id), Some(world), depth + 1, path)?;
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
                    pending.item_id,
                    &pending.layers,
                ),
            });
        }
        Ok(())
    }

    fn annotate_shared_instances(&mut self) {
        let mut mesh_counts = BTreeMap::<usize, usize>::new();
        for node in &self.nodes {
            if let Some(mesh) = node.mesh {
                *mesh_counts.entry(mesh).or_default() += 1;
            }
        }
        let instance_prefix = format!("{}/step/instance", self.asset_id);
        for node in &mut self.nodes {
            let Some(mesh) = node.mesh else {
                continue;
            };
            if mesh_counts.get(&mesh).copied().unwrap_or(0) <= 1 {
                continue;
            }
            let Some(step) = node
                .extras
                .pointer_mut("/ice3/step")
                .and_then(Value::as_object_mut)
            else {
                continue;
            };
            step.insert(
                "instanceGroup".into(),
                json!(format!("{instance_prefix}/{mesh}")),
            );
        }
    }

    fn ensure_part_mesh(
        &mut self,
        part_index: usize,
        path: &[OccurrencePath],
    ) -> Result<Option<usize>, ImportError> {
        let overrides = occurrence_overrides(self.model, part_index, path);
        let key = (part_index, overrides.clone());
        if let Some(mesh) = self.part_meshes.get(&key) {
            return Ok(Some(*mesh));
        }
        let part = self.model.parts.get(part_index).ok_or_else(|| {
            ImportError::InvalidSource(format!("STEP part {part_index} is missing"))
        })?;
        if part.shapes.is_empty() {
            return Ok(None);
        }
        let mut primitives = Vec::new();
        let mut primitive_positions = Vec::new();
        let mut shape_metadata = Vec::new();
        for shape in &part.shapes {
            if shape.mesh.triangles.is_empty() {
                continue;
            }
            let positions = convert_positions(shape, self.model.length_unit_metres)?;
            for (colour, group_positions, indices) in
                grouped_triangle_positions(shape, &positions, &overrides)?
            {
                let material = self.material_for(colour);
                primitives.push(SourcePrimitive {
                    positions: Vec::new(),
                    // The AP242 reader exposes no authored normal buffer. The
                    // canonical writer generates normals from the triangles.
                    normals: None,
                    colors: None,
                    indices,
                    material: Some(material),
                });
                primitive_positions.push(group_positions);
            }
            shape_metadata.push(step_shape_metadata(shape));
        }
        if primitives.is_empty() {
            return Ok(None);
        }
        let mesh_index = self.meshes.len();
        let mesh_id = if overrides.is_empty() {
            format!("{}/step/mesh/{part_index}", self.asset_id)
        } else {
            format!(
                "{}/step/mesh/{part_index}/variant/{mesh_index}",
                self.asset_id
            )
        };
        self.meshes.push(SourceMesh {
            id: mesh_id,
            primitives,
            extras: step_mesh_extras(part.representation, shape_metadata),
        });
        self.mesh_positions.insert(mesh_index, primitive_positions);
        self.part_meshes.insert(key, mesh_index);
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

type CompactTriangleGroup = ([f32; 4], Vec<[f64; 3]>, Vec<u32>);

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
    positions: &[[f64; 3]],
    overrides: &[(u64, TriangleGroupKey)],
) -> Result<Vec<CompactTriangleGroup>, ImportError> {
    let mut groups = BTreeMap::<TriangleGroupKey, Vec<u32>>::new();
    let vertex_count = shape.mesh.positions.len();
    for (triangle_index, triangle) in shape.mesh.triangles.iter().enumerate() {
        let face = shape.triangle_faces.get(triangle_index).copied();
        let colour = override_colour(shape, face, overrides)
            .map(TriangleGroupKey::rgba)
            .or_else(|| {
                shape
                    .triangle_colours
                    .get(triangle_index)
                    .copied()
                    .flatten()
            })
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

fn triangle_group_keys_with_overrides(
    shape: &Shape,
    overrides: &[(u64, TriangleGroupKey)],
) -> BTreeSet<TriangleGroupKey> {
    let mut colours = BTreeSet::new();
    for (triangle_index, _) in shape.mesh.triangles.iter().enumerate() {
        let face = shape.triangle_faces.get(triangle_index).copied();
        let colour = override_colour(shape, face, overrides)
            .map(TriangleGroupKey::rgba)
            .or_else(|| {
                shape
                    .triangle_colours
                    .get(triangle_index)
                    .copied()
                    .flatten()
            })
            .or(shape.colour)
            .unwrap_or(DEFAULT_COLOR);
        colours.insert(colour.into());
    }
    colours
}

fn override_colour(
    shape: &Shape,
    face: Option<u64>,
    overrides: &[(u64, TriangleGroupKey)],
) -> Option<TriangleGroupKey> {
    face.and_then(|face| {
        overrides
            .iter()
            .find(|(item, _)| *item == face)
            .map(|(_, colour)| *colour)
    })
    .or_else(|| {
        overrides
            .iter()
            .find(|(item, _)| *item == shape.item)
            .map(|(_, colour)| *colour)
    })
}

fn path_matches(style_path: &[u64], path: &[OccurrencePath]) -> bool {
    style_path.len() == path.len()
        && path
            .iter()
            .all(|level| style_path.iter().any(|value| level.contains(value)))
}

fn occurrence_path(path: &[OccurrencePath], occurrence: &Occurrence) -> Vec<OccurrencePath> {
    if occurrence.placed_by.is_none() && occurrence.id.is_none() {
        return path.to_vec();
    }
    let left = occurrence.placed_by.unwrap_or(u64::MAX);
    let right = occurrence.id.unwrap_or(left);
    let mut child_path = path.to_vec();
    child_path.push([left, right]);
    child_path
}

fn occurrence_overrides(
    model: &StepModel,
    part_index: usize,
    path: &[OccurrencePath],
) -> Vec<(u64, TriangleGroupKey)> {
    let Some(part) = model.parts.get(part_index) else {
        return Vec::new();
    };
    let mut overrides = model
        .occurrence_colours
        .iter()
        .filter(|override_colour| {
            path_matches(&override_colour.path, path)
                && part.shapes.iter().any(|shape| {
                    shape.item == override_colour.item
                        || shape.triangle_faces.contains(&override_colour.item)
                })
        })
        .map(|override_colour| {
            (
                override_colour.item,
                TriangleGroupKey::from(override_colour.colour),
            )
        })
        .collect::<Vec<_>>();
    overrides.sort_unstable();
    overrides.dedup();
    overrides
}

fn convert_positions(shape: &Shape, unit_scale: f64) -> Result<Vec<[f64; 3]>, ImportError> {
    shape
        .mesh
        .positions
        .iter()
        .map(|point| {
            let converted = [
                point[0] * unit_scale,
                point[2] * unit_scale,
                -point[1] * unit_scale,
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
    meshes: &mut [SourceMesh],
    mesh_positions: &mut BTreeMap<usize, Vec<Vec<[f64; 3]>>>,
    worlds: &BTreeMap<String, DMat4>,
) -> Result<[f64; 3], ImportError> {
    let mesh_anchors = mesh_positions
        .iter()
        .map(|(mesh_index, primitives)| {
            let anchor = primitives
                .iter()
                .find_map(|positions| positions.first().copied())
                .ok_or_else(|| {
                    ImportError::InvalidSource(format!(
                        "STEP mesh {mesh_index} contains no positions"
                    ))
                })?;
            Ok::<_, ImportError>((*mesh_index, DVec3::from_array(anchor)))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
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
        let Some(primitives) = mesh_positions.get(&mesh_index) else {
            return Err(ImportError::InvalidSource(
                "STEP node references a missing mesh".into(),
            ));
        };
        for positions in primitives {
            for position in positions {
                let point = world.transform_point3(DVec3::from_array(*position));
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
    let origin_vec = DVec3::from_array(origin);
    let by_node = nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    let adjusted_worlds = worlds
        .iter()
        .map(|(id, world)| {
            let adjusted = by_node
                .get(id.as_str())
                .and_then(|node| node.mesh)
                .and_then(|mesh_index| mesh_anchors.get(&mesh_index))
                .map_or_else(
                    || DMat4::from_translation(-origin_vec) * *world,
                    |anchor| {
                        DMat4::from_translation(-origin_vec)
                            * *world
                            * DMat4::from_translation(*anchor)
                    },
                );
            (id.clone(), adjusted)
        })
        .collect::<BTreeMap<_, _>>();
    let root_id = nodes
        .iter()
        .find(|node| node.parent.is_none())
        .map(|node| node.id.clone());
    let mut serialized_worlds = BTreeMap::<String, DMat4>::new();
    for node in nodes.iter_mut() {
        let parent_world = node
            .parent
            .as_ref()
            .and_then(|parent| serialized_worlds.get(parent))
            .copied()
            .unwrap_or(DMat4::IDENTITY);
        let is_identity_root_child = node.parent == root_id
            && worlds
                .get(&node.id)
                .is_some_and(|world| world.abs_diff_eq(DMat4::IDENTITY, 1e-12));
        let final_world = if node.parent.is_none() || is_identity_root_child {
            DMat4::IDENTITY
        } else {
            adjusted_worlds
                .get(&node.id)
                .copied()
                .unwrap_or(DMat4::IDENTITY)
        };
        let local = parent_world.inverse() * final_world;
        node.matrix = local.to_cols_array().map(|value| value as f32);
        serialized_worlds.insert(node.id.clone(), final_world);
    }

    for (mesh_index, primitives) in mesh_positions.iter() {
        let anchor = mesh_anchors[mesh_index];
        let mesh = meshes.get_mut(*mesh_index).ok_or_else(|| {
            ImportError::InvalidSource(format!("STEP mesh {mesh_index} is missing"))
        })?;
        if mesh.primitives.len() != primitives.len() {
            return Err(ImportError::InvalidSource(format!(
                "STEP mesh {mesh_index} primitive count changed during rebasing"
            )));
        }
        for (primitive, positions) in mesh.primitives.iter_mut().zip(primitives) {
            primitive.positions = positions
                .iter()
                .map(|point| {
                    let rebased = DVec3::from_array(*point) - anchor;
                    [rebased.x as f32, rebased.y as f32, rebased.z as f32]
                })
                .collect();
            if primitive
                .positions
                .iter()
                .flatten()
                .any(|value| !value.is_finite())
            {
                return Err(ImportError::InvalidSource(
                    "STEP rebased positions contain non-finite values".into(),
                ));
            }
        }
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
    item_id: Option<u64>,
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
    if let Some(item_id) = item_id {
        metadata.insert("itemId".into(), json!(item_id));
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
    use oxideav_step::{ApSchema, OccurrenceColour, Part, Shape, Transform, TriMesh};

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

    fn colour_close(left: [f32; 3], right: [f32; 3]) -> bool {
        left.iter()
            .zip(right)
            .all(|(left, right)| (*left - right).abs() <= 1e-5)
    }

    fn amplified_model(child_count: usize) -> StepModel {
        let mut parent = Part {
            name: Some("Root".into()),
            product_id: Some("Root".into()),
            ..Part::default()
        };
        parent.children = (0..child_count)
            .map(|index| Occurrence {
                part: 1,
                id: Some(100 + index as u64),
                name: Some(format!("Pump {index}")),
                transform: Transform::IDENTITY,
                placed_by: None,
            })
            .collect();
        let mut child = Part {
            name: Some("Pump".into()),
            product_id: Some("Pump".into()),
            ..Part::default()
        };
        child
            .shapes
            .push(triangle_shape(1, [0.0, 0.0, 0.0], [1.0, 0.0, 0.0]));
        StepModel {
            schema: ApSchema::Ap242,
            header: Default::default(),
            length_unit_metres: 1.0,
            parts: vec![parent, child],
            roots: vec![Occurrence {
                part: 0,
                id: None,
                name: None,
                transform: Transform::IDENTITY,
                placed_by: None,
            }],
            occurrence_colours: Vec::new(),
            warnings: Vec::new(),
        }
    }

    #[test]
    fn preflight_rejects_low_node_budget_before_projection() {
        let model = amplified_model(1);
        let limits = NativeImportLimits {
            max_nodes: 3,
            ..NativeImportLimits::default()
        };
        let error = project_model(&model, "step-preflight", limits).unwrap_err();
        assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_LIMIT_EXCEEDED");
    }

    #[test]
    fn preflight_rejects_occurrence_amplification() {
        let model = amplified_model(32);
        let limits = NativeImportLimits {
            max_nodes: 16,
            ..NativeImportLimits::default()
        };
        let error = preflight_model(&model, limits).unwrap_err();
        assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_LIMIT_EXCEEDED");
        assert!(error.to_string().contains("projects at least"));
    }

    #[test]
    fn preflight_rejects_low_memory_budget_before_projection() {
        let model = amplified_model(1);
        let limits = NativeImportLimits {
            max_memory_bytes: STEP_SCENE_ESTIMATE_BYTES,
            ..NativeImportLimits::default()
        };
        let error = preflight_model(&model, limits).unwrap_err();
        assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_LIMIT_EXCEEDED");
        assert!(error.to_string().contains("approximately"));
    }

    #[test]
    fn preflight_accepts_many_triangles_with_one_material() {
        let mut shape = triangle_shape(1, [0.0, 0.0, 0.0], [1.0, 0.0, 0.0]);
        shape.mesh.triangles = vec![[0, 1, 2]; 120_000];
        shape.triangle_faces = vec![1; 120_000];
        let mut part = Part {
            name: Some("Dense Part".into()),
            product_id: Some("Dense Part".into()),
            ..Part::default()
        };
        part.shapes.push(shape);
        let model = StepModel {
            schema: ApSchema::Ap242,
            header: Default::default(),
            length_unit_metres: 1.0,
            parts: vec![part],
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
        let limits = NativeImportLimits {
            max_memory_bytes: 16 * 1024 * 1024,
            ..NativeImportLimits::default()
        };

        preflight_model(&model, limits).unwrap();
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
        assert_eq!(
            pump_nodes[0].extras["ice3"]["step"]["instanceGroup"],
            pump_nodes[1].extras["ice3"]["step"]["instanceGroup"]
        );
        assert_eq!(
            pump_nodes[0].extras["ice3"]["step"]["instanceGroup"],
            "step-instances/step/instance/0"
        );
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
            children: vec![
                Occurrence {
                    part: 1,
                    id: Some(10),
                    name: Some("Offsite Pump A".into()),
                    transform: Transform {
                        cols: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
                        translation: [large_origin, 0.0, 0.0],
                    },
                    placed_by: None,
                },
                Occurrence {
                    part: 1,
                    id: Some(11),
                    name: Some("Offsite Pump B".into()),
                    transform: Transform {
                        cols: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
                        translation: [large_origin + 1.0, 0.0, 0.0],
                    },
                    placed_by: None,
                },
            ],
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
        let mut occurrence_nodes = scene
            .nodes
            .iter()
            .filter(|node| node.parent.is_some() && node.mesh.is_none())
            .filter(|node| node.name.starts_with("Offsite Pump"))
            .collect::<Vec<_>>();
        occurrence_nodes.sort_by(|left, right| left.matrix[12].total_cmp(&right.matrix[12]));
        assert_eq!(occurrence_nodes.len(), 2);
        assert!(
            occurrence_nodes[0].matrix[12].abs() < 1e-5,
            "pump A translation: {}",
            occurrence_nodes[0].matrix[12]
        );
        assert!(
            (occurrence_nodes[1].matrix[12] - 1.0).abs() < 1e-5,
            "pump B translation: {}",
            occurrence_nodes[1].matrix[12]
        );
    }

    #[test]
    fn preserves_adjacent_source_positions_until_after_origin_subtraction() {
        let large = 16_777_216.0;
        let mut part = Part {
            name: Some("Adjacent".into()),
            product_id: Some("Adjacent".into()),
            ..Part::default()
        };
        part.shapes.push(triangle_shape(
            1,
            [large, 0.0, 0.0],
            [large + 1.0, 0.0, 0.0],
        ));
        let model = StepModel {
            schema: ApSchema::Ap242,
            header: Default::default(),
            length_unit_metres: 1.0,
            parts: vec![part],
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

        let scene = project_model(&model, "step-adjacent", NativeImportLimits::default()).unwrap();
        let positions = &scene.meshes[0].primitives[0].positions;
        assert!((positions[0][0] - 0.0).abs() < 1e-6);
        assert!((positions[1][0] - 1.0).abs() < 1e-6);
        assert_eq!(scene.asset_extras["ice3"]["step"]["sourceOrigin"][0], large);
    }

    #[test]
    fn applies_occurrence_specific_colours_as_mesh_variants() {
        let mut child = Part {
            name: Some("Pump".into()),
            product_id: Some("Pump".into()),
            ..Part::default()
        };
        child
            .shapes
            .push(triangle_shape(1, [0.0, 0.0, 0.0], [1.0, 0.0, 0.0]));
        let mut root = Part {
            name: Some("Root".into()),
            product_id: Some("Root".into()),
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
            occurrence_colours: vec![
                OccurrenceColour {
                    path: vec![10],
                    item: 1,
                    colour: [1.0, 0.0, 0.0, 1.0],
                },
                OccurrenceColour {
                    path: vec![11],
                    item: 1,
                    colour: [0.0, 1.0, 0.0, 1.0],
                },
            ],
            warnings: Vec::new(),
        };

        let scene = project_model(&model, "step-overrides", NativeImportLimits::default()).unwrap();
        let mesh_nodes = scene
            .nodes
            .iter()
            .filter(|node| node.mesh.is_some())
            .collect::<Vec<_>>();
        assert_eq!(mesh_nodes.len(), 2);
        assert_ne!(mesh_nodes[0].mesh, mesh_nodes[1].mesh);
        let mesh_ids = scene
            .meshes
            .iter()
            .map(|mesh| mesh.id.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(mesh_ids.len(), 2);
        let colours = mesh_nodes
            .iter()
            .map(|node| {
                let mesh = node.mesh.unwrap();
                let material = scene.meshes[mesh].primitives[0].material.unwrap();
                scene.materials[material].base_color
            })
            .collect::<Vec<_>>();
        assert_eq!(colours.len(), 2);
        assert!(
            colours
                .iter()
                .any(|colour| colour_close(*colour, [1.0, 0.0, 0.0]))
        );
        assert!(
            colours
                .iter()
                .any(|colour| colour_close(*colour, [0.0, 1.0, 0.0]))
        );
    }

    #[test]
    fn face_override_wins_over_shape_override_for_the_same_occurrence() {
        let mut child = Part {
            name: Some("Pump".into()),
            product_id: Some("Pump".into()),
            ..Part::default()
        };
        child.shapes.push(Shape {
            item: 5,
            name: Some("Styled pump".into()),
            mesh: TriMesh {
                positions: vec![
                    [0.0, 0.0, 0.0],
                    [1.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0],
                    [2.0, 0.0, 0.0],
                    [3.0, 0.0, 0.0],
                    [2.0, 1.0, 0.0],
                ],
                triangles: vec![[0, 1, 2], [3, 4, 5]],
            },
            colour: Some([0.2, 0.4, 0.6, 1.0]),
            triangle_colours: Vec::new(),
            triangle_faces: vec![105, 106],
            layers: Vec::new(),
        });
        let mut root = Part {
            name: Some("Root".into()),
            product_id: Some("Root".into()),
            children: vec![Occurrence {
                part: 1,
                id: Some(10),
                name: Some("Pump A".into()),
                transform: Transform::IDENTITY,
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
            occurrence_colours: vec![
                OccurrenceColour {
                    path: vec![10],
                    item: 5,
                    colour: [1.0, 0.0, 0.0, 1.0],
                },
                OccurrenceColour {
                    path: vec![10],
                    item: 105,
                    colour: [0.0, 1.0, 0.0, 1.0],
                },
            ],
            warnings: Vec::new(),
        };

        let scene = project_model(
            &model,
            "step-override-precedence",
            NativeImportLimits::default(),
        )
        .unwrap();
        assert_eq!(scene.meshes.len(), 1);
        assert_eq!(scene.meshes[0].primitives.len(), 2);
        let colours = scene.meshes[0]
            .primitives
            .iter()
            .map(|primitive| {
                let material = primitive.material.unwrap();
                scene.materials[material].base_color
            })
            .collect::<Vec<_>>();
        assert!(
            colours
                .iter()
                .any(|colour| colour_close(*colour, [1.0, 0.0, 0.0]))
        );
        assert!(
            colours
                .iter()
                .any(|colour| colour_close(*colour, [0.0, 1.0, 0.0]))
        );
        assert!(
            !colours
                .iter()
                .any(|colour| colour_close(*colour, [0.2, 0.4, 0.6]))
        );
    }

    #[test]
    fn preflight_counts_occurrence_colour_mesh_variants() {
        let mut child = Part {
            name: Some("Pump".into()),
            product_id: Some("Pump".into()),
            ..Part::default()
        };
        child
            .shapes
            .push(triangle_shape(1, [0.0, 0.0, 0.0], [1.0, 0.0, 0.0]));
        let mut root = Part {
            name: Some("Root".into()),
            product_id: Some("Root".into()),
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
            occurrence_colours: vec![
                OccurrenceColour {
                    path: vec![10],
                    item: 1,
                    colour: [1.0, 0.0, 0.0, 1.0],
                },
                OccurrenceColour {
                    path: vec![11],
                    item: 1,
                    colour: [0.0, 1.0, 0.0, 1.0],
                },
            ],
            warnings: Vec::new(),
        };
        let limits = NativeImportLimits {
            max_triangles: 1,
            ..NativeImportLimits::default()
        };

        let error = preflight_model(&model, limits).unwrap_err();
        assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_LIMIT_EXCEEDED");
        assert!(error.to_string().contains("projects at least 2 triangles"));
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
        let groups = grouped_triangle_positions(&shape, &positions, &[]).unwrap();

        assert_eq!(groups.len(), 2);
        assert!(groups.iter().all(|(_, positions, indices)| {
            positions.len() == 3
                && indices
                    .iter()
                    .all(|index| (*index as usize) < positions.len())
        }));
    }

    #[test]
    fn repeated_occurrences_do_not_copy_complete_item_id_arrays() {
        let mut root = Part {
            name: Some("Root".into()),
            product_id: Some("Root".into()),
            children: (0..64)
                .map(|index| Occurrence {
                    part: 1,
                    id: Some(1_000 + index),
                    name: Some(format!("Pump {index}")),
                    transform: Transform::IDENTITY,
                    placed_by: None,
                })
                .collect(),
            ..Part::default()
        };
        root.product_id = Some("Root".into());
        let mut child = Part {
            name: Some("Pump".into()),
            product_id: Some("Pump".into()),
            ..Part::default()
        };
        child.shapes = (0..4_096)
            .map(|item| {
                let mut shape = triangle_shape(item, [0.0, 0.0, 0.0], [1.0, 0.0, 0.0]);
                if item != 0 {
                    shape.mesh.triangles.clear();
                }
                shape
            })
            .collect();
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
            project_model(&model, "step-item-metadata", NativeImportLimits::default()).unwrap();
        let step_extras = scene
            .nodes
            .iter()
            .filter_map(|node| node.extras.pointer("/ice3/step"))
            .collect::<Vec<_>>();
        assert!(
            step_extras
                .iter()
                .any(|extras| extras.get("itemId").is_some())
        );
        assert!(
            step_extras
                .iter()
                .all(|extras| extras.get("itemIds").is_none())
        );
    }

    #[test]
    fn rejects_excessively_deep_hierarchy_before_recursive_projection() {
        let mut parts = (0..=MAX_STEP_HIERARCHY_DEPTH + 1)
            .map(|index| Part {
                name: Some(format!("Depth {index}")),
                product_id: Some(format!("Depth {index}")),
                ..Part::default()
            })
            .collect::<Vec<_>>();
        for index in 0..parts.len() - 1 {
            parts[index].children.push(Occurrence {
                part: index + 1,
                id: Some(index as u64 + 1),
                name: Some(format!("Child {index}")),
                transform: Transform::IDENTITY,
                placed_by: None,
            });
        }
        parts
            .last_mut()
            .unwrap()
            .shapes
            .push(triangle_shape(1, [0.0, 0.0, 0.0], [1.0, 0.0, 0.0]));
        let model = StepModel {
            schema: ApSchema::Ap242,
            header: Default::default(),
            length_unit_metres: 1.0,
            parts,
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

        let error = project_model(&model, "step-deep-hierarchy", NativeImportLimits::default())
            .unwrap_err();
        assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_LIMIT_EXCEEDED");
        assert!(error.to_string().contains("hierarchy depth"));
    }
}
