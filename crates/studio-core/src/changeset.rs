use sha2::{Digest, Sha256};

use crate::apply::TransactionError;
use crate::document::Document;
use crate::operation::Operation;

pub use crate::apply::{OperationEvaluation, evaluate_operations};
pub type CandidateEvaluation = OperationEvaluation;

pub fn candidate_digest(
    base: &Document,
    operations: &[Operation],
) -> Result<String, TransactionError> {
    let bytes = serde_json::to_vec(&("ice3-changeset-v1", base, operations))
        .map_err(TransactionError::Serialization)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub fn evaluate_change_set(
    base: &Document,
    operations: &[Operation],
) -> Result<CandidateEvaluation, TransactionError> {
    evaluate_operations(base, operations)
}
