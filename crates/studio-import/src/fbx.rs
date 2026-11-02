use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use fbx_rs::syntax::{AsciiValue, Property};
use fbx_rs::{
    Error as DecoderError, Limits as DecoderLimits, Mesh, Node, Scene, UnsupportedKind,
    load_with_limits,
};
use serde_json::{Map, Value, json};

use crate::{
    ImportError, ImportRequest, ImportWarning, NativeImportLimits, SourceCapabilities,
    SourceFormat, SourceImporter, SourceMaterial, SourceMesh, SourceNode, SourcePrimitive,
    SourceScene,
};

const MAX_METADATA_ARRAY_LENGTH: usize = 256;
const MAX_METADATA_DEPTH: usize = 8;

#[derive(Debug, Clone, Copy, Default)]
pub struct FbxImporter;

impl FbxImporter {
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
                requested_adapter: Some("fbx".into()),
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
        let metadata = std::fs::metadata(&request.path)?;
        if metadata.len() > limits.max_memory_bytes as u64 {
            return Err(ImportError::LimitExceeded(format!(
                "FBX input has {} bytes; limit is {}",
                metadata.len(),
                limits.max_memory_bytes
            )));
        }
        let bytes = std::fs::read(&request.path)?;
        if bytes.len() > limits.max_memory_bytes {
            return Err(ImportError::LimitExceeded(format!(
                "FBX input has {} bytes; limit is {}",
                bytes.len(),
                limits.max_memory_bytes
            )));
        }
        let metadata = document_metadata(&bytes, limits)?;
        let scene = load_with_limits(&bytes, &Default::default(), &decoder_limits(limits))
            .map_err(map_decoder_error)?;
        if scene.nodes().len() > limits.max_nodes {
            return Err(ImportError::LimitExceeded(format!(
                "FBX has {} nodes; limit is {}",
                scene.nodes().len(),
                limits.max_nodes
            )));
        }
        let triangle_count = scene
            .meshes()
            .iter()
            .map(|mesh| mesh.triangles().len() / 3)
            .try_fold(0_u64, |total, count| {
                total.checked_add(u64::try_from(count).ok()?)
            })
            .ok_or_else(|| ImportError::LimitExceeded("FBX triangle count overflowed".into()))?;
        if triangle_count == 0 {
            return Err(ImportError::EmptyGeometry);
        }
        if triangle_count > limits.max_triangles {
            return Err(ImportError::LimitExceeded(format!(
                "FBX has {triangle_count} triangles; limit is {}",
                limits.max_triangles
            )));
        }
        import_scene(&scene, &metadata, asset_id)
    }
}

impl SourceImporter for FbxImporter {
    fn id(&self) -> &'static str {
        "fbx"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &[".fbx"]
    }

    fn capabilities(&self) -> SourceCapabilities {
        SourceCapabilities {
            hierarchy: true,
            vertex_colors: true,
            material_colors: true,
            instances: true,
            semantic_metadata: true,
        }
    }

    fn import(&self, request: &ImportRequest) -> Result<SourceScene, ImportError> {
        self.import_with_limits(request, NativeImportLimits::default())
    }
}

#[derive(Debug, Default)]
struct ModelMetadata {
    name: String,
    object_type: String,
    custom_properties: Map<String, Value>,
}

#[derive(Debug)]
struct MaterialMetadata {
    name: String,
}

#[derive(Debug, Default)]
struct DocumentMetadata {
    models: BTreeMap<i64, ModelMetadata>,
    materials: BTreeMap<i64, MaterialMetadata>,
}

fn decoder_limits(limits: NativeImportLimits) -> DecoderLimits {
    let defaults = DecoderLimits::default();
    DecoderLimits {
        max_input_bytes: limits.max_memory_bytes,
        max_nodes: limits.max_nodes,
        max_objects: limits.max_nodes,
        max_meshes: limits.max_nodes,
        max_allocation: limits.max_memory_bytes,
        max_decompressed_bytes: limits.max_memory_bytes,
        ..defaults
    }
}

fn import_scene(
    scene: &Scene,
    metadata: &DocumentMetadata,
    asset_id: &str,
) -> Result<SourceScene, ImportError> {
    let mut warnings = BTreeSet::new();
    add_resource_warnings(scene, &mut warnings);

    let mut mesh_names = scene
        .meshes()
        .iter()
        .map(|mesh| clean_mesh_name(mesh.name()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    mesh_names.sort();
    let instance_groups = mesh_names
        .iter()
        .enumerate()
        .map(|(index, name)| (name.clone(), format!("fbx/mesh-data/{index}")))
        .collect::<BTreeMap<_, _>>();

    let meshes = scene
        .meshes()
        .iter()
        .enumerate()
        .map(|(index, mesh)| import_mesh(mesh, index, asset_id))
        .collect::<Result<Vec<_>, _>>()?;
    let materials = scene
        .materials()
        .iter()
        .enumerate()
        .map(|(index, material)| {
            let id = format!("{asset_id}/material/{index:06}");
            let color_map = material
                .pbr_map("base_color")
                .filter(|map| map.has_value && map.value_components >= 3)
                .or_else(|| {
                    material
                        .fbx_map("diffuse_color")
                        .filter(|map| map.has_value && map.value_components >= 3)
                });
            let base_color = if let Some(map) = color_map {
                [
                    finite_f32(map.value[0], "material red")?,
                    finite_f32(map.value[1], "material green")?,
                    finite_f32(map.value[2], "material blue")?,
                ]
            } else {
                [1.0; 3]
            };
            let opacity = if let Some(map) = material
                .pbr_map("opacity")
                .filter(|map| map.has_value && map.value_components >= 1)
            {
                finite_f32(map.value[0], "material opacity")?
            } else if let Some(map) = material
                .fbx_map("transparency_factor")
                .filter(|map| map.has_value && map.value_components >= 1)
            {
                1.0 - finite_f32(map.value[0], "material transparency")?
            } else {
                1.0
            }
            .clamp(0.0, 1.0);
            Ok(SourceMaterial {
                id: id.clone(),
                name: material
                    .source_id()
                    .and_then(|source_id| metadata.materials.get(&source_id))
                    .map(|metadata| clean_authored_name(&metadata.name))
                    .unwrap_or_else(|| clean_authored_name(material.name())),
                base_color,
                opacity,
                extras: json!({
                    "ice3": {
                        "id": id,
                        "fbx": {
                            "shadingModel": material.shading_model()
                        }
                    }
                }),
            })
        })
        .collect::<Result<Vec<_>, ImportError>>()?;

    let bounds = world_bounds(scene)?;
    let origin = [bounds.min[0], 0.0, bounds.min[2]];
    let nodes = scene
        .nodes()
        .iter()
        .enumerate()
        .map(|(index, node)| {
            let id = format!("node-{index:06}");
            let source_metadata = node.source_id().and_then(|id| metadata.models.get(&id));
            let name = clean_node_name(node, index, source_metadata);
            let mut local_matrix = node.node_to_parent().0;
            if index == 0 {
                local_matrix[9] -= origin[0];
                local_matrix[10] -= origin[1];
                local_matrix[11] -= origin[2];
            }
            let matrix = matrix_3x4_to_4x4(local_matrix)?;
            let mesh = node.mesh();
            let mesh_name =
                mesh.map(|mesh_index| clean_mesh_name(scene.meshes()[mesh_index].name()));
            let instance_group = mesh_name
                .as_ref()
                .and_then(|name| instance_groups.get(name))
                .cloned();
            let object_type = if mesh.is_some() {
                "MESH"
            } else {
                source_metadata
                    .map(|metadata| metadata.object_type.as_str())
                    .unwrap_or("EMPTY")
            };
            Ok(SourceNode {
                id: id.clone(),
                name: name.clone(),
                parent: node.parent().map(|parent| format!("node-{parent:06}")),
                matrix,
                mesh,
                material: None,
                extras: json!({
                    "ice3": {
                        "id": id,
                        "selectionId": format!("{asset_id}/fbx/node/{index}"),
                        "displayName": name,
                        "fbx": {
                            "objectType": object_type,
                            "meshData": mesh_name,
                            "instanceGroup": instance_group,
                            "customProperties": source_metadata
                                .map(|metadata| metadata.custom_properties.clone())
                                .unwrap_or_default()
                        }
                    }
                }),
            })
        })
        .collect::<Result<Vec<_>, ImportError>>()?;
    if !nodes.iter().any(|node| node.mesh.is_some()) {
        return Err(ImportError::EmptyGeometry);
    }

    Ok(SourceScene {
        asset_id: asset_id.into(),
        source_format: SourceFormat::Fbx,
        nodes,
        meshes,
        materials,
        warnings: warnings.into_iter().collect(),
        asset_extras: json!({
            "coordinateSystem": {
                "handedness": "right",
                "upAxis": "y",
                "linearUnit": "meter"
            },
            "ice3": {
                "fbx": {
                    "converter": "native-rust",
                    "version": scene.settings().version(),
                    "ascii": scene.settings().is_ascii(),
                    "sourceOrigin": origin,
                    "unitAssumption": "meter",
                    "originalUnitMeters": finite_json(scene.settings().original_unit_meters())
                        .unwrap_or(Value::Null)
                }
            }
        }),
    })
}

fn import_mesh(mesh: &Mesh, index: usize, asset_id: &str) -> Result<SourceMesh, ImportError> {
    if !mesh.triangles().len().is_multiple_of(3) {
        return Err(ImportError::InvalidSource(
            "FBX triangle index buffer is invalid".into(),
        ));
    }
    let mut triangle_materials = Vec::with_capacity(mesh.triangles().len() / 3);
    for (face_index, face) in mesh.faces().iter().enumerate() {
        let face_triangles = face.count.saturating_sub(2) as usize;
        let material = mesh
            .face_material()
            .get(face_index)
            .and_then(|slot| mesh.materials().get(*slot as usize))
            .copied();
        triangle_materials.extend(std::iter::repeat_n(material, face_triangles));
    }
    if triangle_materials.len() != mesh.triangles().len() / 3 {
        return Err(ImportError::InvalidSource(
            "FBX face and triangle material alignment is invalid".into(),
        ));
    }

    let mut grouped = BTreeMap::<Option<usize>, Vec<u32>>::new();
    for (triangle, material) in triangle_materials.into_iter().enumerate() {
        let triangle = &mesh.triangles()[triangle * 3..triangle * 3 + 3];
        grouped
            .entry(material)
            .or_default()
            .extend(triangle.iter().copied());
    }
    let primitives = grouped
        .into_iter()
        .map(|(material, corners)| {
            let mut positions = Vec::new();
            let mut normals =
                (!mesh.generated_normals() && mesh.normals().is_some()).then(Vec::<[f32; 3]>::new);
            let mut colors = mesh.color_sets().first().map(|_| Vec::<[f32; 3]>::new());
            let mut indices = Vec::with_capacity(corners.len());
            for corner in corners {
                let position = mesh.positions().get(corner as usize).ok_or_else(|| {
                    ImportError::InvalidSource("FBX position corner is missing".into())
                })?;
                positions.push([
                    finite_f32(position[0], "position x")?,
                    finite_f32(position[1], "position y")?,
                    finite_f32(position[2], "position z")?,
                ]);
                if let (Some(output), Some(normal)) = (
                    normals.as_mut(),
                    mesh.normals()
                        .and_then(|normals| normals.get(corner as usize)),
                ) {
                    output.push([
                        finite_f32(normal[0], "normal x")?,
                        finite_f32(normal[1], "normal y")?,
                        finite_f32(normal[2], "normal z")?,
                    ]);
                }
                if let (Some(output), Some(color)) = (
                    colors.as_mut(),
                    mesh.color_sets()
                        .first()
                        .and_then(|colors| colors.color().get(corner as usize)),
                ) {
                    output.push([
                        finite_f32(color[0], "color red")?,
                        finite_f32(color[1], "color green")?,
                        finite_f32(color[2], "color blue")?,
                    ]);
                }
                indices.push(u32::try_from(positions.len() - 1).map_err(|_| {
                    ImportError::LimitExceeded("FBX primitive vertex count overflowed".into())
                })?);
            }
            if normals
                .as_ref()
                .is_some_and(|values| values.len() != positions.len())
            {
                normals = None;
            }
            if colors
                .as_ref()
                .is_some_and(|values| values.len() != positions.len())
            {
                colors = None;
            }
            Ok(SourcePrimitive {
                positions,
                normals,
                colors,
                indices,
                material,
            })
        })
        .collect::<Result<Vec<_>, ImportError>>()?;
    let id = format!("{asset_id}/fbx/mesh/{index}");
    Ok(SourceMesh {
        id: id.clone(),
        primitives,
        extras: json!({
            "ice3": {
                "id": id,
                "fbx": {
                    "sourceName": mesh.name(),
                    "sourceId": mesh.source_id()
                }
            }
        }),
    })
}

fn world_bounds(scene: &Scene) -> Result<Bounds, ImportError> {
    let mut bounds = Bounds::empty();
    for node in scene.nodes() {
        let Some(mesh_index) = node.mesh() else {
            continue;
        };
        let mesh = &scene.meshes()[mesh_index];
        let matrix = node.geometry_to_world();
        for position in mesh.positions().values() {
            bounds.include(matrix.transform_point(*position))?;
        }
    }
    bounds
        .finish()
        .then_some(bounds)
        .ok_or(ImportError::EmptyGeometry)
}

#[derive(Debug)]
struct Bounds {
    min: [f64; 3],
    max: [f64; 3],
}

impl Bounds {
    fn empty() -> Self {
        Self {
            min: [f64::INFINITY; 3],
            max: [f64::NEG_INFINITY; 3],
        }
    }

    fn include(&mut self, point: [f64; 3]) -> Result<(), ImportError> {
        if !point.iter().all(|value| value.is_finite()) {
            return Err(ImportError::InvalidSource(
                "FBX geometry contains a non-finite position".into(),
            ));
        }
        for (axis, value) in point.iter().enumerate() {
            self.min[axis] = self.min[axis].min(*value);
            self.max[axis] = self.max[axis].max(*value);
        }
        Ok(())
    }

    fn finish(&self) -> bool {
        self.min.iter().all(|value| value.is_finite())
            && self.max.iter().all(|value| value.is_finite())
    }
}

fn matrix_3x4_to_4x4(values: [f64; 12]) -> Result<[f32; 16], ImportError> {
    Ok([
        finite_f32(values[0], "matrix")?,
        finite_f32(values[1], "matrix")?,
        finite_f32(values[2], "matrix")?,
        0.0,
        finite_f32(values[3], "matrix")?,
        finite_f32(values[4], "matrix")?,
        finite_f32(values[5], "matrix")?,
        0.0,
        finite_f32(values[6], "matrix")?,
        finite_f32(values[7], "matrix")?,
        finite_f32(values[8], "matrix")?,
        0.0,
        finite_f32(values[9], "matrix")?,
        finite_f32(values[10], "matrix")?,
        finite_f32(values[11], "matrix")?,
        1.0,
    ])
}

fn finite_f32(value: f64, field: &str) -> Result<f32, ImportError> {
    let converted = value as f32;
    if converted.is_finite() {
        Ok(converted)
    } else {
        Err(ImportError::InvalidSource(format!(
            "FBX {field} is not finite"
        )))
    }
}

fn clean_node_name(node: &Node, index: usize, metadata: Option<&ModelMetadata>) -> String {
    let raw_name = metadata
        .map(|metadata| metadata.name.as_str())
        .unwrap_or_else(|| node.name());
    let name = clean_authored_name(raw_name);
    if name.is_empty() {
        if index == 0 {
            "FBX Root".into()
        } else {
            format!("FBX Node {index}")
        }
    } else {
        name
    }
}

fn clean_mesh_name(name: &str) -> String {
    let name = name.trim();
    if name.is_empty() {
        "FBX Mesh".into()
    } else {
        name.to_owned()
    }
}

fn clean_authored_name(name: &str) -> String {
    name.replace("\0\u{1}", "::")
        .replace("::Model", "")
        .replace("::Material", "")
        .trim()
        .to_owned()
}

fn map_decoder_error(error: DecoderError) -> ImportError {
    match error {
        DecoderError::LimitExceeded(_)
        | DecoderError::InputTooLarge { .. }
        | DecoderError::ResourceLimit(_)
        | DecoderError::Allocation => ImportError::LimitExceeded(error.to_string()),
        _ => ImportError::InvalidSource(error.to_string()),
    }
}

fn add_resource_warnings(scene: &Scene, warnings: &mut BTreeSet<ImportWarning>) {
    if !scene.animation_stacks().is_empty()
        || !scene.animation_layers().is_empty()
        || !scene.animation_curves().is_empty()
    {
        warnings.insert(warning(
            "ICE3_STUDIO_IMPORT_FBX_ANIMATION_IGNORED",
            "FBX animation is not imported",
        ));
    }
    if !scene.skin_deformers().is_empty()
        || !scene.skin_clusters().is_empty()
        || !scene.bones().is_empty()
    {
        warnings.insert(warning(
            "ICE3_STUDIO_IMPORT_FBX_SKIN_IGNORED",
            "FBX skin and bone data are not imported",
        ));
    }
    if !scene.cameras().is_empty() {
        warnings.insert(warning(
            "ICE3_STUDIO_IMPORT_FBX_CAMERA_IGNORED",
            "FBX cameras are not imported",
        ));
    }
    if !scene.lights().is_empty() {
        warnings.insert(warning(
            "ICE3_STUDIO_IMPORT_FBX_LIGHT_IGNORED",
            "FBX lights are not imported",
        ));
    }
    if !scene.textures().is_empty() {
        warnings.insert(warning(
            "ICE3_STUDIO_IMPORT_FBX_TEXTURE_IGNORED",
            "FBX textures are not imported",
        ));
    }
    if scene
        .unsupported()
        .iter()
        .any(|item| matches!(item.kind, UnsupportedKind::CurveOrSurfaceGeometry))
    {
        warnings.insert(warning(
            "ICE3_STUDIO_IMPORT_FBX_NURBS_IGNORED",
            "FBX NURBS and surface geometry are not imported",
        ));
    }
    if !scene.warnings().is_empty() {
        warnings.insert(warning(
            "ICE3_STUDIO_IMPORT_FBX_DECODER_WARNING",
            "The native FBX decoder reported unsupported or invalid content",
        ));
    }
}

fn warning(code: &str, message: &str) -> ImportWarning {
    ImportWarning {
        code: code.into(),
        message: message.into(),
        path: None,
    }
}

fn document_metadata(
    bytes: &[u8],
    limits: NativeImportLimits,
) -> Result<DocumentMetadata, ImportError> {
    let syntax_limits = fbx_rs::syntax::Limits {
        max_depth: 128,
        max_nodes: limits.max_nodes,
        max_allocation: limits.max_memory_bytes,
        max_decompressed_bytes: limits.max_memory_bytes,
        ..Default::default()
    };
    let document = fbx_rs::syntax::parse(bytes, &syntax_limits).map_err(|error| {
        ImportError::InvalidSource(format!("FBX metadata parse failed: {error:?}"))
    })?;
    let objects = document.nodes.iter().enumerate().find_map(|(index, node)| {
        (node.name == b"Objects" && node.parent.is_none()).then_some(index)
    });
    let Some(objects) = objects else {
        return Ok(DocumentMetadata::default());
    };

    let mut output = DocumentMetadata::default();
    let mut model_nodes = BTreeMap::new();
    for (index, node) in document.nodes.iter().enumerate() {
        if node.parent != Some(objects) {
            continue;
        }
        let Some(properties) = properties(&document, index) else {
            continue;
        };
        if node.name == b"Model" {
            let Some(source_id) = properties.first().and_then(property_i64) else {
                continue;
            };
            let name = properties
                .get(1)
                .and_then(property_string)
                .unwrap_or_else(|| format!("Model {source_id}"));
            let object_type = properties
                .get(2)
                .and_then(property_string)
                .map(|value| if value == "Mesh" { "MESH" } else { "EMPTY" })
                .unwrap_or("EMPTY")
                .to_owned();
            model_nodes.insert(index, source_id);
            output.models.insert(
                source_id,
                ModelMetadata {
                    name,
                    object_type,
                    custom_properties: Map::new(),
                },
            );
        } else if node.name == b"Material" {
            let Some(source_id) = properties.first().and_then(property_i64) else {
                continue;
            };
            let name = properties
                .get(1)
                .and_then(property_string)
                .unwrap_or_else(|| format!("Material {source_id}"));
            output
                .materials
                .insert(source_id, MaterialMetadata { name });
        }
    }
    let properties70 = document
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| node.name == b"Properties70")
        .filter_map(|(index, node)| {
            node.parent
                .filter(|parent| model_nodes.contains_key(parent))
                .map(|parent| (index, parent))
        })
        .collect::<BTreeMap<_, _>>();
    for (index, node) in document.nodes.iter().enumerate() {
        let Some(model_node) = node
            .parent
            .and_then(|parent| properties70.get(&parent).copied())
        else {
            continue;
        };
        if node.name != b"P" {
            continue;
        }
        let Some(properties) = properties(&document, index) else {
            continue;
        };
        let Some(name) = properties.first().and_then(property_string) else {
            continue;
        };
        let is_user_property = properties
            .get(3)
            .and_then(property_string)
            .is_some_and(|flags| flags.split('+').any(|flag| flag == "U"));
        if !is_user_property {
            continue;
        }
        let Some(value) = properties
            .get(4)
            .and_then(|property| property_json(property, 0))
        else {
            continue;
        };
        let source_id = model_nodes.get(&model_node).copied();
        if let Some(metadata) = source_id.and_then(|source_id| output.models.get_mut(&source_id)) {
            metadata.custom_properties.insert(name, value);
        }
    }
    Ok(output)
}

fn properties<'a>(
    document: &'a fbx_rs::syntax::Document<'a>,
    index: usize,
) -> Option<&'a [Property<'a>]> {
    document
        .properties
        .get(document.nodes.get(index)?.properties.clone())
}

fn property_i64(property: &Property<'_>) -> Option<i64> {
    match property {
        Property::I16(value) => Some(i64::from(*value)),
        Property::I32(value) => Some(i64::from(*value)),
        Property::I64(value) => Some(*value),
        _ => None,
    }
}

fn property_string(property: &Property<'_>) -> Option<String> {
    match property {
        Property::String(value) => String::from_utf8(value.to_vec()).ok(),
        Property::AsciiBare(value) => String::from_utf8(value.to_vec()).ok(),
        _ => None,
    }
}

fn property_json(property: &Property<'_>, depth: usize) -> Option<Value> {
    if depth >= MAX_METADATA_DEPTH {
        return None;
    }
    match property {
        Property::I16(value) => Some(Value::from(*value)),
        Property::I32(value) => Some(Value::from(*value)),
        Property::I64(value) => Some(Value::from(*value)),
        Property::Bool(value) | Property::BoolAlt(value) => Some(Value::from(*value != 0)),
        Property::F32(value) => finite_json(f64::from(f32::from_bits(*value))),
        Property::F64(value) => finite_json(f64::from_bits(*value)),
        Property::String(value) => String::from_utf8(value.to_vec()).ok().map(Value::from),
        Property::AsciiBare(value) => match *value {
            b"T" | b"true" => Some(Value::Bool(true)),
            b"F" | b"false" => Some(Value::Bool(false)),
            _ => String::from_utf8(value.to_vec()).ok().map(Value::from),
        },
        Property::AsciiNegativeZero => Some(Value::from(-0.0)),
        Property::AsciiArray(values) => {
            if values.len() > MAX_METADATA_ARRAY_LENGTH {
                return None;
            }
            values
                .iter()
                .map(|value| ascii_value_json(value, depth + 1))
                .collect::<Option<Vec<_>>>()
                .map(Value::Array)
        }
        Property::Array {
            kind,
            count,
            big_endian,
            bytes,
        } => {
            if *count > MAX_METADATA_ARRAY_LENGTH {
                return None;
            }
            (0..*count)
                .map(|index| array_value_json(*kind, *big_endian, bytes, index, depth + 1))
                .collect::<Option<Vec<_>>>()
                .map(Value::Array)
        }
        Property::Raw(_) => None,
    }
}

fn ascii_value_json(value: &AsciiValue<'_>, depth: usize) -> Option<Value> {
    if depth >= MAX_METADATA_DEPTH {
        return None;
    }
    match value {
        AsciiValue::I64(value) => Some(Value::from(*value)),
        AsciiValue::F64(value) => finite_json(f64::from_bits(*value)),
        AsciiValue::NegativeZero => Some(Value::from(-0.0)),
        AsciiValue::Bare(value) => match *value {
            b"T" | b"true" => Some(Value::Bool(true)),
            b"F" | b"false" => Some(Value::Bool(false)),
            _ => String::from_utf8(value.to_vec()).ok().map(Value::from),
        },
    }
}

fn array_value_json(
    kind: u8,
    big_endian: bool,
    bytes: &[u8],
    index: usize,
    depth: usize,
) -> Option<Value> {
    if depth >= MAX_METADATA_DEPTH {
        return None;
    }
    let read = |width: usize| -> Option<&[u8]> {
        let start = index.checked_mul(width)?;
        bytes.get(start..start.checked_add(width)?)
    };
    match kind {
        b'f' => {
            let value = u32::from_be_bytes(read(4)?.try_into().ok()?);
            finite_json(f64::from(f32::from_bits(if big_endian {
                value
            } else {
                value.swap_bytes()
            })))
        }
        b'd' => {
            let value = u64::from_be_bytes(read(8)?.try_into().ok()?);
            finite_json(f64::from_bits(if big_endian {
                value
            } else {
                value.swap_bytes()
            }))
        }
        b'i' => {
            let bytes: [u8; 4] = read(4)?.try_into().ok()?;
            let value = if big_endian {
                u32::from_be_bytes(bytes)
            } else {
                u32::from_le_bytes(bytes)
            };
            Some(Value::from(i32::from_ne_bytes(value.to_ne_bytes())))
        }
        b'l' => {
            let bytes: [u8; 8] = read(8)?.try_into().ok()?;
            let value = if big_endian {
                u64::from_be_bytes(bytes)
            } else {
                u64::from_le_bytes(bytes)
            };
            Some(Value::from(i64::from_ne_bytes(value.to_ne_bytes())))
        }
        b'b' | b'c' => Some(Value::from(read(1)?[0] != 0)),
        _ => None,
    }
}

fn finite_json(value: f64) -> Option<Value> {
    value.is_finite().then(|| Value::from(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_nonfinite_and_oversized_user_properties() {
        let nonfinite = Property::F64(f64::NAN.to_bits());
        assert_eq!(property_json(&nonfinite, 0), None);

        let oversized = Property::AsciiArray(
            (0..=MAX_METADATA_ARRAY_LENGTH)
                .map(|value| AsciiValue::I64(value as i64))
                .collect(),
        );
        assert_eq!(property_json(&oversized, 0), None);
    }

    #[test]
    fn preserves_shallow_supported_user_properties() {
        let property = Property::AsciiArray(vec![
            AsciiValue::I64(1),
            AsciiValue::Bare(b"value"),
            AsciiValue::Bare(b"T"),
        ]);
        assert_eq!(property_json(&property, 0), Some(json!([1, "value", true])));
    }

    #[test]
    fn decodes_little_endian_integer_property_arrays() {
        let property = Property::Array {
            kind: b'i',
            count: 2,
            big_endian: false,
            bytes: std::borrow::Cow::Owned(
                [1_i32, -2_i32]
                    .into_iter()
                    .flat_map(i32::to_le_bytes)
                    .collect(),
            ),
        };
        assert_eq!(property_json(&property, 0), Some(json!([1, -2])));
    }

    #[test]
    fn normalizes_legacy_fbx_name_separators_once() {
        assert_eq!(
            clean_authored_name("Primary Pump\0\u{1}Model"),
            "Primary Pump"
        );
        assert_eq!(
            clean_authored_name("Pump Steel\0\u{1}Material"),
            "Pump Steel"
        );
    }
}
