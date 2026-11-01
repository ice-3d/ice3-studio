//! Native spatial partitioning, LOD, batching and Tileset generation.

mod partition;

pub use partition::{
    PartitionError, PartitionLeaf, PartitionPlan, PartitionPlanNode, PartitionUnit, analyze_units,
    create_partition_plan, try_create_partition_plan,
};

#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error(transparent)]
    Asset(#[from] studio_assets::AssetError),
    #[error(transparent)]
    Partition(#[from] PartitionError),
}

impl PipelineError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Asset(error) => error.code(),
            Self::Partition(error) => error.code(),
        }
    }
}
