use std::collections::BTreeMap;

use crate::document::{Document, NodeContent};
use crate::ids::ComponentId;
use crate::large_scene::component::ComponentDefinition;
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
    _definitions: &BTreeMap<ComponentId, &ComponentDefinition>,
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
        if !matches!(node.content, NodeContent::ComponentInstance(_)) {
            diagnostics.push(attachment_field_diagnostic(
                DiagnosticCode::RouteAttachmentInvalid,
                index,
                node.id.as_str(),
                "/scene/nodes/{index}/content",
                "route attachments must be component instances",
            ));
        }
    }
    diagnostics
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
