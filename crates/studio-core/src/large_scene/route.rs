use std::collections::BTreeMap;

use glam::{Mat4, Quat, Vec3 as GlamVec3};

use crate::document::{Document, NodeContent, SceneNode};
use crate::ids::ComponentId;
use crate::large_scene::component::ComponentDefinition;
use crate::large_scene::connection::{ConnectionIntent, InstancePortRef, RoutePolicy};
use crate::large_scene::evaluate::{compose_transform, expand_instance_layout};
use crate::large_scene::validate::instance_count;
use crate::math::{Transform, Vec3};

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedPort {
    pub position: Vec3,
    pub direction: Vec3,
    pub medium: String,
    pub diameter: f32,
}

#[derive(Debug, thiserror::Error)]
pub enum ConnectionError {
    #[error("connection endpoint does not exist")]
    MissingEndpoint,
    #[error("connection endpoint port does not exist")]
    MissingPort,
    #[error("connection media are incompatible")]
    IncompatibleMedia,
    #[error("connection route is degenerate")]
    Degenerate,
    #[error("connection route policy is invalid")]
    InvalidPolicy,
    #[error("connection transform is invalid")]
    InvalidTransform,
}

pub fn resolve_port(
    document: &Document,
    definitions: &BTreeMap<ComponentId, &ComponentDefinition>,
    reference: &InstancePortRef,
) -> Result<ResolvedPort, ConnectionError> {
    let (node_id, ordinal, port_name) = match reference {
        InstancePortRef::Instance { node_id, port } => (node_id, None, port),
        InstancePortRef::SetItem {
            set_node_id,
            ordinal,
            port,
        } => (set_node_id, Some(*ordinal), port),
    };
    let node = document
        .scene
        .nodes()
        .iter()
        .find(|candidate| candidate.id == *node_id)
        .ok_or(ConnectionError::MissingEndpoint)?;
    let (definition_id, local_transform) = match (&node.content, ordinal) {
        (NodeContent::ComponentInstance(instance), None) => {
            (&instance.definition_id, world_transform(document, node)?)
        }
        (NodeContent::InstanceSet(set), Some(ordinal)) => {
            if ordinal >= instance_count(&set.layout).unwrap_or(0) {
                return Err(ConnectionError::MissingEndpoint);
            }
            let placement = expand_instance_layout(&set.layout)
                .map_err(|_| ConnectionError::InvalidTransform)?
                .into_iter()
                .nth(ordinal as usize)
                .ok_or(ConnectionError::MissingEndpoint)?;
            (
                &set.definition_id,
                compose_transform(world_transform(document, node)?, placement),
            )
        }
        _ => return Err(ConnectionError::MissingEndpoint),
    };
    let definition = definitions
        .get(definition_id)
        .ok_or(ConnectionError::MissingEndpoint)?;
    let port = definition
        .ports
        .iter()
        .find(|port| port.name == *port_name)
        .ok_or(ConnectionError::MissingPort)?;
    let matrix = transform_matrix(local_transform);
    let position = matrix.transform_point3(GlamVec3::new(
        port.position.x,
        port.position.y,
        port.position.z,
    ));
    let direction = matrix
        .transform_vector3(GlamVec3::new(
            port.direction.x,
            port.direction.y,
            port.direction.z,
        ))
        .normalize_or_zero();
    if direction.length_squared() <= f32::EPSILON {
        return Err(ConnectionError::InvalidTransform);
    }
    Ok(ResolvedPort {
        position: Vec3 {
            x: position.x,
            y: position.y,
            z: position.z,
        },
        direction: Vec3 {
            x: direction.x,
            y: direction.y,
            z: direction.z,
        },
        medium: port.medium.clone(),
        diameter: port.diameter,
    })
}

pub fn route_connection(
    document: &Document,
    definitions: &BTreeMap<ComponentId, &ComponentDefinition>,
    connection: &ConnectionIntent,
) -> Result<Vec<[f32; 3]>, ConnectionError> {
    let from = resolve_port(document, definitions, &connection.from)?;
    let to = resolve_port(document, definitions, &connection.to)?;
    if from.medium != connection.medium || to.medium != connection.medium {
        return Err(ConnectionError::IncompatibleMedia);
    }
    route_between_ports(&from, &to, &connection.route_policy)
}

pub fn route_between_ports(
    from: &ResolvedPort,
    to: &ResolvedPort,
    policy: &RoutePolicy,
) -> Result<Vec<[f32; 3]>, ConnectionError> {
    if from.medium != to.medium {
        return Err(ConnectionError::IncompatibleMedia);
    }
    if distance(from.position, to.position) <= f32::EPSILON {
        return Err(ConnectionError::Degenerate);
    }
    let elevation = match policy {
        RoutePolicy::Direct => None,
        RoutePolicy::Orthogonal => Some(from.position.y.max(to.position.y) + 0.5),
        RoutePolicy::Buried { depth } if depth.is_finite() && *depth >= 0.0 => Some(-*depth),
        RoutePolicy::Rack { elevation } if elevation.is_finite() => Some(*elevation),
        _ => return Err(ConnectionError::InvalidPolicy),
    };
    Ok(build_route(
        from.position,
        from.direction,
        to.position,
        to.direction,
        elevation,
    ))
}

fn build_route(
    start: Vec3,
    start_direction: Vec3,
    end: Vec3,
    end_direction: Vec3,
    elevation: Option<f32>,
) -> Vec<[f32; 3]> {
    let Some(elevation) = elevation else {
        return vec![[start.x, start.y, start.z], [end.x, end.y, end.z]];
    };
    let start_stub = add_scaled(start, start_direction, 0.9);
    let end_stub = add_scaled(end, end_direction, 0.9);
    let mut points = Vec::new();
    push_unique(&mut points, [start.x, start.y, start.z]);
    push_unique(&mut points, [start_stub.x, start_stub.y, start_stub.z]);
    push_unique(&mut points, [start_stub.x, elevation, start_stub.z]);
    push_unique(&mut points, [end_stub.x, elevation, start_stub.z]);
    push_unique(&mut points, [end_stub.x, elevation, end_stub.z]);
    push_unique(&mut points, [end_stub.x, end_stub.y, end_stub.z]);
    push_unique(&mut points, [end.x, end.y, end.z]);
    points
}

fn push_unique(points: &mut Vec<[f32; 3]>, point: [f32; 3]) {
    if points.last() != Some(&point) {
        points.push(point);
    }
}

fn distance(left: Vec3, right: Vec3) -> f32 {
    let delta = [right.x - left.x, right.y - left.y, right.z - left.z];
    (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt()
}

fn add_scaled(value: Vec3, direction: Vec3, distance: f32) -> Vec3 {
    Vec3 {
        x: value.x + direction.x * distance,
        y: value.y + direction.y * distance,
        z: value.z + direction.z * distance,
    }
}

fn world_transform(document: &Document, node: &SceneNode) -> Result<Transform, ConnectionError> {
    let mut transforms = Vec::new();
    let mut current = node;
    for _ in 0..=document.scene.nodes().len() {
        transforms.push(current.transform);
        let Some(parent_id) = &current.parent_id else {
            return Ok(transforms
                .iter()
                .rev()
                .fold(Transform::IDENTITY, |parent, child| {
                    compose_transform(parent, *child)
                }));
        };
        current = document
            .scene
            .nodes()
            .iter()
            .find(|candidate| candidate.id == *parent_id)
            .ok_or(ConnectionError::InvalidTransform)?;
    }
    Err(ConnectionError::InvalidTransform)
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
