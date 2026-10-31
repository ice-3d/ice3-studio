use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use studio_core::document::{Document, NodeContent};
use studio_core::reference_asset::{ReferenceAssetDescriptor, ReferenceAssetEquipment};
use studio_core::validation::{Diagnostic, DiagnosticCode};
use studio_project::reference_asset_store::{
    ReferenceAssetError, ReferenceAssetSource, ReferenceAssetStore,
};
use ts_rs::TS;

use crate::preview::SourceExport;
use crate::rpc::{RpcError, Session, invalid_params};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ReferenceAssetRegisterParams {
    pub source_path: String,
    pub prepared_path: Option<String>,
    pub tileset_path: Option<String>,
    pub equipment_manifest_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ReferenceAssetGetParams {
    pub asset_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ReferenceAssetBindingsParams {
    pub asset_id: String,
    pub cursor: Option<String>,
    #[serde(default = "default_binding_limit")]
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ReferenceAssetRegisterResult {
    pub asset_id: String,
    pub descriptor: ReferenceAssetDescriptor,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ReferenceAssetEndpoint {
    pub descriptor: ReferenceAssetDescriptor,
    pub tileset_url: Option<String>,
    pub prepared_preview_url: Option<String>,
    pub equipment_manifest_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ReferenceAssetBindingPageResult {
    pub items: Vec<ReferenceAssetEquipment>,
    pub next_cursor: Option<String>,
}

pub(crate) fn dispatch(
    session: &Session,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, RpcError> {
    let store = ReferenceAssetStore::new(session.store.project_dir());
    match method {
        "referenceAsset.register" => {
            let params: ReferenceAssetRegisterParams =
                serde_json::from_value(params).map_err(invalid_params)?;
            let source = ReferenceAssetSource {
                source_path: params.source_path.into(),
                prepared_path: params.prepared_path.map(PathBuf::from),
                tileset_path: params.tileset_path.map(PathBuf::from),
                equipment_manifest_path: params.equipment_manifest_path.into(),
            };
            let registered = store.register(&source).map_err(reference_asset_error)?;
            serde_json::to_value(ReferenceAssetRegisterResult {
                asset_id: registered.asset_id,
                descriptor: registered.descriptor,
            })
            .map_err(serialize_error)
        }
        "referenceAsset.get" => {
            let params: ReferenceAssetGetParams =
                serde_json::from_value(params).map_err(invalid_params)?;
            let registered = store.get(&params.asset_id).map_err(reference_asset_error)?;
            let base = format!("/reference-assets/{}", params.asset_id);
            serde_json::to_value(ReferenceAssetEndpoint {
                tileset_url: registered
                    .descriptor
                    .tileset_digest
                    .as_ref()
                    .map(|_| format!("{base}/tileset.json")),
                prepared_preview_url: registered
                    .descriptor
                    .prepared_digest
                    .as_ref()
                    .map(|_| format!("{base}/prepared.glb")),
                equipment_manifest_url: format!("{base}/equipment.json"),
                descriptor: registered.descriptor,
            })
            .map_err(serialize_error)
        }
        "referenceAsset.bindings" => {
            let params: ReferenceAssetBindingsParams =
                serde_json::from_value(params).map_err(invalid_params)?;
            if !(1..=100).contains(&params.limit) {
                return Err(RpcError::with_diagnostic(
                    -32012,
                    DiagnosticCode::BudgetExceeded.as_str(),
                    format!(
                        "{}: reference binding page limit must be between 1 and 100",
                        DiagnosticCode::BudgetExceeded.as_str()
                    ),
                    Some(serde_json::json!({
                        "category": "budget",
                        "retryable": false,
                        "limit": params.limit,
                        "maximum": 100
                    })),
                ));
            }
            let page = store
                .bindings(&params.asset_id, params.cursor.as_deref(), params.limit)
                .map_err(reference_asset_error)?;
            serde_json::to_value(ReferenceAssetBindingPageResult {
                items: page.items,
                next_cursor: page.next_cursor,
            })
            .map_err(serialize_error)
        }
        _ => unreachable!("reference asset dispatcher only receives reference methods"),
    }
}

pub(crate) fn resolve_asset_file(
    project_dir: &Path,
    asset_id: &str,
    relative: &Path,
) -> Result<PathBuf, ReferenceAssetError> {
    if relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(ReferenceAssetError::PathEscape);
    }
    let registered = ReferenceAssetStore::new(project_dir).get(asset_id)?;
    let root = registered
        .root
        .canonicalize()
        .map_err(ReferenceAssetError::Io)?;
    let mut current = root.clone();
    for component in relative.components() {
        let Component::Normal(part) = component else {
            continue;
        };
        current.push(part);
        let metadata = std::fs::symlink_metadata(&current).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                ReferenceAssetError::Missing
            } else {
                ReferenceAssetError::Io(error)
            }
        })?;
        if metadata.file_type().is_symlink() {
            return Err(ReferenceAssetError::PathEscape);
        }
    }
    let canonical = current
        .canonicalize()
        .map_err(|_| ReferenceAssetError::Missing)?;
    if !canonical.starts_with(&root) || !canonical.is_file() {
        return Err(ReferenceAssetError::PathEscape);
    }
    Ok(canonical)
}

pub(crate) fn reference_asset_error(error: ReferenceAssetError) -> RpcError {
    let diagnostic = match error {
        ReferenceAssetError::Io(_) => DiagnosticCode::PersistenceFailed,
        _ => DiagnosticCode::ReferenceInvalid,
    };
    RpcError::with_diagnostic(
        -32012,
        diagnostic.as_str(),
        format!("{}: {error}", diagnostic.as_str()),
        None,
    )
}

pub(crate) fn validate_reference_document(
    project_dir: &Path,
    document: &Document,
) -> Vec<Diagnostic> {
    let store = ReferenceAssetStore::new(project_dir);
    let mut selections = BTreeMap::<String, Option<BTreeSet<String>>>::new();
    let mut seen = BTreeSet::new();
    let mut diagnostics = Vec::new();
    for (index, node) in document.scene.nodes().iter().enumerate() {
        let NodeContent::ReferenceInstance(reference) = &node.content else {
            continue;
        };
        if !seen.insert((reference.asset_id.clone(), reference.selection_id.clone())) {
            diagnostics.push(reference_diagnostic(
                Some(index),
                node.id.as_str(),
                format!("/scene/nodes/{index}/content/selectionId"),
                "reference selection ID is already bound to another node",
            ));
            continue;
        }
        let available = selections
            .entry(reference.asset_id.clone())
            .or_insert_with(|| load_selection_ids(&store, &reference.asset_id));
        let Some(available) = available else {
            diagnostics.push(reference_diagnostic(
                Some(index),
                node.id.as_str(),
                format!("/scene/nodes/{index}/content/assetId"),
                "reference asset does not exist or is unreadable",
            ));
            continue;
        };
        if !available.contains(&reference.selection_id) {
            diagnostics.push(reference_diagnostic(
                Some(index),
                node.id.as_str(),
                format!("/scene/nodes/{index}/content/selectionId"),
                "reference selection ID does not exist in the equipment manifest",
            ));
        }
    }
    diagnostics
}

pub(crate) fn reference_stats(
    project_dir: &Path,
    document: &Document,
) -> Result<ReferenceAggregateStats, RpcError> {
    let store = ReferenceAssetStore::new(project_dir);
    let mut digests = BTreeSet::new();
    let mut aggregate = ReferenceAggregateStats::default();
    for node in document.scene.nodes() {
        let NodeContent::ReferenceInstance(reference) = &node.content else {
            continue;
        };
        if !digests.insert(reference.asset_id.clone()) {
            continue;
        }
        let descriptor = store
            .get(&reference.asset_id)
            .map_err(reference_asset_error)?
            .descriptor;
        if !digests.insert(descriptor.source_digest.clone()) {
            continue;
        }
        aggregate.reference_assets += 1;
        aggregate.effective_triangles = aggregate
            .effective_triangles
            .saturating_add(descriptor.stats.effective_triangles);
        aggregate.unique_meshes = aggregate
            .unique_meshes
            .saturating_add(descriptor.stats.unique_meshes);
    }
    Ok(aggregate)
}

pub(crate) fn export_reference_source(session: &Session) -> Result<Option<SourceExport>, RpcError> {
    let references = session
        .document
        .scene
        .nodes()
        .iter()
        .filter_map(|node| match &node.content {
            NodeContent::ReferenceInstance(reference) => Some((node, reference)),
            _ => None,
        })
        .collect::<Vec<_>>();
    if references.is_empty() {
        return Ok(None);
    }
    let diagnostics = validate_reference_document(session.store.project_dir(), &session.document);
    if !diagnostics.is_empty() {
        return Err(reference_diagnostics_error(diagnostics));
    }
    let store = ReferenceAssetStore::new(session.store.project_dir());
    let mut referenced = BTreeMap::<String, BTreeSet<String>>::new();
    for (_, reference) in &references {
        referenced
            .entry(reference.asset_id.clone())
            .or_default()
            .insert(reference.selection_id.clone());
    }
    for (asset_id, selections) in &referenced {
        let expected = load_selection_ids(&store, asset_id)
            .ok_or_else(|| reference_asset_error(ReferenceAssetError::Missing))?;
        if *selections != expected {
            return Err(export_unsupported(
                "adding or deleting reference equipment bindings cannot be exported as the source GLB",
            ));
        }
    }
    if references.iter().any(|(node, _)| {
        node.transform != studio_core::math::Transform::IDENTITY
            || node.material_id.is_some()
            || !node.visible
    }) {
        return Err(export_unsupported(
            "authored reference transforms, materials, or visibility cannot be exported as the source GLB",
        ));
    }
    let mut source_digest = None;
    let mut source_path = None;
    for (_, reference) in references {
        let registered = store
            .get(&reference.asset_id)
            .map_err(reference_asset_error)?;
        let digest = registered.descriptor.source_digest.clone();
        if let Some(existing) = source_digest.as_ref()
            && existing != &digest
        {
            return Err(export_unsupported(
                "multiple distinct reference assets cannot be exported through one source GLB",
            ));
        }
        let registered_source = registered.root.join("source.glb");
        if !registered_source.is_file() {
            return Err(reference_asset_error(ReferenceAssetError::Missing));
        }
        source_digest = Some(digest);
        source_path = Some(registered_source);
    }
    match (source_digest, source_path) {
        (Some(source_digest), Some(source_path)) => Ok(Some(SourceExport {
            revision: session.document.revision.as_u64(),
            source_digest,
            source_path,
        })),
        _ => Err(reference_asset_error(ReferenceAssetError::Missing)),
    }
}

pub(crate) fn reference_diagnostics_error(diagnostics: Vec<Diagnostic>) -> RpcError {
    let diagnostic = diagnostics
        .first()
        .map(|value| value.code.as_str())
        .unwrap_or(DiagnosticCode::ReferenceInvalid.as_str());
    RpcError::with_diagnostic(
        -32012,
        diagnostic,
        format!("{diagnostic}: reference binding validation failed"),
        serde_json::to_value(diagnostics).ok(),
    )
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ReferenceAggregateStats {
    pub reference_assets: usize,
    pub effective_triangles: u64,
    pub unique_meshes: usize,
}

fn load_selection_ids(store: &ReferenceAssetStore, asset_id: &str) -> Option<BTreeSet<String>> {
    let mut selections = BTreeSet::new();
    let mut cursor = None;
    loop {
        let page = store.bindings(asset_id, cursor.as_deref(), 100).ok()?;
        selections.extend(page.items.into_iter().map(|item| item.selection_id));
        match page.next_cursor {
            Some(next) if cursor.as_deref() != Some(next.as_str()) => cursor = Some(next),
            _ => break,
        }
    }
    Some(selections)
}

fn reference_diagnostic(
    operation_index: Option<usize>,
    object_id: &str,
    pointer: String,
    message: &str,
) -> Diagnostic {
    Diagnostic {
        code: DiagnosticCode::ReferenceInvalid,
        operation_index,
        object_id: Some(object_id.into()),
        pointer,
        message: message.into(),
    }
}

fn export_unsupported(message: &str) -> RpcError {
    let diagnostic = DiagnosticCode::ReferenceExportUnsupported;
    RpcError::with_diagnostic(
        -32012,
        diagnostic.as_str(),
        format!("{}: {message}", diagnostic.as_str()),
        None,
    )
}

fn serialize_error(error: serde_json::Error) -> RpcError {
    RpcError::with_diagnostic(
        -32603,
        DiagnosticCode::ApiInvalid.as_str(),
        format!("{}: {error}", DiagnosticCode::ApiInvalid.as_str()),
        None,
    )
}

const fn default_binding_limit() -> usize {
    25
}
