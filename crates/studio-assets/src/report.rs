use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{AssetMetrics, BudgetResult};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparationMetadata {
    pub runtime_conversion_required: bool,
    pub units_normalized: bool,
    pub matrix_transforms_baked: bool,
    pub required_compression_decoded: bool,
    pub edges_prepared: bool,
    pub selection_groups_prepared: bool,
    pub crease_angle_degrees: f32,
}

impl PreparationMetadata {
    pub fn edges(crease_angle_degrees: f32) -> Self {
        Self {
            runtime_conversion_required: false,
            units_normalized: true,
            matrix_transforms_baked: false,
            required_compression_decoded: true,
            edges_prepared: true,
            selection_groups_prepared: true,
            crease_angle_degrees: crease_angle_degrees.clamp(0.0, 180.0),
        }
    }

    pub fn without_edges() -> Self {
        Self {
            runtime_conversion_required: false,
            units_normalized: true,
            matrix_transforms_baked: false,
            required_compression_decoded: true,
            edges_prepared: false,
            selection_groups_prepared: true,
            crease_angle_degrees: 30.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareWarning {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetBuildReport {
    pub kind: String,
    pub schema_version: String,
    pub command: String,
    pub source_digest: String,
    pub content_digest: String,
    pub asset_id: String,
    pub metrics: AssetMetrics,
    pub preparation: PreparationMetadata,
    pub budget: BudgetResult,
    pub warnings: Vec<PrepareWarning>,
}

impl AssetBuildReport {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        command: impl Into<String>,
        source_digest: impl Into<String>,
        content_digest: impl Into<String>,
        asset_id: impl Into<String>,
        metrics: AssetMetrics,
        preparation: PreparationMetadata,
        budget: BudgetResult,
        mut warnings: Vec<PrepareWarning>,
    ) -> Self {
        warnings.sort();
        warnings.dedup();
        Self {
            kind: "ice3-asset-build-report".into(),
            schema_version: "1.0".into(),
            command: command.into(),
            source_digest: source_digest.into(),
            content_digest: content_digest.into(),
            asset_id: asset_id.into(),
            metrics,
            preparation,
            budget,
            warnings,
        }
    }
}
