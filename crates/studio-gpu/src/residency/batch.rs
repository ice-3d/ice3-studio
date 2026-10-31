use studio_core::gpu_contract::{GeometryBatch, GeometryJobNode, GeometryJobPriority};

#[derive(Debug, Clone)]
pub struct ResidentBatchPlan {
    pub job_id: String,
    pub priority: GeometryJobPriority,
    pub budget_bytes: u64,
    pub nodes: Vec<GeometryJobNode>,
}

impl ResidentBatchPlan {
    pub fn from_batch(batch: GeometryBatch) -> Self {
        Self {
            job_id: batch.job_id,
            priority: batch.priority,
            budget_bytes: batch.budget_bytes,
            nodes: batch.nodes,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResidentBatchSubmission {
    pub job_id: String,
    pub submission_count: usize,
}
