use serde::{Deserialize, Serialize};

use crate::canonical::{CanonicalScene, prepare_canonical_document};
use crate::{
    AssetError, AssetMetrics, BudgetProfile, BudgetResult, PreparationMetadata, PrepareWarning,
};

#[derive(Debug, Clone, PartialEq)]
pub struct PrepareOptions {
    pub asset_id: String,
    pub source_format: String,
    pub source_digest: String,
    pub crease_angle_degrees: f32,
    pub generate_edges: bool,
    pub budget: BudgetProfile,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareArtifact {
    #[serde(skip)]
    pub bytes: Vec<u8>,
    pub metrics: AssetMetrics,
    pub preparation: PreparationMetadata,
    pub budget: BudgetResult,
    pub content_digest: String,
    pub warnings: Vec<PrepareWarning>,
}

pub fn prepare_canonical(
    scene: &CanonicalScene,
    options: PrepareOptions,
) -> Result<PrepareArtifact, AssetError> {
    if !options.crease_angle_degrees.is_finite()
        || !(0.0..=180.0).contains(&options.crease_angle_degrees)
    {
        return Err(AssetError::InvalidGeometry(
            "crease angle must be between 0 and 180 degrees".into(),
        ));
    }
    prepare_canonical_document(scene, options)
}
