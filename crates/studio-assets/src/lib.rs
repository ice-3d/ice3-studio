//! Asset readers and validation for Studio's native asset pipeline.

mod accessor;
pub mod budget;
pub mod canonical;
mod edges;
mod glb;
mod metrics;
mod prepared;
pub mod report;

pub use accessor::{AccessorData, read_accessor};
pub use budget::{
    BudgetCheck, BudgetLimit, BudgetMetric, BudgetProfile, BudgetResult, BudgetStatus,
    evaluate_budget,
};
pub use edges::{edge_color_from_material, generate_edge_indices, material_suppresses_edges};
pub use glb::{AssetError, AssetResult, SourceGlb, read_bounded_uri, read_glb, read_glb_bytes};
pub use metrics::AssetMetrics;
pub use prepared::{PreparedBounds, PreparedProfile, validate_prepared_profile};
pub use report::{AssetBuildReport, PreparationMetadata, PrepareWarning};
