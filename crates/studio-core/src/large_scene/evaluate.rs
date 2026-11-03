use std::collections::{BTreeMap, BTreeSet};

use glam::{Mat4, Quat, Vec3 as GlamVec3};
use sha2::{Digest, Sha256};

use crate::document::{Document, NodeContent, SceneNode};
use crate::geometry::{Bounds, EvaluatedMesh, GeometryError, Primitive};
use crate::gpu_contract::{BackendError, ResidentGeometryId};
use crate::ids::{ComponentId, MaterialId, NodeId};
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
    pub resident_geometry_id: Option<ResidentGeometryId>,
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
    pub resident_geometry_ids: BTreeMap<String, ResidentGeometryId>,
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
    #[error("evaluated scene node was not found: {0}")]
    NodeNotFound(String),
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
    evaluate_large_scene_selected_inner(document, store, evaluator, None)
}

pub fn evaluate_large_scene_selected<S: GeometryStore>(
    document: &Document,
    store: &mut S,
    evaluator: Option<&dyn GeometryEvaluator>,
    selected_source_nodes: &BTreeSet<NodeId>,
) -> Result<EvaluatedScene, LargeSceneError> {
    evaluate_large_scene_selected_inner(document, store, evaluator, Some(selected_source_nodes))
}

fn evaluate_large_scene_selected_inner<S: GeometryStore>(
    document: &Document,
    store: &mut S,
    evaluator: Option<&dyn GeometryEvaluator>,
    selected_source_nodes: Option<&BTreeSet<NodeId>>,
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

    for node in document
        .scene
        .nodes()
        .iter()
        .filter(|node| selected_source_nodes.is_none_or(|selected| selected.contains(&node.id)))
    {
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
    let resolved = resolve_part_content(content, parameters)?;
    resolved_part_geometry_key_with_metadata(
        blueprint_digest,
        part_id,
        &resolved,
        &GeometryCacheMetadata::cpu(),
    )
}

fn resolved_part_geometry_key_with_metadata(
    blueprint_digest: &str,
    part_id: &str,
    resolved: &NodeContent,
    metadata: &GeometryCacheMetadata,
) -> Result<String, LargeSceneError> {
    let bytes = serde_json::to_vec(&(
        metadata.backend,
        &metadata.kernel_version,
        &metadata.parity_status,
        blueprint_digest,
        part_id,
        resolved,
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

/// 把派生节点的 world bounds 按源节点聚合。
///
/// 组件实例等源节点的几何分布在派生部件上；`owner_id` 声明归属，取其首个 `/`
/// 段即源节点 ID。聚合结果即源节点的 world bounds（含所有派生部件）。
pub fn aggregate_source_bounds(
    scene: &EvaluatedScene,
    bounds_by_id: &BTreeMap<String, Bounds>,
) -> BTreeMap<String, Bounds> {
    let mut output: BTreeMap<String, Bounds> = BTreeMap::new();
    for node in &scene.nodes {
        let Some(bounds) = bounds_by_id.get(node.id.as_str()) else {
            continue;
        };
        let source_id = node
            .owner_id
            .split('/')
            .next()
            .filter(|candidate| !candidate.is_empty())
            .unwrap_or(node.owner_id.as_str());
        output
            .entry(source_id.to_owned())
            .and_modify(|current| {
                current.min.x = current.min.x.min(bounds.min.x);
                current.min.y = current.min.y.min(bounds.min.y);
                current.min.z = current.min.z.min(bounds.min.z);
                current.max.x = current.max.x.max(bounds.max.x);
                current.max.y = current.max.y.max(bounds.max.y);
                current.max.z = current.max.z.max(bounds.max.z);
            })
            .or_insert(*bounds);
    }
    output
}

pub fn update_transform_only(
    scene: &mut EvaluatedScene,
    node_id: &str,
    transform: Transform,
) -> Result<(), LargeSceneError> {
    let node = scene
        .nodes
        .iter_mut()
        .find(|node| node.id == node_id)
        .ok_or_else(|| LargeSceneError::NodeNotFound(node_id.into()))?;
    node.transform = transform;
    Ok(())
}

pub fn update_material_only(
    scene: &mut EvaluatedScene,
    node_id: &str,
    material_id: Option<MaterialId>,
) -> Result<(), LargeSceneError> {
    let node = scene
        .nodes
        .iter_mut()
        .find(|node| node.id == node_id)
        .ok_or_else(|| LargeSceneError::NodeNotFound(node_id.into()))?;
    node.material_id = material_id;
    Ok(())
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
    // 先展开 repeat：一个定义里的部件可以按索引重复成多个实例几何。
    let expanded: Vec<(&crate::large_scene::component::ComponentPart, u32, u32)> = definition
        .parts
        .iter()
        .flat_map(|part| {
            let count = part.repeat.unwrap_or(1).max(1);
            (0..count).map(move |index| (part, index, count))
        })
        .collect();
    for (part, index, count) in expanded {
        let part_id = if count > 1 {
            format!("{id_prefix}/{}-{}", part.id, index + 1)
        } else {
            format!("{id_prefix}/{}", part.id)
        };
        let parent_id = part
            .parent_id
            .as_ref()
            .map(|parent| format!("{id_prefix}/{parent}"))
            .or_else(|| definition_parent.clone().or_else(|| source_parent.clone()));
        let mut part_parameters = parameters.clone();
        if count > 1 {
            part_parameters.insert("i".into(), ParameterValue::Number(index as f32));
            part_parameters.insert("count".into(), ParameterValue::Number(count as f32));
        }
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
                    resident_geometry_id: None,
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
                    resident_geometry_id: None,
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
                // 声明了挖除区域时，先把长方体拆成若干子盒，再逐个生成节点。
                // 墙上的门窗洞、管道穿墙口都走这条路径。
                #[allow(clippy::collapsible_if)]
                if !part.subtract.is_empty() {
                    if let PartContent::Box {
                        width,
                        height,
                        depth,
                    } = content
                    {
                        let w = resolve_scalar(width, &part_parameters)?;
                        let h = resolve_scalar(height, &part_parameters)?;
                        let d = resolve_scalar(depth, &part_parameters)?;
                        let mut cuts = Vec::with_capacity(part.subtract.len());
                        for cut in &part.subtract {
                            let cw = resolve_scalar(&cut.width, &part_parameters)?;
                            let ch = resolve_scalar(&cut.height, &part_parameters)?;
                            let cd = resolve_scalar(&cut.depth, &part_parameters)?;
                            let cx = resolve_scalar(&cut.x, &part_parameters)?;
                            let cy = resolve_scalar(&cut.y, &part_parameters)?;
                            let cz = resolve_scalar(&cut.z, &part_parameters)?;
                            cuts.push(Block {
                                min: [cx - cw / 2.0, cy - ch / 2.0, cz - cd / 2.0],
                                max: [cx + cw / 2.0, cy + ch / 2.0, cz + cd / 2.0],
                            });
                        }
                        let whole = Block {
                            min: [-w / 2.0, -h / 2.0, -d / 2.0],
                            max: [w / 2.0, h / 2.0, d / 2.0],
                        };
                        for (index, block) in carve_all(whole, &cuts).iter().enumerate() {
                            let size = block.size();
                            let centre = block.centre();
                            let piece_content = PartContent::Box {
                                width: ScalarBinding::Literal(size[0]),
                                height: ScalarBinding::Literal(size[1]),
                                depth: ScalarBinding::Literal(size[2]),
                            };
                            let piece_id = format!("{}-{}", part.id, index + 1);
                            let (mesh, mesh_key, resident_geometry_id) =
                                load_or_evaluate_part_mesh(
                                    store,
                                    &mut scene.stats,
                                    &mut scene.meshes,
                                    blueprint,
                                    &piece_id,
                                    &piece_content,
                                    &part_parameters,
                                    evaluator,
                                )?;
                            let piece_transform = Transform {
                                translation: Vec3 {
                                    x: transform.translation.x + centre[0],
                                    y: transform.translation.y + centre[1],
                                    z: transform.translation.z + centre[2],
                                },
                                rotation: transform.rotation,
                                scale: transform.scale,
                            };
                            scene.stats.effective_triangles += mesh.indices.len() / 3;
                            scene.nodes.push(EvaluatedSceneNode {
                                id: format!("{part_id}-{index}"),
                                owner_id: instance_id.to_owned(),
                                name: format!("{}/{}", source_node.name, piece_id),
                                parent_id: parent_id.clone(),
                                transform: piece_transform,
                                material_id: material_id.clone(),
                                mesh_key: Some(mesh_key),
                                resident_geometry_id,
                                metadata: metadata.clone(),
                                visible,
                            });
                        }
                        continue;
                    }
                }
                let (mesh, mesh_key, mut resident_geometry_id) = load_or_evaluate_part_mesh(
                    store,
                    &mut scene.stats,
                    &mut scene.meshes,
                    blueprint,
                    &part.id,
                    content,
                    &part_parameters,
                    evaluator,
                )?;
                if resident_geometry_id.is_none() {
                    resident_geometry_id = scene.resident_geometry_ids.get(&mesh_key).cloned();
                }
                if let Some(resident_id) = &resident_geometry_id {
                    scene
                        .resident_geometry_ids
                        .insert(mesh_key.clone(), resident_id.clone());
                }
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
                    resident_geometry_id,
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
    if let Some(mesh_key) = &mesh_key
        && let Some(mesh) = scene.meshes.get(mesh_key)
    {
        scene.stats.effective_triangles += mesh.indices.len() / 3;
    }
    scene.nodes.push(EvaluatedSceneNode {
        id: node.id.as_str().to_owned(),
        owner_id: node.id.as_str().to_owned(),
        name: node.name.clone(),
        parent_id: node.parent_id.as_ref().map(ToString::to_string),
        transform: node.transform,
        material_id: node.material_id.clone(),
        mesh_key,
        resident_geometry_id: None,
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
        ScalarBinding::Parameter { param: name } => match parameters.get(name) {
            Some(ParameterValue::Number(value)) => Ok(*value),
            _ => Err(ParameterError::WrongType(name.clone()).into()),
        },
        ScalarBinding::Expression { expr } => {
            // 表达式可以使用该实例的任意数值参数，派生量不再需要硬编码。
            let mut variables = BTreeMap::new();
            for (name, value) in parameters {
                if let ParameterValue::Number(number) = value {
                    variables.insert(name.clone(), *number);
                }
            }
            crate::large_scene::expression::evaluate(expr, &variables)
                .map_err(|error| ParameterError::Expression(format!("{expr}: {error}")).into())
        }
    }
}

fn resolve_vector3(
    binding: &Vector3Binding,
    parameters: &BTreeMap<String, ParameterValue>,
) -> Result<Vec3, LargeSceneError> {
    match binding {
        Vector3Binding::Literal(value) => Ok(*value),
        Vector3Binding::Parameter { param: name } => match parameters.get(name) {
            Some(ParameterValue::Vector3(value)) => Ok(*value),
            _ => Err(ParameterError::WrongType(name.clone()).into()),
        },
        Vector3Binding::Components { x, y, z } => Ok(Vec3 {
            x: resolve_scalar(x, parameters)?,
            y: resolve_scalar(y, parameters)?,
            z: resolve_scalar(z, parameters)?,
        }),
    }
}

fn resolve_material_binding(
    binding: &MaterialBinding,
    parameters: &BTreeMap<String, ParameterValue>,
) -> Result<String, LargeSceneError> {
    match binding {
        MaterialBinding::Literal(value) => Ok(value.clone()),
        MaterialBinding::Parameter { param: name } => match parameters.get(name) {
            Some(ParameterValue::Enumeration(value)) => Ok(value.clone()),
            _ => Err(ParameterError::WrongType(name.clone()).into()),
        },
    }
}

fn resolve_part_transform(
    transform: &PartTransform,
    parameters: &BTreeMap<String, ParameterValue>,
) -> Result<Transform, LargeSceneError> {
    // yaw 给出绕 Y 轴的偏航角（度），比手写四元数更适合环形布置。
    let rotation = match &transform.yaw {
        Some(binding) => {
            let degrees = resolve_scalar(binding, parameters)?;
            let half = degrees.to_radians() / 2.0;
            [0.0, half.sin(), 0.0, half.cos()]
        }
        None => transform.rotation,
    };
    Ok(Transform {
        translation: resolve_vector3(&transform.translation, parameters)?,
        rotation,
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
        resident_geometry_id: None,
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
) -> Result<(EvaluatedMesh, String, Option<ResidentGeometryId>), LargeSceneError> {
    let node_content = resolve_part_content(content, parameters)?;
    let selected_metadata = evaluator
        .map(|evaluator| evaluator.select_cache_metadata(&node_content))
        .unwrap_or_else(GeometryCacheMetadata::cpu);
    let selected_key = resolved_part_geometry_key_with_metadata(
        blueprint,
        part_id,
        &node_content,
        &selected_metadata,
    )?;
    if let Some((mesh, resident_geometry_id)) =
        load_matching_cache_entry(store, stats, meshes, &selected_key, &selected_metadata)
    {
        return Ok((mesh, selected_key, resident_geometry_id));
    }

    let evaluation = if let Some(evaluator) = evaluator {
        evaluator
            .evaluate(&node_content)
            .map_err(LargeSceneError::Backend)?
    } else {
        GeometryEvaluation {
            mesh: crate::geometry::evaluate_node_content(&node_content)?,
            metadata: GeometryCacheMetadata::cpu(),
            resident_geometry_id: None,
        }
    };
    let actual_key = resolved_part_geometry_key_with_metadata(
        blueprint,
        part_id,
        &node_content,
        &evaluation.metadata,
    )?;
    if actual_key != selected_key
        && let Some((mesh, resident_geometry_id)) =
            load_matching_cache_entry(store, stats, meshes, &actual_key, &evaluation.metadata)
    {
        return Ok((mesh, actual_key, resident_geometry_id));
    }

    stats.cache_misses += 1;
    stats.generated_meshes += 1;
    store.store_entry(
        actual_key.clone(),
        crate::large_scene::cache::GeometryCacheEntry {
            mesh: evaluation.mesh.clone(),
            metadata: evaluation.metadata,
            resident_geometry_id: evaluation.resident_geometry_id.clone(),
        },
    );
    meshes.insert(actual_key.clone(), evaluation.mesh.clone());
    Ok((evaluation.mesh, actual_key, evaluation.resident_geometry_id))
}

fn load_matching_cache_entry<S: GeometryStore>(
    store: &S,
    stats: &mut LargeSceneStats,
    meshes: &mut BTreeMap<String, EvaluatedMesh>,
    key: &str,
    expected_metadata: &GeometryCacheMetadata,
) -> Option<(EvaluatedMesh, Option<ResidentGeometryId>)> {
    if let Some(mesh) = meshes.get(key) {
        let resident_geometry_id = store
            .load_entry(key)
            .and_then(|entry| entry.resident_geometry_id);
        return Some((mesh.clone(), resident_geometry_id));
    }
    let cached = store.load_entry(key)?;
    if &cached.metadata != expected_metadata {
        return None;
    }
    stats.cache_hits += 1;
    meshes.insert(key.to_owned(), cached.mesh.clone());
    Some((cached.mesh, cached.resident_geometry_id))
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

/// 轴对齐包围盒，坐标相对部件中心。
#[derive(Clone, Copy, Debug)]
struct Block {
    min: [f32; 3],
    max: [f32; 3],
}

impl Block {
    fn centre(self) -> [f32; 3] {
        [
            (self.min[0] + self.max[0]) / 2.0,
            (self.min[1] + self.max[1]) / 2.0,
            (self.min[2] + self.max[2]) / 2.0,
        ]
    }

    fn size(self) -> [f32; 3] {
        [
            (self.max[0] - self.min[0]).abs(),
            (self.max[1] - self.min[1]).abs(),
            (self.max[2] - self.min[2]).abs(),
        ]
    }

    fn is_degenerate(self) -> bool {
        let size = self.size();
        size[0] < 1e-4 || size[1] < 1e-4 || size[2] < 1e-4
    }
}

/// 用切块把一个盒子拆成最多 6 个互不重叠的子盒。
///
/// 这是轴对齐盒相减的解析解：先按 X 切出左右，剩余的中段再按 Y 切出上下，
/// 最后按 Z 切出前后。整个过程不需要网格运算，也不会产生退化面。
fn carve(block: Block, cut: Block) -> Vec<Block> {
    let intersects =
        (0..3).all(|axis| block.min[axis] < cut.max[axis] && cut.min[axis] < block.max[axis]);
    if !intersects {
        return vec![block];
    }
    let mut pieces = Vec::new();
    let mut push = |min: [f32; 3], max: [f32; 3]| {
        let piece = Block { min, max };
        if !piece.is_degenerate() {
            pieces.push(piece);
        }
    };

    // X 方向：左片、右片
    if cut.min[0] > block.min[0] {
        push(block.min, [cut.min[0], block.max[1], block.max[2]]);
    }
    if cut.max[0] < block.max[0] {
        push([cut.max[0], block.min[1], block.min[2]], block.max);
    }
    let x0 = block.min[0].max(cut.min[0]);
    let x1 = block.max[0].min(cut.max[0]);

    // Y 方向：下片、上片
    if cut.min[1] > block.min[1] {
        push(
            [x0, block.min[1], block.min[2]],
            [x1, cut.min[1], block.max[2]],
        );
    }
    if cut.max[1] < block.max[1] {
        push(
            [x0, cut.max[1], block.min[2]],
            [x1, block.max[1], block.max[2]],
        );
    }
    let y0 = block.min[1].max(cut.min[1]);
    let y1 = block.max[1].min(cut.max[1]);

    // Z 方向：前片、后片
    if cut.min[2] > block.min[2] {
        push([x0, y0, block.min[2]], [x1, y1, cut.min[2]]);
    }
    if cut.max[2] < block.max[2] {
        push([x0, y0, cut.max[2]], [x1, y1, block.max[2]]);
    }
    pieces
}

/// 依次挖去所有区域，返回剩余的子盒。
fn carve_all(block: Block, cuts: &[Block]) -> Vec<Block> {
    let mut current = vec![block];
    for cut in cuts {
        let mut next = Vec::new();
        for piece in current {
            next.extend(carve(piece, *cut));
        }
        current = next;
    }
    current
}

#[cfg(test)]
mod carve_tests {
    use super::{Block, carve, carve_all};

    fn block(min: [f32; 3], max: [f32; 3]) -> Block {
        Block { min, max }
    }

    #[test]
    fn carving_a_through_hole_leaves_four_pieces() {
        // 一块 1×6×20 的墙，中间挖穿一个 1.1 高的洞
        let wall = block([-0.2, -3.0, -10.0], [0.2, 3.0, 10.0]);
        let hole = block([-0.4, -0.55, -0.9], [0.4, 0.55, 0.9]);
        let pieces = carve(wall, hole);
        // 洞上下各一片，左右各一片
        assert_eq!(pieces.len(), 4);
        let volume: f32 = pieces
            .iter()
            .map(|piece| {
                let size = piece.size();
                size[0] * size[1] * size[2]
            })
            .sum();
        let wall_volume = 0.4 * 6.0 * 20.0;
        let hole_volume = 0.4 * 1.1 * 1.8;
        // 挖掉的部分不进入结果
        assert!((volume - (wall_volume - hole_volume)).abs() < 0.5);
    }

    #[test]
    fn carving_outside_keeps_the_block_whole() {
        let wall = block([-0.2, -3.0, -10.0], [0.2, 3.0, 10.0]);
        let far = block([10.0, 10.0, 10.0], [11.0, 11.0, 11.0]);
        assert_eq!(carve(wall, far).len(), 1);
    }

    #[test]
    fn multiple_cuts_apply_to_every_piece() {
        let wall = block([-0.2, -3.0, -10.0], [0.2, 3.0, 10.0]);
        let hole_a = block([-0.4, -0.55, -6.0], [0.4, 0.55, -4.2]);
        let hole_b = block([-0.4, -0.55, 4.2], [0.4, 0.55, 6.0]);
        let pieces = carve_all(wall, &[hole_a, hole_b]);
        // 两个洞把墙切成三段主体，每段再带上下两片
        assert!(pieces.len() >= 4, "两个洞应当把墙体进一步切开");
        for piece in &pieces {
            for hole in [hole_a, hole_b] {
                let overlaps = (0..3).all(|axis| {
                    piece.min[axis] < hole.max[axis] && hole.min[axis] < piece.max[axis]
                });
                assert!(!overlaps, "剩余块不应与挖除区重叠");
            }
        }
    }

    #[test]
    fn degenerate_slices_are_dropped() {
        let wall = block([-0.2, -3.0, -10.0], [0.2, 3.0, 10.0]);
        // 洞贴近顶面，上面那片厚度为 0，应被丢弃
        let hole = block([-0.4, -0.55, -0.9], [0.4, 3.0, 0.9]);
        let pieces = carve(wall, hole);
        assert!(pieces.iter().all(|piece| !piece.is_degenerate()));
    }
}
