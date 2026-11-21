use std::collections::BTreeMap;

use crate::document::{Document, NodeContent, SceneNode};
use crate::ids::ComponentId;
use crate::large_scene::component::ComponentDefinition;
use crate::large_scene::connection::InstancePortRef;
use crate::large_scene::route::{ResolvedPort, resolve_port};
use crate::math::Vec3;
use crate::validation::{Diagnostic, DiagnosticCode};

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
    #[allow(dead_code)]
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

pub(crate) fn validate_route_attachments(
    document: &Document,
    definitions: &BTreeMap<ComponentId, &ComponentDefinition>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut count = 0_usize;
    for (index, node) in document.scene.nodes().iter().enumerate() {
        let role = node.metadata.get("role").map(String::as_str);
        if !matches!(role, Some("pipe-fitting" | "pipe-penetration")) {
            continue;
        }
        count += 1;
        if count > MAX_ROUTE_ATTACHMENTS {
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
        match resolved_attachment(document, definitions, node, index, kind) {
            Ok(attachment) => {
                if matches!(attachment.kind, AttachmentKind::Tee) {
                    validate_tee(document, &attachment, &mut diagnostics);
                }
            }
            Err(diagnostic) => diagnostics.push(diagnostic),
        }
    }
    diagnostics
}

fn resolved_attachment(
    document: &Document,
    definitions: &BTreeMap<ComponentId, &ComponentDefinition>,
    node: &SceneNode,
    index: usize,
    kind: AttachmentKind,
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
    if definition.ports.len() > MAX_ATTACHMENT_PORTS {
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
}
