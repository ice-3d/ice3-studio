//! Native spatial partitioning, LOD, batching and Tileset generation.

mod partition;
mod tileset;

pub use partition::{
    PartitionError, PartitionLeaf, PartitionPlan, PartitionPlanNode, PartitionUnit, analyze_units,
    create_partition_plan, try_create_partition_plan,
};
pub use tileset::{BuildOptions, PartitionArtifact, PartitionFile, PartitionStats};

use crate::partition::build::build_artifact as build_partition_artifact;

#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error(transparent)]
    Asset(#[from] studio_assets::AssetError),
    #[error(transparent)]
    Partition(#[from] PartitionError),
    #[error("tileset is invalid: {0}")]
    Tileset(String),
}

impl PipelineError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Asset(error) => error.code(),
            Self::Partition(error) => error.code(),
            Self::Tileset(_) => "ICE3_STUDIO_TILESET_INVALID",
        }
    }
}

pub fn build_artifact(
    source: &studio_assets::SourceGlb,
    options: &BuildOptions,
) -> Result<PartitionArtifact, PipelineError> {
    build_partition_artifact(source, options)
}
