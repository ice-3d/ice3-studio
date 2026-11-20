use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use glam::{Mat4, Quat, Vec3};
use serde_json::{Map, Value, json};
use studio_assets::surface::{AlphaMode, TextureSampler, TextureTransform};
use studio_assets::{
    AccessorData, MAX_TEXTURE_BYTES, MAX_TEXTURE_COUNT, MAX_TEXTURE_TOTAL_BYTES, SourceGlb,
    read_accessor, read_glb, read_image_buffer_view, read_image_uri,
};

use crate::{
    ImportError, ImportRequest, ImportWarning, SourceCapabilities, SourceFormat, SourceImporter,
    SourceMaterial, SourceMesh, SourceNode, SourceNormalTexture, SourcePrimitive, SourceResource,
    SourceScene, SourceSurface, SourceTextureBinding,
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
    let materials = raw_materials
        .iter()
        .enumerate()
        .map(|(index, material)| {
            import_material(
                material,
                index,
                asset_id,
                source,
                &mut resources,
                &mut warnings,
            )
        })
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
    source: &SourceGlb,
    resources: &mut BTreeMap<String, SourceResource>,
    warnings: &mut BTreeSet<ImportWarning>,
) -> Result<SourceMaterial, ImportError> {
    let material = value
        .as_object()
        .ok_or_else(|| ImportError::InvalidSource("material is invalid".into()))?;
    let pbr = material
        .get("pbrMetallicRoughness")
        .and_then(Value::as_object);
    let color = vector4(
        pbr.and_then(|value| value.get("baseColorFactor")),
        [1.0, 1.0, 1.0, 1.0],
        "baseColorFactor",
    )?;
    if color.iter().any(|value| !(0.0..=1.0).contains(value)) {
        return Err(ImportError::InvalidSource(
            "baseColorFactor must contain values between 0 and 1".into(),
        ));
    }
    let alpha_mode = match material
        .get("alphaMode")
        .and_then(Value::as_str)
        .unwrap_or("OPAQUE")
    {
        "OPAQUE" => AlphaMode::Opaque,
        "MASK" => AlphaMode::Mask,
        "BLEND" => AlphaMode::Blend,
        other => {
            return Err(ImportError::InvalidSource(format!(
                "unsupported alphaMode {other}"
            )));
        }
    };
    let alpha_cutoff = optional_finite(material.get("alphaCutoff"), 0.5, "alphaCutoff")?;
    if !(0.0..=1.0).contains(&alpha_cutoff) {
        return Err(ImportError::InvalidSource(
            "alphaCutoff must be between 0 and 1".into(),
        ));
    }
    if material.get("emissiveTexture").is_some() {
        warnings.insert(warning(
            "ICE3_STUDIO_IMPORT_TEXTURE_TARGET_UNSUPPORTED",
            "emissiveTexture is not imported",
        ));
    }
    if let Some(extensions) = material.get("extensions").and_then(Value::as_object) {
        for extension in extensions.keys() {
            warnings.insert(warning(
                "ICE3_STUDIO_IMPORT_MATERIAL_EXTENSION_UNSUPPORTED",
                &format!("material extension {extension} is not imported"),
            ));
        }
    }
    let emissive = optional_vec3_value(material.get("emissiveFactor"), "emissiveFactor")?;
    if emissive.is_some_and(|value| value.iter().any(|component| *component < 0.0)) {
        return Err(ImportError::InvalidSource(
            "emissiveFactor must be non-negative".into(),
        ));
    }
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
            color[0].clamp(0.0, 1.0),
            color[1].clamp(0.0, 1.0),
            color[2].clamp(0.0, 1.0),
        ],
        opacity: color[3].clamp(0.0, 1.0),
        alpha_mode,
        alpha_cutoff,
        double_sided: material
            .get("doubleSided")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        surface: Some(SourceSurface {
            metallic: factor(pbr.and_then(|value| value.get("metallicFactor")), 1.0)?,
            roughness: factor(pbr.and_then(|value| value.get("roughnessFactor")), 1.0)?,
            emissive,
            base_color_texture: import_texture_info(
                source,
                resources,
                warnings,
                pbr.and_then(|value| value.get("baseColorTexture")),
                "baseColorTexture",
            )?,
            normal_texture: import_normal_texture(
                source,
                resources,
                warnings,
                material.get("normalTexture"),
            )?,
            occlusion_texture: import_texture_info(
                source,
                resources,
                warnings,
                material.get("occlusionTexture"),
                "occlusionTexture",
            )?,
            metallic_roughness_texture: import_texture_info(
                source,
                resources,
                warnings,
                pbr.and_then(|value| value.get("metallicRoughnessTexture")),
                "metallicRoughnessTexture",
            )?,
        }),
        extras,
    })
}

fn import_texture_info(
    source: &SourceGlb,
    resources: &mut BTreeMap<String, SourceResource>,
    warnings: &mut BTreeSet<ImportWarning>,
    value: Option<&Value>,
    field: &str,
) -> Result<Option<SourceTextureBinding>, ImportError> {
    let Some(info) = value else {
        return Ok(None);
    };
    let info = info
        .as_object()
        .ok_or_else(|| ImportError::InvalidSource(format!("{field} is invalid")))?;
    let texture_index = index_field(info.get("index"), &format!("{field}.index"))?;
    let textures = source
        .json
        .get("textures")
        .and_then(Value::as_array)
        .ok_or_else(|| ImportError::InvalidSource("textures are missing".into()))?;
    let texture = textures
        .get(texture_index)
        .and_then(Value::as_object)
        .ok_or_else(|| ImportError::InvalidSource("texture index is out of range".into()))?;
    let sampler = import_sampler(source, texture.get("sampler"))?;
    let mut tex_coord = u32::try_from(info.get("texCoord").and_then(Value::as_u64).unwrap_or(0))
        .map_err(|_| ImportError::InvalidSource("texCoord is too large".into()))?;
    let mut transform = TextureTransform::default();
    if let Some(extension) = info
        .get("extensions")
        .and_then(Value::as_object)
        .and_then(|value| value.get("KHR_texture_transform"))
        .and_then(Value::as_object)
    {
        if let Some(overridden) = extension.get("texCoord").and_then(Value::as_u64) {
            tex_coord = u32::try_from(overridden)
                .map_err(|_| ImportError::InvalidSource("texCoord is too large".into()))?;
        }
        transform.offset = vector2(extension.get("offset"), [0.0, 0.0], "offset")?;
        transform.rotation = optional_finite(extension.get("rotation"), 0.0, "rotation")?;
        transform.scale = vector2(extension.get("scale"), [1.0, 1.0], "scale")?;
        if transform.scale.iter().any(|value| *value <= 0.0) {
            return Err(ImportError::InvalidSource(
                "texture transform scale must be positive".into(),
            ));
        }
    }
    if tex_coord != 0 {
        warnings.insert(warning(
            "ICE3_STUDIO_IMPORT_TEXTURE_COORDINATE_UNSUPPORTED",
            &format!("{field} uses TEXCOORD_{tex_coord}; only TEXCOORD_0 is imported"),
        ));
        return Ok(None);
    }
    let image_index = index_field(texture.get("source"), "texture.source")?;
    let resource = load_image(source, resources, image_index)?;
    Ok(Some(SourceTextureBinding {
        resource: resource.digest,
        tex_coord,
        transform,
        sampler,
    }))
}

fn import_normal_texture(
    source: &SourceGlb,
    resources: &mut BTreeMap<String, SourceResource>,
    warnings: &mut BTreeSet<ImportWarning>,
    value: Option<&Value>,
) -> Result<Option<SourceNormalTexture>, ImportError> {
    let Some(info) = value else {
        return Ok(None);
    };
    let Some(binding) =
        import_texture_info(source, resources, warnings, Some(info), "normalTexture")?
    else {
        return Ok(None);
    };
    let scale = optional_finite(info.get("scale"), 1.0, "normalTexture.scale")?;
    if !(0.0..=2.0).contains(&scale) {
        return Err(ImportError::InvalidSource(
            "normalTexture.scale must be between 0 and 2".into(),
        ));
    }
    Ok(Some(SourceNormalTexture { binding, scale }))
}

fn import_sampler(
    source: &SourceGlb,
    value: Option<&Value>,
) -> Result<TextureSampler, ImportError> {
    let Some(value) = value else {
        return Ok(TextureSampler::default());
    };
    let index = index_field(Some(value), "texture.sampler")?;
    let sampler = source
        .json
        .get("samplers")
        .and_then(Value::as_array)
        .and_then(|samplers| samplers.get(index))
        .and_then(Value::as_object)
        .ok_or_else(|| ImportError::InvalidSource("sampler index is out of range".into()))?;
    let mag_filter = sampler_enum(sampler.get("magFilter"), &[9728, 9729], "magFilter")?;
    let min_filter = sampler_enum(
        sampler.get("minFilter"),
        &[9728, 9729, 9984, 9985, 9986, 9987],
        "minFilter",
    )?;
    let wrap_s =
        sampler_enum(sampler.get("wrapS"), &[33071, 33648, 10497], "wrapS")?.unwrap_or(10_497);
    let wrap_t =
        sampler_enum(sampler.get("wrapT"), &[33071, 33648, 10497], "wrapT")?.unwrap_or(10_497);
    Ok(TextureSampler {
        mag_filter,
        min_filter,
        wrap_s,
        wrap_t,
    })
}

fn sampler_enum(
    value: Option<&Value>,
    allowed: &[u32],
    field: &str,
) -> Result<Option<u32>, ImportError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| allowed.contains(value))
        .ok_or_else(|| ImportError::InvalidSource(format!("sampler {field} is invalid")))?;
    Ok(Some(value))
}

fn index_field(value: Option<&Value>, field: &str) -> Result<usize, ImportError> {
    value
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| ImportError::InvalidSource(format!("{field} is missing")))
}

fn factor(value: Option<&Value>, default: f32) -> Result<f32, ImportError> {
    let value = optional_finite(value, default, "material factor")?;
    if !(0.0..=1.0).contains(&value) {
        return Err(ImportError::InvalidSource(
            "material factor must be between 0 and 1".into(),
        ));
    }
    Ok(value)
}

fn optional_vec3_value(
    value: Option<&Value>,
    field: &str,
) -> Result<Option<[f32; 3]>, ImportError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let values = value
        .as_array()
        .filter(|values| values.len() == 3)
        .ok_or_else(|| ImportError::InvalidSource(format!("{field} is invalid")))?;
    Ok(Some([
        finite(&values[0], field)?,
        finite(&values[1], field)?,
        finite(&values[2], field)?,
    ]))
}

fn vector2(value: Option<&Value>, default: [f32; 2], field: &str) -> Result<[f32; 2], ImportError> {
    let Some(value) = value else {
        return Ok(default);
    };
    let values = value
        .as_array()
        .filter(|values| values.len() == 2)
        .ok_or_else(|| ImportError::InvalidSource(format!("{field} is invalid")))?;
    Ok([finite(&values[0], field)?, finite(&values[1], field)?])
}

fn optional_finite(value: Option<&Value>, default: f32, field: &str) -> Result<f32, ImportError> {
    value.map_or(Ok(default), |value| finite(value, field))
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
