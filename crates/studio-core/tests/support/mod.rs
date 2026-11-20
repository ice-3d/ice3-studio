use std::collections::BTreeMap;

use studio_core::document::{Document, NodeContent, Relation, SceneNode};
use studio_core::geometry::Primitive;
use studio_core::ids::{ComponentId, NodeId, RelationId};
use studio_core::large_scene::component::*;
use studio_core::large_scene::connection::{ConnectionIntent, InstancePortRef, RoutePolicy};
use studio_core::math::{Transform, Vec3};

pub fn shared_pump_document() -> Document {
    let component = ComponentDefinition {
        id: ComponentId::from("component-000001"),
        name: "Pump".into(),
        parameters: Vec::new(),
        parts: vec![ComponentPart {
            id: "body".into(),
            parent_id: None,
            transform: PartTransform::default(),
            content: PartContent::Box {
                width: ScalarBinding::Literal(2.0),
                height: ScalarBinding::Literal(1.0),
                depth: ScalarBinding::Literal(1.0),
            },
            subtract: Vec::new(),
            repeat: None,
            material_role: None,
            metadata: BTreeMap::new(),
            visible: true,
        }],
        ports: Vec::new(),
        metadata: BTreeMap::new(),
    };
    let mut document = Document::new("shared");
    document.components.push(component);
    for ordinal in 1..=2 {
        let mut transform = Transform::IDENTITY;
        transform.translation = Vec3 {
            x: ordinal as f32 * 4.0,
            y: 0.0,
            z: 0.0,
        };
        document.scene.nodes_mut().push(SceneNode {
            id: NodeId::new("node", ordinal).unwrap(),
            name: format!("Pump {ordinal}"),
            parent_id: None,
            transform,
            material_id: None,
            content: NodeContent::ComponentInstance(ComponentInstance {
                definition_id: ComponentId::from("component-000001"),
                parameters: BTreeMap::new(),
                overrides: BTreeMap::new(),
                metadata: BTreeMap::new(),
            }),
            metadata: BTreeMap::new(),
            visible: true,
        });
    }
    document
}

#[allow(dead_code)]
pub fn route_collision_document() -> Document {
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
