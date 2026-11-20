use std::collections::{BTreeMap, BTreeSet};

use glam::{Mat4, Quat, Vec3 as GlamVec3};
use serde::Serialize;

use crate::document::{Document, NodeContent, SceneNode};
use crate::geometry::{Bounds, evaluate_node_content, segment_bounds_distance, world_bounds};
use crate::ids::{ComponentId, NodeId};
use crate::large_scene::component::ComponentDefinition;
use crate::large_scene::connection::{InstancePortRef, RoutePolicy};
use crate::large_scene::evaluate::{
    compose_transform, resolve_part_content, resolve_part_transform,
};
use crate::large_scene::route::{route_connection, world_transform};
use crate::large_scene::validate::resolve_parameters;
use crate::math::{Transform, Vec3};
use crate::validation::{Diagnostic, DiagnosticCode};

pub const ENDPOINT_EXCLUSION_M: f32 = 1.5;
pub const DEFAULT_MAX_SUPPORT_SPACING_M: f32 = 6.0;
pub const SUPPORT_PROJECTION_TOLERANCE_M: f32 = 1.0;
pub const TURN_THRESHOLD_RADIANS: f32 = 15.0_f32.to_radians();
pub const MAX_OBSTACLE_OWNERS: usize = 50_000;
pub const MAX_ROUTE_SEGMENTS: usize = 50_000;
pub const MAX_TRIANGLE_PAIRS: usize = 1_000_000;
pub const MAX_COLLISIONS_PER_ROUTE: usize = 32;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteSafetyStats {
    pub obstacle_owners: usize,
    pub route_segments: usize,
    pub broad_phase_candidates: usize,
    pub narrow_phase_pairs: usize,
    pub triangle_pairs: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) enum ObstacleClass {
    Solid,
    Ignored,
    ClearanceSurface { clearance: f32 },
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn obstacle_class(
    metadata: &BTreeMap<String, String>,
) -> Result<ObstacleClass, &'static str> {
    match metadata.get("routeObstacle").map(String::as_str) {
        None | Some("solid") => Ok(ObstacleClass::Solid),
        Some("ignored") => Ok(ObstacleClass::Ignored),
        Some("clearance-surface") => {
            let clearance = metadata
                .get("routeClearance")
                .map(|value| value.parse::<f32>())
                .transpose()
                .map_err(|_| "routeClearance must be a finite non-negative number")?
                .unwrap_or(0.0);
            if clearance.is_finite() && clearance >= 0.0 {
                Ok(ObstacleClass::ClearanceSurface { clearance })
            } else {
                Err("routeClearance must be a finite non-negative number")
            }
        }
        Some(_) => Err("routeObstacle must be solid, ignored, or clearance-surface"),
    }
}

#[allow(dead_code)]
struct RouteFact {
    node_index: usize,
    node_id: NodeId,
    from_owner: NodeId,
    to_owner: NodeId,
    radius: f32,
    policy: RoutePolicy,
    line_id: Option<String>,
    max_support_spacing: f32,
    points: Vec<[f32; 3]>,
    cumulative: Vec<f32>,
}

struct ObstacleOwner {
    source_id: NodeId,
    bounds: Bounds,
    class: ObstacleClass,
}

pub fn validate_route_safety(
    document: &Document,
    definitions: &BTreeMap<ComponentId, &ComponentDefinition>,
) -> (Vec<Diagnostic>, RouteSafetyStats) {
    let mut stats = RouteSafetyStats::default();
    let mut diagnostics = Vec::new();
    let routes = route_facts(document, definitions, &mut diagnostics, &mut stats);
    let obstacles = obstacle_owners(document, definitions, &mut diagnostics, &mut stats);

    for route in routes {
        let mut reported = BTreeSet::new();
        for segment in route.points.windows(2) {
            let segment = [segment[0], segment[1]];
            for obstacle in &obstacles {
                if !matches!(obstacle.class, ObstacleClass::Solid) {
                    continue;
                }
                let distance = segment_bounds_distance(segment, &obstacle.bounds);
                if distance > route.radius + crate::geometry::CLASH_EPSILON {
                    continue;
                }
                stats.broad_phase_candidates += 1;
                if !reported.insert(obstacle.source_id.clone()) {
                    continue;
                }
                diagnostics.push(Diagnostic {
                    code: DiagnosticCode::RouteCollision,
                    operation_index: None,
                    object_id: Some(route.node_id.to_string()),
                    pointer: format!("/scene/nodes/{}/content", route.node_index),
                    message: format!(
                        "route intersects obstacle {} at {:.3} m",
                        obstacle.source_id, distance
                    ),
                });
            }
        }
    }

    (diagnostics, stats)
}

fn route_facts(
    document: &Document,
    definitions: &BTreeMap<ComponentId, &ComponentDefinition>,
    diagnostics: &mut Vec<Diagnostic>,
    stats: &mut RouteSafetyStats,
) -> Vec<RouteFact> {
    let mut routes = Vec::new();
    for (index, node) in document.scene.nodes().iter().enumerate() {
        let NodeContent::Connection(connection) = &node.content else {
            continue;
        };
        let Ok(points) = route_connection(document, definitions, connection) else {
            continue;
        };
        let segment_count = points.len().saturating_sub(1);
        if stats.route_segments + segment_count > MAX_ROUTE_SEGMENTS {
            diagnostics.push(Diagnostic {
                code: DiagnosticCode::RouteSafetyBudget,
                operation_index: None,
                object_id: Some(node.id.to_string()),
                pointer: format!("/scene/nodes/{index}/content"),
                message: format!("route safety segment budget {MAX_ROUTE_SEGMENTS} exceeded"),
            });
            break;
        }
        let mut cumulative = Vec::with_capacity(points.len());
        let mut total = 0.0;
        cumulative.push(0.0);
        for pair in points.windows(2) {
            total += distance(pair[0], pair[1]);
            cumulative.push(total);
        }
        stats.route_segments += segment_count;
        routes.push(RouteFact {
            node_index: index,
            node_id: node.id.clone(),
            from_owner: connection_owner(&connection.from),
            to_owner: connection_owner(&connection.to),
            radius: connection.diameter / 2.0,
            policy: connection.route_policy.clone(),
            line_id: connection.metadata.get("lineId").cloned(),
            max_support_spacing: parse_positive(
                connection.metadata.get("maxSupportSpacing"),
                DEFAULT_MAX_SUPPORT_SPACING_M,
            ),
            points,
            cumulative,
        });
    }
    routes
}

fn obstacle_owners(
    document: &Document,
    definitions: &BTreeMap<ComponentId, &ComponentDefinition>,
    diagnostics: &mut Vec<Diagnostic>,
    stats: &mut RouteSafetyStats,
) -> Vec<ObstacleOwner> {
    let mut owners = Vec::new();
    for (index, node) in document.scene.nodes().iter().enumerate() {
        if matches!(
            node.content,
            NodeContent::Connection(_) | NodeContent::ReferenceInstance(_)
        ) {
            continue;
        }
        let class = match obstacle_class(&node.metadata) {
            Ok(class) => class,
            Err(message) => {
                diagnostics.push(Diagnostic {
                    code: DiagnosticCode::OperationInvalid,
                    operation_index: None,
                    object_id: Some(node.id.to_string()),
                    pointer: format!("/scene/nodes/{index}/metadata/routeObstacle"),
                    message: message.into(),
                });
                continue;
            }
        };
        if matches!(class, ObstacleClass::Ignored) {
            continue;
        }
        let bounds = match &node.content {
            NodeContent::ComponentInstance(instance) => definitions
                .get(&instance.definition_id)
                .and_then(|definition| component_bounds(document, node, definition)),
            _ => world_bounds(document, node).ok().flatten(),
        };
        let Some(bounds) = bounds else {
            continue;
        };
        if owners.len() >= MAX_OBSTACLE_OWNERS {
            diagnostics.push(Diagnostic {
                code: DiagnosticCode::RouteSafetyBudget,
                operation_index: None,
                object_id: Some(node.id.to_string()),
                pointer: format!("/scene/nodes/{index}/content"),
                message: format!("route safety obstacle budget {MAX_OBSTACLE_OWNERS} exceeded"),
            });
            break;
        }
        owners.push(ObstacleOwner {
            source_id: node.id.clone(),
            bounds,
            class,
        });
    }
    owners.sort_by(|left, right| left.source_id.as_str().cmp(right.source_id.as_str()));
    stats.obstacle_owners = owners.len();
    owners
}

fn component_bounds(
    document: &Document,
    node: &SceneNode,
    definition: &ComponentDefinition,
) -> Option<Bounds> {
    let NodeContent::ComponentInstance(instance) = &node.content else {
        return None;
    };
    let parameters = resolve_parameters(definition, &instance.parameters).ok()?;
    let mut bounds = None;
    for part in &definition.parts {
        let content = resolve_part_content(&part.content, &parameters).ok()?;
        let mesh = evaluate_node_content(&content).ok()?;
        if mesh.vertices.is_empty() {
            continue;
        }
        let part_transform = resolve_part_transform(&part.transform, &parameters).ok()?;
        let world = world_transform(document, node).ok()?;
        let matrix = transform_matrix(compose_transform(world, part_transform));
        let part_bounds = transform_bounds(mesh.bounds, matrix);
        bounds = Some(match bounds {
            Some(current) => merge_bounds(current, part_bounds),
            None => part_bounds,
        });
    }
    bounds
}

fn transform_bounds(bounds: Bounds, matrix: Mat4) -> Bounds {
    let mut minimum = Vec3 {
        x: f32::INFINITY,
        y: f32::INFINITY,
        z: f32::INFINITY,
    };
    let mut maximum = Vec3 {
        x: f32::NEG_INFINITY,
        y: f32::NEG_INFINITY,
        z: f32::NEG_INFINITY,
    };
    for x in [bounds.min.x, bounds.max.x] {
        for y in [bounds.min.y, bounds.max.y] {
            for z in [bounds.min.z, bounds.max.z] {
                let point = matrix.transform_point3(GlamVec3::new(x, y, z));
                minimum.x = minimum.x.min(point.x);
                minimum.y = minimum.y.min(point.y);
                minimum.z = minimum.z.min(point.z);
                maximum.x = maximum.x.max(point.x);
                maximum.y = maximum.y.max(point.y);
                maximum.z = maximum.z.max(point.z);
            }
        }
    }
    Bounds {
        min: minimum,
        max: maximum,
    }
}

fn merge_bounds(left: Bounds, right: Bounds) -> Bounds {
    Bounds {
        min: Vec3 {
            x: left.min.x.min(right.min.x),
            y: left.min.y.min(right.min.y),
            z: left.min.z.min(right.min.z),
        },
        max: Vec3 {
            x: left.max.x.max(right.max.x),
            y: left.max.y.max(right.max.y),
            z: left.max.z.max(right.max.z),
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

fn connection_owner(reference: &InstancePortRef) -> NodeId {
    match reference {
        InstancePortRef::Instance { node_id, .. } => node_id.clone(),
        InstancePortRef::SetItem { set_node_id, .. } => set_node_id.clone(),
    }
}

fn parse_positive(value: Option<&String>, fallback: f32) -> f32 {
    value
        .and_then(|value| value.parse::<f32>().ok())
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(fallback)
}

fn distance(left: [f32; 3], right: [f32; 3]) -> f32 {
    let x = right[0] - left[0];
    let y = right[1] - left[1];
    let z = right[2] - left[2];
    (x * x + y * y + z * z).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routing_limits_and_defaults_are_stable() {
        assert_eq!(DEFAULT_MAX_SUPPORT_SPACING_M, 6.0);
        assert_eq!(ENDPOINT_EXCLUSION_M, 1.5);
        assert_eq!(MAX_OBSTACLE_OWNERS, 50_000);
        assert_eq!(MAX_ROUTE_SEGMENTS, 50_000);
        assert_eq!(MAX_TRIANGLE_PAIRS, 1_000_000);
    }

    #[test]
    fn metadata_parser_rejects_unknown_obstacle_values() {
        let mut metadata = BTreeMap::new();
        metadata.insert("routeObstacle".into(), "mystery".into());
        assert_eq!(
            obstacle_class(&metadata),
            Err("routeObstacle must be solid, ignored, or clearance-surface"),
        );
    }
}
