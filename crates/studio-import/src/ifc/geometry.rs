use std::collections::{BTreeMap, BTreeSet};

use glam::{DMat4, DVec3};
use ifc_lite_processing::{InstanceRecord, MeshData, ProcessingResult};
use serde_json::{Value, json};

use super::{
    IfcMetadata, IfcNodeMetadata, IfcProductMetadata, IfcPropertySetMetadata,
    IfcQuantitySetMetadata, IfcSystemReference,
};
use crate::{
    ImportError, ImportWarning, NativeImportLimits, SourceFormat, SourceMaterial, SourceMesh,
    SourceNode, SourcePrimitive, SourceScene, validate_scene_limits,
};

const ESTIMATED_PRODUCT_METADATA_BYTES: usize = 2_048;

#[derive(Debug)]
struct GeometryTemplate {
    mesh_index: usize,
    express_id: u32,
}

type GeometryProjection = (Vec<SourceMesh>, Vec<SourceMaterial>, Vec<GeometryTemplate>);

#[derive(Debug)]
struct PendingNode {
    id: String,
    name: String,
    parent: Option<String>,
    world: DMat4,
    mesh: Option<usize>,
    extras: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct MaterialKey {
    name: String,
    rgba: [u32; 4],
}

pub fn validate_ifc_geometry(
    metadata: &IfcMetadata,
    result: &ProcessingResult,
    limits: NativeImportLimits,
) -> Result<(), ImportError> {
    let node_count = projected_node_count(metadata, result)?;
    if node_count > limits.max_nodes {
        return Err(ImportError::LimitExceeded(format!(
            "IFC has {node_count} nodes; limit is {}",
            limits.max_nodes
        )));
    }

    let triangles = result
        .meshes
        .iter()
        .try_fold(0_u64, |total, mesh| {
            total.checked_add(u64::try_from(mesh.indices.len() / 3).ok()?)
        })
        .ok_or_else(|| ImportError::LimitExceeded("IFC triangle count overflowed".into()))?;
    if triangles == 0 {
        return Err(ImportError::EmptyGeometry);
    }
    if triangles > limits.max_triangles {
        return Err(ImportError::LimitExceeded(format!(
            "IFC has {triangles} triangles; limit is {}",
            limits.max_triangles
        )));
    }

    let geometry_bytes = result
        .meshes
        .iter()
        .try_fold(0_usize, |total, mesh| {
            let bytes = mesh
                .positions
                .len()
                .checked_mul(std::mem::size_of::<f32>())?
                .checked_add(mesh.normals.len().checked_mul(std::mem::size_of::<f32>())?)?
                .checked_add(mesh.indices.len().checked_mul(std::mem::size_of::<u32>())?)?;
            total.checked_add(bytes)
        })
        .ok_or_else(|| ImportError::LimitExceeded("IFC geometry memory overflowed".into()))?;
    let metadata_bytes = metadata
        .nodes
        .len()
        .checked_add(metadata.products.len())
        .and_then(|count| count.checked_mul(ESTIMATED_PRODUCT_METADATA_BYTES))
        .ok_or_else(|| ImportError::LimitExceeded("IFC metadata memory overflowed".into()))?;
    let estimated_bytes = geometry_bytes
        .checked_add(metadata_bytes)
        .ok_or_else(|| ImportError::LimitExceeded("IFC scene memory overflowed".into()))?;
    if estimated_bytes > limits.max_memory_bytes {
        return Err(ImportError::LimitExceeded(format!(
            "IFC scene uses approximately {estimated_bytes} bytes; limit is {}",
            limits.max_memory_bytes
        )));
    }
    Ok(())
}

pub fn project_ifc_scene(
    metadata: &IfcMetadata,
    result: &ProcessingResult,
    asset_id: &str,
    limits: NativeImportLimits,
) -> Result<SourceScene, ImportError> {
    validate_ifc_geometry(metadata, result, limits)?;

    let source_origin = source_origin_for(&result.meshes, &result.instances).unwrap_or(DVec3::ZERO);
    let (mut meshes, materials, templates) = build_geometry(&result.meshes, source_origin)?;
    let template_by_express = templates
        .iter()
        .map(|template| (template.express_id, template.mesh_index))
        .collect::<BTreeMap<_, _>>();
    let instance_by_id = result
        .instances
        .iter()
        .map(|instance| (instance.express_id, instance))
        .collect::<BTreeMap<_, _>>();
    let spatial_order = ordered_spatial_nodes(metadata);
    let spatial_parent_by_element = metadata
        .nodes
        .values()
        .flat_map(|node| {
            node.elements
                .iter()
                .map(move |element| (*element, node.express_id))
        })
        .collect::<BTreeMap<_, _>>();
    let spatial_id = |express_id: u32| format!("{asset_id}/ifc/express/{express_id}");
    let product_id = |product: &IfcProductMetadata| product.selection_id.clone();

    let mut warnings = metadata.warnings.iter().cloned().collect::<BTreeSet<_>>();
    collect_mesh_warnings(&result.meshes, &mut warnings);

    let mut pending = Vec::new();
    for express_id in spatial_order {
        let node = &metadata.nodes[&express_id];
        pending.push(PendingNode {
            id: spatial_id(express_id),
            name: node.name.clone(),
            parent: node.parent_id.map(spatial_id),
            world: DMat4::IDENTITY,
            mesh: None,
            extras: spatial_extras(node, metadata.schema.as_str()),
        });
    }

    let project_parent = spatial_id(metadata.project_id);
    let has_unassigned = metadata
        .products
        .values()
        .any(|product| !spatial_parent_by_element.contains_key(&product.express_id))
        || result.instances.iter().any(|instance| {
            !metadata.products.contains_key(&instance.express_id)
                && !spatial_parent_by_element.contains_key(&instance.express_id)
        });
    let unassigned_parent = has_unassigned.then(|| format!("{asset_id}/ifc/unassigned"));
    if let Some(unassigned_id) = &unassigned_parent {
        pending.push(PendingNode {
            id: unassigned_id.clone(),
            name: "Unassigned IFC Products".into(),
            parent: Some(project_parent),
            world: DMat4::IDENTITY,
            mesh: None,
            extras: json!({ "ice3": { "ifc": { "ifcType": "UNASSIGNED" } } }),
        });
    }

    for product in metadata.products.values() {
        let mut mesh = template_by_express.get(&product.express_id).copied();
        let mut local = DMat4::IDENTITY;
        let mut instance_group = None;
        if let Some(instance) = instance_by_id.get(&product.express_id) {
            mesh = template_by_express
                .get(&instance.template_express_id)
                .copied();
            local = row_major_to_y_up(instance.transform);
            instance_group = Some(format!("ifc/instance/{}", instance.rep_identity));
        }
        let parent = spatial_parent_by_element
            .get(&product.express_id)
            .map(|id| spatial_id(*id))
            .or_else(|| unassigned_parent.clone())
            .unwrap_or_else(|| spatial_id(metadata.project_id));
        pending.push(PendingNode {
            id: product_id(product),
            name: product.name.clone(),
            parent: Some(parent),
            world: local,
            mesh,
            extras: product_extras(product, metadata, instance_group.as_deref()),
        });
    }

    for instance in &result.instances {
        if metadata.products.contains_key(&instance.express_id) {
            continue;
        }
        let product = fallback_instance_product(instance, asset_id);
        let parent = spatial_parent_by_element
            .get(&instance.express_id)
            .map(|id| spatial_id(*id))
            .or_else(|| unassigned_parent.clone())
            .unwrap_or_else(|| spatial_id(metadata.project_id));
        pending.push(PendingNode {
            id: product.selection_id.clone(),
            name: product.name.clone(),
            parent: Some(parent),
            world: row_major_to_y_up(instance.transform),
            mesh: template_by_express
                .get(&instance.template_express_id)
                .copied(),
            extras: product_extras(
                &product,
                metadata,
                Some(&format!("ifc/instance/{}", instance.rep_identity)),
            ),
        });
    }

    add_shared_mesh_groups(&mut pending);
    let nodes = pending
        .into_iter()
        .map(|node| SourceNode {
            id: node.id,
            name: node.name,
            parent: node.parent,
            matrix: node.world.to_cols_array().map(|value| value as f32),
            mesh: node.mesh,
            material: None,
            extras: node.extras,
        })
        .collect::<Vec<_>>();

    let scene = SourceScene {
        asset_id: asset_id.into(),
        source_format: SourceFormat::Ifc,
        nodes,
        meshes: std::mem::take(&mut meshes),
        materials,
        warnings: warnings.into_iter().collect(),
        asset_extras: json!({
            "ice3": {
                "coordinateSystem": {
                    "handedness": "right",
                    "upAxis": "y",
                    "linearUnit": "meter"
                },
                "ifc": {
                    "schema": metadata.schema,
                    "sourceOrigin": [source_origin.x, 0.0, source_origin.z],
                }
            }
        }),
    };
    validate_scene_limits(&scene, &limits)?;
    Ok(scene)
}

fn build_geometry(
    source_meshes: &[MeshData],
    source_origin: DVec3,
) -> Result<GeometryProjection, ImportError> {
    let mut meshes: Vec<SourceMesh> = Vec::new();
    let mut materials = Vec::new();
    let mut templates = Vec::new();
    let mut material_by_key = BTreeMap::<MaterialKey, usize>::new();
    let mut mesh_index_by_express = BTreeMap::<u32, usize>::new();

    for source in source_meshes.iter().filter(|mesh| !mesh.is_empty()) {
        let primitive_count = source.indices.len() / 3;
        if primitive_count == 0 {
            continue;
        }
        let positions = convert_positions(source, source_origin)?;
        let normals = convert_normals(source);
        let key = material_key(source);
        let material = if let Some(index) = material_by_key.get(&key) {
            *index
        } else {
            let index = materials.len();
            materials.push(SourceMaterial {
                id: format!("ifc/material/{index}"),
                name: key.name.clone(),
                base_color: rgba_components(key.rgba)[..3]
                    .try_into()
                    .expect("three components"),
                opacity: rgba_components(key.rgba)[3],
                extras: json!({ "ice3": { "ifc": { "materialId": source.material_id } } }),
            });
            material_by_key.insert(key, index);
            index
        };
        let primitive = SourcePrimitive {
            positions,
            normals,
            colors: None,
            indices: source.indices.clone(),
            material: Some(material),
        };
        let mesh_index = if let Some(index) = mesh_index_by_express.get(&source.express_id) {
            meshes[*index].primitives.push(primitive);
            *index
        } else {
            let index = meshes.len();
            meshes.push(SourceMesh {
                id: format!("ifc/mesh/{}", source.express_id),
                primitives: vec![primitive],
                extras: json!({
                    "ice3": {
                        "ifc": {
                            "expressId": source.express_id,
                            "geometryItemId": source.geometry_item_id,
                        }
                    }
                }),
            });
            mesh_index_by_express.insert(source.express_id, index);
            index
        };
        templates.push(GeometryTemplate {
            mesh_index,
            express_id: source.express_id,
        });
    }
    Ok((meshes, materials, templates))
}

fn convert_positions(mesh: &MeshData, source_origin: DVec3) -> Result<Vec<[f32; 3]>, ImportError> {
    if !mesh.positions.len().is_multiple_of(3) {
        return Err(ImportError::InvalidSource(
            "IFC mesh position length is not divisible by three".into(),
        ));
    }
    let vertex_count = mesh.positions.len() / 3;
    if mesh
        .indices
        .iter()
        .any(|index| *index as usize >= vertex_count)
    {
        return Err(ImportError::InvalidSource(
            "IFC mesh index is out of range".into(),
        ));
    }
    mesh.positions
        .as_chunks::<3>()
        .0
        .iter()
        .map(|position| {
            let x = mesh.origin[0] + f64::from(position[0]);
            let y = mesh.origin[1] + f64::from(position[1]);
            let z = mesh.origin[2] + f64::from(position[2]);
            if !x.is_finite() || !y.is_finite() || !z.is_finite() {
                return Err(ImportError::InvalidSource(
                    "IFC mesh contains non-finite positions".into(),
                ));
            }
            let rebased_x = x - source_origin.x;
            let rebased_y = z - source_origin.y;
            let rebased_z = -y - source_origin.z;
            Ok([rebased_x as f32, rebased_y as f32, rebased_z as f32])
        })
        .collect()
}

fn convert_normals(mesh: &MeshData) -> Option<Vec<[f32; 3]>> {
    if mesh.normals.len() != mesh.positions.len() || !mesh.normals.len().is_multiple_of(3) {
        return None;
    }
    mesh.normals
        .as_chunks::<3>()
        .0
        .iter()
        .map(|normal| {
            let values = [normal[0], normal[2], -normal[1]];
            values
                .iter()
                .all(|value| value.is_finite())
                .then_some(values)
        })
        .collect()
}

fn material_key(mesh: &MeshData) -> MaterialKey {
    MaterialKey {
        name: mesh
            .material_name
            .clone()
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| format!("ifc/material/{}", mesh.express_id)),
        rgba: mesh.color.map(normalized_colour_bits),
    }
}

fn normalized_colour_bits(color: f32) -> u32 {
    if !color.is_finite() {
        return 0_f32.to_bits();
    }
    color.clamp(0.0, 1.0).to_bits()
}

fn rgba_components(bits: [u32; 4]) -> [f32; 4] {
    bits.map(f32::from_bits)
}

fn source_origin_for(meshes: &[MeshData], instances: &[InstanceRecord]) -> Option<DVec3> {
    let by_express = meshes
        .iter()
        .map(|mesh| (mesh.express_id, mesh))
        .collect::<BTreeMap<_, _>>();
    let mut min = DVec3::splat(f64::INFINITY);
    for mesh in meshes {
        update_mesh_bounds(mesh, DMat4::IDENTITY, &mut min);
    }
    for instance in instances {
        let Some(mesh) = by_express.get(&instance.template_express_id) else {
            continue;
        };
        update_mesh_bounds(mesh, row_major_to_y_up(instance.transform), &mut min);
    }
    min.is_finite().then_some(DVec3::new(min.x, 0.0, min.z))
}

fn update_mesh_bounds(mesh: &MeshData, transform: DMat4, min: &mut DVec3) {
    for position in mesh.positions.as_chunks::<3>().0 {
        let x = mesh.origin[0] + f64::from(position[0]);
        let y = mesh.origin[1] + f64::from(position[1]);
        let z = mesh.origin[2] + f64::from(position[2]);
        let point = transform.transform_point3(DVec3::new(x, z, -y));
        *min = min.min(point);
    }
}

fn projected_node_count(
    metadata: &IfcMetadata,
    result: &ProcessingResult,
) -> Result<usize, ImportError> {
    let spatial_parents = metadata
        .nodes
        .values()
        .flat_map(|node| node.elements.iter().copied())
        .collect::<BTreeSet<_>>();
    let has_unassigned = metadata
        .products
        .values()
        .any(|product| !spatial_parents.contains(&product.express_id))
        || result.instances.iter().any(|instance| {
            !metadata.products.contains_key(&instance.express_id)
                && !spatial_parents.contains(&instance.express_id)
        });
    let fallback_instances = result
        .instances
        .iter()
        .filter(|instance| !metadata.products.contains_key(&instance.express_id))
        .count();
    projected_node_count_parts(
        metadata.nodes.len(),
        metadata.products.len(),
        fallback_instances,
        has_unassigned,
    )
}

fn projected_node_count_parts(
    spatial_nodes: usize,
    products: usize,
    fallback_instances: usize,
    has_unassigned: bool,
) -> Result<usize, ImportError> {
    spatial_nodes
        .checked_add(products)
        .and_then(|count| count.checked_add(fallback_instances))
        .and_then(|count| count.checked_add(usize::from(has_unassigned)))
        .ok_or_else(|| ImportError::LimitExceeded("IFC node count overflowed".into()))
}

fn add_shared_mesh_groups(nodes: &mut [PendingNode]) {
    let mut counts = BTreeMap::<usize, usize>::new();
    for node in nodes.iter() {
        if let Some(mesh) = node.mesh {
            *counts.entry(mesh).or_default() += 1;
        }
    }
    for node in nodes {
        let Some(mesh) = node.mesh else {
            continue;
        };
        if counts.get(&mesh).copied().unwrap_or(0) <= 1 {
            continue;
        }
        let Some(ifc) = node
            .extras
            .get_mut("ice3")
            .and_then(|value| value.get_mut("ifc"))
        else {
            continue;
        };
        if let Some(object) = ifc.as_object_mut() {
            object
                .entry("instanceGroup")
                .or_insert_with(|| Value::String(format!("ifc/instance/mesh-{mesh}")));
        }
    }
}

fn ordered_spatial_nodes(metadata: &IfcMetadata) -> Vec<u32> {
    let mut order = Vec::new();
    let mut seen = BTreeSet::new();
    let mut stack = vec![metadata.project_id];
    while let Some(express_id) = stack.pop() {
        if !seen.insert(express_id) {
            continue;
        }
        let Some(node) = metadata.nodes.get(&express_id) else {
            continue;
        };
        order.push(express_id);
        for child in node.children.iter().rev() {
            stack.push(*child);
        }
    }
    for express_id in metadata.nodes.keys() {
        if seen.insert(*express_id) {
            order.push(*express_id);
        }
    }
    order
}

fn spatial_extras(node: &IfcNodeMetadata, schema: &str) -> Value {
    json!({
        "ice3": {
            "ifc": {
                "schema": schema,
                "expressId": node.express_id,
                "ifcType": node.ifc_type,
                "name": node.name,
                "spatialPath": node.path,
                "elevation": node.elevation,
            }
        }
    })
}

fn product_extras(
    product: &IfcProductMetadata,
    metadata: &IfcMetadata,
    instance_group: Option<&str>,
) -> Value {
    let spatial_path = metadata
        .nodes
        .values()
        .find(|node| node.elements.contains(&product.express_id))
        .map(|node| node.path.as_str());
    let mut ifc = json!({
        "schema": metadata.schema,
        "expressId": product.express_id,
        "ifcType": product.ifc_type,
        "name": product.name,
        "propertySets": product.property_sets,
        "quantitySets": product.quantity_sets,
        "systems": product.systems,
    });
    if let Some(global_id) = &product.global_id {
        ifc["globalId"] = Value::String(global_id.clone());
    }
    if let Some(description) = &product.description {
        ifc["description"] = Value::String(description.clone());
    }
    if let Some(object_type) = &product.object_type {
        ifc["objectType"] = Value::String(object_type.clone());
    }
    if let Some(predefined_type) = &product.predefined_type {
        ifc["predefinedType"] = Value::String(predefined_type.clone());
    }
    if let Some(tag) = &product.tag {
        ifc["tag"] = Value::String(tag.clone());
    }
    if let Some(path) = spatial_path {
        ifc["spatialPath"] = Value::String(path.into());
    }
    if let Some(group) = instance_group {
        ifc["instanceGroup"] = Value::String(group.into());
    }
    json!({ "ice3": { "ifc": ifc } })
}

fn fallback_instance_product(instance: &InstanceRecord, asset_id: &str) -> IfcProductMetadata {
    IfcProductMetadata {
        express_id: instance.express_id,
        ifc_type: instance.ifc_type.to_ascii_uppercase(),
        global_id: instance.global_id.clone(),
        selection_id: instance.global_id.as_ref().map_or_else(
            || format!("{asset_id}/ifc/express/{}", instance.express_id),
            |id| format!("{asset_id}/ifc/{id}"),
        ),
        name: instance
            .name
            .clone()
            .unwrap_or_else(|| format!("ifc/{}/{}", instance.ifc_type, instance.express_id)),
        description: None,
        object_type: None,
        predefined_type: None,
        tag: None,
        property_sets: Vec::<IfcPropertySetMetadata>::new(),
        quantity_sets: Vec::<IfcQuantitySetMetadata>::new(),
        systems: Vec::<IfcSystemReference>::new(),
    }
}

fn row_major_to_y_up(values: [f32; 16]) -> DMat4 {
    let matrix = DMat4::from_cols_array(&[
        f64::from(values[0]),
        f64::from(values[4]),
        f64::from(values[8]),
        f64::from(values[12]),
        f64::from(values[1]),
        f64::from(values[5]),
        f64::from(values[9]),
        f64::from(values[13]),
        f64::from(values[2]),
        f64::from(values[6]),
        f64::from(values[10]),
        f64::from(values[14]),
        f64::from(values[3]),
        f64::from(values[7]),
        f64::from(values[11]),
        f64::from(values[15]),
    ]);
    let to_y_up = DMat4::from_cols_array(&[
        1.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]);
    to_y_up * matrix * to_y_up.inverse()
}

fn ifc_warning(code: &str, message: String) -> ImportWarning {
    ImportWarning {
        code: code.into(),
        message,
        path: None,
    }
}

fn collect_mesh_warnings(source_meshes: &[MeshData], warnings: &mut BTreeSet<ImportWarning>) {
    for mesh in source_meshes {
        if let Some(texture) = &mesh.texture {
            warnings.insert(ifc_warning(
                "ICE3_STUDIO_IMPORT_IFC_UNSUPPORTED_RESOURCE",
                format!("surface texture {}", texture.texture_id),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ifc_lite_processing::MeshTextureData;

    #[test]
    fn warns_without_failing_on_surface_textures() {
        let mut mesh = MeshData::new(
            42,
            "IfcWall".into(),
            vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
            Vec::new(),
            vec![0, 1, 2],
            [1.0, 1.0, 1.0, 1.0],
        );
        mesh.texture = Some(MeshTextureData {
            texture_id: 7,
            rgba: None,
            width: 0,
            height: 0,
            url: Some("texture.png".into()),
            repeat_s: false,
            repeat_t: false,
        });
        let mut warnings = BTreeSet::new();
        collect_mesh_warnings(&[mesh], &mut warnings);
        assert!(warnings.iter().any(|warning| {
            warning.code == "ICE3_STUDIO_IMPORT_IFC_UNSUPPORTED_RESOURCE"
                && warning.message == "surface texture 7"
        }));
    }

    #[test]
    fn rebases_large_origins_before_narrowing_positions() {
        let mut mesh = MeshData::new(
            9,
            "IfcPump".into(),
            vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
            Vec::new(),
            vec![0, 1, 2],
            [1.0, 1.0, 1.0, 1.0],
        );
        mesh.origin = [16_777_216.0, 0.0, -5.0];
        let origin = source_origin_for(std::slice::from_ref(&mesh), &[]).unwrap();
        let (meshes, _, _) = build_geometry(&[mesh], origin).unwrap();
        let positions = &meshes[0].primitives[0].positions;
        assert!((positions[1][0] - positions[0][0] - 1.0).abs() < 1e-6);
        assert!(
            positions
                .iter()
                .all(|position| { position.iter().all(|value| value.is_finite()) })
        );
    }

    #[test]
    fn node_count_includes_fallback_instances_and_unassigned_container() {
        let count = projected_node_count_parts(5, 3, 2, true).unwrap();
        assert_eq!(count, 11);
    }
}
