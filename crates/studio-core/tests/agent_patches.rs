use std::collections::BTreeMap;

use studio_core::apply::{ApplyRequest, TransactionError, apply_transaction};
use studio_core::document::{Document, NodeContent, Relation, SceneNode};
use studio_core::ids::{ComponentId, MaterialId, NodeId, RelationId, Revision};
use studio_core::large_scene::component::{
    ComponentDefinition, ComponentInstance, ComponentParameter, ComponentPart, ComponentPort,
    ParameterType, ParameterValue, PartContent, PartOverride, PartTransform, ScalarBinding,
};
use studio_core::large_scene::connection::{ConnectionIntent, InstancePortRef, RoutePolicy};
use studio_core::math::Vec3;
use studio_core::operation::Operation;
use studio_core::patch::{ComponentPatch, ConnectionPatch, InstancePatch, MaterialPatch};
use ts_rs::TS;

#[test]
fn sparse_patch_typescript_marks_omittable_fields_optional() {
    let instance = InstancePatch::decl();
    assert!(instance.contains("nodeId: NodeId"));
    assert!(instance.contains("name?: string | null"));
    assert!(instance.contains("setParameters?:"));
    assert!(instance.contains("removeParameters?: Array<string>"));
    assert!(instance.contains("clearMaterial?: boolean"));

    let material = MaterialPatch::decl();
    assert!(material.contains("materialId: MaterialId"));
    assert!(material.contains("baseColor?: [number, number, number, number] | null"));
    assert!(material.contains("clearEdgeColor?: boolean"));

    let component = ComponentPatch::decl();
    assert!(component.contains("componentId: ComponentId"));
    assert!(component.contains("setParameterDefaults?:"));
    assert!(component.contains("removeMetadataKeys?: Array<string>"));

    let connection = ConnectionPatch::decl();
    assert!(connection.contains("nodeId: NodeId"));
    assert!(connection.contains("from?: InstancePortRef | null"));
    assert!(connection.contains("setMetadata?:"));
}

#[test]
fn instance_patch_changes_only_named_fields() {
    let mut document = fixture_document();
    let original_transform = document.scene.nodes()[0].transform;
    let result = apply_transaction(
        &mut document,
        request(Operation::InstancePatch(InstancePatch {
            node_id: NodeId::from("node-000001"),
            name: Some("Pump 2".into()),
            transform: None,
            visible: Some(false),
            definition_id: None,
            set_parameters: [("capacity".into(), ParameterValue::Number(6.0))]
                .into_iter()
                .collect(),
            remove_parameters: Vec::new(),
            set_metadata: [("service".into(), "standby".into())].into_iter().collect(),
            remove_metadata_keys: vec!["area".into()],
            material_id: None,
            clear_material: true,
        })),
    )
    .unwrap();

    let node = &document.scene.nodes()[0];
    let NodeContent::ComponentInstance(instance) = &node.content else {
        panic!("fixture node must remain a component instance");
    };
    assert_eq!(node.name, "Pump 2");
    assert_eq!(node.transform, original_transform);
    assert!(!node.visible);
    assert_eq!(instance.parameters["capacity"], ParameterValue::Number(6.0));
    assert_eq!(instance.parameters["speed"], ParameterValue::Number(2.0));
    assert_eq!(node.metadata["role"], "pump");
    assert_eq!(node.metadata["service"], "standby");
    assert!(!node.metadata.contains_key("area"));
    assert_eq!(
        instance.overrides["body"].metadata.as_ref().unwrap()["finish"],
        "paint"
    );
    assert_eq!(node.material_id, None);
    assert_eq!(result.diff.updated_node_ids, vec![node.id.clone()]);
    assert!(result.diff.added_node_ids.is_empty());
    assert!(result.diff.removed_node_ids.is_empty());
}

#[test]
fn material_patch_changes_only_requested_fields() {
    let mut document = fixture_document();
    let original = document.materials[0].clone();
    let result = apply_transaction(
        &mut document,
        request(Operation::MaterialPatch(MaterialPatch {
            material_id: MaterialId::from("material-000001"),
            name: None,
            base_color: None,
            opacity: Some(0.4),
            edge_color: None,
            clear_edge_color: false,
        })),
    )
    .unwrap();

    let patched = &document.materials[0];
    assert_eq!(patched.name, original.name);
    assert_eq!(patched.base_color, original.base_color);
    assert_eq!(patched.edge_color, original.edge_color);
    assert_eq!(patched.opacity, 0.4);
    assert_eq!(result.diff.updated_material_ids, vec![patched.id.clone()]);
}

#[test]
fn instance_patch_rejects_conflicting_clear_and_material_assignment() {
    let mut document = fixture_document();
    let error = apply_transaction(
        &mut document,
        request(Operation::InstancePatch(InstancePatch {
            node_id: NodeId::from("node-000001"),
            name: None,
            transform: None,
            visible: None,
            definition_id: None,
            set_parameters: BTreeMap::new(),
            remove_parameters: Vec::new(),
            set_metadata: BTreeMap::new(),
            remove_metadata_keys: Vec::new(),
            material_id: Some(MaterialId::from("material-000001")),
            clear_material: true,
        })),
    )
    .unwrap_err();

    assert!(matches!(
        error,
        TransactionError::OperationInvalid {
            code: studio_core::validation::DiagnosticCode::OperationInvalid,
            pointer,
            ..
        } if pointer == "/clearMaterial"
    ));
    assert_eq!(document.revision, Revision::new(0));
}

#[test]
fn instance_patch_rejects_empty_and_overlapping_parameter_changes() {
    let mut document = fixture_document();
    let empty = apply_transaction(
        &mut document,
        request(Operation::InstancePatch(InstancePatch {
            node_id: NodeId::from("node-000001"),
            name: None,
            transform: None,
            visible: None,
            definition_id: None,
            set_parameters: BTreeMap::new(),
            remove_parameters: Vec::new(),
            set_metadata: BTreeMap::new(),
            remove_metadata_keys: Vec::new(),
            material_id: None,
            clear_material: false,
        })),
    );
    assert!(empty.is_err());

    let overlap = apply_transaction(
        &mut document,
        request(Operation::InstancePatch(InstancePatch {
            node_id: NodeId::from("node-000001"),
            name: None,
            transform: None,
            visible: None,
            definition_id: None,
            set_parameters: [("capacity".into(), ParameterValue::Number(5.0))]
                .into_iter()
                .collect(),
            remove_parameters: vec!["capacity".into()],
            set_metadata: BTreeMap::new(),
            remove_metadata_keys: Vec::new(),
            material_id: None,
            clear_material: false,
        })),
    );
    assert!(overlap.is_err());
    assert_eq!(document.revision, Revision::new(0));
}

#[test]
fn instance_patch_rejects_metadata_set_remove_overlap() {
    let mut document = fixture_document();
    let error = apply_transaction(
        &mut document,
        request(Operation::InstancePatch(InstancePatch {
            node_id: NodeId::from("node-000001"),
            name: None,
            transform: None,
            visible: None,
            definition_id: None,
            set_parameters: BTreeMap::new(),
            remove_parameters: Vec::new(),
            set_metadata: [("role".into(), "pump-2".into())].into_iter().collect(),
            remove_metadata_keys: vec!["role".into()],
            material_id: None,
            clear_material: false,
        })),
    );

    assert!(error.is_err());
    assert_eq!(document.revision, Revision::new(0));
}

#[test]
fn instance_patch_rejects_unknown_parameter_name() {
    let mut document = fixture_document();
    let error = apply_transaction(
        &mut document,
        request(Operation::InstancePatch(InstancePatch {
            node_id: NodeId::from("node-000001"),
            name: None,
            transform: None,
            visible: None,
            definition_id: None,
            set_parameters: [("unknown".into(), ParameterValue::Number(1.0))]
                .into_iter()
                .collect(),
            remove_parameters: Vec::new(),
            set_metadata: BTreeMap::new(),
            remove_metadata_keys: Vec::new(),
            material_id: None,
            clear_material: false,
        })),
    );

    assert!(error.is_err());
    assert_eq!(document.revision, Revision::new(0));
}

#[test]
fn component_patch_changes_only_requested_definition_fields() {
    let mut document = fixture_document();
    let original = document.components[0].clone();
    let result = apply_transaction(
        &mut document,
        request(Operation::ComponentPatch(ComponentPatch {
            component_id: ComponentId::from("component-000001"),
            name: Some("Pump v2".into()),
            set_parameter_defaults: [("capacity".into(), ParameterValue::Number(5.0))]
                .into_iter()
                .collect(),
            set_metadata: [("service".into(), "process".into())].into_iter().collect(),
            remove_metadata_keys: vec!["category".into()],
        })),
    )
    .unwrap();

    let patched = &document.components[0];
    assert_eq!(patched.name, "Pump v2");
    assert_eq!(patched.parameters[0].default, ParameterValue::Number(5.0));
    assert!(!patched.metadata.contains_key("category"));
    assert_eq!(patched.metadata["service"], "process");
    assert_eq!(
        patched.parameters[0].value_type,
        original.parameters[0].value_type
    );
    assert_eq!(patched.parameters[1], original.parameters[1]);
    assert_eq!(patched.parts, original.parts);
    assert_eq!(patched.ports, original.ports);
    assert_eq!(result.diff.updated_component_ids, vec![patched.id.clone()]);
}

#[test]
fn component_patch_rejects_empty_input() {
    let mut document = fixture_document();
    let error = apply_transaction(
        &mut document,
        request(Operation::ComponentPatch(ComponentPatch {
            component_id: ComponentId::from("component-000001"),
            name: None,
            set_parameter_defaults: BTreeMap::new(),
            set_metadata: BTreeMap::new(),
            remove_metadata_keys: Vec::new(),
        })),
    );
    assert!(error.is_err());
}

#[test]
fn connection_patch_changes_only_requested_connection_fields() {
    let mut document = connection_fixture_document();
    let original_node = document.scene.nodes()[2].clone();
    let NodeContent::Connection(original_connection) = &original_node.content else {
        panic!("fixture node must be a connection");
    };
    let original_connection = original_connection.clone();
    let result = apply_transaction(
        &mut document,
        request(Operation::ConnectionPatch(ConnectionPatch {
            node_id: NodeId::from("node-000010"),
            from: None,
            to: None,
            medium: None,
            diameter: Some(2.5),
            route_policy: None,
            set_metadata: [("insulated".into(), "true".into())].into_iter().collect(),
            remove_metadata_keys: vec!["line".into()],
        })),
    )
    .unwrap();

    let node = &document.scene.nodes()[2];
    let NodeContent::Connection(connection) = &node.content else {
        panic!("fixture node must remain a connection");
    };
    assert_eq!(connection.diameter, 2.5);
    assert_eq!(connection.from, original_connection.from);
    assert_eq!(connection.to, original_connection.to);
    assert_eq!(connection.medium, original_connection.medium);
    assert_eq!(connection.route_policy, original_connection.route_policy);
    assert_eq!(connection.relation_id, original_connection.relation_id);
    assert_eq!(node.transform, original_node.transform);
    assert!(!node.metadata.contains_key("line"));
    assert_eq!(node.metadata["insulated"], "true");
    assert_eq!(result.diff.updated_node_ids, vec![node.id.clone()]);
}

#[test]
fn connection_patch_updates_endpoint_and_route_together() {
    let mut document = connection_fixture_document();
    apply_transaction(
        &mut document,
        request(Operation::ConnectionPatch(ConnectionPatch {
            node_id: NodeId::from("node-000010"),
            from: Some(InstancePortRef::Instance {
                node_id: NodeId::from("node-000002"),
                port: "outlet".into(),
            }),
            to: Some(InstancePortRef::Instance {
                node_id: NodeId::from("node-000001"),
                port: "outlet".into(),
            }),
            medium: None,
            diameter: None,
            route_policy: Some(RoutePolicy::Direct),
            set_metadata: BTreeMap::new(),
            remove_metadata_keys: Vec::new(),
        })),
    )
    .unwrap();

    let NodeContent::Connection(connection) = &document.scene.nodes()[2].content else {
        panic!("fixture node must remain a connection");
    };
    assert_eq!(
        connection.from,
        InstancePortRef::Instance {
            node_id: NodeId::from("node-000002"),
            port: "outlet".into(),
        }
    );
    assert_eq!(
        connection.to,
        InstancePortRef::Instance {
            node_id: NodeId::from("node-000001"),
            port: "outlet".into(),
        }
    );
    assert_eq!(connection.route_policy, RoutePolicy::Direct);
}

#[test]
fn connection_patch_rejects_empty_input() {
    let mut document = connection_fixture_document();
    let error = apply_transaction(
        &mut document,
        request(Operation::ConnectionPatch(ConnectionPatch {
            node_id: NodeId::from("node-000010"),
            from: None,
            to: None,
            medium: None,
            diameter: None,
            route_policy: None,
            set_metadata: BTreeMap::new(),
            remove_metadata_keys: Vec::new(),
        })),
    );
    assert!(error.is_err());
}

fn request(operation: Operation) -> ApplyRequest {
    ApplyRequest {
        expected_revision: Revision::new(0),
        dry_run: false,
        idempotency_key: "agent-patch".into(),
        operations: vec![operation],
    }
}

fn fixture_document() -> Document {
    let component_id = ComponentId::from("component-000001");
    let mut document = Document::new("agent-patches");
    document.materials.push(studio_core::document::Material {
        id: MaterialId::from("material-000001"),
        name: "Steel".into(),
        base_color: [0.2, 0.3, 0.4, 1.0],
        opacity: 1.0,
        edge_color: Some([0.1, 0.1, 0.1, 1.0]),
    });
    document.components.push(ComponentDefinition {
        id: component_id.clone(),
        name: "Pump".into(),
        parameters: vec![
            ComponentParameter {
                name: "capacity".into(),
                value_type: ParameterType::Number {
                    minimum: Some(0.0),
                    maximum: Some(10.0),
                },
                default: ParameterValue::Number(4.0),
                unit: Some("m3/h".into()),
            },
            ComponentParameter {
                name: "speed".into(),
                value_type: ParameterType::Number {
                    minimum: Some(0.0),
                    maximum: Some(5.0),
                },
                default: ParameterValue::Number(2.0),
                unit: None,
            },
        ],
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
        ports: vec![ComponentPort {
            name: "outlet".into(),
            part_id: "body".into(),
            position: Vec3::default(),
            direction: Vec3 {
                x: 1.0,
                y: 0.0,
                z: 0.0,
            },
            medium: "water".into(),
            diameter: 1.0,
        }],
        metadata: [("category".into(), "pump".into())].into_iter().collect(),
    });
    let mut node = instance_node("node-000001", "Pump A", 3.0, component_id.clone());
    node.material_id = Some(MaterialId::from("material-000001"));
    node.metadata = [("role".into(), "pump".into()), ("area".into(), "A".into())]
        .into_iter()
        .collect();
    let NodeContent::ComponentInstance(instance) = &mut node.content else {
        unreachable!();
    };
    instance.overrides.insert(
        "body".into(),
        PartOverride {
            material_id: None,
            visible: Some(true),
            metadata: Some([("finish".into(), "paint".into())].into_iter().collect()),
            parameters: BTreeMap::new(),
        },
    );
    document.scene.nodes_mut().push(node);
    document
}

fn connection_fixture_document() -> Document {
    let mut document = fixture_document();
    let component_id = document.components[0].id.clone();
    document
        .scene
        .nodes_mut()
        .push(instance_node("node-000002", "Pump B", 7.0, component_id));
    document.relations.push(Relation {
        id: RelationId::from("relation-000001"),
        relation_type: "process".into(),
        from_node_id: NodeId::from("node-000001"),
        to_node_id: NodeId::from("node-000002"),
        metadata: BTreeMap::new(),
    });
    let mut connection_node = SceneNode::group(NodeId::from("node-000010"), "Process line");
    connection_node.metadata.insert("line".into(), "A".into());
    connection_node.content = NodeContent::Connection(ConnectionIntent {
        from: InstancePortRef::Instance {
            node_id: NodeId::from("node-000001"),
            port: "outlet".into(),
        },
        to: InstancePortRef::Instance {
            node_id: NodeId::from("node-000002"),
            port: "outlet".into(),
        },
        medium: "water".into(),
        diameter: 1.0,
        route_policy: RoutePolicy::Orthogonal,
        relation_id: RelationId::from("relation-000001"),
        metadata: BTreeMap::new(),
    });
    document.scene.nodes_mut().push(connection_node);
    document
}

fn instance_node(id: &str, name: &str, x: f32, component_id: ComponentId) -> SceneNode {
    let mut node = SceneNode::group(NodeId::from(id), name);
    node.transform.translation = Vec3 { x, y: 0.0, z: 0.0 };
    node.content = NodeContent::ComponentInstance(ComponentInstance {
        definition_id: component_id,
        parameters: [
            ("capacity".into(), ParameterValue::Number(4.0)),
            ("speed".into(), ParameterValue::Number(2.0)),
        ]
        .into_iter()
        .collect(),
        overrides: BTreeMap::new(),
        metadata: BTreeMap::new(),
    });
    node
}
