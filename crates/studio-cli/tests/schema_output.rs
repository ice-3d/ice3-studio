use std::path::{Path, PathBuf};
use studio_cli::schema;

#[test]
fn writes_deterministic_schema_output() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("schemas");
    schema::generate(&output, false).unwrap();
    let first = std::fs::read(output.join("studio-api.schema.json")).unwrap();
    let first_types = std::fs::read(output.join("studio-types.ts")).unwrap();
    schema::generate(&output, false).unwrap();
    assert_eq!(
        std::fs::read(output.join("studio-api.schema.json")).unwrap(),
        first
    );
    assert_eq!(
        std::fs::read(output.join("studio-types.ts")).unwrap(),
        first_types
    );
    assert!(first.ends_with(b"\n"));
    assert!(!first.ends_with(b"\n\n"));
    assert!(first_types.ends_with(b"\n"));
    assert!(!first_types.ends_with(b"\n\n"));
    let text = String::from_utf8_lossy(&first);
    assert!(text.contains("node.create"));
    assert!(text.contains("export.runtimePreview"));
    assert!(text.contains("changeset.impact"));
    let schema: serde_json::Value = serde_json::from_slice(&first).unwrap();
    assert!(schema["types"].get("ChangeSetImpactParams").is_some());
    assert!(schema["types"].get("ChangeSetImpactResult").is_some());
    assert!(schema["types"].get("IterationImpact").is_some());
    let types = String::from_utf8(first_types).unwrap();
    assert!(types.contains("export type ChangeSetImpactResult"));
    assert!(types.contains("export type IterationImpact"));
}

#[test]
fn rejects_retired_toolchain_bindings_in_any_crate() {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut stale = Vec::new();
    for entry in std::fs::read_dir(workspace.join("crates")).unwrap() {
        let crate_dir = entry.unwrap().path();
        let bindings = crate_dir.join("bindings");
        if bindings.is_dir() {
            collect_stale_bindings(&bindings, &mut stale);
        }
    }

    assert!(
        stale.is_empty(),
        "retired Toolchain bindings remain: {stale:?}"
    );
}

fn collect_stale_bindings(directory: &Path, stale: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_stale_bindings(&path, stale);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "ts" || extension == "json")
            && std::fs::read_to_string(&path)
                .unwrap()
                .contains("ICE3_STUDIO_TOOLCHAIN_")
        {
            stale.push(path);
        }
    }
}
