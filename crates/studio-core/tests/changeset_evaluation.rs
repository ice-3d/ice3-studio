use studio_core::changeset::{candidate_digest, evaluate_operations};
use studio_core::document::Document;
use studio_core::ids::Revision;
use studio_core::operation::Operation;

#[test]
fn evaluates_a_clone_without_mutating_the_base() {
    let base = Document::new("candidate-test");
    let operations = vec![Operation::NodeCreate {
        node_id: "node-000001".into(),
        parent_id: None,
        name: "Assembly".into(),
        transform: Default::default(),
        content: Default::default(),
        material_id: None,
        metadata: Default::default(),
    }];

    let candidate = evaluate_operations(&base, &operations).unwrap();

    assert!(base.scene.nodes().is_empty());
    assert_eq!(candidate.document.revision, Revision::new(0));
    assert_eq!(candidate.document.scene.nodes().len(), 1);
    assert!(candidate.diagnostics.is_empty());
}

#[test]
fn candidate_digest_changes_with_operation_content() {
    let base = Document::new("digest-test");
    let first = vec![Operation::NodeCreate {
        node_id: "node-000001".into(),
        parent_id: None,
        name: "A".into(),
        transform: Default::default(),
        content: Default::default(),
        material_id: None,
        metadata: Default::default(),
    }];
    let mut second = first.clone();
    if let Operation::NodeCreate { name, .. } = &mut second[0] {
        *name = "B".into();
    }

    assert_eq!(
        candidate_digest(&base, &first).unwrap(),
        candidate_digest(&base, &first).unwrap()
    );
    assert_ne!(
        candidate_digest(&base, &first).unwrap(),
        candidate_digest(&base, &second).unwrap()
    );
}
