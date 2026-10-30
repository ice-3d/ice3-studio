use std::collections::{BTreeMap, BTreeSet};

use glam::{Mat4, Quat, Vec3 as GlamVec3};
use sha2::{Digest, Sha256};

use crate::document::{Document, NodeContent, SceneNode};
use crate::geometry::{EvaluatedMesh, GeometryError, Primitive};
use crate::ids::{ComponentId, MaterialId};
use crate::large_scene::component::{
    ComponentDefinition, ComponentInstance, MaterialBinding, ParameterValue, PartContent,
    PartTransform, ScalarBinding, Vector3Binding,
};
use crate::large_scene::validate::{ParameterError, resolve_parameters};
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
    #[error("component reference is missing or cyclic")]
    InvalidComponentReference,
}

pub fn evaluate_large_scene(document: &Document) -> Result<EvaluatedScene, LargeSceneError> {
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
                expand_instance(node, instance, definition, &definitions, &mut scene)?;
            }
            NodeContent::InstanceSet(_) | NodeContent::Connection(_) => {}
            content => append_legacy_node(node, content, &mut scene)?,
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
    let bytes = serde_json::to_vec(&(
        "cpu",
        "geometry-kernel-v1",
        blueprint_digest,
        part_id,
        content,
        parameters,
    ))
    .map_err(|_| LargeSceneError::InvalidComponentReference)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn expand_instance(
    node: &SceneNode,
    instance: &ComponentInstance,
    definition: &ComponentDefinition,
    definitions: &BTreeMap<ComponentId, &ComponentDefinition>,
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
        &node.id.as_str().to_owned(),
        &node.id.as_str().to_owned(),
        None,
        node.transform,
        node.parent_id.as_ref().map(ToString::to_string),
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
                    path,
                    scene,
                )?;
            }
            content => {
                let mesh_key =
                    resolved_part_geometry_key(blueprint, &part.id, content, &part_parameters)?;
                let mesh = evaluate_part_content(content, &part_parameters)?;
                let triangle_count = mesh.indices.len() / 3;
                scene.stats.effective_triangles += triangle_count;
                scene.meshes.entry(mesh_key.clone()).or_insert(mesh);
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
    scene: &mut EvaluatedScene,
) -> Result<(), LargeSceneError> {
    let mesh = crate::geometry::evaluate_node_content(content)?;
    let mesh_key = if mesh.vertices.is_empty() {
        None
    } else {
        let bytes =
            serde_json::to_vec(content).map_err(|_| LargeSceneError::InvalidComponentReference)?;
        let key = format!("legacy-{:x}", Sha256::digest(bytes));
        scene.meshes.entry(key.clone()).or_insert(mesh);
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

fn evaluate_part_content(
    content: &PartContent,
    parameters: &BTreeMap<String, ParameterValue>,
) -> Result<EvaluatedMesh, LargeSceneError> {
    let content = match content {
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
    };
    crate::geometry::evaluate_node_content(&content).map_err(LargeSceneError::Geometry)
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

fn compose_transform(parent: Transform, child: Transform) -> Transform {
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
