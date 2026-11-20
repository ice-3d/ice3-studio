mod support;

use std::collections::BTreeMap;

use studio_core::document::{Document, NodeContent, Relation, SceneNode};
use studio_core::geometry::Primitive;
use studio_core::ids::{ComponentId, NodeId, RelationId};
use studio_core::large_scene::component::{
    ComponentDefinition, ComponentInstance, ComponentPart, ComponentPort, PartContent,
    PartTransform, ScalarBinding, Vector3Binding,
};
use studio_core::large_scene::connection::{ConnectionIntent, InstancePortRef, RoutePolicy};
use studio_core::large_scene::route_safety::{RouteSafetyStats, validate_route_safety};
use studio_core::math::{Transform, Vec3};
use studio_core::validation::{Diagnostic, DiagnosticCode};
use support::shared_pump_document;

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
    let document = collision_document();
    let (diagnostics, stats) = run_route_safety(&document);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::RouteCollision
            && diagnostic.object_id.as_deref() == Some("node-000003")
    }));
    assert_eq!(stats.obstacle_owners, 3);
    assert!(stats.route_segments > 0);
}

#[test]
fn endpoint_owner_contact_is_ignored_but_reentry_is_not() {
    assert!(
        !route_safety_codes(&endpoint_contact_document()).contains(&DiagnosticCode::RouteCollision)
    );
    assert!(
        route_safety_codes(&endpoint_reentry_document()).contains(&DiagnosticCode::RouteCollision)
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

fn collision_document() -> Document {
    let mut document = shared_pump_document();
    document.components[0].ports = vec![
        ComponentPort {
            name: "outlet".into(),
            part_id: "body".into(),
            position: Vec3 {
                x: 1.0,
                y: 0.0,
                z: 0.0,
            },
            direction: Vec3 {
                x: 1.0,
                y: 0.0,
                z: 0.0,
            },
            medium: "water".into(),
            diameter: 0.2,
        },
        ComponentPort {
            name: "inlet".into(),
            part_id: "body".into(),
            position: Vec3 {
                x: -1.0,
                y: 0.0,
                z: 0.0,
            },
            direction: Vec3 {
                x: -1.0,
                y: 0.0,
                z: 0.0,
            },
            medium: "water".into(),
            diameter: 0.2,
        },
    ];
    document.relations.push(Relation {
        id: RelationId::from("relation-000001"),
        relation_type: "process".into(),
        from_node_id: NodeId::from("node-000001"),
        to_node_id: NodeId::from("node-000002"),
        metadata: BTreeMap::new(),
    });
    document.scene.nodes_mut().push(SceneNode {
        id: NodeId::from("node-000003"),
        name: "Pipe".into(),
        parent_id: None,
        transform: Transform::IDENTITY,
        material_id: None,
        content: NodeContent::Connection(ConnectionIntent {
            from: InstancePortRef::Instance {
                node_id: NodeId::from("node-000001"),
                port: "outlet".into(),
            },
            to: InstancePortRef::Instance {
                node_id: NodeId::from("node-000002"),
                port: "inlet".into(),
            },
            medium: "water".into(),
            diameter: 0.2,
            route_policy: RoutePolicy::Direct,
            relation_id: RelationId::from("relation-000001"),
            metadata: BTreeMap::from([("lineId".into(), "L-001".into())]),
        }),
        metadata: BTreeMap::new(),
        visible: true,
    });
    document.scene.nodes_mut().push(SceneNode {
        id: NodeId::from("node-000004"),
        name: "Obstacle".into(),
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
        content: NodeContent::Primitive(Primitive::Box {
            width: 0.5,
            height: 0.5,
            depth: 2.0,
        }),
        metadata: BTreeMap::new(),
        visible: true,
    });
    document
}

fn endpoint_contact_document() -> Document {
    let mut document = collision_document();
    document
        .scene
        .nodes_mut()
        .retain(|node| node.name != "Obstacle");
    document
}

fn endpoint_reentry_document() -> Document {
    let mut document = shared_pump_document();
    document
        .scene
        .nodes_mut()
        .retain(|node| node.id.as_str() == "node-000001");
    document.components[0].parts[0].content = PartContent::Box {
        width: ScalarBinding::Literal(10.0),
        height: ScalarBinding::Literal(1.0),
        depth: ScalarBinding::Literal(1.0),
    };
    document.components[0].ports = vec![
        ComponentPort {
            name: "left".into(),
            part_id: "body".into(),
            position: Vec3 {
                x: -5.0,
                y: 0.0,
                z: 0.0,
            },
            direction: Vec3 {
                x: -1.0,
                y: 0.0,
                z: 0.0,
            },
            medium: "water".into(),
            diameter: 0.2,
        },
        ComponentPort {
            name: "right".into(),
            part_id: "body".into(),
            position: Vec3 {
                x: 5.0,
                y: 0.0,
                z: 0.0,
            },
            direction: Vec3 {
                x: 1.0,
                y: 0.0,
                z: 0.0,
            },
            medium: "water".into(),
            diameter: 0.2,
        },
    ];
    document.relations.push(Relation {
        id: RelationId::from("relation-000001"),
        relation_type: "process".into(),
        from_node_id: NodeId::from("node-000001"),
        to_node_id: NodeId::from("node-000001"),
        metadata: BTreeMap::new(),
    });
    document.scene.nodes_mut().push(SceneNode {
        id: NodeId::from("node-000002"),
        name: "Reentering pipe".into(),
        parent_id: None,
        transform: Transform::IDENTITY,
        material_id: None,
        content: NodeContent::Connection(ConnectionIntent {
            from: InstancePortRef::Instance {
                node_id: NodeId::from("node-000001"),
                port: "left".into(),
            },
            to: InstancePortRef::Instance {
                node_id: NodeId::from("node-000001"),
                port: "right".into(),
            },
            medium: "water".into(),
            diameter: 0.2,
            route_policy: RoutePolicy::Direct,
            relation_id: RelationId::from("relation-000001"),
            metadata: BTreeMap::from([("lineId".into(), "L-001".into())]),
        }),
        metadata: BTreeMap::new(),
        visible: true,
    });
    document
}

fn rotated_part_clearance_document() -> Document {
    let mut document = collision_document();
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
