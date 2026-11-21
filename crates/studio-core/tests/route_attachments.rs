use std::collections::BTreeMap;

use studio_core::document::{Document, NodeContent, Relation, SceneNode};
use studio_core::geometry::Primitive;
use studio_core::ids::{ComponentId, NodeId, RelationId};
use studio_core::large_scene::component::{
    ComponentDefinition, ComponentInstance, ComponentParameter, ComponentPart, ComponentPort,
    ParameterType, ParameterValue, PartContent, PartTransform, ScalarBinding, Vector3Binding,
};
use studio_core::large_scene::connection::{ConnectionIntent, InstancePortRef, RoutePolicy};
use studio_core::math::{Transform, Vec3};
use studio_core::validation::{DiagnosticCode, validate_document};

fn endpoint_definition() -> ComponentDefinition {
    endpoint_definition_with_diameter("component-000001", 0.2)
}

fn endpoint_definition_with_diameter(id: &str, diameter: f32) -> ComponentDefinition {
    definition(
        id,
        "Endpoint",
        vec![ComponentPort {
            name: "port".into(),
            part_id: "body".into(),
            position: Vector3Binding::Literal(Vec3 {
                x: 1.0,
                y: 0.0,
                z: 0.0,
            }),
            direction: Vector3Binding::Literal(Vec3 {
                x: 1.0,
                y: 0.0,
                z: 0.0,
            }),
            medium: "water".into(),
            diameter: ScalarBinding::Literal(diameter),
        }],
    )
}

fn tee_definition() -> ComponentDefinition {
    let mut definition = definition(
        "component-000002",
        "Tee",
        vec![
            ComponentPort {
                name: "run-in".into(),
                part_id: "run".into(),
                position: Vector3Binding::Literal(Vec3 {
                    x: -1.0,
                    y: 0.0,
                    z: 0.0,
                }),
                direction: Vector3Binding::Literal(Vec3 {
                    x: -1.0,
                    y: 0.0,
                    z: 0.0,
                }),
                medium: "water".into(),
                diameter: ScalarBinding::Literal(0.2),
            },
            ComponentPort {
                name: "run-out".into(),
                part_id: "run".into(),
                position: Vector3Binding::Literal(Vec3 {
                    x: 1.0,
                    y: 0.0,
                    z: 0.0,
                }),
                direction: Vector3Binding::Literal(Vec3 {
                    x: 1.0,
                    y: 0.0,
                    z: 0.0,
                }),
                medium: "water".into(),
                diameter: ScalarBinding::Literal(0.2),
            },
            ComponentPort {
                name: "branch".into(),
                part_id: "branch".into(),
                position: Vector3Binding::Literal(Vec3 {
                    x: 0.0,
                    y: 0.0,
                    z: 1.0,
                }),
                direction: Vector3Binding::Literal(Vec3 {
                    x: 0.0,
                    y: 0.0,
                    z: 1.0,
                }),
                medium: "water".into(),
                diameter: ScalarBinding::Literal(0.2),
            },
        ],
    );
    definition.parameters.push(ComponentParameter {
        name: "unused".into(),
        value_type: ParameterType::Number {
            minimum: Some(0.1),
            maximum: Some(1.0),
        },
        default: ParameterValue::Number(0.2),
        unit: None,
    });
    definition.parts = vec![
        part("run", 1.0, 0.0, 0.0, 0.0),
        part("branch", 0.2, 0.2, 1.1, 0.2),
        part("collar", 0.3, 0.3, 0.0, 0.9),
    ];
    definition
}

fn part(id: &str, width: f32, height: f32, _y: f32, depth: f32) -> ComponentPart {
    ComponentPart {
        id: id.into(),
        parent_id: None,
        transform: PartTransform::default(),
        content: PartContent::Box {
            width: ScalarBinding::Literal(width),
            height: ScalarBinding::Literal(height),
            depth: ScalarBinding::Literal(depth),
        },
        subtract: Vec::new(),
        repeat: None,
        material_role: None,
        metadata: BTreeMap::new(),
        visible: true,
    }
}

fn definition(id: &str, name: &str, ports: Vec<ComponentPort>) -> ComponentDefinition {
    ComponentDefinition {
        id: ComponentId::from(id),
        name: name.into(),
        parameters: Vec::new(),
        parts: vec![part("body", 1.0, 1.0, 0.0, 1.0)],
        ports,
        metadata: BTreeMap::new(),
    }
}

fn instance(
    id: &str,
    name: &str,
    translation: Vec3,
    definition_id: &str,
    metadata: BTreeMap<String, String>,
) -> SceneNode {
    SceneNode {
        id: NodeId::from(id),
        name: name.into(),
        parent_id: None,
        transform: Transform {
            translation,
            ..Transform::IDENTITY
        },
        material_id: None,
        content: NodeContent::ComponentInstance(ComponentInstance {
            definition_id: ComponentId::from(definition_id),
            parameters: BTreeMap::new(),
            overrides: BTreeMap::new(),
            metadata: metadata.clone(),
        }),
        metadata,
        visible: true,
    }
}

fn connection(
    id: &str,
    from_node: &str,
    from_port: &str,
    to_node: &str,
    to_port: &str,
    relation_id: &str,
) -> SceneNode {
    connection_with(
        id,
        from_node,
        from_port,
        to_node,
        to_port,
        relation_id,
        0.2,
        "L-TEE",
    )
}

#[allow(clippy::too_many_arguments)]
fn connection_with(
    id: &str,
    from_node: &str,
    from_port: &str,
    to_node: &str,
    to_port: &str,
    relation_id: &str,
    diameter: f32,
    line_id: &str,
) -> SceneNode {
    SceneNode {
        id: NodeId::from(id),
        name: format!("Connection {id}"),
        parent_id: None,
        transform: Transform::IDENTITY,
        material_id: None,
        content: NodeContent::Connection(ConnectionIntent {
            from: InstancePortRef::Instance {
                node_id: NodeId::from(from_node),
                port: from_port.into(),
            },
            to: InstancePortRef::Instance {
                node_id: NodeId::from(to_node),
                port: to_port.into(),
            },
            medium: "water".into(),
            diameter,
            route_policy: RoutePolicy::Direct,
            relation_id: RelationId::from(relation_id),
            metadata: BTreeMap::from([("lineId".into(), line_id.into())]),
        }),
        metadata: BTreeMap::new(),
        visible: true,
    }
}

fn reducer_definition(large: f32, small: f32) -> ComponentDefinition {
    let mut definition = definition(
        "component-000004",
        "Reducer",
        vec![
            ComponentPort {
                name: "large".into(),
                part_id: "large-flange".into(),
                position: Vector3Binding::Literal(Vec3 {
                    x: -1.0,
                    y: 0.0,
                    z: 0.0,
                }),
                direction: Vector3Binding::Literal(Vec3 {
                    x: -1.0,
                    y: 0.0,
                    z: 0.0,
                }),
                medium: "water".into(),
                diameter: ScalarBinding::Literal(large),
            },
            ComponentPort {
                name: "small".into(),
                part_id: "small-flange".into(),
                position: Vector3Binding::Literal(Vec3 {
                    x: 1.0,
                    y: 0.0,
                    z: 0.0,
                }),
                direction: Vector3Binding::Literal(Vec3 {
                    x: 1.0,
                    y: 0.0,
                    z: 0.0,
                }),
                medium: "water".into(),
                diameter: ScalarBinding::Literal(small),
            },
        ],
    );
    definition.parts = vec![
        part("large-flange", 0.2, 0.2, 0.0, 0.2),
        part("small-flange", 0.2, 0.2, 0.0, 0.2),
        part("shell", 1.8, 0.1, 0.0, 0.1),
    ];
    definition
}

fn reducer_document_with_diameters(large: f32, small: f32) -> Document {
    let mut document = Document::new("reducer");
    document.components = vec![
        endpoint_definition_with_diameter("component-000003", large),
        reducer_definition(large, small),
        endpoint_definition_with_diameter("component-000005", small),
    ];
    let reducer_metadata = BTreeMap::from([
        ("role".into(), "pipe-fitting".into()),
        ("fittingKind".into(), "reducer".into()),
        ("lineId".into(), "L-REDUCER".into()),
    ]);
    document.scene.nodes_mut().extend([
        instance(
            "node-000001",
            "Large side",
            Vec3 {
                x: -5.0,
                y: 0.0,
                z: 0.0,
            },
            "component-000003",
            BTreeMap::new(),
        ),
        instance(
            "node-000002",
            "Small side",
            Vec3 {
                x: 5.0,
                y: 0.0,
                z: 0.0,
            },
            "component-000005",
            BTreeMap::new(),
        ),
        instance(
            "node-000005",
            "Reducer",
            Vec3::default(),
            "component-000004",
            reducer_metadata,
        ),
        connection_with(
            "node-000006",
            "node-000001",
            "port",
            "node-000005",
            "large",
            "relation-000001",
            large,
            "L-REDUCER",
        ),
        connection_with(
            "node-000007",
            "node-000005",
            "small",
            "node-000002",
            "port",
            "relation-000002",
            small,
            "L-REDUCER",
        ),
    ]);
    document.relations.extend([
        relation("relation-000001", "node-000001", "node-000005"),
        relation("relation-000002", "node-000005", "node-000002"),
    ]);
    document
}

fn reducer_document() -> Document {
    reducer_document_with_diameters(0.4, 0.2)
}

fn sleeve_definition() -> ComponentDefinition {
    let mut definition = definition(
        "component-000006",
        "Wall sleeve",
        vec![
            ComponentPort {
                name: "wall-in".into(),
                part_id: "wall-in".into(),
                position: Vector3Binding::Literal(Vec3 {
                    x: -0.35,
                    y: 0.0,
                    z: 0.0,
                }),
                direction: Vector3Binding::Literal(Vec3 {
                    x: -1.0,
                    y: 0.0,
                    z: 0.0,
                }),
                medium: "water".into(),
                diameter: ScalarBinding::Literal(0.4),
            },
            ComponentPort {
                name: "wall-out".into(),
                part_id: "wall-out".into(),
                position: Vector3Binding::Literal(Vec3 {
                    x: 0.35,
                    y: 0.0,
                    z: 0.0,
                }),
                direction: Vector3Binding::Literal(Vec3 {
                    x: 1.0,
                    y: 0.0,
                    z: 0.0,
                }),
                medium: "water".into(),
                diameter: ScalarBinding::Literal(0.4),
            },
        ],
    );
    definition.parts = vec![
        part("wall-in", 0.3, 0.4, 0.0, 0.4),
        part("wall-out", 0.3, 0.4, 0.0, 0.4),
    ];
    definition
}

fn sleeve_document(valid_crossing: bool) -> Document {
    let z = if valid_crossing { 0.0 } else { 1.05 };
    let mut document = Document::new("sleeve");
    document.components = vec![
        endpoint_definition_with_diameter("component-000001", 0.2),
        sleeve_definition(),
    ];
    document.scene.nodes_mut().extend([
        instance(
            "node-000001",
            "Left endpoint",
            Vec3 { x: -5.0, y: 0.0, z },
            "component-000001",
            BTreeMap::new(),
        ),
        instance(
            "node-000002",
            "Right endpoint",
            Vec3 { x: 5.0, y: 0.0, z },
            "component-000001",
            BTreeMap::new(),
        ),
        SceneNode {
            id: NodeId::from("node-000005"),
            name: "Wall".into(),
            parent_id: None,
            transform: Transform::IDENTITY,
            material_id: None,
            content: NodeContent::Primitive(Primitive::Box {
                width: 0.5,
                height: 2.0,
                depth: 2.0,
            }),
            metadata: BTreeMap::from([("role".into(), "structure".into())]),
            visible: true,
        },
        instance(
            "node-000006",
            "Sleeve",
            Vec3 { x: 0.0, y: 0.0, z },
            "component-000006",
            BTreeMap::from([
                ("role".into(), "pipe-penetration".into()),
                ("fittingKind".into(), "sleeve".into()),
                ("lineId".into(), "L-SLV".into()),
                ("hostNodeId".into(), "node-000005".into()),
            ]),
        ),
        connection_with(
            "node-000007",
            "node-000001",
            "port",
            "node-000002",
            "port",
            "relation-000001",
            0.2,
            "L-SLV",
        ),
    ]);
    document
        .relations
        .push(relation("relation-000001", "node-000001", "node-000002"));
    document
}

fn clone_sleeve(document: &mut Document, source_id: &str) {
    let source = document
        .scene
        .nodes()
        .iter()
        .find(|node| node.id.as_str() == source_id)
        .expect("sleeve source")
        .clone();
    let mut duplicate = source;
    duplicate.id = NodeId::from("node-000009");
    document.scene.nodes_mut().push(duplicate);
}

fn relation(id: &str, from: &str, to: &str) -> Relation {
    Relation {
        id: RelationId::from(id),
        relation_type: "process".into(),
        from_node_id: NodeId::from(from),
        to_node_id: NodeId::from(to),
        metadata: BTreeMap::new(),
    }
}

fn tee_document() -> Document {
    let mut document = Document::new("tee");
    document.components = vec![endpoint_definition(), tee_definition()];
    let tee_metadata = BTreeMap::from([
        ("role".into(), "pipe-fitting".into()),
        ("fittingKind".into(), "tee".into()),
        ("lineId".into(), "L-TEE".into()),
    ]);
    document.scene.nodes_mut().extend([
        instance(
            "node-000001",
            "Upstream",
            Vec3 {
                x: -5.0,
                y: 0.0,
                z: 0.0,
            },
            "component-000001",
            BTreeMap::new(),
        ),
        instance(
            "node-000002",
            "Downstream",
            Vec3 {
                x: 5.0,
                y: 0.0,
                z: 0.0,
            },
            "component-000001",
            BTreeMap::new(),
        ),
        instance(
            "node-000003",
            "Branch target",
            Vec3 {
                x: 0.0,
                y: 0.0,
                z: 5.0,
            },
            "component-000001",
            BTreeMap::new(),
        ),
        instance(
            "node-000004",
            "Unused endpoint",
            Vec3 {
                x: 0.0,
                y: 0.0,
                z: -5.0,
            },
            "component-000001",
            BTreeMap::new(),
        ),
        instance(
            "node-000005",
            "Tee",
            Vec3::default(),
            "component-000002",
            tee_metadata,
        ),
        connection(
            "node-000006",
            "node-000001",
            "port",
            "node-000005",
            "run-in",
            "relation-000001",
        ),
        connection(
            "node-000007",
            "node-000005",
            "run-out",
            "node-000002",
            "port",
            "relation-000002",
        ),
        connection(
            "node-000008",
            "node-000005",
            "branch",
            "node-000003",
            "port",
            "relation-000003",
        ),
    ]);
    document.relations.extend([
        relation("relation-000001", "node-000001", "node-000005"),
        relation("relation-000002", "node-000005", "node-000002"),
        relation("relation-000003", "node-000005", "node-000003"),
    ]);
    document
}

fn connection_for_port<'a>(
    document: &'a mut Document,
    port_name: &str,
) -> &'a mut ConnectionIntent {
    document
        .scene
        .nodes_mut()
        .iter_mut()
        .find_map(|node| {
            let NodeContent::Connection(connection) = &mut node.content else {
                return None;
            };
            let references_tee_port =
                [&connection.from, &connection.to]
                    .iter()
                    .any(|endpoint| match endpoint {
                        InstancePortRef::Instance { node_id, port } => {
                            node_id.as_str() == "node-000005" && port == port_name
                        }
                        InstancePortRef::SetItem { .. } => false,
                    });
            references_tee_port.then_some(connection)
        })
        .expect("tee port connection")
}

#[test]
fn accepts_a_tee_with_three_distinct_connected_ports() {
    let diagnostics = validate_document(&tee_document());
    assert!(
        !diagnostics.iter().any(|diagnostic| matches!(
            diagnostic.code,
            DiagnosticCode::RouteAttachmentInvalid | DiagnosticCode::RouteAttachmentMisaligned
        )),
        "{diagnostics:?}",
    );
}

#[test]
fn rejects_a_reused_or_missing_tee_port() {
    let mut document = tee_document();
    let branch = connection_for_port(&mut document, "branch");
    branch.from = branch.to.clone();
    let diagnostics = validate_document(&document);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::RouteAttachmentInvalid
            && diagnostic.object_id.as_deref() == Some("node-000005")
    }));
}

#[test]
fn rejects_a_tee_branch_that_is_collinear_with_the_run() {
    let mut document = tee_document();
    let tee = document
        .components
        .iter_mut()
        .find(|definition| definition.id == ComponentId::from("component-000002"))
        .unwrap();
    let branch = tee
        .ports
        .iter_mut()
        .find(|port| port.name == "branch")
        .unwrap();
    branch.direction = Vector3Binding::Literal(Vec3 {
        x: 1.0,
        y: 0.0,
        z: 0.0,
    });
    let diagnostics = validate_document(&document);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code == DiagnosticCode::RouteAttachmentMisaligned })
    );
}

#[test]
fn accepts_a_reducer_with_two_different_diameters() {
    let diagnostics = validate_document(&reducer_document());
    assert!(
        !diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code == DiagnosticCode::RouteAttachmentInvalid })
    );
}

#[test]
fn rejects_equal_or_reversed_reducer_diameters() {
    for (large, small) in [(0.4, 0.4), (0.2, 0.4)] {
        let document = reducer_document_with_diameters(large, small);
        let diagnostics = validate_document(&document);
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.code == DiagnosticCode::RouteAttachmentInvalid })
        );
    }
}

#[test]
fn accepts_a_sleeve_that_crosses_a_real_wall() {
    let diagnostics = validate_document(&sleeve_document(true));
    assert!(!diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::RoutePenetrationInvalid
            || diagnostic.code == DiagnosticCode::RouteAttachmentMisaligned
    }));
}

#[test]
fn rejects_a_sleeve_that_only_touches_the_host_bounds() {
    let diagnostics = validate_document(&sleeve_document(false));
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code == DiagnosticCode::RoutePenetrationInvalid })
    );
}

#[test]
fn rejects_duplicate_sleeves_for_the_same_line_and_host() {
    let mut document = sleeve_document(true);
    clone_sleeve(&mut document, "node-000006");
    let diagnostics = validate_document(&document);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code == DiagnosticCode::RoutePenetrationInvalid })
    );
}
