use serde::{Deserialize, Serialize};

use crate::canonical::{
    CanonicalResource, CanonicalScene, prepare_canonical_document, resource_uri, texture_uri_prefix,
};
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
    pub texture_namespace: String,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedResource {
    pub uri: String,
    pub digest: String,
    pub media_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreparedAssetBundle {
    pub glb: Vec<u8>,
    pub texture_uri_prefix: String,
    pub resources: Vec<PreparedResource>,
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
    validate_crease_angle(options.crease_angle_degrees)?;
    prepare_canonical_document(scene, options)
}

pub fn prepare_canonical_bundle(
    scene: &CanonicalScene,
    options: PrepareOptions,
) -> Result<PreparedAssetBundle, AssetError> {
    let namespace = options.texture_namespace.clone();
    let texture_uri_prefix = texture_uri_prefix(&namespace)?;
    let artifact = prepare_canonical(scene, options)?;
    let resources = prepared_resources(&scene.resources, &texture_uri_prefix)?;
    Ok(PreparedAssetBundle {
        glb: artifact.bytes,
        texture_uri_prefix,
        resources,
        metrics: artifact.metrics,
        preparation: artifact.preparation,
        budget: artifact.budget,
        content_digest: artifact.content_digest,
        warnings: artifact.warnings,
    })
}

fn prepared_resources(
    resources: &[CanonicalResource],
    texture_uri_prefix: &str,
) -> Result<Vec<PreparedResource>, AssetError> {
    let mut output = resources
        .iter()
        .map(|resource| {
            Ok::<_, AssetError>(PreparedResource {
                uri: resource_uri(resource, texture_uri_prefix)?,
                digest: resource.digest.clone(),
                media_type: resource.media_type.clone(),
                bytes: resource.bytes.clone(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    output.sort_by(|left, right| left.digest.cmp(&right.digest));
    Ok(output)
}

pub fn validate_crease_angle(crease_angle_degrees: f32) -> Result<(), AssetError> {
    if !crease_angle_degrees.is_finite() || !(0.0..=180.0).contains(&crease_angle_degrees) {
        return Err(AssetError::InvalidGeometry(
            "crease angle must be between 0 and 180 degrees".into(),
        ));
    }
    Ok(())
}
