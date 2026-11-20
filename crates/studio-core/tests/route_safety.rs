mod support;

use std::collections::BTreeMap;

use studio_core::document::{Document, NodeContent, Relation, SceneNode};
use studio_core::geometry::Primitive;
use studio_core::ids::{ComponentId, NodeId, RelationId};
use studio_core::large_scene::component::{ComponentDefinition, ComponentPort};
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
