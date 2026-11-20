mod support;

use std::collections::BTreeMap;

use studio_core::document::{Document, NodeContent, SceneNode};
use studio_core::geometry::Primitive;
use studio_core::ids::{ComponentId, NodeId};
use studio_core::large_scene::component::{
    ComponentDefinition, ComponentInstance, ComponentPart, PartContent, PartTransform,
    ScalarBinding, Vector3Binding,
};
use studio_core::large_scene::connection::RoutePolicy;
use studio_core::large_scene::route_safety::{
    RouteSafetyLimits, RouteSafetyStats, validate_route_safety, validate_route_safety_with_limits,
};
use studio_core::math::{Transform, Vec3};
use studio_core::validation::{Diagnostic, DiagnosticCode};
use support::route_collision_document;

fn definitions(document: &Document) -> BTreeMap<ComponentId, &ComponentDefinition> {
    document
        .components
        .iter()
        .map(|definition| (definition.id.clone(), definition))
        .collect()
}

fn run_route_safety(document: &Document) -> (Vec<Diagnostic>, RouteSafetyStats) {
    let definitions = definitions(document);
    validate_route_safety(document, &definitions)
}

#[allow(dead_code)]
fn route_safety_codes(document: &Document) -> Vec<DiagnosticCode> {
    run_route_safety(document)
        .0
        .into_iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

#[test]
fn direct_route_through_a_static_box_is_rejected() {
    let document = route_collision_document();
    let (diagnostics, stats) = run_route_safety(&document);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::RouteCollision
            && diagnostic.object_id.as_deref() == Some("node-000003")
    }));
    assert_eq!(stats.obstacle_owners, 3);
    assert!(stats.route_segments > 0);
}

#[test]
fn endpoint_owner_contact_is_ignored() {
    assert!(
        !route_safety_codes(&endpoint_contact_document()).contains(&DiagnosticCode::RouteCollision)
    );
}

#[test]
fn rotated_component_part_uses_narrow_phase_after_box_overlap() {
    let (diagnostics, stats) = run_route_safety(&rotated_part_clearance_document());
    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::RouteCollision)
    );
    assert!(stats.narrow_phase_pairs > 0);
    assert!(stats.triangle_pairs > 0);
}

#[test]
fn clearance_surface_rejects_a_low_crossing() {
    for (surface_y, clearance, expected) in [(0.0, 5.0, true), (4.0, 1.0, true), (3.9, 0.5, false)]
    {
        let diagnostics = clearance_diagnostics(surface_y, clearance);
        assert_eq!(
            diagnostics.iter().any(|diagnostic| {
                diagnostic.code == DiagnosticCode::RouteClearanceInsufficient
            }),
            expected,
        );
    }
}

#[test]
fn buried_route_below_clearance_surface_is_allowed() {
    let mut document = route_collision_document();
    set_route_policy(&mut document, RoutePolicy::Buried { depth: 1.5 });
    for node in document.scene.nodes_mut() {
        if node.name == "Obstacle" {
            node.metadata
                .insert("routeObstacle".into(), "clearance-surface".into());
            node.metadata.insert("routeClearance".into(), "5".into());
        }
    }
    assert!(!route_safety_codes(&document).contains(&DiagnosticCode::RouteClearanceInsufficient));
}

#[test]
fn site_surfaces_are_not_solid_route_obstacles() {
    let mut document = route_collision_document();
    for node in document.scene.nodes_mut() {
        if node.name == "Obstacle" {
            node.metadata.insert("role".into(), "site".into());
        }
    }
    assert!(!route_safety_codes(&document).contains(&DiagnosticCode::RouteCollision));
}

#[test]
fn support_gap_across_clearance_surface_is_allowed() {
    let mut document = support_document();
    document.scene.nodes_mut()[1].transform.translation.x = 20.0;
    add_support(&mut document, "node-000004", 6.0, 0.0, 0.0);
    add_support(&mut document, "node-000005", 14.0, 0.0, 0.0);
    document.scene.nodes_mut().push(SceneNode {
        id: NodeId::from("node-000006"),
        name: "Road".into(),
        parent_id: None,
        transform: Transform {
            translation: Vec3 {
                x: 8.0,
                y: 0.0,
                z: 0.0,
            },
            ..Transform::IDENTITY
        },
        material_id: None,
        content: NodeContent::Primitive(Primitive::Box {
            width: 4.0,
            height: 0.1,
            depth: 4.0,
        }),
        metadata: BTreeMap::from([
            ("routeObstacle".into(), "clearance-surface".into()),
            ("routeClearance".into(), "5".into()),
        ]),
        visible: true,
    });
    let diagnostics = run_route_safety(&document).0;
    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::RouteSupportSpacing),
        "{diagnostics:?}",
    );
}

#[test]
fn support_layout_rejects_missing_sparse_and_off_route_stations() {
    assert_support_code(
        missing_support_document(),
        DiagnosticCode::RouteSupportMissing,
    );
    assert_support_code(
        sparse_support_document(),
        DiagnosticCode::RouteSupportSpacing,
    );
    assert_support_code(
        off_route_support_document(),
        DiagnosticCode::RouteSupportSpacing,
    );
    assert_support_code(
        turn_without_support_document(),
        DiagnosticCode::RouteSupportSpacing,
    );
    assert_support_code(orphan_support_document(), DiagnosticCode::OperationInvalid);
    assert_eq!(buried_without_support_codes(), vec![]);
    assert_eq!(without_line_id_support_codes(), vec![]);
}

#[test]
fn route_safety_budget_exhaustion_fails_closed() {
    let (diagnostics, _) = validate_with_segment_limit(0);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::RouteSafetyBudget)
    );
}

fn validate_with_segment_limit(limit: usize) -> (Vec<Diagnostic>, RouteSafetyStats) {
    let document = route_collision_document();
    let definitions = definitions(&document);
    validate_route_safety_with_limits(
        &document,
        &definitions,
        RouteSafetyLimits {
            max_route_segments: limit,
            ..RouteSafetyLimits::default()
        },
    )
}

fn assert_support_code(document: Document, code: DiagnosticCode) {
    let diagnostics = run_route_safety(&document).0;
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.code == code),
        "{diagnostics:?}",
    );
}

fn missing_support_document() -> Document {
    support_document()
}

fn sparse_support_document() -> Document {
    let mut document = support_document();
    document.scene.nodes_mut()[1].transform.translation.x = 20.0;
    add_support(&mut document, "node-000004", 3.0, 0.0, 0.0);
    add_support(&mut document, "node-000005", 13.0, 0.0, 0.0);
    document
}

fn off_route_support_document() -> Document {
    let mut document = support_document();
    add_support(&mut document, "node-000004", 6.0, 0.0, 5.0);
    document
}

fn turn_without_support_document() -> Document {
    let mut document = support_document();
    document.scene.nodes_mut()[1].transform.translation.x = 20.0;
    set_route_policy(&mut document, RoutePolicy::Orthogonal);
    set_support_spacing(&mut document, "100");
    add_support(&mut document, "node-000004", 10.0, 0.5, 0.0);
    document
}

fn orphan_support_document() -> Document {
    let mut document = support_document();
    add_support_with_line(&mut document, "node-000004", 6.0, 0.0, 0.0, "L-ORPHAN");
    document
}

fn buried_without_support_codes() -> Vec<DiagnosticCode> {
    let mut document = support_document();
    set_route_policy(&mut document, RoutePolicy::Buried { depth: 1.0 });
    route_safety_codes(&document)
        .into_iter()
        .filter(|code| {
            matches!(
                code,
                DiagnosticCode::RouteSupportMissing | DiagnosticCode::RouteSupportSpacing
            )
        })
        .collect()
}

fn without_line_id_support_codes() -> Vec<DiagnosticCode> {
    let mut document = support_document();
    let connection = document
        .scene
        .nodes_mut()
        .iter_mut()
        .find_map(|node| match &mut node.content {
            NodeContent::Connection(connection) => Some(connection),
            _ => None,
        })
        .expect("route fixture must contain a connection");
    connection.metadata.remove("lineId");
    route_safety_codes(&document)
        .into_iter()
        .filter(|code| {
            matches!(
                code,
                DiagnosticCode::RouteSupportMissing | DiagnosticCode::RouteSupportSpacing
            )
        })
        .collect()
}

fn support_document() -> Document {
    let mut document = endpoint_contact_document();
    set_support_spacing(&mut document, "6");
    document
}

fn set_route_policy(document: &mut Document, policy: RoutePolicy) {
    let connection = document
        .scene
        .nodes_mut()
        .iter_mut()
        .find_map(|node| match &mut node.content {
            NodeContent::Connection(connection) => Some(connection),
            _ => None,
        })
        .expect("route fixture must contain a connection");
    connection.route_policy = policy;
}

fn set_support_spacing(document: &mut Document, spacing: &str) {
    let connection = document
        .scene
        .nodes_mut()
        .iter_mut()
        .find_map(|node| match &mut node.content {
            NodeContent::Connection(connection) => Some(connection),
            _ => None,
        })
        .expect("route fixture must contain a connection");
    connection
        .metadata
        .insert("maxSupportSpacing".into(), spacing.into());
}

fn add_support(document: &mut Document, id: &str, x: f32, y: f32, z: f32) {
    add_support_with_line(document, id, x, y, z, "L-001");
}

fn add_support_with_line(document: &mut Document, id: &str, x: f32, y: f32, z: f32, line_id: &str) {
    document.scene.nodes_mut().push(SceneNode {
        id: NodeId::from(id),
        name: format!("Support {id}"),
        parent_id: None,
        transform: Transform {
            translation: Vec3 { x, y, z },
            ..Transform::IDENTITY
        },
        material_id: None,
        content: NodeContent::Primitive(Primitive::Box {
            width: 0.2,
            height: 0.2,
            depth: 0.2,
        }),
        metadata: BTreeMap::from([
            ("role".into(), "pipe-support".into()),
            ("supportPart".into(), "beam".into()),
            ("lineId".into(), line_id.into()),
            ("routeObstacle".into(), "ignored".into()),
        ]),
        visible: true,
    });
}

fn clearance_diagnostics(surface_y: f32, clearance: f32) -> Vec<Diagnostic> {
    let mut document = route_collision_document();
    for node in document.scene.nodes_mut() {
        if node.name.starts_with("Pump") {
            node.transform.translation.y = 5.0;
        }
        if node.name == "Obstacle" {
            node.transform.translation.y = surface_y;
            node.metadata
                .insert("routeObstacle".into(), "clearance-surface".into());
            node.metadata
                .insert("routeClearance".into(), clearance.to_string());
        }
    }
    run_route_safety(&document).0
}

fn endpoint_contact_document() -> Document {
    let mut document = route_collision_document();
    document
        .scene
        .nodes_mut()
        .retain(|node| node.name != "Obstacle");
    document
}

fn rotated_part_clearance_document() -> Document {
    let mut document = route_collision_document();
    document
        .scene
        .nodes_mut()
        .retain(|node| node.name == "Pump 1" || node.name == "Pump 2" || node.name == "Pipe");
    for node in document.scene.nodes_mut() {
        if node.name.starts_with("Pump") {
            node.metadata
                .insert("routeObstacle".into(), "ignored".into());
        }
        if let NodeContent::Connection(connection) = &mut node.content {
            connection.route_policy = RoutePolicy::Orthogonal;
        }
    }
    document.components.push(ComponentDefinition {
        id: ComponentId::from("component-000002"),
        name: "Clearance frame".into(),
        parameters: Vec::new(),
        parts: [-0.75, 0.75]
            .into_iter()
            .enumerate()
            .map(|(index, z)| ComponentPart {
                id: format!("rail-{index}"),
                parent_id: None,
                transform: PartTransform {
                    translation: Vector3Binding::Literal(Vec3 { x: 0.0, y: 0.0, z }),
                    ..PartTransform::default()
                },
                content: PartContent::Box {
                    width: ScalarBinding::Literal(4.0),
                    height: ScalarBinding::Literal(0.5),
                    depth: ScalarBinding::Literal(0.5),
                },
                subtract: Vec::new(),
                repeat: None,
                material_role: None,
                metadata: BTreeMap::new(),
                visible: true,
            })
            .collect(),
        ports: Vec::new(),
        metadata: BTreeMap::new(),
    });
    document.scene.nodes_mut().push(SceneNode {
        id: NodeId::from("node-000004"),
        name: "Clearance frame".into(),
        parent_id: None,
        transform: Transform {
            translation: Vec3 {
                x: 6.0,
                y: 0.0,
                z: 0.0,
            },
            ..Transform::IDENTITY
        },
        material_id: None,
        content: NodeContent::ComponentInstance(ComponentInstance {
            definition_id: ComponentId::from("component-000002"),
            parameters: BTreeMap::new(),
            overrides: BTreeMap::new(),
            metadata: BTreeMap::new(),
        }),
        metadata: BTreeMap::new(),
        visible: true,
    });
    document
}
