mod support;

use std::collections::BTreeMap;

use studio_core::document::{NodeContent, SceneNode};
use studio_core::ids::Revision;
use studio_core::large_scene::route_inspection::{RouteInspectionLimits, inspect_route};
use studio_core::math::Transform;

fn definitions(
    document: &studio_core::document::Document,
) -> BTreeMap<
    studio_core::ids::ComponentId,
    &studio_core::large_scene::component::ComponentDefinition,
> {
    document
        .components
        .iter()
        .map(|definition| (definition.id.clone(), definition))
        .collect()
}

#[test]
fn inspects_the_formal_route_and_resolved_endpoints() {
    let mut document = support::route_collision_document();
    document.revision = Revision::new(1);
    let result = inspect_route(
        &document,
        &definitions(&document),
        "L-001",
        Some(1),
        RouteInspectionLimits::default(),
    )
    .unwrap();

    assert_eq!(result.revision, 1);
    assert_eq!(result.line_id, "L-001");
    assert_eq!(result.segments.len(), 1);
    let segment = &result.segments[0];
    assert_eq!(segment.node_id, "node-000003");
    assert_eq!(segment.from.port, "outlet");
    assert_eq!(segment.to.port, "inlet");
    assert_eq!(segment.from.position.x, 5.0);
    assert_eq!(segment.to.position.x, 7.0);
    assert_eq!(segment.points.first(), Some(&segment.from_position_array()));
    assert_eq!(segment.points.last(), Some(&segment.to_position_array()));
    assert!(!result.truncated.segments);
    assert!(!result.truncated.points);
}

fn add_inspection_nodes(document: &mut studio_core::document::Document) {
    let fitting = SceneNode {
        id: studio_core::ids::NodeId::from("node-000009"),
        name: "Reducer".into(),
        parent_id: None,
        transform: Transform::IDENTITY,
        material_id: None,
        content: NodeContent::Group,
        metadata: BTreeMap::from([
            ("role".into(), "pipe-fitting".into()),
            ("fittingKind".into(), "reducer".into()),
            ("lineId".into(), "L-001".into()),
        ]),
        visible: true,
    };
    let host = SceneNode {
        id: studio_core::ids::NodeId::from("node-000008"),
        name: "Wall".into(),
        parent_id: None,
        transform: Transform::IDENTITY,
        material_id: None,
        content: NodeContent::Group,
        metadata: BTreeMap::new(),
        visible: true,
    };
    let mut sleeve = fitting.clone();
    sleeve.id = studio_core::ids::NodeId::from("node-000010");
    sleeve.name = "Sleeve".into();
    sleeve.metadata = BTreeMap::from([
        ("role".into(), "pipe-penetration".into()),
        ("fittingKind".into(), "sleeve".into()),
        ("lineId".into(), "L-001".into()),
        ("hostNodeId".into(), "node-000008".into()),
    ]);
    let support_a = support_node("node-000012", "S-A", "L-SUPPORT", 12.0);
    let support_b = support_node("node-000011", "S-B", "L-SUPPORT", 6.0);
    let unrelated = support_node("node-000013", "S-X", "L-OTHER", 8.0);
    document
        .scene
        .nodes_mut()
        .extend([host, fitting, sleeve, support_a, support_b, unrelated]);
    if let Some(connection) = document.scene.nodes_mut().iter_mut().find_map(|node| {
        let NodeContent::Connection(connection) = &mut node.content else {
            return None;
        };
        Some(connection)
    }) {
        connection
            .metadata
            .insert("supportLineId".into(), "L-SUPPORT".into());
    }
}

fn support_node(id: &str, name: &str, line: &str, x: f32) -> SceneNode {
    SceneNode {
        id: studio_core::ids::NodeId::from(id),
        name: name.into(),
        parent_id: None,
        transform: Transform {
            translation: studio_core::math::Vec3 { x, y: 0.0, z: 0.0 },
            ..Transform::IDENTITY
        },
        material_id: None,
        content: NodeContent::Group,
        metadata: BTreeMap::from([
            ("role".into(), "pipe-support".into()),
            ("supportLineId".into(), line.into()),
            ("supportGroup".into(), format!("{line}-group")),
        ]),
        visible: true,
    }
}

#[test]
fn aggregates_only_matching_line_facts_in_stable_order() {
    let mut document = support::route_collision_document();
    add_inspection_nodes(&mut document);
    let result = inspect_route(
        &document,
        &definitions(&document),
        "L-001",
        None,
        RouteInspectionLimits::default(),
    )
    .unwrap();

    assert_eq!(result.support_line_ids, vec!["L-SUPPORT"]);
    assert_eq!(
        result
            .attachments
            .iter()
            .map(|item| item.node_id.as_str())
            .collect::<Vec<_>>(),
        vec!["node-000009", "node-000010"]
    );
    assert_eq!(result.attachments[1].host_name.as_deref(), Some("Wall"));
    assert_eq!(
        result
            .supports
            .iter()
            .map(|item| item.node_id.as_str())
            .collect::<Vec<_>>(),
        vec!["node-000011", "node-000012"]
    );
}

#[test]
fn reports_revision_and_missing_line_errors() {
    let document = support::route_collision_document();
    assert!(matches!(
        inspect_route(
            &document,
            &definitions(&document),
            "L-001",
            Some(99),
            RouteInspectionLimits::default(),
        ),
        Err(
            studio_core::large_scene::route_inspection::RouteInspectionError::RevisionConflict { .. }
        )
    ));
    assert!(matches!(
        inspect_route(
            &document,
            &definitions(&document),
            "L-MISSING",
            None,
            RouteInspectionLimits::default(),
        ),
        Err(studio_core::large_scene::route_inspection::RouteInspectionError::LineNotFound { .. })
    ));
}

#[test]
fn marks_every_budget_when_results_are_truncated() {
    let mut document = support::route_collision_document();
    add_inspection_nodes(&mut document);
    let result = inspect_route(
        &document,
        &definitions(&document),
        "L-001",
        None,
        RouteInspectionLimits {
            max_segments: 1,
            max_points: 1,
            max_attachments: 1,
            max_supports: 1,
        },
    )
    .unwrap();
    assert!(result.truncated.points);
    assert!(result.truncated.attachments);
    assert!(result.truncated.supports);
    assert_eq!(result.attachments.len(), 1);
    assert_eq!(result.supports.len(), 1);
}
