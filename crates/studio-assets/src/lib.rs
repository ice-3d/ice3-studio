//! Asset readers and validation for Studio's native asset pipeline.

mod accessor;
mod glb;
mod prepared;

pub use accessor::{AccessorData, read_accessor};
pub use glb::{AssetError, AssetResult, SourceGlb, read_glb, read_glb_bytes};
pub use prepared::{PreparedBounds, PreparedProfile, validate_prepared_profile};
