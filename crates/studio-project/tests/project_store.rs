use sha2::Digest;
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
    assert_eq!(document.schema_version, 2);
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

#[test]
fn open_rejects_a_history_with_a_missing_revision() {
    let root = tempfile::tempdir().unwrap();
    let path = WorkspacePath::new(root.path(), "history-gap").unwrap();
    let store = ProjectStore::create(path.clone()).unwrap();
    let mut first = Document::new("history-gap");
    store.write_snapshot(&first).unwrap();
    first.revision = first.revision.next();
    store.write_snapshot(&first).unwrap();
    drop(store);

    let history_path = root.path().join("history-gap/history/operations.jsonl");
    let history = std::fs::read_to_string(&history_path).unwrap();
    std::fs::write(
        &history_path,
        history.lines().skip(1).collect::<Vec<_>>().join("\n"),
    )
    .unwrap();

    assert!(ProjectStore::open(path).is_err());
}

#[test]
fn recovers_a_lock_left_by_a_dead_process() {
    let root = tempfile::tempdir().unwrap();
    let path = WorkspacePath::new(root.path(), "stale-lock").unwrap();
    let store = ProjectStore::create(path.clone()).unwrap();
    store.write_snapshot(&Document::new("stale-lock")).unwrap();
    drop(store);
    std::fs::write(root.path().join("stale-lock/.lock"), "pid=999999999\n").unwrap();

    let reopened = ProjectStore::open(path);
    assert!(reopened.is_ok());
}

#[cfg(unix)]
#[test]
fn rejects_a_project_symlink_that_escapes_the_workspace() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let escaped = outside.path().join("project");
    std::fs::create_dir_all(&escaped).unwrap();
    std::fs::write(escaped.join("project.json"), "{}").unwrap();
    symlink(&escaped, root.path().join("linked-project")).unwrap();

    if let Ok(path) = WorkspacePath::new(root.path(), "linked-project") {
        assert!(ProjectStore::open(path).is_err());
    }
}

#[test]
fn opens_a_version_one_project_and_writes_the_next_revision_as_version_two() {
    let root = tempfile::tempdir().unwrap();
    let path = WorkspacePath::new(root.path(), "legacy-schema").unwrap();
    let store = ProjectStore::create(path.clone()).unwrap();
    let mut legacy = Document::new("legacy-schema");
    legacy.schema_version = 1;
    let bytes = serde_json::to_vec(&legacy).unwrap();
    let digest = format!("{:x}", sha2::Sha256::digest(&bytes));
    std::fs::create_dir_all(root.path().join("legacy-schema/revisions")).unwrap();
    std::fs::create_dir_all(root.path().join("legacy-schema/history")).unwrap();
    std::fs::write(
        root.path().join("legacy-schema/revisions/00000000.json"),
        &bytes,
    )
    .unwrap();
    std::fs::write(
        root.path().join("legacy-schema/history/operations.jsonl"),
        format!("{{\"revision\":0,\"digest\":\"{digest}\",\"event\":{{\"kind\":\"snapshot\"}}}}\n"),
    )
    .unwrap();
    std::fs::write(
        root.path().join("legacy-schema/project.json"),
        format!(
            "{{\"schemaVersion\":1,\"projectId\":\"legacy-schema\",\"currentRevision\":0,\"snapshotDigest\":\"{digest}\",\"undoStack\":[],\"redoStack\":[]}}"
        ),
    )
    .unwrap();
    drop(store);

    let reopened = ProjectStore::open(path).unwrap();
    let mut loaded = reopened.load_current().unwrap();
    assert_eq!(loaded.schema_version, 2);
    loaded.revision = loaded.revision.next();
    reopened.write_snapshot(&loaded).unwrap();
    assert_eq!(reopened.load_current().unwrap().schema_version, 2);
}

#[test]
fn bounds_revision_snapshots_and_keeps_recent_undo_available() {
    let root = tempfile::tempdir().unwrap();
    let path = WorkspacePath::new(root.path(), "bounded").unwrap();
    let store = ProjectStore::create(path).unwrap();

    let document = Document::new("bounded");
    store.write_snapshot(&document).unwrap();
    let mut current = document;
    let extra = 12usize;
    for _ in 0..(studio_project::MAX_UNDO_DEPTH + extra) {
        current.revision = current.revision.next();
        store.write_snapshot(&current).unwrap();
    }

    // revisions/ 的文件数必须与撤销深度相关，而不是与编辑次数线性增长。
    let snapshots = std::fs::read_dir(root.path().join("bounded/revisions"))
        .unwrap()
        .count();
    assert!(
        snapshots <= studio_project::MAX_UNDO_DEPTH + 2,
        "snapshot count must stay bounded, found {snapshots}"
    );

    let loaded = store.load_current().unwrap();
    assert_eq!(
        loaded.revision.as_u64(),
        (studio_project::MAX_UNDO_DEPTH + extra) as u64
    );

    // 最近一步撤销仍然可用，并且指向被回收边界之内的 revision。
    let undone = store.undo().unwrap();
    assert_eq!(
        undone.revision.as_u64(),
        (studio_project::MAX_UNDO_DEPTH + extra + 1) as u64
    );
    assert_eq!(
        store.load_current().unwrap().revision.as_u64(),
        (studio_project::MAX_UNDO_DEPTH + extra + 1) as u64
    );
}

#[test]
fn dropped_undo_entries_no_longer_hold_snapshots() {
    let root = tempfile::tempdir().unwrap();
    let path = WorkspacePath::new(root.path(), "trimmed").unwrap();
    let store = ProjectStore::create(path).unwrap();

    let document = Document::new("trimmed");
    store.write_snapshot(&document).unwrap();
    let mut current = document;
    for _ in 0..(studio_project::MAX_UNDO_DEPTH + 5) {
        current.revision = current.revision.next();
        store.write_snapshot(&current).unwrap();
    }

    // 最早的 revision 已被回收，读取它必须失败而不是返回错误内容。
    assert!(
        store
            .load_revision(studio_core::ids::Revision::new(1))
            .is_err()
    );
}

#[test]
fn reopens_a_project_after_old_snapshots_were_reclaimed() {
    let root = tempfile::tempdir().unwrap();
    let path = WorkspacePath::new(root.path(), "reopen").unwrap();
    let store = ProjectStore::create(path.clone()).unwrap();

    let document = Document::new("reopen");
    store.write_snapshot(&document).unwrap();
    let mut current = document;
    for _ in 0..(studio_project::MAX_UNDO_DEPTH + 8) {
        current.revision = current.revision.next();
        store.write_snapshot(&current).unwrap();
    }
    drop(store);

    // 历史记录被回收后必须仍能重新打开，并且当前 revision 可读。
    let reopened = ProjectStore::open(path).unwrap();
    let loaded = reopened.load_current().unwrap();
    assert_eq!(
        loaded.revision.as_u64(),
        (studio_project::MAX_UNDO_DEPTH + 8) as u64
    );
    assert!(reopened.history().unwrap().len() > studio_project::MAX_UNDO_DEPTH);
}
