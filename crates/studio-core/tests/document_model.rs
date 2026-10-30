use studio_core::document::{Document, NodeContent, SceneNode};
use studio_core::ids::{NodeId, Revision};
use studio_core::math::Transform;

#[test]
fn creates_deterministic_ids_and_default_document() {
    assert_eq!(NodeId::new("node", 7).unwrap().as_str(), "node-000007");
    assert_eq!(Revision::new(0).as_u64(), 0);

    let document = Document::new("wwtp-mini");
    assert_eq!(document.revision, Revision::new(0));
    assert_eq!(document.project_id, "wwtp-mini");
    assert_eq!(document.schema_version, 1);
    assert!(document.scene.nodes().is_empty());
}

#[test]
fn rejects_invalid_ids() {
    assert!(NodeId::new("", 1).is_err());
    assert!(NodeId::new("node space", 1).is_err());
    assert!(!NodeId::from("invalid").is_valid());
    assert!(NodeId::from("node-000002").is_valid());
}

#[test]
fn stores_identity_independently_from_display_name() {
    let id = NodeId::new("node", 1).unwrap();
    let node = SceneNode::group(id.clone(), "Ground");
    assert_eq!(node.id, id);
    assert_eq!(node.name, "Ground");
    assert_eq!(node.transform, Transform::IDENTITY);
    assert!(matches!(node.content, NodeContent::Group));
}
