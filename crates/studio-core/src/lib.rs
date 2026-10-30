pub mod apply;
pub mod diff;
pub mod document;
pub mod export;
pub mod geometry;
pub mod ids;
pub mod math;
pub mod operation;
pub mod validation;

pub const PROJECT_SCHEMA_VERSION: u32 = 1;
pub const API_VERSION: &str = "1.0";
pub const DIAGNOSTIC_CODES: &[&str] = &[
    "ICE3_STUDIO_REFERENCE_INVALID",
    "ICE3_STUDIO_GEOMETRY_INVALID",
    "ICE3_STUDIO_CONSTRAINT_FAILED",
];
