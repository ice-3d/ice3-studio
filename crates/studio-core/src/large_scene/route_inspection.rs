use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::document::{Document, NodeContent};
use crate::ids::{ComponentId, NodeId};
use crate::large_scene::component::ComponentDefinition;
use crate::large_scene::connection::{InstancePortRef, RoutePolicy};
use crate::large_scene::route::{ConnectionError, resolve_port, route_connection};
use crate::math::Vec3;

pub const MAX_INSPECTION_SEGMENTS: usize = 128;
pub const MAX_INSPECTION_POINTS: usize = 4_096;
pub const MAX_INSPECTION_ATTACHMENTS: usize = 128;
pub const MAX_INSPECTION_SUPPORTS: usize = 512;
pub const MAX_LINE_ID_CHARS: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct RouteInspectionParams {
    pub line_id: String,
    pub revision: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteInspectionLimits {
    pub max_segments: usize,
    pub max_points: usize,
    pub max_attachments: usize,
    pub max_supports: usize,
}

impl Default for RouteInspectionLimits {
    fn default() -> Self {
        Self {
            max_segments: MAX_INSPECTION_SEGMENTS,
            max_points: MAX_INSPECTION_POINTS,
            max_attachments: MAX_INSPECTION_ATTACHMENTS,
            max_supports: MAX_INSPECTION_SUPPORTS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RouteInspectionPort {
    pub node_id: String,
    pub port: String,
    pub medium: String,
    pub diameter: f32,
    pub position: Vec3,
    pub direction: Vec3,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RouteInspectionSegment {
    pub node_id: String,
    pub name: String,
    pub medium: String,
    pub diameter: f32,
    pub route_policy: RoutePolicy,
    pub from: RouteInspectionPort,
    pub to: RouteInspectionPort,
    pub points: Vec<[f32; 3]>,
}

impl RouteInspectionSegment {
    pub fn from_position_array(&self) -> [f32; 3] {
        [
            self.from.position.x,
            self.from.position.y,
            self.from.position.z,
        ]
    }

    pub fn to_position_array(&self) -> [f32; 3] {
        [self.to.position.x, self.to.position.y, self.to.position.z]
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RouteInspectionTruncation {
    pub segments: bool,
    pub points: bool,
    pub attachments: bool,
    pub supports: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RouteInspection {
    pub revision: u64,
    pub line_id: String,
    pub support_line_ids: Vec<String>,
    pub segments: Vec<RouteInspectionSegment>,
    pub attachments: Vec<RouteInspectionAttachment>,
    pub supports: Vec<RouteInspectionSupport>,
    pub truncated: RouteInspectionTruncation,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RouteInspectionAttachment {
    pub node_id: String,
    pub name: String,
    pub role: String,
    pub fitting_kind: String,
    pub line_id: String,
    pub support_line_id: Option<String>,
    pub host_node_id: Option<String>,
    pub host_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RouteInspectionSupport {
    pub node_id: String,
    pub name: String,
    pub support_line_id: String,
    pub support_group: Option<String>,
    pub position: Vec3,
}

#[derive(Debug, thiserror::Error)]
pub enum RouteInspectionError {
    #[error("route inspection revision conflict: expected {expected}, actual {actual}")]
    RevisionConflict { expected: u64, actual: u64 },
    #[error("route line {line_id} was not found")]
    LineNotFound { line_id: String },
    #[error("route line id is invalid")]
    InvalidLineId,
    #[error("route {node_id} cannot be resolved")]
    Route {
        node_id: NodeId,
        #[source]
        source: ConnectionError,
    },
}

pub fn inspect_route(
    document: &Document,
    definitions: &BTreeMap<ComponentId, &ComponentDefinition>,
    line_id: &str,
    expected_revision: Option<u64>,
    limits: RouteInspectionLimits,
) -> Result<RouteInspection, RouteInspectionError> {
    if line_id.trim().is_empty() || line_id.chars().count() > MAX_LINE_ID_CHARS {
        return Err(RouteInspectionError::InvalidLineId);
    }
    let actual = document.revision.as_u64();
    if let Some(expected) = expected_revision
        && expected != actual
    {
        return Err(RouteInspectionError::RevisionConflict { expected, actual });
    }
    let mut connections = document
        .scene
        .nodes()
        .iter()
        .filter(|node| {
            matches!(&node.content, NodeContent::Connection(connection)
                if connection.metadata.get("lineId").map(String::as_str) == Some(line_id))
        })
        .collect::<Vec<_>>();
    if connections.is_empty() {
        return Err(RouteInspectionError::LineNotFound {
            line_id: line_id.to_string(),
        });
    }
    connections.sort_by(|left, right| left.id.as_str().cmp(right.id.as_str()));
    let segments_truncated = connections.len() > limits.max_segments;
    let mut points_left = limits.max_points;
    let mut points_truncated = false;
    let mut segments = Vec::new();
    for node in connections.into_iter().take(limits.max_segments) {
        let NodeContent::Connection(connection) = &node.content else {
            unreachable!();
        };
        let from = resolve_port(document, definitions, &connection.from).map_err(|source| {
            RouteInspectionError::Route {
                node_id: node.id.clone(),
                source,
            }
        })?;
        let to = resolve_port(document, definitions, &connection.to).map_err(|source| {
            RouteInspectionError::Route {
                node_id: node.id.clone(),
                source,
            }
        })?;
        let mut points = route_connection(document, definitions, connection).map_err(|source| {
            RouteInspectionError::Route {
                node_id: node.id.clone(),
                source,
            }
        })?;
        if points.len() > points_left {
            points.truncate(points_left);
            points_truncated = true;
        }
        points_left = points_left.saturating_sub(points.len());
        segments.push(RouteInspectionSegment {
            node_id: node.id.to_string(),
            name: node.name.clone(),
            medium: connection.medium.clone(),
            diameter: connection.diameter,
            route_policy: connection.route_policy.clone(),
            from: RouteInspectionPort {
                node_id: connection_owner(&connection.from),
                port: endpoint_port(&connection.from),
                medium: from.medium,
                diameter: from.diameter,
                position: from.position,
                direction: from.direction,
            },
            to: RouteInspectionPort {
                node_id: connection_owner(&connection.to),
                port: endpoint_port(&connection.to),
                medium: to.medium,
                diameter: to.diameter,
                position: to.position,
                direction: to.direction,
            },
            points,
        });
    }
    Ok(RouteInspection {
        revision: actual,
        line_id: line_id.to_string(),
        support_line_ids: Vec::new(),
        segments,
        attachments: Vec::new(),
        supports: Vec::new(),
        truncated: RouteInspectionTruncation {
            segments: segments_truncated,
            points: points_truncated,
            attachments: false,
            supports: false,
        },
    })
}

fn connection_owner(reference: &InstancePortRef) -> String {
    match reference {
        InstancePortRef::Instance { node_id, .. } => node_id.to_string(),
        InstancePortRef::SetItem { set_node_id, .. } => set_node_id.to_string(),
    }
}

fn endpoint_port(reference: &InstancePortRef) -> String {
    match reference {
        InstancePortRef::Instance { port, .. } | InstancePortRef::SetItem { port, .. } => {
            port.clone()
        }
    }
}
