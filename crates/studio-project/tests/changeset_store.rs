use studio_core::document::Document;
use studio_core::ids::Revision;
use studio_project::changeset::{ChangeSet, ChangeSetId, ChangeSetStatus};
use studio_project::{ProjectStore, WorkspacePath};

#[test]
fn persists_and_lists_change_sets_without_creating_a_revision() {
    let root = tempfile::tempdir().unwrap();
    let path = WorkspacePath::new(root.path(), "generic-project").unwrap();
    let store = ProjectStore::create(path).unwrap();
    store
        .write_snapshot(&Document::new("generic-project"))
        .unwrap();
    let change_sets = store.change_set_store();
    let change_set = ChangeSet::new(
        ChangeSetId::parse("changeset-0123456789abcdef").unwrap(),
        "generic-project",
        Revision::new(0),
        "Adjust assembly",
        1_793_359_200_000,
    );
    change_sets.save(&change_set).unwrap();

    assert_eq!(change_sets.list().unwrap(), vec![change_set.clone()]);
    assert_eq!(store.load_current().unwrap().revision, Revision::new(0));
    assert_eq!(change_set.status, ChangeSetStatus::Draft);
}

#[test]
fn rejects_change_set_ids_with_path_characters() {
    assert!(ChangeSetId::parse("../../escape").is_err());
}

#[test]
fn isolates_a_corrupt_change_set_file() {
    let root = tempfile::tempdir().unwrap();
    let path = WorkspacePath::new(root.path(), "corrupt-draft").unwrap();
    let store = ProjectStore::create(path).unwrap();
    store
        .write_snapshot(&Document::new("corrupt-draft"))
        .unwrap();
    let change_sets = store.change_set_store();
    std::fs::create_dir_all(change_sets.directory()).unwrap();
    std::fs::write(change_sets.directory().join("changeset-bad.json"), "{").unwrap();

    assert!(change_sets.list().unwrap().is_empty());
    assert_eq!(store.load_current().unwrap().project_id, "corrupt-draft");
    assert_eq!(change_sets.diagnostics().len(), 1);
}
