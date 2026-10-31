use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use rand::RngCore;
use serde::Deserialize;
use studio_core::apply::{ApplyRequest, apply_transaction};
use studio_core::changeset::{candidate_digest, evaluate_operations};
use studio_core::diff::DocumentDiff;
use studio_core::document::Document;
use studio_core::ids::Revision;
use studio_core::operation::Operation;
use studio_core::validation::{Diagnostic, DiagnosticCode};
use studio_project::changeset::{
    ChangeSet, ChangeSetEntry, ChangeSetEvaluation, ChangeSetId, ChangeSetSource, ChangeSetStatus,
    EntryId,
};

use crate::ServerEvent;
use crate::preview::{CandidatePreview, PreviewAdapter};
use crate::rpc::{RpcError, Session, project_error, transaction_error};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateParams {
    base_revision: Revision,
    title: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GetParams {
    change_set_id: ChangeSetId,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NewEntry {
    source: ChangeSetSource,
    operation: Operation,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AddParams {
    change_set_id: ChangeSetId,
    expected_version: u64,
    entries: Vec<NewEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RemoveParams {
    change_set_id: ChangeSetId,
    expected_version: u64,
    entry_ids: Vec<EntryId>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EvaluateParams {
    change_set_id: ChangeSetId,
    expected_version: u64,
    #[serde(default)]
    include_candidate: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CommitParams {
    change_set_id: ChangeSetId,
    expected_version: u64,
    approved_digest: String,
    idempotency_key: String,
}

pub(crate) fn dispatch(
    session: &mut Session,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, RpcError> {
    match method {
        "changeset.create" => create(session, parse(params)?),
        "changeset.list" => list(session),
        "changeset.get" => get(session, parse(params)?),
        "changeset.addOperations" => add_operations(session, parse(params)?),
        "changeset.removeOperations" => remove_operations(session, parse(params)?),
        "changeset.evaluate" => evaluate_rpc(session, parse(params)?),
        "changeset.getCandidate" => candidate_rpc(session, parse(params)?),
        "changeset.commit" => commit(session, parse(params)?),
        "changeset.discard" => discard(session, parse(params)?),
        _ => unreachable!("filtered change set method"),
    }
}

pub(crate) fn export_candidate_preview(
    session: &mut Session,
    digest: &str,
) -> Result<CandidatePreview, RpcError> {
    let prepared = prepare_candidate_preview(session, digest)?;
    let adapter = PreviewAdapter::from_service(Arc::clone(&session.toolchain));
    match adapter.generate_candidate(&prepared.document, &prepared.project_dir, &prepared.digest) {
        Ok(preview) => Ok(preview),
        Err(error) => {
            record_candidate_preview_failure(session, digest, error.to_string())?;
            Err(preview_error(error))
        }
    }
}

pub(crate) struct PreparedCandidatePreview {
    pub document: Document,
    pub project_dir: std::path::PathBuf,
    pub digest: String,
}

pub(crate) fn prepare_candidate_preview(
    session: &mut Session,
    digest: &str,
) -> Result<PreparedCandidatePreview, RpcError> {
    let store = session.store.change_set_store();
    let change_set = store
        .list()
        .map_err(project_error)?
        .into_iter()
        .find(|change_set| change_set.candidate_digest.as_deref() == Some(digest))
        .ok_or_else(|| operation_error("candidate digest is not registered"))?;
    let base = session
        .store
        .load_revision(change_set.base_revision)
        .map_err(project_error)?;
    let operations = change_set.operations();
    let candidate = evaluate_operations(&base, &operations).map_err(transaction_error)?;
    let current_digest = candidate_digest(&base, &operations).map_err(transaction_error)?;
    if current_digest != digest || !candidate.diagnostics.is_empty() {
        return Err(operation_error("candidate digest is stale or blocked"));
    }
    Ok(PreparedCandidatePreview {
        document: candidate.document,
        project_dir: session.store.project_dir().to_path_buf(),
        digest: digest.into(),
    })
}

pub(crate) fn record_candidate_preview_failure(
    session: &mut Session,
    digest: &str,
    message: String,
) -> Result<(), RpcError> {
    let store = session.store.change_set_store();
    let mut change_set = store
        .list()
        .map_err(project_error)?
        .into_iter()
        .find(|change_set| change_set.candidate_digest.as_deref() == Some(digest))
        .ok_or_else(|| operation_error("candidate digest is not registered"))?;
    change_set.status = ChangeSetStatus::Blocked;
    change_set.diagnostics.push(Diagnostic {
        code: DiagnosticCode::RuntimePreviewFailed,
        operation_index: None,
        object_id: None,
        pointer: "/candidate/preview".into(),
        message,
    });
    change_set.updated_at = now_ms();
    store.save(&change_set).map_err(project_error)
}

fn create(session: &mut Session, params: CreateParams) -> Result<serde_json::Value, RpcError> {
    let base = session
        .store
        .load_revision(params.base_revision)
        .map_err(project_error)?;
    let change_set = ChangeSet::new(
        new_change_set_id(),
        base.project_id,
        params.base_revision,
        params.title,
        now_ms(),
    );
    let store = session.store.change_set_store();
    store.save(&change_set).map_err(project_error)?;
    emit_created(session, &change_set);
    serde_json::to_value(change_set).map_err(serialize_error)
}

fn list(session: &mut Session) -> Result<serde_json::Value, RpcError> {
    serde_json::to_value(
        session
            .store
            .change_set_store()
            .list()
            .map_err(project_error)?,
    )
    .map_err(serialize_error)
}

fn get(session: &mut Session, params: GetParams) -> Result<serde_json::Value, RpcError> {
    let change_set = session
        .store
        .change_set_store()
        .load(&params.change_set_id)
        .map_err(project_error)?;
    serde_json::to_value(change_set).map_err(serialize_error)
}

fn add_operations(session: &mut Session, params: AddParams) -> Result<serde_json::Value, RpcError> {
    let store = session.store.change_set_store();
    let mut change_set = store.load(&params.change_set_id).map_err(project_error)?;
    let mut next = change_set
        .entries
        .iter()
        .filter_map(|entry| entry.id.as_str().strip_prefix("entry-"))
        .filter_map(|value| value.parse::<u64>().ok())
        .max()
        .unwrap_or(0)
        + 1;
    let now = now_ms();
    let entries = params
        .entries
        .into_iter()
        .map(|entry| {
            let entry_id =
                EntryId::parse(&format!("entry-{next:06}")).expect("generated entry ID is valid");
            next += 1;
            ChangeSetEntry {
                id: entry_id,
                source: entry.source,
                operation: entry.operation,
                created_at: now,
            }
        })
        .collect();
    change_set
        .add_entries(params.expected_version, entries, now)
        .map_err(changeset_error)?;
    store.save(&change_set).map_err(project_error)?;
    emit_updated(session, &change_set);
    serde_json::to_value(change_set).map_err(serialize_error)
}

fn remove_operations(
    session: &mut Session,
    params: RemoveParams,
) -> Result<serde_json::Value, RpcError> {
    let store = session.store.change_set_store();
    let mut change_set = store.load(&params.change_set_id).map_err(project_error)?;
    change_set
        .remove_entries(params.expected_version, &params.entry_ids, now_ms())
        .map_err(changeset_error)?;
    store.save(&change_set).map_err(project_error)?;
    emit_updated(session, &change_set);
    serde_json::to_value(change_set).map_err(serialize_error)
}

fn evaluate_rpc(
    session: &mut Session,
    params: EvaluateParams,
) -> Result<serde_json::Value, RpcError> {
    let evaluation = evaluate(
        session,
        &params.change_set_id,
        params.expected_version,
        params.include_candidate,
    )?;
    serde_json::to_value(evaluation).map_err(serialize_error)
}

fn candidate_rpc(session: &mut Session, params: GetParams) -> Result<serde_json::Value, RpcError> {
    let change_set = session
        .store
        .change_set_store()
        .load(&params.change_set_id)
        .map_err(project_error)?;
    let evaluation = evaluate(session, &params.change_set_id, change_set.version, true)?;
    serde_json::to_value(evaluation).map_err(serialize_error)
}

fn commit(session: &mut Session, params: CommitParams) -> Result<serde_json::Value, RpcError> {
    let change_sets = session.store.change_set_store();
    let mut change_set = change_sets
        .load(&params.change_set_id)
        .map_err(project_error)?;
    if change_set.status == ChangeSetStatus::Committed {
        if change_set.commit_idempotency_key.as_deref() == Some(params.idempotency_key.as_str()) {
            return serde_json::to_value(
                change_set
                    .commit_result
                    .expect("committed ChangeSet stores its result"),
            )
            .map_err(serialize_error);
        }
        return Err(operation_error("ChangeSet is already committed"));
    }
    if change_set.version != params.expected_version {
        return Err(operation_error("ChangeSet version conflict"));
    }
    if session.document.revision != change_set.base_revision {
        change_set.mark_stale(now_ms());
        change_sets.save(&change_set).map_err(project_error)?;
        return Err(RpcError::with_diagnostic(
            -32010,
            "ICE3_STUDIO_REVISION_CONFLICT",
            "ICE3_STUDIO_REVISION_CONFLICT".into(),
            None,
        ));
    }
    let evaluation = evaluate(session, &change_set.id, change_set.version, false)?;
    if evaluation.candidate_digest.as_deref() != Some(params.approved_digest.as_str()) {
        return Err(operation_error("approved candidate digest is stale"));
    }
    if change_set.status == ChangeSetStatus::Blocked {
        return Err(operation_error("blocked ChangeSet cannot be committed"));
    }
    let previous = session.document.clone();
    if session.compute.policy().incremental_scene_cache {
        let _ = crate::rpc::evaluate_session_scene(session);
    }
    let commit_key = params.idempotency_key.clone();
    let result = apply_transaction(
        &mut session.document,
        ApplyRequest {
            expected_revision: change_set.base_revision,
            dry_run: false,
            idempotency_key: params.idempotency_key,
            operations: change_set.operations(),
        },
    )
    .map_err(transaction_error)?;
    let history = serde_json::json!({
        "kind": "changeset.commit",
        "changeSetId": change_set.id,
        "approvedDigest": params.approved_digest,
        "entryCount": change_set.entries.len()
    });
    if let Err(error) = session
        .store
        .write_snapshot_with_history(&session.document, &history)
    {
        session.document = previous;
        return Err(project_error(error));
    }
    change_set.mark_committed(result.clone(), commit_key, now_ms());
    crate::rpc::stage_committed_scene(session, &result.diff);
    change_sets.save(&change_set).map_err(project_error)?;
    let _ = session.events.send(ServerEvent::ChangeSetCommitted {
        change_set_id: change_set.id,
        version: change_set.version,
        revision: result.revision,
        candidate_digest: evaluation.candidate_digest.unwrap_or_default(),
    });
    let _ = session.events.send(ServerEvent::RevisionCommitted {
        revision: result.revision,
    });
    serde_json::to_value(result).map_err(serialize_error)
}

fn discard(session: &mut Session, params: GetParams) -> Result<serde_json::Value, RpcError> {
    let store = session.store.change_set_store();
    let mut change_set = store.load(&params.change_set_id).map_err(project_error)?;
    if matches!(
        change_set.status,
        ChangeSetStatus::Committed | ChangeSetStatus::Discarded
    ) {
        return Err(operation_error("ChangeSet cannot be discarded"));
    }
    change_set.discard(now_ms());
    store.save(&change_set).map_err(project_error)?;
    let _ = session.events.send(ServerEvent::ChangeSetDiscarded {
        change_set_id: change_set.id.clone(),
        version: change_set.version,
    });
    serde_json::to_value(change_set).map_err(serialize_error)
}

fn evaluate(
    session: &mut Session,
    id: &ChangeSetId,
    expected_version: u64,
    include_candidate: bool,
) -> Result<ChangeSetEvaluation, RpcError> {
    let store = session.store.change_set_store();
    let mut change_set = store.load(id).map_err(project_error)?;
    if change_set.version != expected_version {
        return Err(operation_error("ChangeSet version conflict"));
    }
    if change_set.base_revision != session.document.revision {
        change_set.mark_stale(now_ms());
        store.save(&change_set).map_err(project_error)?;
        return Ok(ChangeSetEvaluation {
            change_set_id: change_set.id,
            version: change_set.version,
            base_revision: change_set.base_revision,
            status: ChangeSetStatus::Stale,
            candidate_digest: None,
            candidate: None,
            diff: DocumentDiff::default(),
            diagnostics: Vec::new(),
        });
    }
    let base = session
        .store
        .load_revision(change_set.base_revision)
        .map_err(project_error)?;
    let operations = change_set.operations();
    if operations.is_empty() {
        let diagnostics = vec![Diagnostic {
            code: DiagnosticCode::OperationInvalid,
            operation_index: None,
            object_id: None,
            pointer: "/entries".into(),
            message: "ChangeSet contains no operations".into(),
        }];
        change_set
            .register_evaluation(expected_version, None, diagnostics.clone(), now_ms())
            .map_err(changeset_error)?;
        store.save(&change_set).map_err(project_error)?;
        let _ = session.events.send(ServerEvent::ChangeSetEvaluated {
            change_set_id: change_set.id.clone(),
            version: change_set.version,
            status: change_set.status.clone(),
            candidate_digest: None,
        });
        return Ok(ChangeSetEvaluation {
            change_set_id: change_set.id,
            version: change_set.version,
            base_revision: change_set.base_revision,
            status: change_set.status,
            candidate_digest: None,
            candidate: None,
            diff: DocumentDiff::default(),
            diagnostics,
        });
    }
    let candidate = evaluate_operations(&base, &operations).map_err(transaction_error)?;
    let digest = if candidate.diagnostics.is_empty() {
        Some(candidate_digest(&base, &operations).map_err(transaction_error)?)
    } else {
        None
    };
    change_set
        .register_evaluation(
            expected_version,
            digest.clone(),
            candidate.diagnostics.clone(),
            now_ms(),
        )
        .map_err(changeset_error)?;
    store.save(&change_set).map_err(project_error)?;
    let _ = session.events.send(ServerEvent::ChangeSetEvaluated {
        change_set_id: change_set.id.clone(),
        version: change_set.version,
        status: change_set.status.clone(),
        candidate_digest: digest.clone(),
    });
    Ok(ChangeSetEvaluation {
        change_set_id: change_set.id,
        version: change_set.version,
        base_revision: change_set.base_revision,
        status: change_set.status,
        candidate_digest: digest,
        candidate: include_candidate.then_some(candidate.document),
        diff: candidate.diff,
        diagnostics: candidate.diagnostics,
    })
}

fn emit_created(session: &Session, change_set: &ChangeSet) {
    let _ = session.events.send(ServerEvent::ChangeSetCreated {
        change_set_id: change_set.id.clone(),
        version: change_set.version,
        status: change_set.status.clone(),
    });
}

fn emit_updated(session: &Session, change_set: &ChangeSet) {
    let _ = session.events.send(ServerEvent::ChangeSetUpdated {
        change_set_id: change_set.id.clone(),
        version: change_set.version,
        status: change_set.status.clone(),
    });
}

fn parse<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> Result<T, RpcError> {
    serde_json::from_value(value).map_err(invalid_params)
}

fn invalid_params(error: serde_json::Error) -> RpcError {
    RpcError::with_diagnostic(
        -32602,
        "ICE3_STUDIO_API_INVALID",
        format!("ICE3_STUDIO_API_INVALID: {error}"),
        None,
    )
}

fn serialize_error(error: serde_json::Error) -> RpcError {
    RpcError::with_diagnostic(
        -32603,
        "ICE3_STUDIO_API_INVALID",
        format!("ICE3_STUDIO_API_INVALID: {error}"),
        None,
    )
}

fn changeset_error(error: studio_project::changeset::ChangeSetError) -> RpcError {
    operation_error(&error.to_string())
}

fn operation_error(message: &str) -> RpcError {
    RpcError::with_diagnostic(
        -32010,
        "ICE3_STUDIO_OPERATION_INVALID",
        format!("ICE3_STUDIO_OPERATION_INVALID: {message}"),
        None,
    )
}

fn preview_error(error: impl std::fmt::Display) -> RpcError {
    RpcError::with_diagnostic(
        -32013,
        "ICE3_STUDIO_RUNTIME_PREVIEW_FAILED",
        format!("ICE3_STUDIO_RUNTIME_PREVIEW_FAILED: {error}"),
        None,
    )
}

fn new_change_set_id() -> ChangeSetId {
    let mut bytes = [0_u8; 8];
    rand::rng().fill_bytes(&mut bytes);
    ChangeSetId::parse(&format!(
        "changeset-{}",
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    ))
    .expect("generated ChangeSet ID is valid")
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
