use studio_core::apply::TransactionError;
use studio_core::apply::{ApplyRequest, apply_transaction};
use studio_core::document::{Constraint, Document, NodeContent, Relation};
use studio_core::geometry::Primitive;
use studio_core::ids::{ConstraintId, RelationId, Revision};
use studio_core::operation::Operation;

#[test]
fn accepts_both_wire_spellings_and_serialises_camel_case() {
    let camel: Operation = serde_json::from_value(serde_json::json!({
        "type": "node.delete",
        "nodeId": "node-000001"
    }))
    .expect("camelCase spelling is accepted");

    let legacy: Operation = serde_json::from_value(serde_json::json!({
        "type": "node.delete",
        "node_id": "node-000001"
    }))
    .expect("legacy snake_case spelling is still accepted during migration");

    assert_eq!(camel, legacy);

    let duplicate: Operation = serde_json::from_value(serde_json::json!({
        "type": "node.update",
        "nodeId": "node-000001",
        "transform": null
    }))
    .expect("one object may mix spellings while clients migrate");
    assert_eq!(
        duplicate,
        serde_json::from_value(serde_json::json!({
            "type": "node.update",
            "nodeId": "node-000001"
        }))
        .unwrap()
    );

    let encoded = serde_json::to_value(&legacy).unwrap();
    assert!(encoded.get("nodeId").is_some(), "{encoded}");
    assert!(encoded.get("node_id").is_none(), "{encoded}");
}

#[test]
fn rejects_unknown_operation_fields() {
    let result = serde_json::from_value::<studio_core::apply::ApplyRequest>(serde_json::json!({
        "expectedRevision": 0,
        "idempotencyKey": "strict-operation",
        "operations": [{
            "type": "node.delete",
            "node_id": "node-000001",
            "unexpected": true
        }]
    }));
    assert!(result.is_err());
}

#[test]
fn rejects_unknown_apply_request_fields() {
    let result = serde_json::from_value::<studio_core::apply::ApplyRequest>(serde_json::json!({
        "expectedRevision": 0,
        "idempotencyKey": "strict-request",
        "operations": [],
        "unexpected": true
    }));
    assert!(result.is_err());
}

#[test]
fn applies_a_batch_and_increments_revision_once() {
    let mut document = Document::new("demo");
    let result = apply_transaction(
        &mut document,
        ApplyRequest {
            expected_revision: Revision::new(0),
            dry_run: false,
            idempotency_key: "batch-1".into(),
            operations: vec![
                Operation::MaterialUpsert {
                    material_id: "mat-000001".into(),
                    name: "Concrete".into(),
                    base_color: [0.4, 0.39, 0.36, 1.0],
                    opacity: 1.0,
                    edge_color: None,
                    shading: None,
                    surface: None,
                },
                Operation::NodeCreate {
                    node_id: "node-000001".into(),
                    parent_id: None,
                    name: "Ground".into(),
                    transform: Default::default(),
                    content: Default::default(),
                    material_id: Some("mat-000001".into()),
                    metadata: Default::default(),
                },
            ],
        },
    )
    .unwrap();

    assert_eq!(result.previous_revision, Revision::new(0));
    assert_eq!(result.revision, Revision::new(1));
    assert_eq!(document.scene.nodes().len(), 1);
}

#[test]
fn dry_run_reports_changes_without_mutating_the_document() {
    let mut document = Document::new("demo");
    let before = document.clone();
    let result = apply_transaction(
        &mut document,
        ApplyRequest {
            expected_revision: Revision::new(0),
            dry_run: true,
            idempotency_key: "preview-1".into(),
            operations: vec![Operation::NodeCreate {
                node_id: "node-000001".into(),
                parent_id: None,
                name: "Ground".into(),
                transform: Default::default(),
                content: Default::default(),
                material_id: None,
                metadata: Default::default(),
            }],
        },
    )
    .unwrap();
    assert_eq!(result.revision, Revision::new(0));
    assert_eq!(document, before);
}

#[test]
fn rejects_stale_revision_without_partial_changes() {
    let mut document = Document::new("demo");
    let before = document.clone();
    assert!(
        apply_transaction(
            &mut document,
            ApplyRequest {
                expected_revision: Revision::new(2),
                dry_run: false,
                idempotency_key: "conflict".into(),
                operations: vec![],
            },
        )
        .is_err()
    );
    assert_eq!(document, before);
}

#[test]
fn rejects_a_singular_rotation() {
    let mut document = Document::new("demo");
    let mut transform = studio_core::math::Transform::IDENTITY;
    transform.rotation = [0.0, 0.0, 0.0, 0.0];
    let result = apply_transaction(
        &mut document,
        ApplyRequest {
            expected_revision: Revision::new(0),
            dry_run: false,
            idempotency_key: "singular-rotation".into(),
            operations: vec![Operation::NodeCreate {
                node_id: "node-000001".into(),
                parent_id: None,
                name: "Singular".into(),
                transform,
                content: Default::default(),
                material_id: None,
                metadata: Default::default(),
            }],
        },
    );
    assert!(result.is_err());
    assert_eq!(document.revision, Revision::new(0));
}

#[test]
fn returns_the_original_result_for_a_repeated_idempotency_key() {
    let mut document = Document::new("demo");
    let request = ApplyRequest {
        expected_revision: Revision::new(0),
        dry_run: false,
        idempotency_key: "same-batch".into(),
        operations: vec![Operation::NodeCreate {
            node_id: "node-000001".into(),
            parent_id: None,
            name: "Ground".into(),
            transform: Default::default(),
            content: Default::default(),
            material_id: None,
            metadata: Default::default(),
        }],
    };
    let first = apply_transaction(&mut document, request.clone()).unwrap();
    let second = apply_transaction(&mut document, request).unwrap();
    assert_eq!(second.revision, first.revision);
    assert_eq!(second.changed_node_ids, first.changed_node_ids);
    assert_eq!(document.revision, Revision::new(1));
}

fn box_operation(node_id: &str, x: f32) -> Operation {
    let mut transform = studio_core::math::Transform::IDENTITY;
    transform.translation.x = x;
    Operation::NodeCreate {
        node_id: node_id.into(),
        parent_id: None,
        name: node_id.into(),
        transform,
        content: NodeContent::Primitive(Primitive::Box {
            width: 2.0,
            height: 2.0,
            depth: 2.0,
        }),
        material_id: None,
        metadata: Default::default(),
    }
}

#[test]
fn rejects_a_broken_reference_without_committing() {
    let mut document = Document::new("demo");
    let before = document.clone();
    let error = apply_transaction(
        &mut document,
        ApplyRequest {
            expected_revision: Revision::new(0),
            dry_run: false,
            idempotency_key: "broken-reference".into(),
            operations: vec![Operation::RelationCreate {
                relation: Relation {
                    id: RelationId::from("rel-000001"),
                    relation_type: "process".into(),
                    from_node_id: "node-000001".into(),
                    to_node_id: "node-999999".into(),
                    metadata: Default::default(),
                },
            }],
        },
    )
    .unwrap_err();

    assert!(matches!(error, TransactionError::ValidationFailed(_)));
    assert_eq!(document, before);
}

#[test]
fn rejects_a_no_overlap_constraint_violation() {
    let mut document = Document::new("demo");
    let error = apply_transaction(
        &mut document,
        ApplyRequest {
            expected_revision: Revision::new(0),
            dry_run: false,
            idempotency_key: "overlap".into(),
            operations: vec![
                box_operation("node-000001", 0.0),
                box_operation("node-000002", 1.0),
                Operation::ConstraintUpsert {
                    constraint: Constraint {
                        id: ConstraintId::from("constraint-000001"),
                        constraint_type: "no_overlap".into(),
                        node_ids: vec!["node-000001".into(), "node-000002".into()],
                        metadata: Default::default(),
                    },
                },
            ],
        },
    )
    .unwrap_err();

    assert!(matches!(error, TransactionError::ValidationFailed(_)));
    assert_eq!(document.revision, Revision::new(0));
}

#[test]
fn rejects_duplicate_ids_and_hierarchy_cycles() {
    let mut duplicate_document = Document::new("duplicate");
    let duplicate_error = apply_transaction(
        &mut duplicate_document,
        ApplyRequest {
            expected_revision: Revision::new(0),
            dry_run: false,
            idempotency_key: "duplicate".into(),
            operations: vec![
                box_operation("node-000001", 0.0),
                box_operation("node-000001", 3.0),
            ],
        },
    )
    .unwrap_err();
    assert!(matches!(
        duplicate_error,
        TransactionError::ValidationFailed(_)
    ));

    let mut cycle_document = Document::new("cycle");
    let cycle_error = apply_transaction(
        &mut cycle_document,
        ApplyRequest {
            expected_revision: Revision::new(0),
            dry_run: false,
            idempotency_key: "cycle".into(),
            operations: vec![
                Operation::NodeCreate {
                    node_id: "node-000001".into(),
                    parent_id: Some("node-000002".into()),
                    name: "A".into(),
                    transform: Default::default(),
                    content: NodeContent::Group,
                    material_id: None,
                    metadata: Default::default(),
                },
                Operation::NodeCreate {
                    node_id: "node-000002".into(),
                    parent_id: Some("node-000001".into()),
                    name: "B".into(),
                    transform: Default::default(),
                    content: NodeContent::Group,
                    material_id: None,
                    metadata: Default::default(),
                },
            ],
        },
    )
    .unwrap_err();
    assert!(matches!(cycle_error, TransactionError::ValidationFailed(_)));
}
