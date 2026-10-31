use std::path::{Component, Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use studio_core::reference_asset::{ReferenceAssetDescriptor, ReferenceAssetEquipment};
use studio_core::validation::DiagnosticCode;
use studio_project::reference_asset_store::{
    ReferenceAssetError, ReferenceAssetSource, ReferenceAssetStore,
};
use ts_rs::TS;

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
