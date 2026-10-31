mod batch;
mod dirty;
mod key;
mod store;

pub use batch::{ResidentBatchPlan, ResidentBatchSubmission};
pub use dirty::DirtyPlan;
pub use key::ResidencyKey;
pub(crate) use store::{GpuGeometryResidency, ResidentBatchInput};
pub use store::{ResidentGeometryRecord, ResidentPhaseCounters};
