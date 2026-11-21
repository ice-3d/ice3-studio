use std::collections::{BTreeMap, BTreeSet};

use glam::{Mat4, Quat, Vec3 as GlamVec3};
use serde::Serialize;

use crate::document::{Document, NodeContent, SceneNode};
use crate::geometry::{Bounds, evaluate_node_content, segment_bounds_distance, world_bounds};
use crate::geometry::{segment_triangle, segment_triangle_distance};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteSafetyLimits {
    pub max_obstacle_owners: usize,
    pub max_route_segments: usize,
    pub max_triangle_pairs: usize,
    pub max_collisions_per_route: usize,
}

impl Default for RouteSafetyLimits {
    fn default() -> Self {
        Self {
            max_obstacle_owners: MAX_OBSTACLE_OWNERS,
            max_route_segments: MAX_ROUTE_SEGMENTS,
            max_triangle_pairs: MAX_TRIANGLE_PAIRS,
            max_collisions_per_route: MAX_COLLISIONS_PER_ROUTE,
        }
    }
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
    source_index: usize,
    source_id: NodeId,
    bounds: Bounds,
    class: ObstacleClass,
    narrow_phase: bool,
    attachment_line_id: Option<String>,
}

pub fn validate_route_safety(
    document: &Document,
    definitions: &BTreeMap<ComponentId, &ComponentDefinition>,
) -> (Vec<Diagnostic>, RouteSafetyStats) {
    validate_route_safety_with_limits(document, definitions, RouteSafetyLimits::default())
}

pub fn validate_route_safety_with_limits(
    document: &Document,
    definitions: &BTreeMap<ComponentId, &ComponentDefinition>,
    limits: RouteSafetyLimits,
) -> (Vec<Diagnostic>, RouteSafetyStats) {
    let mut stats = RouteSafetyStats::default();
    let mut diagnostics = Vec::new();
    let routes = route_facts(document, definitions, &mut diagnostics, &mut stats, limits);
    let obstacles = obstacle_owners(document, definitions, &mut diagnostics, &mut stats, limits);
    let stations = support_stations(document, &routes, &mut diagnostics);
    let penetration_pairs = document
        .scene
        .nodes()
        .iter()
        .filter_map(|node| {
            (node.metadata.get("role").map(String::as_str) == Some("pipe-penetration"))
                .then(|| {
                    Some((
                        node.metadata.get("lineId")?.clone(),
                        node.metadata.get("hostNodeId")?.clone(),
                    ))
                })
                .flatten()
        })
        .collect::<BTreeSet<_>>();

    for route in &routes {
        let mut reported = BTreeSet::new();
        for obstacle in &obstacles {
            if obstacle.attachment_line_id.as_ref() == route.line_id.as_ref()
                && obstacle.attachment_line_id.is_some()
            {
                continue;
            }
            if route.line_id.as_ref().is_some_and(|line_id| {
                penetration_pairs.contains(&(line_id.clone(), obstacle.source_id.to_string()))
            }) {
                continue;
            }
            if let ObstacleClass::ClearanceSurface { clearance } = obstacle.class {
                for points in route.points.windows(2) {
                    let segment = [points[0], points[1]];
                    if !segment_crosses_footprint(segment, &obstacle.bounds) {
                        continue;
                    }
                    let highest = segment[0][1].max(segment[1][1]) + route.radius;
                    if highest + crate::geometry::CLASH_EPSILON <= obstacle.bounds.max.y {
                        continue;
                    }
                    let lowest = segment[0][1].min(segment[1][1]) - route.radius;
                    let required = obstacle.bounds.max.y + clearance;
                    if lowest + crate::geometry::CLASH_EPSILON >= required {
                        continue;
                    }
                    diagnostics.push(Diagnostic {
                        code: DiagnosticCode::RouteClearanceInsufficient,
                        operation_index: None,
                        object_id: Some(route.node_id.to_string()),
                        pointer: format!("/scene/nodes/{}/content", route.node_index),
                        message: format!(
                            "route clearance above {} is {:.3} m, requires {:.3} m",
                            obstacle.source_id,
                            lowest - obstacle.bounds.max.y,
                            clearance,
                        ),
                    });
                    break;
                }
                continue;
            }
            if !matches!(obstacle.class, ObstacleClass::Solid) {
                continue;
            }
            let candidates = route
                .points
                .windows(2)
                .enumerate()
                .filter_map(|(index, points)| {
                    let segment = [points[0], points[1]];
                    let distance = segment_bounds_distance(segment, &obstacle.bounds);
                    (distance <= route.radius + crate::geometry::CLASH_EPSILON)
                        .then_some((index, segment, distance))
                })
                .collect::<Vec<_>>();
            if candidates.is_empty() {
                continue;
            }
            stats.broad_phase_candidates += candidates.len();
            let exempt = endpoint_exempt_segments(
                candidates.iter().map(|(index, _, _)| *index),
                &route.cumulative,
                route.from_owner.as_str() == obstacle.source_id.as_str(),
                route.to_owner.as_str() == obstacle.source_id.as_str(),
            );
            let triangles = if obstacle.narrow_phase {
                let Some(node) = document.scene.nodes().get(obstacle.source_index) else {
                    continue;
                };
                let Some(definition) = component_definition(node, definitions) else {
                    continue;
                };
                stats.narrow_phase_pairs += 1;
                Some(component_triangles(document, node, definition))
            } else {
                None
            };
            let mut confirmed = Vec::new();
            for (index, segment, distance) in candidates {
                if exempt.contains(&index) {
                    continue;
                }
                let hit = if let Some(triangles) = &triangles {
                    let mut hit = false;
                    let mut exhausted = false;
                    for triangle in triangles {
                        if stats.triangle_pairs >= limits.max_triangle_pairs {
                            exhausted = true;
                            break;
                        }
                        stats.triangle_pairs += 1;
                        if segment_triangle_distance(segment, *triangle)
                            <= route.radius + crate::geometry::CLASH_EPSILON
                        {
                            hit = true;
                            break;
                        }
                    }
                    if !hit && !exhausted {
                        for step in 1..=5 {
                            if triangles.len()
                                > limits
                                    .max_triangle_pairs
                                    .saturating_sub(stats.triangle_pairs)
                            {
                                stats.triangle_pairs = limits.max_triangle_pairs;
                                exhausted = true;
                                break;
                            }
                            let ratio = step as f32 / 6.0;
                            let sample = [
                                segment[0][0] + (segment[1][0] - segment[0][0]) * ratio,
                                segment[0][1] + (segment[1][1] - segment[0][1]) * ratio,
                                segment[0][2] + (segment[1][2] - segment[0][2]) * ratio,
                            ];
                            let (inside, pair_count) = point_in_mesh(sample, triangles);
                            stats.triangle_pairs += pair_count;
                            if inside {
                                hit = true;
                                break;
                            }
                        }
                    }
                    if exhausted {
                        diagnostics.push(Diagnostic {
                            code: DiagnosticCode::RouteSafetyBudget,
                            operation_index: None,
                            object_id: Some(route.node_id.to_string()),
                            pointer: format!("/scene/nodes/{}/content", route.node_index),
                            message: format!(
                                "route safety triangle-pair budget {} exceeded",
                                limits.max_triangle_pairs
                            ),
                        });
                        validate_support_layout(&routes, &stations, &obstacles, &mut diagnostics);
                        return (diagnostics, stats);
                    }
                    hit
                } else {
                    true
                };
                if hit {
                    confirmed.push((index, distance));
                }
            }
            if confirmed.is_empty() {
                continue;
            }
            let (_, distance) = confirmed[0];
            if reported.len() >= limits.max_collisions_per_route {
                diagnostics.push(Diagnostic {
                    code: DiagnosticCode::RouteSafetyBudget,
                    operation_index: None,
                    object_id: Some(route.node_id.to_string()),
                    pointer: format!("/scene/nodes/{}/content", route.node_index),
                    message: format!(
                        "route collision report budget {} exceeded",
                        limits.max_collisions_per_route
                    ),
                });
                break;
            }
            reported.insert(obstacle.source_id.clone());
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

    validate_support_layout(&routes, &stations, &obstacles, &mut diagnostics);
    (diagnostics, stats)
}

fn route_facts(
    document: &Document,
    definitions: &BTreeMap<ComponentId, &ComponentDefinition>,
    diagnostics: &mut Vec<Diagnostic>,
    stats: &mut RouteSafetyStats,
    limits: RouteSafetyLimits,
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
        if stats.route_segments + segment_count > limits.max_route_segments {
            diagnostics.push(Diagnostic {
                code: DiagnosticCode::RouteSafetyBudget,
                operation_index: None,
                object_id: Some(node.id.to_string()),
                pointer: format!("/scene/nodes/{index}/content"),
                message: format!(
                    "route safety segment budget {} exceeded",
                    limits.max_route_segments
                ),
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
        let line_id = match connection.metadata.get("lineId") {
            Some(value) if value.trim().is_empty() => {
                diagnostics.push(Diagnostic {
                    code: DiagnosticCode::OperationInvalid,
                    operation_index: None,
                    object_id: Some(node.id.to_string()),
                    pointer: format!("/scene/nodes/{index}/metadata/lineId"),
                    message: "connection lineId must not be empty".into(),
                });
                None
            }
            Some(value) => Some(value.clone()),
            None => None,
        };
        let max_support_spacing = match connection.metadata.get("maxSupportSpacing") {
            None => DEFAULT_MAX_SUPPORT_SPACING_M,
            Some(value) => match value
                .parse::<f32>()
                .ok()
                .filter(|value| value.is_finite() && *value > 0.0)
            {
                Some(value) => value,
                None => {
                    diagnostics.push(Diagnostic {
                        code: DiagnosticCode::OperationInvalid,
                        operation_index: None,
                        object_id: Some(node.id.to_string()),
                        pointer: format!("/scene/nodes/{index}/metadata/maxSupportSpacing"),
                        message: "maxSupportSpacing must be a finite positive number".into(),
                    });
                    DEFAULT_MAX_SUPPORT_SPACING_M
                }
            },
        };
        routes.push(RouteFact {
            node_index: index,
            node_id: node.id.clone(),
            from_owner: connection_owner(&connection.from),
            to_owner: connection_owner(&connection.to),
            radius: connection.diameter / 2.0,
            policy: connection.route_policy.clone(),
            line_id,
            max_support_spacing,
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
    limits: RouteSafetyLimits,
) -> Vec<ObstacleOwner> {
    let mut owners = Vec::new();
    for (index, node) in document.scene.nodes().iter().enumerate() {
        if matches!(
            node.content,
            NodeContent::Connection(_) | NodeContent::ReferenceInstance(_)
        ) {
            continue;
        }
        let class = if !node.metadata.contains_key("routeObstacle")
            && node.metadata.get("role").map(String::as_str) == Some("site")
        {
            ObstacleClass::Ignored
        } else {
            match obstacle_class(&node.metadata) {
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
        let attachment_line_id = matches!(
            node.metadata.get("role").map(String::as_str),
            Some("pipe-fitting" | "pipe-penetration")
        )
        .then(|| node.metadata.get("lineId").cloned())
        .flatten();
        if owners.len() >= limits.max_obstacle_owners {
            diagnostics.push(Diagnostic {
                code: DiagnosticCode::RouteSafetyBudget,
                operation_index: None,
                object_id: Some(node.id.to_string()),
                pointer: format!("/scene/nodes/{index}/content"),
                message: format!(
                    "route safety obstacle budget {} exceeded",
                    limits.max_obstacle_owners
                ),
            });
            break;
        }
        owners.push(ObstacleOwner {
            source_index: index,
            source_id: node.id.clone(),
            bounds,
            class,
            narrow_phase: matches!(node.content, NodeContent::ComponentInstance(_)),
            attachment_line_id,
        });
    }
    owners.sort_by(|left, right| left.source_id.as_str().cmp(right.source_id.as_str()));
    stats.obstacle_owners = owners.len();
    owners
}

fn endpoint_exempt_segments(
    candidates: impl Iterator<Item = usize>,
    cumulative: &[f32],
    from_endpoint: bool,
    to_endpoint: bool,
) -> BTreeSet<usize> {
    let candidates = candidates.collect::<BTreeSet<_>>();
    let mut exempt = BTreeSet::new();
    if from_endpoint {
        collect_endpoint_cluster(&candidates, cumulative, false, &mut exempt);
    }
    if to_endpoint {
        collect_endpoint_cluster(&candidates, cumulative, true, &mut exempt);
    }
    exempt
}

fn collect_endpoint_cluster(
    candidates: &BTreeSet<usize>,
    cumulative: &[f32],
    reverse: bool,
    exempt: &mut BTreeSet<usize>,
) {
    let ordered: Vec<usize> = if reverse {
        candidates.iter().rev().copied().collect()
    } else {
        candidates.iter().copied().collect()
    };
    let mut previous_arc = if reverse {
        cumulative.last().copied().unwrap_or(0.0)
    } else {
        0.0
    };
    let mut started = false;
    for index in ordered {
        let segment_end = cumulative.get(index + 1).copied().unwrap_or(previous_arc);
        let segment_start = cumulative.get(index).copied().unwrap_or(segment_end);
        let gap = if reverse {
            previous_arc - segment_end
        } else {
            segment_start - previous_arc
        };
        if !started {
            if gap > ENDPOINT_EXCLUSION_M {
                break;
            }
            started = true;
        } else if gap > ENDPOINT_EXCLUSION_M {
            break;
        }
        exempt.insert(index);
        previous_arc = if reverse { segment_start } else { segment_end };
    }
}

pub(crate) fn component_bounds(
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

fn component_definition<'a>(
    node: &SceneNode,
    definitions: &'a BTreeMap<ComponentId, &'a ComponentDefinition>,
) -> Option<&'a ComponentDefinition> {
    let NodeContent::ComponentInstance(instance) = &node.content else {
        return None;
    };
    definitions.get(&instance.definition_id).copied()
}

fn component_triangles(
    document: &Document,
    node: &SceneNode,
    definition: &ComponentDefinition,
) -> Vec<[[f32; 3]; 3]> {
    let NodeContent::ComponentInstance(instance) = &node.content else {
        return Vec::new();
    };
    let Ok(parameters) = resolve_parameters(definition, &instance.parameters) else {
        return Vec::new();
    };
    let Ok(world) = world_transform(document, node) else {
        return Vec::new();
    };
    let mut triangles = Vec::new();
    for part in &definition.parts {
        let Ok(content) = resolve_part_content(&part.content, &parameters) else {
            continue;
        };
        let Ok(mesh) = evaluate_node_content(&content) else {
            continue;
        };
        let Ok(part_transform) = resolve_part_transform(&part.transform, &parameters) else {
            continue;
        };
        let matrix = transform_matrix(compose_transform(world, part_transform));
        for triangle in mesh.indices.as_chunks::<3>().0 {
            let mut world_triangle = [[0.0; 3]; 3];
            for (target, index) in world_triangle.iter_mut().zip(triangle) {
                let position = mesh.vertices[*index as usize].position;
                let point =
                    matrix.transform_point3(GlamVec3::new(position[0], position[1], position[2]));
                *target = [point.x, point.y, point.z];
            }
            triangles.push(world_triangle);
        }
    }
    triangles
}

fn point_in_mesh(point: [f32; 3], triangles: &[[[f32; 3]; 3]]) -> (bool, usize) {
    let far = [point[0] + 1_000_000.0, point[1] + 0.125, point[2] + 0.0625];
    let crossings = triangles
        .iter()
        .filter(|triangle| segment_triangle([point, far], **triangle))
        .count();
    (crossings % 2 == 1, triangles.len())
}

#[derive(Debug)]
struct SupportStation {
    position: [f32; 3],
}

fn support_stations(
    document: &Document,
    routes: &[RouteFact],
    diagnostics: &mut Vec<Diagnostic>,
) -> BTreeMap<String, Vec<SupportStation>> {
    let route_lines = routes
        .iter()
        .filter_map(|route| route.line_id.clone())
        .collect::<BTreeSet<_>>();
    let mut stations = BTreeMap::<String, Vec<SupportStation>>::new();
    for (index, node) in document.scene.nodes().iter().enumerate() {
        if node.metadata.get("role").map(String::as_str) != Some("pipe-support")
            || node.metadata.get("supportPart").map(String::as_str) != Some("beam")
        {
            continue;
        }
        let Some(line_id) = node
            .metadata
            .get("lineId")
            .filter(|value| !value.is_empty())
        else {
            diagnostics.push(route_diagnostic(
                DiagnosticCode::OperationInvalid,
                &node.id,
                index,
                "support beam lineId must not be empty",
            ));
            continue;
        };
        if !route_lines.contains(line_id) {
            diagnostics.push(route_diagnostic(
                DiagnosticCode::OperationInvalid,
                &node.id,
                index,
                format!("support beam references unknown lineId {line_id}"),
            ));
            continue;
        }
        stations
            .entry(line_id.clone())
            .or_default()
            .push(SupportStation {
                position: [
                    node.transform.translation.x,
                    node.transform.translation.y,
                    node.transform.translation.z,
                ],
            });
    }
    stations
}

fn validate_support_layout(
    routes: &[RouteFact],
    stations: &BTreeMap<String, Vec<SupportStation>>,
    obstacles: &[ObstacleOwner],
    diagnostics: &mut Vec<Diagnostic>,
) {
    for route in routes {
        if matches!(route.policy, RoutePolicy::Buried { .. }) {
            continue;
        }
        if route.line_id.is_none() {
            continue;
        }
        let line_stations = route
            .line_id
            .as_ref()
            .and_then(|line_id| stations.get(line_id))
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        if line_stations.is_empty() {
            diagnostics.push(route_content_diagnostic(
                DiagnosticCode::RouteSupportMissing,
                route,
                "non-buried route has no support stations",
            ));
            continue;
        }
        let mut valid = Vec::new();
        let mut off_route = false;
        for station in line_stations {
            let Some((arc, distance)) = project_to_route(station.position, route) else {
                continue;
            };
            if distance > SUPPORT_PROJECTION_TOLERANCE_M {
                off_route = true;
            } else {
                valid.push(arc);
            }
        }
        if valid.is_empty() {
            diagnostics.push(route_content_diagnostic(
                DiagnosticCode::RouteSupportSpacing,
                route,
                format!(
                    "all support stations are more than {SUPPORT_PROJECTION_TOLERANCE_M:.1} m off-route"
                ),
            ));
            continue;
        }
        if off_route {
            diagnostics.push(route_content_diagnostic(
                DiagnosticCode::RouteSupportSpacing,
                route,
                format!(
                    "one or more supports are more than {SUPPORT_PROJECTION_TOLERANCE_M:.1} m off-route"
                ),
            ));
        }
        valid.sort_by(f32::total_cmp);
        let total = *route.cumulative.last().unwrap_or(&0.0);
        let mut positions = vec![0.0];
        positions.extend(valid.iter().copied());
        positions.push(total);
        let invalid_gap = positions.windows(2).any(|pair| {
            pair[1] - pair[0] > route.max_support_spacing + 1e-5
                && !route_interval_crosses_surface(route, pair[0], pair[1], obstacles)
        });
        if invalid_gap {
            diagnostics.push(route_content_diagnostic(
                DiagnosticCode::RouteSupportSpacing,
                route,
                format!("support gap exceeds {:.3} m", route.max_support_spacing),
            ));
        }
        for turn in route_turns(route) {
            let required = (route.radius * 4.0).max(1.0);
            if !valid.iter().any(|arc| (arc - turn).abs() <= required) {
                diagnostics.push(route_content_diagnostic(
                    DiagnosticCode::RouteSupportSpacing,
                    route,
                    format!("turn at {turn:.3} m has no support within {required:.3} m"),
                ));
            }
        }
    }
}

fn route_interval_crosses_surface(
    route: &RouteFact,
    start: f32,
    end: f32,
    obstacles: &[ObstacleOwner],
) -> bool {
    for (index, pair) in route.points.windows(2).enumerate() {
        let segment_start = route.cumulative[index];
        let segment_end = route.cumulative[index + 1];
        if segment_end < start || segment_start > end {
            continue;
        }
        let local_start = start.max(segment_start);
        let local_end = end.min(segment_end);
        if local_end <= local_start {
            continue;
        }
        let length = segment_end - segment_start;
        if length <= f32::EPSILON {
            continue;
        }
        let begin = (local_start - segment_start) / length;
        let finish = (local_end - segment_start) / length;
        let clipped = [
            [
                pair[0][0] + (pair[1][0] - pair[0][0]) * begin,
                pair[0][1] + (pair[1][1] - pair[0][1]) * begin,
                pair[0][2] + (pair[1][2] - pair[0][2]) * begin,
            ],
            [
                pair[0][0] + (pair[1][0] - pair[0][0]) * finish,
                pair[0][1] + (pair[1][1] - pair[0][1]) * finish,
                pair[0][2] + (pair[1][2] - pair[0][2]) * finish,
            ],
        ];
        if obstacles.iter().any(|obstacle| {
            (matches!(obstacle.class, ObstacleClass::ClearanceSurface { .. })
                && segment_crosses_footprint(clipped, &obstacle.bounds))
        }) {
            return true;
        }
    }
    false
}

fn project_to_route(position: [f32; 3], route: &RouteFact) -> Option<(f32, f32)> {
    let mut best: Option<(f32, f32)> = None;
    let mut best_score = f32::INFINITY;
    for (index, pair) in route.points.windows(2).enumerate() {
        let from = pair[0];
        let to = pair[1];
        let dx = to[0] - from[0];
        let dy = to[1] - from[1];
        let dz = to[2] - from[2];
        let denominator = dx * dx + dy * dy + dz * dz;
        if denominator <= f32::EPSILON {
            continue;
        }
        let ratio = (((position[0] - from[0]) * dx
            + (position[1] - from[1]) * dy
            + (position[2] - from[2]) * dz)
            / denominator)
            .clamp(0.0, 1.0);
        let nearest = [
            from[0] + dx * ratio,
            from[1] + dy * ratio,
            from[2] + dz * ratio,
        ];
        let horizontal_distance =
            ((position[0] - nearest[0]).powi(2) + (position[2] - nearest[2]).powi(2)).sqrt();
        let score = ((position[0] - nearest[0]).powi(2)
            + (position[1] - nearest[1]).powi(2)
            + (position[2] - nearest[2]).powi(2))
        .sqrt();
        let length = route.cumulative[index + 1] - route.cumulative[index];
        let arc = route.cumulative[index] + length * ratio;
        if score < best_score {
            best = Some((arc, horizontal_distance));
            best_score = score;
        }
    }
    best
}

fn route_turns(route: &RouteFact) -> Vec<f32> {
    let mut turns = Vec::new();
    let window = 4;
    for index in window..route.points.len().saturating_sub(window) {
        let incoming = unit3([
            route.points[index][0] - route.points[index - window][0],
            route.points[index][1] - route.points[index - window][1],
            route.points[index][2] - route.points[index - window][2],
        ]);
        let outgoing = unit3([
            route.points[index + window][0] - route.points[index][0],
            route.points[index + window][1] - route.points[index][1],
            route.points[index + window][2] - route.points[index][2],
        ]);
        let Some(incoming) = incoming else {
            continue;
        };
        let Some(outgoing) = outgoing else {
            continue;
        };
        let angle =
            (incoming[0] * outgoing[0] + incoming[1] * outgoing[1] + incoming[2] * outgoing[2])
                .clamp(-1.0, 1.0)
                .acos();
        if angle > TURN_THRESHOLD_RADIANS {
            turns.push(route.cumulative[index]);
            continue;
        }
    }
    turns
}

fn unit3(value: [f32; 3]) -> Option<[f32; 3]> {
    let length = (value[0] * value[0] + value[1] * value[1] + value[2] * value[2]).sqrt();
    (length > f32::EPSILON).then(|| [value[0] / length, value[1] / length, value[2] / length])
}

fn route_diagnostic(
    code: DiagnosticCode,
    node_id: &NodeId,
    node_index: usize,
    message: impl Into<String>,
) -> Diagnostic {
    Diagnostic {
        code,
        operation_index: None,
        object_id: Some(node_id.to_string()),
        pointer: format!("/scene/nodes/{node_index}/metadata/lineId"),
        message: message.into(),
    }
}

fn route_content_diagnostic(
    code: DiagnosticCode,
    route: &RouteFact,
    message: impl Into<String>,
) -> Diagnostic {
    Diagnostic {
        code,
        operation_index: None,
        object_id: Some(route.node_id.to_string()),
        pointer: format!("/scene/nodes/{}/content", route.node_index),
        message: message.into(),
    }
}

fn segment_crosses_footprint(segment: [[f32; 3]; 2], bounds: &Bounds) -> bool {
    let mut minimum = 0.0_f32;
    let mut maximum = 1.0_f32;
    for axis in [0_usize, 2] {
        let direction = segment[1][axis] - segment[0][axis];
        let low = if axis == 0 {
            bounds.min.x
        } else {
            bounds.min.z
        };
        let high = if axis == 0 {
            bounds.max.x
        } else {
            bounds.max.z
        };
        if direction.abs() <= f32::EPSILON {
            if segment[0][axis] < low || segment[0][axis] > high {
                return false;
            }
            continue;
        }
        let inverse = 1.0 / direction;
        let mut near = (low - segment[0][axis]) * inverse;
        let mut far = (high - segment[0][axis]) * inverse;
        if near > far {
            std::mem::swap(&mut near, &mut far);
        }
        minimum = minimum.max(near);
        maximum = maximum.min(far);
        if minimum > maximum {
            return false;
        }
    }
    true
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

    #[test]
    fn support_projection_uses_vertical_arc_position() {
        let route = RouteFact {
            node_index: 0,
            node_id: NodeId::from("node-000001"),
            from_owner: NodeId::from("node-000002"),
            to_owner: NodeId::from("node-000003"),
            radius: 0.1,
            policy: RoutePolicy::Rack { elevation: 10.0 },
            line_id: Some("L-001".into()),
            max_support_spacing: 6.0,
            points: vec![[0.0, 0.0, 0.0], [0.0, 10.0, 0.0]],
            cumulative: vec![0.0, 10.0],
        };
        let (arc, distance) =
            project_to_route([0.0, 5.0, 0.0], &route).expect("support projects onto route");
        assert!((arc - 5.0).abs() < 1e-5, "{arc}");
        assert!(distance < 1e-5);
    }

    #[test]
    fn endpoint_exemption_stops_after_the_contact_run() {
        let exempt = endpoint_exempt_segments(
            [0, 1, 4, 5].into_iter(),
            &[0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
            true,
            false,
        );
        assert_eq!(exempt, BTreeSet::from([0, 1]));
    }
}
