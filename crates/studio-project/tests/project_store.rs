use studio_core::document::Document;
use studio_project::{ProjectStore, WorkspacePath};

#[test]
fn creates_and_reopens_deterministic_project_state() {
    let root = tempfile::tempdir().unwrap();
    let path = WorkspacePath::new(root.path(), "wwtp-mini").unwrap();
    let store = ProjectStore::create(path.clone()).unwrap();
    store.write_snapshot(&Document::new("wwtp-mini")).unwrap();
    drop(store);

    let reopened = ProjectStore::open(path).unwrap();
    let document = reopened.load_current().unwrap();
    assert_eq!(document.project_id, "wwtp-mini");
    assert_eq!(document.schema_version, 1);
}

#[test]
fn rejects_project_paths_outside_the_workspace() {
    let root = tempfile::tempdir().unwrap();
    assert!(WorkspacePath::new(root.path(), "../escape").is_err());
}

#[test]
fn failed_second_write_keeps_the_previous_revision() {
    let root = tempfile::tempdir().unwrap();
    let path = WorkspacePath::new(root.path(), "safe").unwrap();
    let store = ProjectStore::create(path.clone()).unwrap();
    store.write_snapshot(&Document::new("safe")).unwrap();
    let before = store.load_current().unwrap();
    std::fs::create_dir(root.path().join("safe/revisions/00000001.json")).unwrap();

    let mut next = before.clone();
    next.revision = next.revision.next();
    assert!(store.write_snapshot(&next).is_err());
    assert_eq!(store.load_current().unwrap(), before);
}

#[test]
fn undo_and_redo_restore_document_content() {
    let root = tempfile::tempdir().unwrap();
    let path = WorkspacePath::new(root.path(), "history").unwrap();
    let store = ProjectStore::create(path).unwrap();
    let mut first = Document::new("history");
    store.write_snapshot(&first).unwrap();
    first.revision = first.revision.next();
    first.metadata.insert("phase".into(), "built".into());
    store.write_snapshot(&first).unwrap();

    let undone = store.undo().unwrap();
    assert_eq!(undone.metadata.get("phase"), None);
    let redone = store.redo().unwrap();
    assert_eq!(
        redone.metadata.get("phase").map(String::as_str),
        Some("built")
    );
}
