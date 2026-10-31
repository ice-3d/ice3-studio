use std::collections::BTreeMap;

use studio_core::apply::{ApplyRequest, apply_transaction};
use studio_core::document::Document;
use studio_core::ids::{ComponentId, NodeId, Revision};
use studio_core::large_scene::component::*;
use studio_core::math::Transform;
use studio_core::operation::Operation;

#[test]
fn applies_a_component_and_instance_in_one_revision() {
    let mut document = Document::new("transaction");
    let definition = ComponentDefinition {
        id: ComponentId::from("component-000001"),
        name: "Pump".into(),
        parameters: Vec::new(),
        parts: vec![ComponentPart {
            id: "body".into(),
            parent_id: None,
            transform: PartTransform::default(),
            content: PartContent::Box {
                width: ScalarBinding::Literal(1.0),
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
    let operations = vec![
        Operation::ComponentDefine { definition },
        Operation::InstanceCreate {
            node_id: NodeId::from("node-000001"),
            parent_id: None,
            name: "Pump A".into(),
            transform: Transform::IDENTITY,
            instance: ComponentInstance {
                definition_id: ComponentId::from("component-000001"),
                parameters: BTreeMap::new(),
                overrides: BTreeMap::new(),
                metadata: BTreeMap::new(),
            },
            material_id: None,
            metadata: BTreeMap::new(),
        },
    ];
    let result = apply_transaction(
        &mut document,
        ApplyRequest {
            expected_revision: Revision::new(0),
            dry_run: false,
            idempotency_key: "create-pumps".into(),
            operations,
        },
    )
    .unwrap();
    assert_eq!(result.revision, Revision::new(1));
    assert_eq!(document.components.len(), 1);
    assert_eq!(document.scene.nodes().len(), 1);
}
