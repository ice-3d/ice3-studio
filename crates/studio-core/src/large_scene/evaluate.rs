use std::collections::{BTreeMap, BTreeSet};

use glam::{Mat4, Quat, Vec3 as GlamVec3};
use sha2::{Digest, Sha256};

use crate::document::{Document, NodeContent, SceneNode};
use crate::geometry::{Bounds, EvaluatedMesh, GeometryError, Primitive};
use crate::gpu_contract::BackendError;
use crate::ids::{ComponentId, MaterialId};
use crate::large_scene::cache::{
    GeometryCacheMetadata, GeometryEvaluation, GeometryEvaluator, GeometryStore,
    MemoryGeometryStore,
};
use crate::large_scene::component::{
    ComponentDefinition, ComponentInstance, InstanceLayout, InstanceSet, MaterialBinding,
    ParameterValue, PartContent, PartTransform, ScalarBinding, Vector3Binding,
};
use crate::large_scene::route::{ConnectionError, route_connection};
use crate::large_scene::validate::{MAX_INSTANCE_SET_ITEMS, ParameterError, resolve_parameters};
use crate::math::{Transform, Vec3};

#[derive(Debug, Clone, PartialEq)]
pub struct EvaluatedSceneNode {
    pub id: String,
    pub owner_id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub transform: Transform,
    pub material_id: Option<MaterialId>,
    pub mesh_key: Option<String>,
    pub metadata: BTreeMap<String, String>,
    pub visible: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LargeSceneStats {
    pub source_nodes: usize,
    pub expanded_nodes: usize,
    pub unique_meshes: usize,
    pub effective_triangles: usize,
    pub unique_triangles: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub generated_meshes: usize,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EvaluatedScene {
    pub nodes: Vec<EvaluatedSceneNode>,
    pub meshes: BTreeMap<String, EvaluatedMesh>,
    pub stats: LargeSceneStats,
}

#[derive(Debug, thiserror::Error)]
pub enum LargeSceneError {
    #[error(transparent)]
    Parameter(#[from] ParameterError),
    #[error(transparent)]
    Geometry(#[from] GeometryError),
    #[error(transparent)]
    Layout(#[from] LayoutError),
    #[error(transparent)]
    Connection(#[from] ConnectionError),
    #[error("component reference is missing or cyclic")]
    InvalidComponentReference,
    #[error(transparent)]
    Backend(#[from] BackendError),
}

#[derive(Debug, thiserror::Error)]
pub enum LayoutError {
    #[error("instance layout requires at least one item")]
    Empty,
    #[error("instance layout exceeds the configured item limit")]
    TooManyItems,
    #[error("instance layout contains non-finite or invalid values")]
    Invalid,
}

pub fn evaluate_large_scene(document: &Document) -> Result<EvaluatedScene, LargeSceneError> {
    let mut store = MemoryGeometryStore::default();
    evaluate_large_scene_with_cache(document, &mut store)
}

pub fn evaluate_large_scene_with_cache<S: GeometryStore>(
    document: &Document,
    store: &mut S,
) -> Result<EvaluatedScene, LargeSceneError> {
    evaluate_large_scene_with_cache_and_evaluator(document, store, None)
}

pub fn evaluate_large_scene_with_cache_and_evaluator<S: GeometryStore>(
    document: &Document,
    store: &mut S,
    evaluator: Option<&dyn GeometryEvaluator>,
) -> Result<EvaluatedScene, LargeSceneError> {
    let definitions = document
        .components
        .iter()
        .map(|definition| (definition.id.clone(), definition))
        .collect::<BTreeMap<_, _>>();
    let mut scene = EvaluatedScene {
        stats: LargeSceneStats {
            source_nodes: document.scene.nodes().len(),
            ..LargeSceneStats::default()
        },
        ..EvaluatedScene::default()
    };

    for node in document.scene.nodes() {
        match &node.content {
            NodeContent::ComponentInstance(instance) => {
                let definition = definitions
                    .get(&instance.definition_id)
                    .ok_or(LargeSceneError::InvalidComponentReference)?;
                expand_instance(
                    node,
                    instance,
                    definition,
                    &definitions,
                    store,
                    evaluator,
                    &mut scene,
                )?;
            }
            NodeContent::InstanceSet(set) => {
                let definition = definitions
                    .get(&set.definition_id)
                    .ok_or(LargeSceneError::InvalidComponentReference)?;
                expand_instance_set(
                    node,
                    set,
                    definition,
                    &definitions,
                    store,
                    evaluator,
                    &mut scene,
                )?;
            }
            NodeContent::Connection(connection) => {
                append_connection(node, connection, document, &definitions, store, &mut scene)?;
            }
            content => append_legacy_node(node, content, store, &mut scene)?,
        }
    }
    scene.stats.expanded_nodes = scene.nodes.len();
    scene.stats.unique_meshes = scene.meshes.len();
    scene.stats.unique_triangles = scene
        .meshes
        .values()
        .map(|mesh| mesh.indices.len() / 3)
        .sum();
    Ok(scene)
}

pub fn resolved_part_geometry_key(
    blueprint_digest: &str,
    part_id: &str,
    content: &PartContent,
    parameters: &BTreeMap<String, ParameterValue>,
) -> Result<String, LargeSceneError> {
    resolved_part_geometry_key_with_metadata(
        blueprint_digest,
        part_id,
        content,
        parameters,
        &GeometryCacheMetadata::cpu(),
    )
}

fn resolved_part_geometry_key_with_metadata(
    blueprint_digest: &str,
    part_id: &str,
    content: &PartContent,
    parameters: &BTreeMap<String, ParameterValue>,
    metadata: &GeometryCacheMetadata,
) -> Result<String, LargeSceneError> {
    let bytes = serde_json::to_vec(&(
        metadata.backend,
        &metadata.kernel_version,
        &metadata.parity_status,
        blueprint_digest,
        part_id,
        content,
        parameters,
    ))
    .map_err(|_| LargeSceneError::InvalidComponentReference)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub fn evaluated_scene_bounds(scene: &EvaluatedScene) -> BTreeMap<String, Bounds> {
    let by_id = scene
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    let mut output = BTreeMap::new();
    for node in &scene.nodes {
        let Some(mesh_key) = &node.mesh_key else {
            continue;
        };
        let Some(mesh) = scene.meshes.get(mesh_key) else {
            continue;
        };
        let mut transforms = Vec::new();
        let mut current = Some(node);
        while let Some(candidate) = current {
            transforms.push(candidate.transform);
            current = candidate
                .parent_id
                .as_deref()
                .and_then(|parent| by_id.get(parent).copied());
        }
        let matrix = transforms
            .iter()
            .rev()
            .fold(Mat4::IDENTITY, |parent, child| {
                parent * transform_matrix(*child)
            });
        let mut minimum = [f32::INFINITY; 3];
        let mut maximum = [f32::NEG_INFINITY; 3];
        for x in [mesh.bounds.min.x, mesh.bounds.max.x] {
            for y in [mesh.bounds.min.y, mesh.bounds.max.y] {
                for z in [mesh.bounds.min.z, mesh.bounds.max.z] {
                    let point = matrix.transform_point3(GlamVec3::new(x, y, z));
                    minimum = [
                        minimum[0].min(point.x),
                        minimum[1].min(point.y),
                        minimum[2].min(point.z),
                    ];
                    maximum = [
                        maximum[0].max(point.x),
                        maximum[1].max(point.y),
                        maximum[2].max(point.z),
                    ];
                }
            }
        }
        output.insert(
            node.id.clone(),
            Bounds {
                min: Vec3 {
                    x: minimum[0],
                    y: minimum[1],
                    z: minimum[2],
                },
                max: Vec3 {
                    x: maximum[0],
                    y: maximum[1],
                    z: maximum[2],
                },
            },
        );
    }
    output
}

pub fn expand_instance_layout(layout: &InstanceLayout) -> Result<Vec<Transform>, LayoutError> {
    match layout {
        InstanceLayout::Linear {
            origin,
            direction,
            spacing,
            count,
        } => {
            validate_count(*count)?;
            validate_vec3(*origin)?;
            let direction = normalize(*direction)?;
            validate_positive_finite(*spacing)?;
            Ok((0..*count)
                .map(|ordinal| {
                    let mut transform = Transform::IDENTITY;
                    transform.translation = Vec3 {
                        x: origin.x + direction.x * (*spacing) * ordinal as f32,
                        y: origin.y + direction.y * (*spacing) * ordinal as f32,
                        z: origin.z + direction.z * (*spacing) * ordinal as f32,
                    };
                    transform
                })
                .collect())
        }
        InstanceLayout::Grid {
            origin,
            x_direction,
            z_direction,
            rows,
            columns,
            row_spacing,
            column_spacing,
        } => {
            validate_count(*rows)?;
            validate_count(*columns)?;
            let total = (*rows)
                .checked_mul(*columns)
                .ok_or(LayoutError::TooManyItems)?;
            if total > MAX_INSTANCE_SET_ITEMS {
                return Err(LayoutError::TooManyItems);
            }
            validate_vec3(*origin)?;
            let x = normalize(*x_direction)?;
            let z = normalize(*z_direction)?;
            validate_positive_finite(*row_spacing)?;
            validate_positive_finite(*column_spacing)?;
            let mut placements = Vec::with_capacity(total as usize);
            for row in 0..*rows {
                for column in 0..*columns {
                    let mut transform = Transform::IDENTITY;
                    transform.translation = Vec3 {
                        x: origin.x
                            + x.x * (*column_spacing) * column as f32
                            + z.x * (*row_spacing) * row as f32,
                        y: origin.y
                            + x.y * (*column_spacing) * column as f32
                            + z.y * (*row_spacing) * row as f32,
                        z: origin.z
                            + x.z * (*column_spacing) * column as f32
                            + z.z * (*row_spacing) * row as f32,
                    };
                    placements.push(transform);
                }
            }
            Ok(placements)
        }
        InstanceLayout::Radial {
            centre,
            radius,
            start_angle_radians,
            count,
        } => {
            validate_count(*count)?;
            validate_vec3(*centre)?;
            validate_positive_finite(*radius)?;
            if !start_angle_radians.is_finite() {
                return Err(LayoutError::Invalid);
            }
            Ok((0..*count)
                .map(|ordinal| {
                    let angle = *start_angle_radians
                        + std::f32::consts::TAU * ordinal as f32 / *count as f32;
                    let mut transform = Transform::IDENTITY;
                    transform.translation = Vec3 {
                        x: centre.x + (*radius) * angle.cos(),
                        y: centre.y,
                        z: centre.z + (*radius) * angle.sin(),
                    };
                    transform.rotation = Quat::from_rotation_y(-angle).to_array();
                    transform
                })
                .collect())
        }
        InstanceLayout::Path { placements } => {
            if placements.is_empty() {
                return Err(LayoutError::Empty);
            }
            if placements.len() > MAX_INSTANCE_SET_ITEMS as usize {
                return Err(LayoutError::TooManyItems);
            }
            if placements.iter().any(|transform| {
                !vec3_is_finite(transform.translation)
                    || !vec3_is_finite(transform.scale)
                    || transform.scale.x <= 0.0
                    || transform.scale.y <= 0.0
                    || transform.scale.z <= 0.0
                    || !transform.rotation.iter().all(|value| value.is_finite())
                    || transform
                        .rotation
                        .iter()
                        .map(|value| value * value)
                        .sum::<f32>()
                        <= f32::EPSILON
            }) {
                return Err(LayoutError::Invalid);
            }
            Ok(placements.clone())
        }
    }
}

fn expand_instance(
    node: &SceneNode,
    instance: &ComponentInstance,
    definition: &ComponentDefinition,
    definitions: &BTreeMap<ComponentId, &ComponentDefinition>,
    store: &mut impl GeometryStore,
    evaluator: Option<&dyn GeometryEvaluator>,
    scene: &mut EvaluatedScene,
) -> Result<(), LargeSceneError> {
    let instance_id = node.id.as_str().to_owned();
    expand_instance_with_ids(
        node,
        instance,
        definition,
        definitions,
        &instance_id,
        &instance_id,
        node.transform,
        store,
        evaluator,
        scene,
    )
}

fn expand_instance_set(
    node: &SceneNode,
    set: &InstanceSet,
    definition: &ComponentDefinition,
    definitions: &BTreeMap<ComponentId, &ComponentDefinition>,
    store: &mut impl GeometryStore,
    evaluator: Option<&dyn GeometryEvaluator>,
    scene: &mut EvaluatedScene,
) -> Result<(), LargeSceneError> {
    for (ordinal, placement) in expand_instance_layout(&set.layout)?.into_iter().enumerate() {
        let ordinal = u32::try_from(ordinal).map_err(|_| LayoutError::TooManyItems)?;
        let mut parameters = set.shared_parameters.clone();
        if let Some(overrides) = set.item_overrides.get(&ordinal) {
            parameters.extend(overrides.clone());
        }
        let mut item_node = node.clone();
        item_node.transform = compose_transform(node.transform, placement);
        item_node.metadata = set.metadata.clone();
        let owner = format!("{}/{:06}", node.id.as_str(), ordinal);
        let instance = ComponentInstance {
            definition_id: set.definition_id.clone(),
            parameters,
            overrides: BTreeMap::new(),
            metadata: set.metadata.clone(),
        };
        expand_instance_with_ids(
            &item_node,
            &instance,
            definition,
            definitions,
            &owner,
            &owner,
            item_node.transform,
            store,
            evaluator,
            scene,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn expand_instance_with_ids(
    node: &SceneNode,
    instance: &ComponentInstance,
    definition: &ComponentDefinition,
    definitions: &BTreeMap<ComponentId, &ComponentDefinition>,
    owner_id: &str,
    id_prefix: &str,
    root_transform: Transform,
    store: &mut impl GeometryStore,
    evaluator: Option<&dyn GeometryEvaluator>,
    scene: &mut EvaluatedScene,
) -> Result<(), LargeSceneError> {
    let parameters = resolve_parameters(definition, &instance.parameters)?;
    let blueprint = blueprint_digest(definition.id.clone(), definitions)?;
    let mut path = BTreeSet::new();
    expand_definition_parts(
        definition,
        instance,
        definitions,
        node,
        &parameters,
        &blueprint,
        id_prefix,
        owner_id,
        None,
        root_transform,
        node.parent_id.as_ref().map(ToString::to_string),
        store,
        evaluator,
        &mut path,
        scene,
    )
}

#[allow(clippy::too_many_arguments)]
fn expand_definition_parts(
    definition: &ComponentDefinition,
    instance: &ComponentInstance,
    definitions: &BTreeMap<ComponentId, &ComponentDefinition>,
    source_node: &SceneNode,
    parameters: &BTreeMap<String, ParameterValue>,
    blueprint: &str,
    id_prefix: &str,
    instance_id: &str,
    definition_parent: Option<String>,
    root_transform: Transform,
    source_parent: Option<String>,
    store: &mut impl GeometryStore,
    evaluator: Option<&dyn GeometryEvaluator>,
    path: &mut BTreeSet<ComponentId>,
    scene: &mut EvaluatedScene,
) -> Result<(), LargeSceneError> {
    if !path.insert(definition.id.clone()) {
        return Err(LargeSceneError::InvalidComponentReference);
    }
    for part in &definition.parts {
        let part_id = format!("{id_prefix}/{}", part.id);
        let parent_id = part
            .parent_id
            .as_ref()
            .map(|parent| format!("{id_prefix}/{parent}"))
            .or_else(|| definition_parent.clone().or_else(|| source_parent.clone()));
        let mut part_parameters = parameters.clone();
        if let Some(override_value) = instance.overrides.get(&part.id) {
            for (name, value) in &override_value.parameters {
                part_parameters.insert(name.clone(), value.clone());
            }
        }
        let local_transform = resolve_part_transform(&part.transform, &part_parameters)?;
        let transform = if definition_parent.is_none() {
            compose_transform(root_transform, local_transform)
        } else {
            local_transform
        };
        let override_value = instance.overrides.get(&part.id);
        let visible = source_node.visible
            && part.visible
            && override_value
                .and_then(|value| value.visible)
                .unwrap_or(true);
        let material_id = override_value
            .and_then(|value| value.material_id.clone())
            .or_else(|| source_node.material_id.clone());
        let mut metadata = source_node.metadata.clone();
        metadata.extend(part.metadata.clone());
        if let Some(value) = override_value.and_then(|value| value.metadata.clone()) {
            metadata.extend(value);
        }
        if let Some(role) = &part.material_role {
            metadata.insert(
                "materialRole".into(),
                resolve_material_binding(role, parameters)?,
            );
        }

        match &part.content {
            PartContent::Group => {
                scene.nodes.push(EvaluatedSceneNode {
                    id: part_id,
                    owner_id: instance_id.to_owned(),
                    name: format!("{}/{}", source_node.name, part.id),
                    parent_id,
                    transform,
                    material_id,
                    mesh_key: None,
                    metadata,
                    visible,
                });
            }
            PartContent::ComponentReference {
                definition_id,
                parameters: supplied,
            } => {
                let nested_definition = definitions
                    .get(definition_id)
                    .ok_or(LargeSceneError::InvalidComponentReference)?;
                let nested_parameters = resolve_parameters(nested_definition, supplied)?;
                let nested_blueprint = blueprint_digest(definition_id.clone(), definitions)?;
                let owner = format!("{instance_id}/{}", part.id);
                scene.nodes.push(EvaluatedSceneNode {
                    id: part_id.clone(),
                    owner_id: owner.clone(),
                    name: format!("{}/{}", source_node.name, part.id),
                    parent_id: parent_id.clone(),
                    transform,
                    material_id: material_id.clone(),
                    mesh_key: None,
                    metadata: metadata.clone(),
                    visible,
                });
                expand_definition_parts(
                    nested_definition,
                    instance,
                    definitions,
                    source_node,
                    &nested_parameters,
                    &nested_blueprint,
                    &part_id,
                    &owner,
                    Some(part_id.clone()),
                    Transform::IDENTITY,
                    parent_id,
                    store,
                    evaluator,
                    path,
                    scene,
                )?;
            }
            content => {
                let (mesh, mesh_key) = load_or_evaluate_part_mesh(
                    store,
                    &mut scene.stats,
                    &mut scene.meshes,
                    blueprint,
                    &part.id,
                    content,
                    &part_parameters,
                    evaluator,
                )?;
                let triangle_count = mesh.indices.len() / 3;
                scene.stats.effective_triangles += triangle_count;
                scene.nodes.push(EvaluatedSceneNode {
                    id: part_id,
                    owner_id: instance_id.to_owned(),
                    name: format!("{}/{}", source_node.name, part.id),
                    parent_id,
                    transform,
                    material_id,
                    mesh_key: Some(mesh_key),
                    metadata,
                    visible,
                });
            }
        }
    }
    path.remove(&definition.id);
    Ok(())
}

fn append_legacy_node(
    node: &SceneNode,
    content: &NodeContent,
    store: &mut impl GeometryStore,
    scene: &mut EvaluatedScene,
) -> Result<(), LargeSceneError> {
    let mesh = crate::geometry::evaluate_node_content(content)?;
    let mesh_key = if mesh.vertices.is_empty() {
        None
    } else {
        let bytes =
            serde_json::to_vec(content).map_err(|_| LargeSceneError::InvalidComponentReference)?;
        let key = format!("legacy-{:x}", Sha256::digest(bytes));
        load_or_evaluate_mesh(store, &key, &mut scene.stats, &mut scene.meshes, || {
            Ok(mesh)
        })?;
        Some(key)
    };
    scene.nodes.push(EvaluatedSceneNode {
        id: node.id.as_str().to_owned(),
        owner_id: node.id.as_str().to_owned(),
        name: node.name.clone(),
        parent_id: node.parent_id.as_ref().map(ToString::to_string),
        transform: node.transform,
        material_id: node.material_id.clone(),
        mesh_key,
        metadata: node.metadata.clone(),
        visible: node.visible,
    });
    Ok(())
}

fn blueprint_digest(
    root: ComponentId,
    definitions: &BTreeMap<ComponentId, &ComponentDefinition>,
) -> Result<String, LargeSceneError> {
    fn collect<'a>(
        id: &ComponentId,
        definitions: &'a BTreeMap<ComponentId, &'a ComponentDefinition>,
        seen: &mut BTreeSet<ComponentId>,
        output: &mut Vec<&'a ComponentDefinition>,
    ) -> Result<(), LargeSceneError> {
        if !seen.insert(id.clone()) {
            return Err(LargeSceneError::InvalidComponentReference);
        }
        let definition = definitions
            .get(id)
            .ok_or(LargeSceneError::InvalidComponentReference)?;
        output.push(definition);
        for part in &definition.parts {
            if let PartContent::ComponentReference { definition_id, .. } = &part.content {
                collect(definition_id, definitions, seen, output)?;
            }
        }
        seen.remove(id);
        Ok(())
    }

    let mut seen = BTreeSet::new();
    let mut dependencies = Vec::new();
    collect(&root, definitions, &mut seen, &mut dependencies)?;
    let bytes = serde_json::to_vec(&dependencies)
        .map_err(|_| LargeSceneError::InvalidComponentReference)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn resolve_part_content(
    content: &PartContent,
    parameters: &BTreeMap<String, ParameterValue>,
) -> Result<NodeContent, LargeSceneError> {
    Ok(match content {
        PartContent::Group => NodeContent::Group,
        PartContent::Box {
            width,
            height,
            depth,
        } => NodeContent::Primitive(Primitive::Box {
            width: resolve_scalar(width, parameters)?,
            height: resolve_scalar(height, parameters)?,
            depth: resolve_scalar(depth, parameters)?,
        }),
        PartContent::Plane { width, depth } => NodeContent::Primitive(Primitive::Plane {
            width: resolve_scalar(width, parameters)?,
            depth: resolve_scalar(depth, parameters)?,
        }),
        PartContent::Cylinder {
            radius,
            height,
            segments,
        } => NodeContent::Primitive(Primitive::Cylinder {
            radius: resolve_scalar(radius, parameters)?,
            height: resolve_scalar(height, parameters)?,
            segments: *segments,
        }),
        PartContent::Sphere {
            radius,
            segments,
            rings,
        } => NodeContent::Primitive(Primitive::Sphere {
            radius: resolve_scalar(radius, parameters)?,
            segments: *segments,
            rings: *rings,
        }),
        PartContent::CustomMesh { mesh } => NodeContent::CustomMesh(mesh.clone()),
        PartContent::Sweep {
            points,
            radius,
            radial_segments,
        } => NodeContent::Sweep {
            points: points.clone(),
            radius: resolve_scalar(radius, parameters)?,
            radial_segments: *radial_segments,
        },
        PartContent::ComponentReference { .. } => NodeContent::Group,
    })
}

fn resolve_scalar(
    binding: &ScalarBinding,
    parameters: &BTreeMap<String, ParameterValue>,
) -> Result<f32, LargeSceneError> {
    match binding {
        ScalarBinding::Literal(value) => Ok(*value),
        ScalarBinding::Parameter(name) => match parameters.get(name) {
            Some(ParameterValue::Number(value)) => Ok(*value),
            _ => Err(ParameterError::WrongType(name.clone()).into()),
        },
    }
}

fn resolve_vector3(
    binding: &Vector3Binding,
    parameters: &BTreeMap<String, ParameterValue>,
) -> Result<Vec3, LargeSceneError> {
    match binding {
        Vector3Binding::Literal(value) => Ok(*value),
        Vector3Binding::Parameter(name) => match parameters.get(name) {
            Some(ParameterValue::Vector3(value)) => Ok(*value),
            _ => Err(ParameterError::WrongType(name.clone()).into()),
        },
    }
}

fn resolve_material_binding(
    binding: &MaterialBinding,
    parameters: &BTreeMap<String, ParameterValue>,
) -> Result<String, LargeSceneError> {
    match binding {
        MaterialBinding::Literal(value) => Ok(value.clone()),
        MaterialBinding::Parameter(name) => match parameters.get(name) {
            Some(ParameterValue::Enumeration(value)) => Ok(value.clone()),
            _ => Err(ParameterError::WrongType(name.clone()).into()),
        },
    }
}

fn resolve_part_transform(
    transform: &PartTransform,
    parameters: &BTreeMap<String, ParameterValue>,
) -> Result<Transform, LargeSceneError> {
    Ok(Transform {
        translation: resolve_vector3(&transform.translation, parameters)?,
        rotation: transform.rotation,
        scale: resolve_vector3(&transform.scale, parameters)?,
    })
}

pub(crate) fn compose_transform(parent: Transform, child: Transform) -> Transform {
    let parent = transform_matrix(parent);
    let child = transform_matrix(child);
    let (scale, rotation, translation) = (parent * child).to_scale_rotation_translation();
    Transform {
        translation: Vec3 {
            x: translation.x,
            y: translation.y,
            z: translation.z,
        },
        rotation: rotation.to_array(),
        scale: Vec3 {
            x: scale.x,
            y: scale.y,
            z: scale.z,
        },
    }
}

fn append_connection(
    node: &SceneNode,
    connection: &crate::large_scene::connection::ConnectionIntent,
    document: &Document,
    definitions: &BTreeMap<ComponentId, &ComponentDefinition>,
    store: &mut impl GeometryStore,
    scene: &mut EvaluatedScene,
) -> Result<(), LargeSceneError> {
    let points = route_connection(document, definitions, connection)?;
    let bytes = serde_json::to_vec(&(
        node.id.as_str(),
        &connection.from,
        &connection.to,
        &connection.route_policy,
        connection.diameter,
        &points,
    ))
    .map_err(|_| LargeSceneError::InvalidComponentReference)?;
    let mesh_key = format!("connection-{:x}", Sha256::digest(bytes));
    let mesh = load_or_evaluate_mesh(
        store,
        &mesh_key,
        &mut scene.stats,
        &mut scene.meshes,
        || {
            crate::geometry::evaluate_node_content(&NodeContent::Sweep {
                points,
                radius: connection.diameter / 2.0,
                radial_segments: 12,
            })
            .map_err(LargeSceneError::Geometry)
        },
    )?;
    let triangle_count = mesh.indices.len() / 3;
    scene.nodes.push(EvaluatedSceneNode {
        id: format!("{}/route", node.id.as_str()),
        owner_id: node.id.as_str().to_owned(),
        name: format!("{}/route", node.name),
        parent_id: node.parent_id.as_ref().map(ToString::to_string),
        transform: Transform::IDENTITY,
        material_id: node.material_id.clone(),
        mesh_key: Some(mesh_key),
        metadata: node.metadata.clone(),
        visible: node.visible,
    });
    scene.stats.effective_triangles += triangle_count;
    Ok(())
}

fn transform_matrix(transform: Transform) -> Mat4 {
    Mat4::from_scale_rotation_translation(
        GlamVec3::new(transform.scale.x, transform.scale.y, transform.scale.z),
        Quat::from_array(transform.rotation),
        GlamVec3::new(
            transform.translation.x,
            transform.translation.y,
            transform.translation.z,
        ),
    )
}

fn validate_count(count: u32) -> Result<(), LayoutError> {
    if count == 0 {
        Err(LayoutError::Empty)
    } else if count > MAX_INSTANCE_SET_ITEMS {
        Err(LayoutError::TooManyItems)
    } else {
        Ok(())
    }
}

fn validate_positive_finite(value: f32) -> Result<(), LayoutError> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(LayoutError::Invalid)
    }
}

fn validate_vec3(value: Vec3) -> Result<(), LayoutError> {
    if vec3_is_finite(value) {
        Ok(())
    } else {
        Err(LayoutError::Invalid)
    }
}

fn normalize(value: Vec3) -> Result<Vec3, LayoutError> {
    validate_vec3(value)?;
    let length = (value.x * value.x + value.y * value.y + value.z * value.z).sqrt();
    if !length.is_finite() || length <= f32::EPSILON {
        return Err(LayoutError::Invalid);
    }
    Ok(Vec3 {
        x: value.x / length,
        y: value.y / length,
        z: value.z / length,
    })
}

fn vec3_is_finite(value: Vec3) -> bool {
    value.x.is_finite() && value.y.is_finite() && value.z.is_finite()
}

#[allow(clippy::too_many_arguments)]
fn load_or_evaluate_part_mesh<S: GeometryStore>(
    store: &mut S,
    stats: &mut LargeSceneStats,
    meshes: &mut BTreeMap<String, EvaluatedMesh>,
    blueprint: &str,
    part_id: &str,
    content: &PartContent,
    parameters: &BTreeMap<String, ParameterValue>,
    evaluator: Option<&dyn GeometryEvaluator>,
) -> Result<(EvaluatedMesh, String), LargeSceneError> {
    let node_content = resolve_part_content(content, parameters)?;
    let selected_metadata = evaluator
        .map(|evaluator| evaluator.select_cache_metadata(&node_content))
        .unwrap_or_else(GeometryCacheMetadata::cpu);
    let selected_key = resolved_part_geometry_key_with_metadata(
        blueprint,
        part_id,
        content,
        parameters,
        &selected_metadata,
    )?;
    if let Some(mesh) =
        load_matching_cache_entry(store, stats, meshes, &selected_key, &selected_metadata)
    {
        return Ok((mesh, selected_key));
    }

    let evaluation = if let Some(evaluator) = evaluator {
        evaluator
            .evaluate(&node_content)
            .map_err(LargeSceneError::Backend)?
    } else {
        GeometryEvaluation {
            mesh: crate::geometry::evaluate_node_content(&node_content)?,
            metadata: GeometryCacheMetadata::cpu(),
        }
    };
    let actual_key = resolved_part_geometry_key_with_metadata(
        blueprint,
        part_id,
        content,
        parameters,
        &evaluation.metadata,
    )?;
    if actual_key != selected_key
        && let Some(mesh) =
            load_matching_cache_entry(store, stats, meshes, &actual_key, &evaluation.metadata)
    {
        return Ok((mesh, actual_key));
    }

    stats.cache_misses += 1;
    stats.generated_meshes += 1;
    store.store_entry(
        actual_key.clone(),
        crate::large_scene::cache::GeometryCacheEntry {
            mesh: evaluation.mesh.clone(),
            metadata: evaluation.metadata,
        },
    );
    meshes.insert(actual_key.clone(), evaluation.mesh.clone());
    Ok((evaluation.mesh, actual_key))
}

fn load_matching_cache_entry<S: GeometryStore>(
    store: &S,
    stats: &mut LargeSceneStats,
    meshes: &mut BTreeMap<String, EvaluatedMesh>,
    key: &str,
    expected_metadata: &GeometryCacheMetadata,
) -> Option<EvaluatedMesh> {
    if let Some(mesh) = meshes.get(key) {
        return Some(mesh.clone());
    }
    let cached = store.load_entry(key)?;
    if &cached.metadata != expected_metadata {
        return None;
    }
    stats.cache_hits += 1;
    meshes.insert(key.to_owned(), cached.mesh.clone());
    Some(cached.mesh)
}

fn load_or_evaluate_mesh<S: GeometryStore>(
    store: &mut S,
    key: &str,
    stats: &mut LargeSceneStats,
    meshes: &mut BTreeMap<String, EvaluatedMesh>,
    evaluate: impl FnOnce() -> Result<EvaluatedMesh, LargeSceneError>,
) -> Result<EvaluatedMesh, LargeSceneError> {
    if let Some(mesh) = meshes.get(key) {
        return Ok(mesh.clone());
    }
    if let Some(mesh) = store.load(key) {
        stats.cache_hits += 1;
        meshes.insert(key.to_owned(), mesh.clone());
        return Ok(mesh);
    }
    stats.cache_misses += 1;
    let mesh = evaluate()?;
    stats.generated_meshes += 1;
    store.store(key.to_owned(), mesh.clone());
    meshes.insert(key.to_owned(), mesh.clone());
    Ok(mesh)
}
