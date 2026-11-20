use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use glam::{Mat4, Quat, Vec3};
use serde_json::{Map, Value, json};
use studio_assets::surface::AlphaMode;
use studio_assets::{
    AccessorData, MAX_TEXTURE_BYTES, MAX_TEXTURE_COUNT, MAX_TEXTURE_TOTAL_BYTES, SourceGlb,
    read_accessor, read_glb, read_image_buffer_view, read_image_uri,
};

use crate::{
    ImportError, ImportRequest, ImportWarning, SourceCapabilities, SourceFormat, SourceImporter,
    SourceMaterial, SourceMesh, SourceNode, SourcePrimitive, SourceResource, SourceScene,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct GltfImporter;

impl GltfImporter {
    pub const fn new() -> Self {
        Self
    }

    pub fn import_file(&self, path: &Path, asset_id: &str) -> Result<SourceScene, ImportError> {
        self.import(&ImportRequest {
            path: path.to_path_buf(),
            asset_id: asset_id.into(),
            requested_adapter: Some("gltf".into()),
        })
    }
}

impl SourceImporter for GltfImporter {
    fn id(&self) -> &'static str {
        "gltf"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &[".gltf", ".glb"]
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
        let source = read_glb(&request.path)
            .map_err(|error| ImportError::InvalidSource(error.to_string()))?;
        import_source_glb(&source, request.asset_id.trim())
    }
}

pub(crate) fn import_source_glb(
    source: &SourceGlb,
    asset_id: &str,
) -> Result<SourceScene, ImportError> {
    if asset_id.is_empty() {
        return Err(ImportError::InvalidSource(
            "asset ID must not be empty".into(),
        ));
    }
    let nodes = source
        .json
        .get("nodes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let meshes = source
        .json
        .get("meshes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let raw_materials = source
        .json
        .get("materials")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let parent = parent_map(&nodes)?;
    let roots = active_roots(source, &nodes)?;
    let mut active = BTreeSet::new();
    let mut stack = roots.clone();
    while let Some(index) = stack.pop() {
        if !active.insert(index) {
            return Err(ImportError::InvalidSource(
                "node hierarchy contains a cycle".into(),
            ));
        }
        let children = integer_array(nodes[index].get("children"))?;
        for child in children {
            if child >= nodes.len() {
                return Err(ImportError::InvalidSource("node child is missing".into()));
            }
            stack.push(child);
        }
    }

    let mut warnings = BTreeSet::new();
    let mut resources = BTreeMap::new();
    load_referenced_images(source, &raw_materials, &mut resources)?;
    let materials = raw_materials
        .iter()
        .enumerate()
        .map(|(index, material)| import_material(material, index, asset_id))
        .collect::<Result<Vec<_>, _>>()?;
    let mut mesh_map = BTreeMap::new();
    let mut output_meshes = Vec::new();
    for (index, mesh) in meshes.iter().enumerate() {
        if !active.iter().any(|node| {
            nodes[*node]
                .get("mesh")
                .and_then(Value::as_u64)
                .is_some_and(|value| value as usize == index)
        }) {
            continue;
        }
        let imported = import_mesh(source, mesh, index, asset_id, &mut warnings)?;
        mesh_map.insert(index, output_meshes.len());
        output_meshes.push(imported);
    }
    let raw_ids = (0..nodes.len())
        .map(|index| node_id(&nodes[index], index, asset_id))
        .collect::<Result<Vec<_>, _>>()?;
    let mut output_nodes = Vec::new();
    let mut ordered = active.into_iter().collect::<Vec<_>>();
    ordered.sort_unstable();
    for index in ordered {
        let node = nodes[index]
            .as_object()
            .ok_or_else(|| ImportError::InvalidSource("node is invalid".into()))?;
        let id = raw_ids[index].clone();
        let parent = parent
            .get(&index)
            .copied()
            .flatten()
            .map(|value| raw_ids[value].clone());
        let mesh = node
            .get("mesh")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .map(|value| {
                mesh_map
                    .get(&value)
                    .copied()
                    .ok_or_else(|| ImportError::InvalidSource("node mesh is missing".into()))
            })
            .transpose()?;
        let mut extras = node.get("extras").cloned().unwrap_or_else(|| json!({}));
        ensure_node_identity(
            &mut extras,
            &id,
            node.get("name").and_then(Value::as_str).unwrap_or(&id),
            &format!("{asset_id}/node/{id}"),
        )?;
        output_nodes.push(SourceNode {
            id,
            name: node
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_else(|| format!("Node {index}")),
            parent,
            matrix: node_matrix(node)?,
            mesh,
            material: None,
            extras,
        });
    }
    add_unsupported_resource_warnings(&source.json, &mut warnings);
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
        source_format: SourceFormat::Gltf,
        nodes: output_nodes,
        meshes: output_meshes,
        materials,
        resources: resources.into_values().collect(),
        warnings: warnings.into_iter().collect(),
        asset_extras: json!({
            "coordinateSystem": source
                .json
                .get("asset")
                .and_then(Value::as_object)
                .and_then(|asset| asset.get("extras"))
                .and_then(Value::as_object)
                .and_then(|extras| extras.get("ice3"))
                .and_then(Value::as_object)
                .and_then(|ice3| ice3.get("coordinateSystem"))
                .cloned()
                .unwrap_or_else(|| json!({
                    "handedness": "right",
                    "upAxis": "y",
                    "linearUnit": "meter"
                })),
            "gltf": {
                "generator": source
                    .json
                    .get("asset")
                    .and_then(Value::as_object)
                    .and_then(|asset| asset.get("generator"))
                    .cloned()
                    .unwrap_or(Value::Null)
            }
        }),
    })
}

fn import_material(
    value: &Value,
    index: usize,
    asset_id: &str,
) -> Result<SourceMaterial, ImportError> {
    let material = value
        .as_object()
        .ok_or_else(|| ImportError::InvalidSource("material is invalid".into()))?;
    let color = material
        .get("pbrMetallicRoughness")
        .and_then(Value::as_object)
        .and_then(|pbr| pbr.get("baseColorFactor"))
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .take(4)
                .map(|value| value.as_f64().unwrap_or(1.0) as f32)
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| vec![1.0, 1.0, 1.0, 1.0]);
    let mut extras = material.get("extras").cloned().unwrap_or_else(|| json!({}));
    let id = material
        .get("extras")
        .and_then(Value::as_object)
        .and_then(|extras| extras.get("ice3"))
        .and_then(Value::as_object)
        .and_then(|ice3| ice3.get("id"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{asset_id}/material/{index:06}"));
    ensure_material_id(&mut extras, &id)?;
    Ok(SourceMaterial {
        id,
        name: material
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| format!("material-{index}")),
        base_color: [
            *color.first().unwrap_or(&1.0),
            *color.get(1).unwrap_or(&1.0),
            *color.get(2).unwrap_or(&1.0),
        ],
        opacity: color.get(3).copied().unwrap_or(1.0).clamp(0.0, 1.0),
        alpha_mode: AlphaMode::Opaque,
        alpha_cutoff: 0.5,
        double_sided: false,
        surface: None,
        extras,
    })
}

fn load_referenced_images(
    source: &SourceGlb,
    materials: &[Value],
    resources: &mut BTreeMap<String, SourceResource>,
) -> Result<(), ImportError> {
    let Some(textures) = source.json.get("textures").and_then(Value::as_array) else {
        return Ok(());
    };
    let mut texture_indices = BTreeSet::new();
    for material in materials {
        for info in [
            material
                .pointer("/pbrMetallicRoughness/baseColorTexture")
                .and_then(Value::as_object),
            material
                .pointer("/pbrMetallicRoughness/metallicRoughnessTexture")
                .and_then(Value::as_object),
            material.get("normalTexture").and_then(Value::as_object),
            material.get("occlusionTexture").and_then(Value::as_object),
        ]
        .into_iter()
        .flatten()
        {
            let Some(index) = info.get("index").and_then(Value::as_u64) else {
                continue;
            };
            texture_indices.insert(
                usize::try_from(index)
                    .map_err(|_| ImportError::InvalidSource("texture index is invalid".into()))?,
            );
        }
    }
    for texture_index in texture_indices {
        let texture = textures
            .get(texture_index)
            .and_then(Value::as_object)
            .ok_or_else(|| ImportError::InvalidSource("texture index is out of range".into()))?;
        let image_index = texture
            .get("source")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| ImportError::InvalidSource("texture image is missing".into()))?;
        load_image(source, resources, image_index)?;
    }
    Ok(())
}

fn load_image(
    source: &SourceGlb,
    resources: &mut BTreeMap<String, SourceResource>,
    image_index: usize,
) -> Result<SourceResource, ImportError> {
    let image = source
        .json
        .get("images")
        .and_then(Value::as_array)
        .and_then(|images| images.get(image_index))
        .ok_or_else(|| ImportError::InvalidSource("image index is out of range".into()))?;
    let declared = image.get("mimeType").and_then(Value::as_str);
    let imported = if let Some(uri) = image.get("uri").and_then(Value::as_str) {
        read_image_uri(uri, &source.base_dir, declared, MAX_TEXTURE_BYTES)
    } else if image.get("bufferView").is_some() {
        read_image_buffer_view(source, image, MAX_TEXTURE_BYTES)
    } else {
        return Err(ImportError::InvalidSource(
            "image has no data source".into(),
        ));
    }
    .map_err(|error| ImportError::InvalidSource(error.to_string()))?;

    if let Some(existing) = resources.get(&imported.digest) {
        if existing.media_type != imported.media_type || existing.bytes != imported.bytes {
            return Err(ImportError::InvalidSource(
                "image digest has conflicting content".into(),
            ));
        }
        return Ok(existing.clone());
    }
    if resources.len() >= MAX_TEXTURE_COUNT {
        return Err(ImportError::LimitExceeded(format!(
            "scene has more than {MAX_TEXTURE_COUNT} texture resources"
        )));
    }
    let current_bytes = resources
        .values()
        .try_fold(0_usize, |total, resource| {
            total.checked_add(resource.bytes.len())
        })
        .ok_or_else(|| ImportError::LimitExceeded("texture byte count overflowed".into()))?;
    let next_bytes = current_bytes
        .checked_add(imported.bytes.len())
        .ok_or_else(|| ImportError::LimitExceeded("texture byte count overflowed".into()))?;
    if next_bytes > MAX_TEXTURE_TOTAL_BYTES {
        return Err(ImportError::LimitExceeded(format!(
            "scene texture byte count exceeds {MAX_TEXTURE_TOTAL_BYTES}"
        )));
    }
    let resource = SourceResource {
        digest: imported.digest,
        media_type: imported.media_type,
        bytes: imported.bytes,
    };
    resources.insert(resource.digest.clone(), resource.clone());
    Ok(resource)
}

fn import_mesh(
    source: &SourceGlb,
    value: &Value,
    index: usize,
    asset_id: &str,
    warnings: &mut BTreeSet<ImportWarning>,
) -> Result<SourceMesh, ImportError> {
    let mesh = value
        .as_object()
        .ok_or_else(|| ImportError::InvalidSource("mesh is invalid".into()))?;
    let primitives = mesh
        .get("primitives")
        .and_then(Value::as_array)
        .ok_or_else(|| ImportError::InvalidSource("mesh primitives are missing".into()))?;
    let mut output = Vec::new();
    for primitive in primitives {
        let primitive = primitive
            .as_object()
            .ok_or_else(|| ImportError::InvalidSource("primitive is invalid".into()))?;
        let mode = primitive.get("mode").and_then(Value::as_u64).unwrap_or(4);
        if mode != 4 {
            warnings.insert(warning(
                "ICE3_STUDIO_IMPORT_PRIMITIVE_UNSUPPORTED",
                "only triangle primitives are imported",
            ));
            continue;
        }
        let attributes = primitive
            .get("attributes")
            .and_then(Value::as_object)
            .ok_or_else(|| ImportError::InvalidSource("primitive attributes are missing".into()))?;
        let position_index = accessor_index(attributes.get("POSITION"), "POSITION")?;
        let AccessorData::F32 {
            values,
            components: 3,
        } = read_accessor(source, position_index)
            .map_err(|error| ImportError::InvalidSource(error.to_string()))?
        else {
            return Err(ImportError::InvalidSource(
                "POSITION must be float VEC3".into(),
            ));
        };
        let positions = values
            .as_chunks::<3>()
            .0
            .iter()
            .map(|value| [value[0], value[1], value[2]])
            .collect::<Vec<_>>();
        let normals = optional_vec3(source, attributes.get("NORMAL"))?;
        let colors = optional_vec3(source, attributes.get("COLOR_0"))?;
        let uvs = optional_triangle_uvs(source, attributes, warnings)?;
        let indices = match primitive.get("indices") {
            Some(value) => {
                let index = value
                    .as_u64()
                    .and_then(|value| usize::try_from(value).ok())
                    .ok_or_else(|| {
                        ImportError::InvalidSource("index accessor is invalid".into())
                    })?;
                match read_accessor(source, index)
                    .map_err(|error| ImportError::InvalidSource(error.to_string()))?
                {
                    AccessorData::U16 { values, .. } => values.into_iter().map(u32::from).collect(),
                    AccessorData::U32 { values, .. } => values,
                    AccessorData::F32 { .. } | AccessorData::U8 { .. } => {
                        return Err(ImportError::InvalidSource(
                            "indices must be integers".into(),
                        ));
                    }
                }
            }
            None => (0..positions.len() as u32).collect(),
        };
        if indices
            .iter()
            .any(|value| *value as usize >= positions.len())
        {
            return Err(ImportError::InvalidSource(
                "primitive index is out of range".into(),
            ));
        }
        output.push(SourcePrimitive {
            positions,
            normals,
            colors,
            uvs,
            indices,
            material: primitive
                .get("material")
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok()),
        });
    }
    if output.is_empty() {
        return Err(ImportError::EmptyGeometry);
    }
    let mut extras = mesh.get("extras").cloned().unwrap_or_else(|| json!({}));
    let id = mesh
        .get("extras")
        .and_then(Value::as_object)
        .and_then(|extras| extras.get("ice3"))
        .and_then(Value::as_object)
        .and_then(|ice3| ice3.get("id"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("{asset_id}/mesh/{index}"));
    ensure_mesh_id(&mut extras, &id)?;
    Ok(SourceMesh {
        id,
        primitives: output,
        extras,
    })
}

fn optional_vec3(
    source: &SourceGlb,
    value: Option<&Value>,
) -> Result<Option<Vec<[f32; 3]>>, ImportError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let index = value
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| ImportError::InvalidSource("attribute accessor is invalid".into()))?;
    let AccessorData::F32 {
        values,
        components: 3,
    } = read_accessor(source, index)
        .map_err(|error| ImportError::InvalidSource(error.to_string()))?
    else {
        return Err(ImportError::InvalidSource(
            "attribute must be float VEC3".into(),
        ));
    };
    Ok(Some(
        values
            .as_chunks::<3>()
            .0
            .iter()
            .map(|value| [value[0], value[1], value[2]])
            .collect(),
    ))
}

fn optional_triangle_uvs(
    source: &SourceGlb,
    attributes: &Map<String, Value>,
    warnings: &mut BTreeSet<ImportWarning>,
) -> Result<Option<Vec<[f32; 2]>>, ImportError> {
    let Some(value) = attributes.get("TEXCOORD_0") else {
        return Ok(None);
    };
    let index = value
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| ImportError::InvalidSource("TEXCOORD_0 accessor is invalid".into()))?;
    let accessor = source
        .json
        .get("accessors")
        .and_then(Value::as_array)
        .and_then(|accessors| accessors.get(index))
        .and_then(Value::as_object)
        .ok_or_else(|| ImportError::InvalidSource("TEXCOORD_0 accessor is missing".into()))?;
    let component_type = accessor.get("componentType").and_then(Value::as_u64);
    let normalized = accessor.get("normalized").and_then(Value::as_bool) == Some(true);
    let supported_components =
        component_type == Some(5126) || (normalized && matches!(component_type, Some(5121 | 5123)));
    if accessor.contains_key("sparse")
        || accessor.get("type").and_then(Value::as_str) != Some("VEC2")
        || !supported_components
    {
        warnings.insert(warning(
            "ICE3_STUDIO_IMPORT_TEXTURE_COORDINATE_UNSUPPORTED",
            "only dense float or normalized integer VEC2 TEXCOORD_0 accessors are imported",
        ));
        return Ok(None);
    }
    let AccessorData::F32 {
        values,
        components: 2,
    } = read_accessor(source, index)
        .map_err(|error| ImportError::InvalidSource(error.to_string()))?
    else {
        warnings.insert(warning(
            "ICE3_STUDIO_IMPORT_TEXTURE_COORDINATE_UNSUPPORTED",
            "TEXCOORD_0 accessor encoding is not imported",
        ));
        return Ok(None);
    };
    Ok(Some(
        values
            .as_chunks::<2>()
            .0
            .iter()
            .map(|value| [value[0], value[1]])
            .collect(),
    ))
}

fn parent_map(nodes: &[Value]) -> Result<BTreeMap<usize, Option<usize>>, ImportError> {
    let mut output = (0..nodes.len())
        .map(|index| (index, None))
        .collect::<BTreeMap<_, _>>();
    for (index, node) in nodes.iter().enumerate() {
        let node = node
            .as_object()
            .ok_or_else(|| ImportError::InvalidSource("node is invalid".into()))?;
        for child in integer_array(node.get("children"))? {
            if child >= nodes.len()
                || output
                    .insert(child, Some(index))
                    .is_some_and(|v| v.is_some())
            {
                return Err(ImportError::InvalidSource(
                    "node has multiple parents".into(),
                ));
            }
        }
    }
    Ok(output)
}

fn active_roots(source: &SourceGlb, nodes: &[Value]) -> Result<Vec<usize>, ImportError> {
    let scene_index = source
        .json
        .get("scene")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let roots = source
        .json
        .get("scenes")
        .and_then(Value::as_array)
        .and_then(|scenes| scenes.get(scene_index))
        .and_then(Value::as_object)
        .ok_or_else(|| ImportError::InvalidSource("active scene is missing".into()))?
        .get("nodes");
    let roots = integer_array(roots)?;
    if roots.iter().any(|index| *index >= nodes.len()) {
        return Err(ImportError::InvalidSource(
            "active scene node is missing".into(),
        ));
    }
    Ok(roots)
}

fn node_matrix(node: &Map<String, Value>) -> Result<[f32; 16], ImportError> {
    if let Some(matrix) = node.get("matrix") {
        let values = matrix
            .as_array()
            .filter(|values| values.len() == 16)
            .ok_or_else(|| ImportError::InvalidSource("node matrix is invalid".into()))?;
        let values = values
            .iter()
            .map(|value| {
                value
                    .as_f64()
                    .filter(|value| value.is_finite())
                    .map(|value| value as f32)
                    .ok_or_else(|| ImportError::InvalidSource("node matrix is invalid".into()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        return values
            .try_into()
            .map_err(|_| ImportError::InvalidSource("node matrix is invalid".into()));
    }
    let translation = vector3(node.get("translation"), [0.0; 3], "translation")?;
    let rotation = vector4(node.get("rotation"), [0.0, 0.0, 0.0, 1.0], "rotation")?;
    let scale = vector3(node.get("scale"), [1.0; 3], "scale")?;
    let quaternion = Quat::from_xyzw(rotation[0], rotation[1], rotation[2], rotation[3]);
    if quaternion.length_squared() <= f32::EPSILON {
        return Err(ImportError::InvalidSource(
            "node rotation is invalid".into(),
        ));
    }
    Ok(Mat4::from_scale_rotation_translation(
        Vec3::from_array(scale),
        quaternion.normalize(),
        Vec3::from_array(translation),
    )
    .to_cols_array())
}

fn vector3(value: Option<&Value>, default: [f32; 3], field: &str) -> Result<[f32; 3], ImportError> {
    let Some(value) = value else {
        return Ok(default);
    };
    let values = value
        .as_array()
        .filter(|values| values.len() == 3)
        .ok_or_else(|| ImportError::InvalidSource(format!("node {field} is invalid")))?;
    Ok([
        finite(&values[0], field)?,
        finite(&values[1], field)?,
        finite(&values[2], field)?,
    ])
}

fn vector4(value: Option<&Value>, default: [f32; 4], field: &str) -> Result<[f32; 4], ImportError> {
    let Some(value) = value else {
        return Ok(default);
    };
    let values = value
        .as_array()
        .filter(|values| values.len() == 4)
        .ok_or_else(|| ImportError::InvalidSource(format!("node {field} is invalid")))?;
    Ok([
        finite(&values[0], field)?,
        finite(&values[1], field)?,
        finite(&values[2], field)?,
        finite(&values[3], field)?,
    ])
}

fn finite(value: &Value, field: &str) -> Result<f32, ImportError> {
    value
        .as_f64()
        .filter(|value| value.is_finite())
        .map(|value| value as f32)
        .ok_or_else(|| ImportError::InvalidSource(format!("node {field} is invalid")))
}

fn integer_array(value: Option<&Value>) -> Result<Vec<usize>, ImportError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    value
        .as_array()
        .ok_or_else(|| ImportError::InvalidSource("node index list is invalid".into()))?
        .iter()
        .map(|value| {
            value
                .as_u64()
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(|| ImportError::InvalidSource("node index is invalid".into()))
        })
        .collect()
}

fn accessor_index(value: Option<&Value>, field: &str) -> Result<usize, ImportError> {
    value
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| ImportError::InvalidSource(format!("{field} accessor is missing")))
}

fn node_id(node: &Value, index: usize, _asset_id: &str) -> Result<String, ImportError> {
    let node = node
        .as_object()
        .ok_or_else(|| ImportError::InvalidSource("node is invalid".into()))?;
    Ok(node
        .get("extras")
        .and_then(Value::as_object)
        .and_then(|extras| extras.get("ice3"))
        .and_then(Value::as_object)
        .and_then(|ice3| ice3.get("id"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("node-{index:06}")))
}

fn ensure_node_identity(
    extras: &mut Value,
    id: &str,
    display_name: &str,
    selection_id: &str,
) -> Result<(), ImportError> {
    let object = extras
        .as_object_mut()
        .ok_or_else(|| ImportError::InvalidSource("node extras must be an object".into()))?;
    let ice3 = object
        .entry("ice3")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| ImportError::InvalidSource("node ice3 extras must be an object".into()))?;
    ice3.insert("id".into(), Value::from(id));
    ice3.entry("selectionId")
        .or_insert_with(|| Value::from(selection_id));
    ice3.entry("displayName")
        .or_insert_with(|| Value::from(display_name));
    Ok(())
}

fn ensure_mesh_id(extras: &mut Value, id: &str) -> Result<(), ImportError> {
    ensure_ice3_object(extras)?.insert("id".into(), Value::from(id));
    Ok(())
}

fn ensure_material_id(extras: &mut Value, id: &str) -> Result<(), ImportError> {
    ensure_ice3_object(extras)?.insert("id".into(), Value::from(id));
    Ok(())
}

fn ensure_ice3_object(extras: &mut Value) -> Result<&mut Map<String, Value>, ImportError> {
    extras
        .as_object_mut()
        .ok_or_else(|| ImportError::InvalidSource("extras must be an object".into()))?
        .entry("ice3")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| ImportError::InvalidSource("ice3 extras must be an object".into()))
}

fn add_unsupported_resource_warnings(json: &Value, warnings: &mut BTreeSet<ImportWarning>) {
    for field in ["animations", "skins", "cameras"] {
        if json
            .get(field)
            .and_then(Value::as_array)
            .is_some_and(|values| !values.is_empty())
        {
            warnings.insert(warning(
                "ICE3_STUDIO_IMPORT_RESOURCE_UNSUPPORTED",
                &format!("{field} are not imported"),
            ));
        }
    }
}

fn warning(code: &str, message: &str) -> ImportWarning {
    ImportWarning {
        code: code.into(),
        message: message.into(),
        path: None,
    }
}
