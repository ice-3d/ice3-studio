use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Value, json};
use studio_core::large_scene::route_safety::{RouteSafetyStats, validate_route_safety_with_limits};
use studio_core::validation::Diagnostic;
use studio_project::{ProjectStore, WorkspacePath};

pub fn build_report(revision: u64, diagnostics: Vec<Diagnostic>, stats: RouteSafetyStats) -> Value {
    json!({
        "revision": revision,
        "diagnostics": diagnostics,
        "stats": stats,
    })
}

pub fn report_project(project: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let project_id = project
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("project path must end with a project ID")?;
    let workspace_root = project
        .parent()
        .ok_or("project path must have a parent directory")?;
    let workspace = WorkspacePath::new(workspace_root, project_id)?;
    let store = ProjectStore::open_read_only(workspace)?;
    let document = store.load_current_read_only()?;
    let definitions = document
        .components
        .iter()
        .map(|definition| (definition.id.clone(), definition))
        .collect::<BTreeMap<_, _>>();
    let (diagnostics, stats) =
        validate_route_safety_with_limits(&document, &definitions, Default::default());
    Ok(build_report(document.revision.as_u64(), diagnostics, stats))
}
