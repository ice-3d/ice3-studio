use std::collections::BTreeMap;

use studio_core::document::{Document, NodeContent, SceneNode};
use studio_core::ids::{ComponentId, NodeId};
use studio_core::large_scene::component::*;
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
