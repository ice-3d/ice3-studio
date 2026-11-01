use std::collections::BTreeMap;

use studio_core::document::Constraint;
use studio_core::document::Document;
use studio_core::ids::{ComponentId, ConstraintId};
use studio_core::large_scene::component::*;
use studio_core::large_scene::validate::{resolve_parameters, validate_large_scene};
use studio_core::math::Vec3;
use studio_core::validation::{DiagnosticCode, validate_document};

mod support;

use support::shared_pump_document;

fn definition(id: &str) -> ComponentDefinition {
    ComponentDefinition {
        id: ComponentId::from(id),
        name: "Equipment".into(),
        parameters: vec![ComponentParameter {
            name: "width".into(),
            value_type: ParameterType::Number {
                minimum: Some(0.1),
                maximum: Some(10.0),
            },
            default: ParameterValue::Number(1.0),
            unit: Some("m".into()),
        }],
        parts: vec![ComponentPart {
            id: "body".into(),
            parent_id: None,
            transform: PartTransform::default(),
            content: PartContent::Box {
                width: ScalarBinding::Parameter {
                    param: "width".into(),
                },
                height: ScalarBinding::Literal(1.0),
                depth: ScalarBinding::Literal(1.0),
            },
            subtract: Vec::new(),
            repeat: None,
            material_role: None,
            metadata: BTreeMap::new(),
            visible: true,
        }],
        ports: vec![ComponentPort {
            name: "inlet".into(),
            part_id: "body".into(),
            position: Vec3 {
                x: -0.5,
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
        }],
        metadata: BTreeMap::new(),
    }
}

#[test]
fn accepts_a_valid_component_definition() {
    let mut document = Document::new("valid");
    document.components.push(definition("component-000001"));
    assert!(validate_large_scene(&document).is_empty());
}

#[test]
fn rejects_parameter_out_of_range_and_wrong_type() {
    let component = definition("component-000001");
    let mut values = BTreeMap::new();
    values.insert("width".into(), ParameterValue::Number(20.0));
    assert!(resolve_parameters(&component, &values).is_err());

    values.insert("width".into(), ParameterValue::Enumeration("wide".into()));
    assert!(resolve_parameters(&component, &values).is_err());
}

#[test]
fn rejects_unknown_and_cyclic_component_references() {
    let mut first = definition("component-000001");
    first.parts[0].content = PartContent::ComponentReference {
        definition_id: ComponentId::from("component-000002"),
        parameters: BTreeMap::new(),
    };
    let mut second = definition("component-000002");
    second.parts[0].content = PartContent::ComponentReference {
        definition_id: ComponentId::from("component-000001"),
        parameters: BTreeMap::new(),
    };
    let mut document = Document::new("cycle");
    document.components.extend([first, second]);
    let messages = validate_large_scene(&document)
        .into_iter()
        .map(|diagnostic| diagnostic.message)
        .collect::<Vec<_>>();
    assert!(messages.iter().any(|message| message.contains("cycle")));
}

#[test]
fn enforces_no_overlap_for_component_instances() {
    let mut document = shared_pump_document();
    document.scene.nodes_mut()[1].transform.translation.x = 3.5;
    document.constraints.push(Constraint {
        id: ConstraintId::from("constraint-000001"),
        constraint_type: "no_overlap".into(),
        node_ids: vec!["node-000001".into(), "node-000002".into()],
        metadata: BTreeMap::new(),
    });

    let diagnostics = validate_document(&document);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::ConstraintFailed)
    );
}
