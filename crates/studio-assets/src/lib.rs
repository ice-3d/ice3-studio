//! Asset readers and validation for Studio's native asset pipeline.

mod accessor;
pub mod budget;
pub mod canonical;
mod edges;
mod glb;
pub mod image;
mod metrics;
mod prepare;
mod prepared;
pub mod report;
pub mod surface;

pub use accessor::{AccessorData, read_accessor};
pub use budget::{
    BudgetCheck, BudgetLimit, BudgetMetric, BudgetProfile, BudgetResult, BudgetStatus,
    evaluate_budget,
};
pub use edges::{edge_color_from_material, generate_edge_indices, material_suppresses_edges};
pub use glb::{
    AssetError, AssetResult, SourceGlb, read_bounded_uri, read_bounded_uri_with_limit, read_glb,
    read_glb_bytes,
};
pub use image::{
    ImageResource, MAX_TEXTURE_BYTES, MAX_TEXTURE_COUNT, MAX_TEXTURE_TOTAL_BYTES,
    read_image_buffer_view, read_image_uri, validate_image_bytes,
};
pub use metrics::AssetMetrics;
pub use prepare::{PrepareArtifact, PrepareOptions, prepare_canonical, validate_crease_angle};
pub use prepared::{PreparedBounds, PreparedProfile, validate_prepared_profile};
pub use report::{AssetBuildReport, PreparationMetadata, PrepareWarning};
