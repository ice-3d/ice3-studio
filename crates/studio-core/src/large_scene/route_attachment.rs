use std::collections::{BTreeMap, BTreeSet};

use crate::document::{Document, NodeContent, SceneNode};
use crate::geometry::{segment_bounds_distance, segment_triangle_distance, world_bounds};
use crate::ids::{ComponentId, NodeId};
use crate::large_scene::cache::MemoryGeometryStore;
use crate::large_scene::component::ComponentDefinition;
use crate::large_scene::connection::InstancePortRef;
use crate::large_scene::evaluate::evaluate_large_scene_selected;
use crate::large_scene::route::{ResolvedPort, resolve_port, route_connection};
use crate::math::{Transform, Vec3};
use crate::validation::{Diagnostic, DiagnosticCode};
use glam::{Mat4, Quat, Vec3 as GlamVec3};

pub const MAX_ROUTE_ATTACHMENTS: usize = 512;
pub const MAX_ATTACHMENT_PORTS: usize = 3;
pub const MAX_PENETRATION_CANDIDATES: usize = 4_096;
pub const MAX_ATTACHMENT_TRIANGLE_PAIRS: usize = 250_000;
pub const MAX_ATTACHMENT_DIAGNOSTICS: usize = 8;
pub const ATTACHMENT_POSITION_TOLERANCE_FLOOR_M: f32 = 0.02;
pub const ATTACHMENT_POSITION_TOLERANCE_RATIO: f32 = 0.05;
pub const ATTACHMENT_POSITION_TOLERANCE_CAP_M: f32 = 0.10;
pub const ATTACHMENT_AXIS_TOLERANCE_RADIANS: f32 = 2.0_f32.to_radians();
pub const SLEEVE_SURFACE_TOLERANCE_M: f32 = 0.15;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AttachmentKind {
    Tee,
    Reducer,
    Sleeve,
}

struct Attachment {
    node_index: usize,
    node_id: String,
    kind: AttachmentKind,
    line_id: String,
    host_node_id: Option<String>,
    ports: BTreeMap<String, ResolvedPort>,
}

impl AttachmentKind {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "tee" => Some(Self::Tee),
            "reducer" => Some(Self::Reducer),
            "sleeve" => Some(Self::Sleeve),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RouteAttachmentLimits {
    pub max_attachments: usize,
    pub max_ports: usize,
    pub max_penetration_candidates: usize,
    pub max_triangle_pairs: usize,
    pub max_diagnostics_per_attachment: usize,
}

impl Default for RouteAttachmentLimits {
    fn default() -> Self {
        Self {
            max_attachments: MAX_ROUTE_ATTACHMENTS,
            max_ports: MAX_ATTACHMENT_PORTS,
            max_penetration_candidates: MAX_PENETRATION_CANDIDATES,
            max_triangle_pairs: MAX_ATTACHMENT_TRIANGLE_PAIRS,
            max_diagnostics_per_attachment: MAX_ATTACHMENT_DIAGNOSTICS,
        }
    }
}

pub(crate) fn validate_route_attachments(
    document: &Document,
    definitions: &BTreeMap<ComponentId, &ComponentDefinition>,
) -> Vec<Diagnostic> {
    validate_route_attachments_with_limits(document, definitions, RouteAttachmentLimits::default())
}

pub(crate) fn validate_route_attachments_with_limits(
    document: &Document,
    definitions: &BTreeMap<ComponentId, &ComponentDefinition>,
    limits: RouteAttachmentLimits,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut count = 0_usize;
    let mut sleeve_keys = BTreeSet::new();
    let mut penetration_candidates = 0_usize;
    let mut triangle_pairs = 0_usize;
    for (index, node) in document.scene.nodes().iter().enumerate() {
        let role = node.metadata.get("role").map(String::as_str);
        if !matches!(role, Some("pipe-fitting" | "pipe-penetration")) {
            continue;
        }
        count += 1;
        if count > limits.max_attachments {
            diagnostics.push(attachment_diagnostic(
                DiagnosticCode::RouteAttachmentBudget,
                index,
                node.id.as_str(),
                "route attachment budget exceeded",
            ));
            break;
        }
        let Some(kind) = node
            .metadata
            .get("fittingKind")
            .and_then(|value| AttachmentKind::parse(value))
        else {
            diagnostics.push(attachment_field_diagnostic(
                DiagnosticCode::RouteAttachmentInvalid,
                index,
                node.id.as_str(),
                "/scene/nodes/{index}/metadata/fittingKind",
                "fittingKind must be tee, reducer, or sleeve",
            ));
            continue;
        };
        if node
            .metadata
            .get("lineId")
            .is_none_or(|value| value.trim().is_empty())
        {
            diagnostics.push(attachment_field_diagnostic(
                DiagnosticCode::RouteAttachmentInvalid,
                index,
                node.id.as_str(),
                "/scene/nodes/{index}/metadata/lineId",
                "attachment lineId must not be empty",
            ));
            continue;
        }
        if matches!(kind, AttachmentKind::Sleeve)
            && node
                .metadata
                .get("hostNodeId")
                .is_none_or(|value| value.trim().is_empty())
        {
            diagnostics.push(attachment_field_diagnostic(
                DiagnosticCode::RouteAttachmentInvalid,
                index,
                node.id.as_str(),
                "/scene/nodes/{index}/metadata/hostNodeId",
                "sleeve hostNodeId must not be empty",
            ));
            continue;
        }
        let diagnostics_before = diagnostics.len();
        match resolved_attachment(document, definitions, node, index, kind, limits) {
            Ok(attachment) => {
                if matches!(attachment.kind, AttachmentKind::Tee) {
                    validate_tee(document, &attachment, &mut diagnostics);
                } else if matches!(attachment.kind, AttachmentKind::Reducer) {
                    validate_reducer(document, &attachment, &mut diagnostics);
                } else if matches!(attachment.kind, AttachmentKind::Sleeve) {
                    let host_id = attachment
                        .host_node_id
                        .as_ref()
                        .expect("sleeve metadata validated");
                    let key = format!("{}\0{host_id}", attachment.line_id);
                    if !sleeve_keys.insert(key) {
                        diagnostics.push(attachment_diagnostic(
                            DiagnosticCode::RoutePenetrationInvalid,
                            attachment.node_index,
                            &attachment.node_id,
                            "duplicate sleeve for route and host",
                        ));
                    }
                    validate_sleeve(
                        document,
                        definitions,
                        &attachment,
                        limits,
                        &mut penetration_candidates,
                        &mut triangle_pairs,
                        &mut diagnostics,
                    );
                }
            }
            Err(diagnostic) => diagnostics.push(diagnostic),
        }
        let produced = diagnostics.len() - diagnostics_before;
        if produced > limits.max_diagnostics_per_attachment {
            diagnostics.truncate(diagnostics_before + limits.max_diagnostics_per_attachment);
            diagnostics.push(attachment_diagnostic(
                DiagnosticCode::RouteAttachmentBudget,
                index,
                node.id.as_str(),
                format!(
                    "attachment diagnostic budget {} exceeded",
                    limits.max_diagnostics_per_attachment
                ),
            ));
        }
    }
    diagnostics.sort_by(|left, right| {
        left.object_id
            .cmp(&right.object_id)
            .then_with(|| left.pointer.cmp(&right.pointer))
            .then_with(|| left.code.as_str().cmp(right.code.as_str()))
    });
    diagnostics
}

fn resolved_attachment(
    document: &Document,
    definitions: &BTreeMap<ComponentId, &ComponentDefinition>,
    node: &SceneNode,
    index: usize,
    kind: AttachmentKind,
    limits: RouteAttachmentLimits,
) -> Result<Attachment, Diagnostic> {
    let NodeContent::ComponentInstance(instance) = &node.content else {
        return Err(attachment_field_diagnostic(
            DiagnosticCode::RouteAttachmentInvalid,
            index,
            node.id.as_str(),
            "/scene/nodes/{index}/content",
            "route attachments must be component instances",
        ));
    };
    let definition = definitions.get(&instance.definition_id).ok_or_else(|| {
        attachment_field_diagnostic(
            DiagnosticCode::RouteAttachmentInvalid,
            index,
            node.id.as_str(),
            "/scene/nodes/{index}/content",
            "attachment component definition is missing",
        )
    })?;
    if definition.ports.len() > limits.max_ports {
        return Err(attachment_field_diagnostic(
            DiagnosticCode::RouteAttachmentInvalid,
            index,
            node.id.as_str(),
            "/scene/nodes/{index}/content",
            "attachment has too many ports",
        ));
    }
    let ports = definition
        .ports
        .iter()
        .map(|port| {
            let resolved = resolve_port(
                document,
                definitions,
                &InstancePortRef::Instance {
                    node_id: node.id.clone(),
                    port: port.name.clone(),
                },
            )
            .map_err(|_| {
                attachment_field_diagnostic(
                    DiagnosticCode::RouteAttachmentInvalid,
                    index,
                    node.id.as_str(),
                    "/scene/nodes/{index}/content",
                    format!("attachment port {} cannot be resolved", port.name),
                )
            })?;
            Ok((port.name.clone(), resolved))
        })
        .collect::<Result<BTreeMap<_, _>, Diagnostic>>()?;
    Ok(Attachment {
        node_index: index,
        node_id: node.id.to_string(),
        kind,
        line_id: node.metadata["lineId"].clone(),
        host_node_id: node.metadata.get("hostNodeId").cloned(),
        ports,
    })
}

fn validate_tee(document: &Document, attachment: &Attachment, diagnostics: &mut Vec<Diagnostic>) {
    let required = ["run-in", "run-out", "branch"];
    if required
        .iter()
        .any(|name| !attachment.ports.contains_key(*name))
    {
        diagnostics.push(attachment_diagnostic(
            DiagnosticCode::RouteAttachmentInvalid,
            attachment.node_index,
            &attachment.node_id,
            "tee requires run-in, run-out and branch ports",
        ));
        return;
    }
    let run_in = &attachment.ports["run-in"];
    let run_out = &attachment.ports["run-out"];
    let branch = &attachment.ports["branch"];
    if !approximately_equal(run_in.diameter, run_out.diameter)
        || run_in.medium != run_out.medium
        || branch.medium != run_in.medium
        || branch.diameter > run_in.diameter + 1.0e-5
    {
        diagnostics.push(attachment_diagnostic(
            DiagnosticCode::RouteAttachmentInvalid,
            attachment.node_index,
            &attachment.node_id,
            "tee run ports must share diameter and medium, and branch diameter must not exceed the run",
        ));
    }
    if dot(run_in.direction, run_out.direction) > -ATTACHMENT_AXIS_TOLERANCE_RADIANS.cos() {
        diagnostics.push(attachment_diagnostic(
            DiagnosticCode::RouteAttachmentMisaligned,
            attachment.node_index,
            &attachment.node_id,
            "tee run ports must face in opposite directions",
        ));
    }
    let run_axis = normalize(Vec3 {
        x: run_in.position.x - run_out.position.x,
        y: run_in.position.y - run_out.position.y,
        z: run_in.position.z - run_out.position.z,
    });
    if run_axis
        .is_none_or(|axis| dot(axis, run_in.direction) < ATTACHMENT_AXIS_TOLERANCE_RADIANS.cos())
    {
        diagnostics.push(attachment_diagnostic(
            DiagnosticCode::RouteAttachmentMisaligned,
            attachment.node_index,
            &attachment.node_id,
            "tee run ports must be collinear",
        ));
    }
    let branch_dot = dot(run_in.direction, branch.direction);
    if branch_dot.abs() > 30.0_f32.to_radians().cos() {
        diagnostics.push(attachment_diagnostic(
            DiagnosticCode::RouteAttachmentMisaligned,
            attachment.node_index,
            &attachment.node_id,
            "tee branch angle must be between 30 and 150 degrees",
        ));
    }
    let counts = connection_port_counts(document, &attachment.line_id, &attachment.node_id);
    if required
        .iter()
        .any(|port| counts.get(*port).copied() != Some(1))
        || counts.len() != required.len()
    {
        diagnostics.push(attachment_diagnostic(
            DiagnosticCode::RouteAttachmentInvalid,
            attachment.node_index,
            &attachment.node_id,
            "tee requires one connection on each independent port",
        ));
    }
}

fn validate_reducer(
    document: &Document,
    attachment: &Attachment,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let (Some(large), Some(small)) = (attachment.ports.get("large"), attachment.ports.get("small"))
    else {
        diagnostics.push(attachment_diagnostic(
            DiagnosticCode::RouteAttachmentInvalid,
            attachment.node_index,
            &attachment.node_id,
            "reducer requires large and small ports",
        ));
        return;
    };
    if large.diameter <= small.diameter
        || large.medium != small.medium
        || dot(large.direction, small.direction) > -ATTACHMENT_AXIS_TOLERANCE_RADIANS.cos()
    {
        diagnostics.push(attachment_diagnostic(
            DiagnosticCode::RouteAttachmentInvalid,
            attachment.node_index,
            &attachment.node_id,
            "reducer requires decreasing diameters and opposite directions",
        ));
    }
    let axis = normalize(Vec3 {
        x: large.position.x - small.position.x,
        y: large.position.y - small.position.y,
        z: large.position.z - small.position.z,
    });
    if axis.is_none_or(|axis| dot(axis, large.direction) < ATTACHMENT_AXIS_TOLERANCE_RADIANS.cos())
    {
        diagnostics.push(attachment_diagnostic(
            DiagnosticCode::RouteAttachmentMisaligned,
            attachment.node_index,
            &attachment.node_id,
            "reducer ports must be collinear",
        ));
    }
    let counts = connection_port_counts(document, &attachment.line_id, &attachment.node_id);
    if counts.get("large").copied() != Some(1)
        || counts.get("small").copied() != Some(1)
        || counts.len() != 2
    {
        diagnostics.push(attachment_diagnostic(
            DiagnosticCode::RouteAttachmentInvalid,
            attachment.node_index,
            &attachment.node_id,
            "reducer requires one connection on each port",
        ));
    }
}

fn validate_sleeve(
    document: &Document,
    definitions: &BTreeMap<ComponentId, &ComponentDefinition>,
    attachment: &Attachment,
    limits: RouteAttachmentLimits,
    penetration_candidates: &mut usize,
    triangle_pairs: &mut usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let host_id = attachment
        .host_node_id
        .as_ref()
        .expect("sleeve metadata validated");
    let Some(host) = document
        .scene
        .nodes()
        .iter()
        .find(|node| node.id.as_str() == host_id)
    else {
        diagnostics.push(attachment_diagnostic(
            DiagnosticCode::RouteAttachmentInvalid,
            attachment.node_index,
            &attachment.node_id,
            format!("sleeve host {host_id} does not exist"),
        ));
        return;
    };
    if host.metadata.get("routeObstacle").map(String::as_str) == Some("ignored") {
        diagnostics.push(attachment_diagnostic(
            DiagnosticCode::RouteAttachmentInvalid,
            attachment.node_index,
            &attachment.node_id,
            "sleeve host must be a route barrier",
        ));
        return;
    }
    let Some(route) = first_route_for_line(document, definitions, &attachment.line_id) else {
        diagnostics.push(attachment_diagnostic(
            DiagnosticCode::RouteAttachmentInvalid,
            attachment.node_index,
            &attachment.node_id,
            format!("sleeve line {} has no route", attachment.line_id),
        ));
        return;
    };
    let pipe_diameter = route_pipe_diameter(document, &attachment.line_id).unwrap_or(0.0);
    let candidate = attachment_host_bounds(document, definitions, host).is_none_or(|bounds| {
        route.windows(2).any(|segment| {
            segment_bounds_distance([segment[0], segment[1]], &bounds)
                <= pipe_diameter / 2.0 + crate::geometry::CLASH_EPSILON
        })
    });
    if !candidate {
        diagnostics.push(attachment_diagnostic(
            DiagnosticCode::RoutePenetrationInvalid,
            attachment.node_index,
            &attachment.node_id,
            format!("line {} does not cross host {host_id}", attachment.line_id),
        ));
        return;
    }
    *penetration_candidates += 1;
    if *penetration_candidates > limits.max_penetration_candidates {
        diagnostics.push(attachment_diagnostic(
            DiagnosticCode::RouteAttachmentBudget,
            attachment.node_index,
            &attachment.node_id,
            format!(
                "penetration candidate budget {} exceeded",
                limits.max_penetration_candidates
            ),
        ));
        return;
    }
    let Ok(triangles) = host_triangles(document, &host.id) else {
        diagnostics.push(attachment_diagnostic(
            DiagnosticCode::RoutePenetrationInvalid,
            attachment.node_index,
            &attachment.node_id,
            "sleeve host geometry cannot be evaluated",
        ));
        return;
    };
    let mut crossing = false;
    'segments: for segment in route.windows(2) {
        for triangle in &triangles {
            if *triangle_pairs >= limits.max_triangle_pairs {
                diagnostics.push(attachment_diagnostic(
                    DiagnosticCode::RouteAttachmentBudget,
                    attachment.node_index,
                    &attachment.node_id,
                    format!(
                        "attachment triangle-pair budget {} exceeded",
                        limits.max_triangle_pairs
                    ),
                ));
                return;
            }
            *triangle_pairs += 1;
            if crate::geometry::segment_triangle([segment[0], segment[1]], *triangle) {
                crossing = true;
                break 'segments;
            }
        }
    }
    if !crossing {
        diagnostics.push(attachment_diagnostic(
            DiagnosticCode::RoutePenetrationInvalid,
            attachment.node_index,
            &attachment.node_id,
            format!("line {} does not cross host {host_id}", attachment.line_id),
        ));
    }
    let (Some(wall_in), Some(wall_out)) = (
        attachment.ports.get("wall-in"),
        attachment.ports.get("wall-out"),
    ) else {
        diagnostics.push(attachment_diagnostic(
            DiagnosticCode::RouteAttachmentInvalid,
            attachment.node_index,
            &attachment.node_id,
            "sleeve requires wall-in and wall-out ports",
        ));
        return;
    };
    if wall_in.diameter <= pipe_diameter
        || !approximately_equal(wall_out.diameter, wall_in.diameter)
        || wall_in.medium != wall_out.medium
        || dot(wall_in.direction, wall_out.direction) > -ATTACHMENT_AXIS_TOLERANCE_RADIANS.cos()
    {
        diagnostics.push(attachment_diagnostic(
            DiagnosticCode::RouteAttachmentMisaligned,
            attachment.node_index,
            &attachment.node_id,
            "sleeve diameter, media, or axis is invalid",
        ));
    }
    for port in [wall_in, wall_out] {
        let route_distance = point_to_route_distance(port.position, &route);
        let mut surface_distance = f32::INFINITY;
        for triangle in &triangles {
            if *triangle_pairs >= limits.max_triangle_pairs {
                diagnostics.push(attachment_diagnostic(
                    DiagnosticCode::RouteAttachmentBudget,
                    attachment.node_index,
                    &attachment.node_id,
                    format!(
                        "attachment triangle-pair budget {} exceeded",
                        limits.max_triangle_pairs
                    ),
                ));
                return;
            }
            *triangle_pairs += 1;
            surface_distance = surface_distance.min(segment_triangle_distance(
                [to_array(port.position); 2],
                *triangle,
            ));
        }
        if route_distance > ATTACHMENT_POSITION_TOLERANCE_CAP_M
            || surface_distance > SLEEVE_SURFACE_TOLERANCE_M
        {
            diagnostics.push(attachment_diagnostic(
                DiagnosticCode::RouteAttachmentMisaligned,
                attachment.node_index,
                &attachment.node_id,
                format!(
                    "sleeve port is {route_distance:.3} m off route and {surface_distance:.3} m off the host surface"
                ),
            ));
        }
    }
}

fn first_route_for_line(
    document: &Document,
    definitions: &BTreeMap<ComponentId, &ComponentDefinition>,
    line_id: &str,
) -> Option<Vec<[f32; 3]>> {
    document.scene.nodes().iter().find_map(|node| {
        let NodeContent::Connection(connection) = &node.content else {
            return None;
        };
        (connection.metadata.get("lineId").map(String::as_str) == Some(line_id))
            .then(|| route_connection(document, definitions, connection).ok())
            .flatten()
    })
}

fn route_pipe_diameter(document: &Document, line_id: &str) -> Option<f32> {
    document.scene.nodes().iter().find_map(|node| {
        let NodeContent::Connection(connection) = &node.content else {
            return None;
        };
        (connection.metadata.get("lineId").map(String::as_str) == Some(line_id))
            .then_some(connection.diameter)
    })
}

fn attachment_host_bounds(
    document: &Document,
    definitions: &BTreeMap<ComponentId, &ComponentDefinition>,
    host: &SceneNode,
) -> Option<crate::geometry::Bounds> {
    if let NodeContent::ComponentInstance(instance) = &host.content {
        return definitions
            .get(&instance.definition_id)
            .and_then(|definition| {
                crate::large_scene::route_safety::component_bounds(document, host, definition)
            });
    }
    world_bounds(document, host).ok().flatten()
}

fn host_triangles(document: &Document, host_id: &NodeId) -> Result<Vec<[[f32; 3]; 3]>, ()> {
    let mut store = MemoryGeometryStore::default();
    let selected = BTreeSet::from([host_id.clone()]);
    let evaluated =
        evaluate_large_scene_selected(document, &mut store, None, &selected).map_err(|_| ())?;
    let mut triangles = Vec::new();
    for node in evaluated.nodes.iter().filter(|node| {
        node.owner_id
            .split('/')
            .next()
            .is_some_and(|owner| owner == host_id.as_str())
    }) {
        let Some(mesh_key) = &node.mesh_key else {
            continue;
        };
        let Some(mesh) = evaluated.meshes.get(mesh_key) else {
            continue;
        };
        let matrix = transform_matrix(node.transform);
        for triangle in mesh.indices.as_chunks::<3>().0 {
            let mut world = [[0.0_f32; 3]; 3];
            for (target, index) in world.iter_mut().zip(triangle) {
                let position = mesh.vertices[*index as usize].position;
                let point =
                    matrix.transform_point3(GlamVec3::new(position[0], position[1], position[2]));
                *target = [point.x, point.y, point.z];
            }
            triangles.push(world);
        }
    }
    Ok(triangles)
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

fn point_to_route_distance(point: Vec3, route: &[[f32; 3]]) -> f32 {
    route
        .windows(2)
        .map(|segment| point_segment_distance(to_array(point), [segment[0], segment[1]]))
        .fold(f32::INFINITY, f32::min)
}

fn point_segment_distance(point: [f32; 3], segment: [[f32; 3]; 2]) -> f32 {
    let direction = [
        segment[1][0] - segment[0][0],
        segment[1][1] - segment[0][1],
        segment[1][2] - segment[0][2],
    ];
    let length_squared =
        direction[0] * direction[0] + direction[1] * direction[1] + direction[2] * direction[2];
    if length_squared <= f32::EPSILON {
        return distance(point, segment[0]);
    }
    let offset = [
        point[0] - segment[0][0],
        point[1] - segment[0][1],
        point[2] - segment[0][2],
    ];
    let ratio = ((offset[0] * direction[0] + offset[1] * direction[1] + offset[2] * direction[2])
        / length_squared)
        .clamp(0.0, 1.0);
    distance(
        point,
        [
            segment[0][0] + direction[0] * ratio,
            segment[0][1] + direction[1] * ratio,
            segment[0][2] + direction[2] * ratio,
        ],
    )
}

fn distance(left: [f32; 3], right: [f32; 3]) -> f32 {
    ((left[0] - right[0]).powi(2) + (left[1] - right[1]).powi(2) + (left[2] - right[2]).powi(2))
        .sqrt()
}

fn to_array(value: Vec3) -> [f32; 3] {
    [value.x, value.y, value.z]
}

fn connection_port_counts(
    document: &Document,
    line_id: &str,
    node_id: &str,
) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for node in document.scene.nodes() {
        let NodeContent::Connection(connection) = &node.content else {
            continue;
        };
        if connection.metadata.get("lineId").map(String::as_str) != Some(line_id) {
            continue;
        }
        for endpoint in [&connection.from, &connection.to] {
            let InstancePortRef::Instance {
                node_id: endpoint_node,
                port,
            } = endpoint
            else {
                continue;
            };
            if endpoint_node.as_str() == node_id {
                *counts.entry(port.clone()).or_insert(0) += 1;
            }
        }
    }
    counts
}

fn approximately_equal(left: f32, right: f32) -> bool {
    (left - right).abs() <= 1.0e-5
}

fn dot(left: Vec3, right: Vec3) -> f32 {
    left.x * right.x + left.y * right.y + left.z * right.z
}

fn normalize(value: Vec3) -> Option<Vec3> {
    let length = (value.x * value.x + value.y * value.y + value.z * value.z).sqrt();
    (length > f32::EPSILON).then_some(Vec3 {
        x: value.x / length,
        y: value.y / length,
        z: value.z / length,
    })
}

fn attachment_diagnostic(
    code: DiagnosticCode,
    node_index: usize,
    node_id: &str,
    message: impl Into<String>,
) -> Diagnostic {
    Diagnostic {
        code,
        operation_index: None,
        object_id: Some(node_id.into()),
        pointer: format!("/scene/nodes/{node_index}/metadata"),
        message: message.into(),
    }
}

fn attachment_field_diagnostic(
    code: DiagnosticCode,
    node_index: usize,
    node_id: &str,
    pointer: &str,
    message: impl Into<String>,
) -> Diagnostic {
    Diagnostic {
        code,
        operation_index: None,
        object_id: Some(node_id.into()),
        pointer: pointer.replace("{index}", &node_index.to_string()),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Document, NodeContent, SceneNode};
    use crate::geometry::Primitive;
    use crate::ids::NodeId;
    use crate::math::Transform;
    use crate::validation::DiagnosticCode;

    #[test]
    fn rejects_unknown_attachment_kind() {
        let mut document = Document::new("bad-attachment");
        document.scene.nodes_mut().push(SceneNode {
            id: NodeId::from("node-000001"),
            name: "Bad fitting".into(),
            parent_id: None,
            transform: Transform::IDENTITY,
            material_id: None,
            content: NodeContent::Primitive(Primitive::Box {
                width: 1.0,
                height: 1.0,
                depth: 1.0,
            }),
            metadata: BTreeMap::from([
                ("role".into(), "pipe-fitting".into()),
                ("fittingKind".into(), "elbow".into()),
                ("lineId".into(), "L-001".into()),
            ]),
            visible: true,
        });

        let diagnostics = validate_route_attachments(&document, &BTreeMap::new());

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == DiagnosticCode::RouteAttachmentInvalid
                && diagnostic.object_id.as_deref() == Some("node-000001")
                && diagnostic.pointer.ends_with("/metadata/fittingKind")
        }));
    }

    #[test]
    fn attachment_budget_exhaustion_fails_closed() {
        let mut document = Document::new("budget-attachment");
        document.scene.nodes_mut().push(SceneNode {
            id: NodeId::from("node-000001"),
            name: "Attachment".into(),
            parent_id: None,
            transform: Transform::IDENTITY,
            material_id: None,
            content: NodeContent::Primitive(Primitive::Box {
                width: 1.0,
                height: 1.0,
                depth: 1.0,
            }),
            metadata: BTreeMap::from([
                ("role".into(), "pipe-fitting".into()),
                ("fittingKind".into(), "tee".into()),
                ("lineId".into(), "L-001".into()),
            ]),
            visible: true,
        });

        let diagnostics = validate_route_attachments_with_limits(
            &document,
            &BTreeMap::new(),
            RouteAttachmentLimits {
                max_attachments: 0,
                ..RouteAttachmentLimits::default()
            },
        );

        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.code == DiagnosticCode::RouteAttachmentBudget })
        );
    }
}
