use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use schemars::schema_for;
use studio_core::agent_contract::{
    AgentManifest, EvaluationQueryRequest, EvaluationQueryResult, RuntimePreviewRequest,
    SceneContext, SceneSearchRequest, SceneSearchResult,
};
use studio_core::apply::{ApplyRequest, ApplyResult};
use studio_core::document::Document;
use studio_core::operation::Operation;
use studio_core::patch::{ComponentPatch, ConnectionPatch, InstancePatch, MaterialPatch};
use studio_core::validation::Diagnostic;
use studio_core::{API_VERSION, PROJECT_SCHEMA_VERSION};
use studio_project::changeset::{
    ChangeSet, ChangeSetEntry, ChangeSetEvaluation, ChangeSetId, ChangeSetSource, ChangeSetStatus,
};
use studio_server::changeset::{
    ChangeSetAddParams, ChangeSetCommitParams, ChangeSetCreateParams, ChangeSetEvaluateParams,
    ChangeSetGetParams, ChangeSetNewEntry, ChangeSetRemoveParams,
};
use studio_server::preview::CandidatePreview;
use studio_server::{Capabilities, EvaluatedSceneStats, RpcRequest, RpcResponse, ServerEvent};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SchemaError {
    #[error("schema I/O failed")]
    Io(#[source] std::io::Error),
    #[error("schema JSON encoding failed")]
    Json(#[source] serde_json::Error),
    #[error("generated schema is out of date")]
    OutOfDate,
}

pub fn generate(output: &Path, check: bool) -> Result<(), SchemaError> {
    let api = serde_json::json!({
        "title": "ICE3 Studio API",
        "apiVersion": API_VERSION,
        "schemaVersion": PROJECT_SCHEMA_VERSION,
        "methods": [
            "system.health",
            "system.capabilities",
            "system.agentManifest",
            "project.create",
            "project.open",
            "project.get",
            "project.save",
            "project.close",
            "document.get",
            "document.apply",
            "document.undo",
            "document.redo",
            "document.history",
            "scene.get",
            "scene.query",
            "scene.bounds",
            "scene.context",
            "scene.search",
            "validation.run",
            "evaluation.evaluate",
            "evaluation.stats",
            "evaluation.query",
            "export.sourceGlb",
            "export.runtimePreview",
            "changeset.create",
            "changeset.list",
            "changeset.get",
            "changeset.addOperations",
            "changeset.removeOperations",
            "changeset.evaluate",
            "changeset.getCandidate",
            "changeset.commit",
            "changeset.discard"
        ],
        "types": {
            "Document": schema_for!(Document),
            "Operation": schema_for!(Operation),
            "ApplyRequest": schema_for!(ApplyRequest),
            "ApplyResult": schema_for!(ApplyResult),
            "Diagnostic": schema_for!(Diagnostic),
            "RpcRequest": schema_for!(RpcRequest),
            "RpcResponse": schema_for!(RpcResponse),
            "Capabilities": schema_for!(Capabilities),
            "EvaluatedSceneStats": schema_for!(EvaluatedSceneStats),
            "ServerEvent": schema_for!(ServerEvent),
            "ChangeSet": schema_for!(ChangeSet),
            "ChangeSetId": schema_for!(ChangeSetId),
            "ChangeSetStatus": schema_for!(ChangeSetStatus),
            "ChangeSetSource": schema_for!(ChangeSetSource),
            "ChangeSetEntry": schema_for!(ChangeSetEntry),
            "ChangeSetEvaluation": schema_for!(ChangeSetEvaluation),
            "CandidatePreview": schema_for!(CandidatePreview)
            ,
            "AgentManifest": schema_for!(AgentManifest),
            "SceneContext": schema_for!(SceneContext),
            "SceneSearchRequest": schema_for!(SceneSearchRequest),
            "SceneSearchResult": schema_for!(SceneSearchResult),
            "EvaluationQueryRequest": schema_for!(EvaluationQueryRequest),
            "EvaluationQueryResult": schema_for!(EvaluationQueryResult)
            ,
            "InstancePatch": schema_for!(InstancePatch),
            "ComponentPatch": schema_for!(ComponentPatch),
            "ConnectionPatch": schema_for!(ConnectionPatch),
            "MaterialPatch": schema_for!(MaterialPatch),
            "RuntimePreviewRequest": schema_for!(RuntimePreviewRequest),
            "ChangeSetCreateParams": schema_for!(ChangeSetCreateParams),
            "ChangeSetGetParams": schema_for!(ChangeSetGetParams),
            "ChangeSetNewEntry": schema_for!(ChangeSetNewEntry),
            "ChangeSetAddParams": schema_for!(ChangeSetAddParams),
            "ChangeSetRemoveParams": schema_for!(ChangeSetRemoveParams),
            "ChangeSetEvaluateParams": schema_for!(ChangeSetEvaluateParams),
            "ChangeSetCommitParams": schema_for!(ChangeSetCommitParams)
        }
    });
    let mut api_bytes = serde_json::to_vec_pretty(&api).map_err(SchemaError::Json)?;
    api_bytes.push(b'\n');
    let types = collect_types()?;
    if check {
        expect_equal(&output.join("studio-api.schema.json"), &api_bytes)?;
        expect_equal(&output.join("studio-types.ts"), types.as_bytes())?;
        return Ok(());
    }
    fs::create_dir_all(output).map_err(SchemaError::Io)?;
    fs::write(output.join("studio-api.schema.json"), api_bytes).map_err(SchemaError::Io)?;
    fs::write(output.join("studio-types.ts"), types).map_err(SchemaError::Io)
}

fn collect_types() -> Result<String, SchemaError> {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = BTreeMap::new();
    for directory in [
        workspace.join("crates/studio-project/bindings"),
        workspace.join("crates/studio-core/bindings"),
        workspace.join("crates/studio-server/bindings"),
    ] {
        if !directory.exists() {
            continue;
        }
        for entry in fs::read_dir(directory).map_err(SchemaError::Io)? {
            let entry = entry.map_err(SchemaError::Io)?;
            if entry.path().extension().is_some_and(|value| value == "ts") {
                let name = entry.file_name().to_string_lossy().into_owned();
                files.insert(name, entry.path());
            }
        }
    }
    let mut output = String::from("// Generated by ICE3 Studio. Do not edit.\n\n");
    for file in files.values() {
        output.push_str(&fs::read_to_string(file).map_err(SchemaError::Io)?);
        if !output.ends_with('\n') {
            output.push('\n');
        }
    }
    Ok(output)
}

fn expect_equal(path: &Path, expected: &[u8]) -> Result<(), SchemaError> {
    if fs::read(path).map_err(SchemaError::Io)? == expected {
        Ok(())
    } else {
        Err(SchemaError::OutOfDate)
    }
}
