use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use glam::{DMat4, DVec4};
use oxideav_step::{
    ApSchema, Error as StepError, GeometryLimits, Occurrence, ReadOptions, Shape, StepLimits,
    StepModel, Tolerance,
};
use serde_json::json;

use crate::{
    ImportError, ImportRequest, ImportWarning, NativeImportLimits, SourceCapabilities,
    SourceFormat, SourceImporter, SourceMaterial, SourceMesh, SourceNode, SourcePrimitive,
    SourceScene,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct StepImporter;

impl StepImporter {
    pub const fn new() -> Self {
        Self
    }

    pub fn import_file(&self, path: &Path, asset_id: &str) -> Result<SourceScene, ImportError> {
        self.import_file_with_limits(path, asset_id, NativeImportLimits::default())
    }

    pub fn import_file_with_limits(
        &self,
        path: &Path,
        asset_id: &str,
        limits: NativeImportLimits,
    ) -> Result<SourceScene, ImportError> {
        self.import_with_limits(
            &ImportRequest {
                path: path.to_path_buf(),
                asset_id: asset_id.into(),
                requested_adapter: Some("step".into()),
            },
            limits,
        )
    }

    pub fn import_with_limits(
        &self,
        request: &ImportRequest,
        limits: NativeImportLimits,
    ) -> Result<SourceScene, ImportError> {
        let asset_id = request.asset_id.trim();
        if asset_id.is_empty() {
            return Err(ImportError::InvalidSource(
                "asset ID must not be empty".into(),
            ));
        }
        let bytes = read_bounded_source(&request.path, limits.max_memory_bytes)?;
        let model =
            oxideav_step::read_step_with(&bytes, &read_options(limits)?).map_err(map_step_error)?;
        if model.schema != ApSchema::Ap242 {
            return Err(ImportError::SourceDiagnostic {
                code: "ICE3_STUDIO_IMPORT_STEP_SCHEMA_UNSUPPORTED".into(),
                message: format!("unsupported STEP schema: {:?}", model.schema),
            });
        }
        project_model(&model, asset_id, limits)
    }
}

impl SourceImporter for StepImporter {
    fn id(&self) -> &'static str {
        "step"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &[".step", ".stp"]
    }

    fn capabilities(&self) -> SourceCapabilities {
        SourceCapabilities {
            hierarchy: true,
            vertex_colors: false,
            material_colors: true,
            instances: true,
            semantic_metadata: true,
        }
    }

    fn import(&self, request: &ImportRequest) -> Result<SourceScene, ImportError> {
        self.import_with_limits(request, NativeImportLimits::default())
    }
}

fn read_bounded_source(path: &Path, max_bytes: usize) -> Result<Vec<u8>, ImportError> {
    let metadata = std::fs::metadata(path)?;
    if metadata.len() > max_bytes as u64 {
        return Err(ImportError::LimitExceeded(format!(
            "STEP input has {} bytes; limit is {max_bytes}",
            metadata.len()
        )));
    }
    let bytes = std::fs::read(path)?;
    if bytes.len() > max_bytes {
        return Err(ImportError::LimitExceeded(format!(
            "STEP input has {} bytes; limit is {max_bytes}",
            bytes.len()
        )));
    }
    Ok(bytes)
}

fn read_options(limits: NativeImportLimits) -> Result<ReadOptions, ImportError> {
    Ok(ReadOptions {
        parse_limits: StepLimits {
            max_input_len: limits.max_memory_bytes,
            max_instances: limits.max_nodes,
            ..Default::default()
        },
        geometry_limits: GeometryLimits {
            max_triangles: usize::try_from(limits.max_triangles)
                .map_err(|_| ImportError::LimitExceeded("STEP triangle budget".into()))?,
            ..Default::default()
        },
        tolerance: Tolerance {
            relative: 0.001,
            absolute: None,
            max_angle: 0.5,
        },
        keep_invisible: false,
    })
}

fn map_step_error(error: StepError) -> ImportError {
    match error {
        StepError::LimitExceeded(message) => ImportError::LimitExceeded(message),
        StepError::NoGeometry(_) => ImportError::EmptyGeometry,
        StepError::Parse(_) => ImportError::InvalidSource(error.to_string()),
    }
}

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

fn project_model(
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
                    "originalUnitMeters": model.length_unit_metres
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
        let (raw_name, product_id, definition, representation, children) = {
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
                        "occurrenceId": occurrence.and_then(|value| value.id)
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
        let shapes = part.shapes.clone();
        let mut primitives = Vec::new();
        for shape in &shapes {
            if shape.mesh.triangles.is_empty() {
                continue;
            }
            let (positions, indices) = convert_mesh(shape, self.model.length_unit_metres)?;
            let material = self.material_for(shape.colour.unwrap_or([0.72, 0.74, 0.76, 1.0]));
            primitives.push(SourcePrimitive {
                positions,
                normals: None,
                colors: None,
                indices,
                material: Some(material),
            });
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
                        "representationId": part.representation
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

fn convert_mesh(shape: &Shape, unit_scale: f64) -> Result<(Vec<[f32; 3]>, Vec<u32>), ImportError> {
    let positions = shape
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
        .collect::<Result<Vec<_>, _>>()?;
    let indices = shape
        .mesh
        .triangles
        .iter()
        .flatten()
        .map(|index| {
            let index = *index;
            if usize::try_from(index)
                .ok()
                .is_some_and(|value| value < positions.len())
            {
                Ok(index)
            } else {
                Err(ImportError::InvalidSource(
                    "STEP index is outside the vertex buffer".into(),
                ))
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok((positions, indices))
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
