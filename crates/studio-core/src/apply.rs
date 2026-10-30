use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use ts_rs::TS;

use crate::diff::{DiffBuilder, DocumentDiff};
use crate::document::{Document, Material, NodeContent};
use crate::ids::{MaterialId, NodeId, RelationId, Revision};
use crate::operation::Operation;
use crate::validation::{Diagnostic, DiagnosticCode, validate_document};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ApplyRequest {
    pub expected_revision: Revision,
    #[serde(default)]
    pub dry_run: bool,
    pub idempotency_key: String,
    pub operations: Vec<Operation>,
}

impl ApplyRequest {
    fn semantic_digest(&self) -> Result<String, TransactionError> {
        let bytes = serde_json::to_vec(&(&self.expected_revision, &self.operations))
            .map_err(TransactionError::Serialization)?;
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ApplyResult {
    pub previous_revision: Revision,
    pub revision: Revision,
    pub changed_node_ids: Vec<NodeId>,
    pub changed_material_ids: Vec<MaterialId>,
    pub changed_relation_ids: Vec<RelationId>,
    pub diff: DocumentDiff,
    pub diagnostics: Vec<Diagnostic>,
    pub dry_run: bool,
}

impl ApplyResult {
    fn dry_run(revision: Revision, diff: DocumentDiff) -> Self {
        Self::from_diff(revision, revision, diff, Vec::new(), true)
    }

    fn committed(previous_revision: Revision, revision: Revision, diff: DocumentDiff) -> Self {
        Self::from_diff(previous_revision, revision, diff, Vec::new(), false)
    }

    fn from_diff(
        previous_revision: Revision,
        revision: Revision,
        diff: DocumentDiff,
        diagnostics: Vec<Diagnostic>,
        dry_run: bool,
    ) -> Self {
        let mut changed_node_ids = diff.added_node_ids.clone();
        changed_node_ids.extend(diff.updated_node_ids.iter().cloned());
        changed_node_ids.extend(diff.removed_node_ids.iter().cloned());
        changed_node_ids.sort();
        changed_node_ids.dedup();
        let mut changed_material_ids = diff.added_material_ids.clone();
        changed_material_ids.extend(diff.updated_material_ids.iter().cloned());
        changed_material_ids.extend(diff.removed_material_ids.iter().cloned());
        changed_material_ids.sort();
        changed_material_ids.dedup();
        let mut changed_relation_ids = diff.added_relation_ids.clone();
        changed_relation_ids.extend(diff.updated_relation_ids.iter().cloned());
        changed_relation_ids.extend(diff.removed_relation_ids.iter().cloned());
        changed_relation_ids.sort();
        changed_relation_ids.dedup();
        Self {
            previous_revision,
            revision,
            changed_node_ids,
            changed_material_ids,
            changed_relation_ids,
            diff,
            diagnostics,
            dry_run,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct IdempotencyRecord {
    pub key: String,
    pub request_digest: String,
    pub result: ApplyResult,
}

#[derive(Debug, Error)]
pub enum TransactionError {
    #[error("expected revision {expected:?}, current revision is {actual:?}")]
    RevisionConflict {
        expected: Revision,
        actual: Revision,
    },
    #[error("transaction contains no operations")]
    EmptyTransaction,
    #[error("idempotency key was already used for a different transaction")]
    IdempotencyKeyReused,
    #[error("operation {operation_index} failed: {message}")]
    OperationInvalid {
        operation_index: usize,
        code: DiagnosticCode,
        object_id: Option<String>,
        pointer: String,
        message: String,
    },
    #[error("document validation failed")]
    ValidationFailed(Vec<Diagnostic>),
    #[error("transaction could not be serialized")]
    Serialization(#[source] serde_json::Error),
}

pub fn apply_transaction(
    document: &mut Document,
    request: ApplyRequest,
) -> Result<ApplyResult, TransactionError> {
    let digest = request.semantic_digest()?;
    if let Some(previous) = document.idempotency_record(&request.idempotency_key) {
        if previous.request_digest != digest {
            return Err(TransactionError::IdempotencyKeyReused);
        }
        return Ok(previous.result.clone());
    }
    if request.expected_revision != document.revision {
        return Err(TransactionError::RevisionConflict {
            expected: request.expected_revision,
            actual: document.revision,
        });
    }
    if request.operations.is_empty() {
        return Err(TransactionError::EmptyTransaction);
    }

    let mut candidate = document.clone();
    let mut diff = DiffBuilder::default();
    for (index, operation) in request.operations.iter().cloned().enumerate() {
        apply_operation(&mut candidate, operation, &mut diff, index)?;
    }
    let diagnostics = validate_document(&candidate);
    if !diagnostics.is_empty() {
        return Err(TransactionError::ValidationFailed(diagnostics));
    }
    let diff = diff.finish();
    if request.dry_run {
        return Ok(ApplyResult::dry_run(document.revision, diff));
    }

    let previous_revision = document.revision;
    candidate.revision = previous_revision.next();
    let result = ApplyResult::committed(previous_revision, candidate.revision, diff);
    candidate.idempotency_keys.push(IdempotencyRecord {
        key: request.idempotency_key,
        request_digest: digest,
        result: result.clone(),
    });
    *document = candidate;
    Ok(result)
}

fn apply_operation(
    document: &mut Document,
    operation: Operation,
    diff: &mut DiffBuilder,
    operation_index: usize,
) -> Result<(), TransactionError> {
    match operation {
        Operation::MaterialUpsert {
            material_id,
            name,
            base_color,
            opacity,
            edge_color,
        } => {
            let material = Material {
                id: material_id.clone(),
                name,
                base_color,
                opacity,
                edge_color,
            };
            if let Some(existing) = document
                .materials
                .iter_mut()
                .find(|candidate| candidate.id == material_id)
            {
                *existing = material;
                diff.material_updated(material_id);
            } else {
                document.materials.push(material);
                diff.material_added(material_id);
            }
        }
        Operation::NodeCreate {
            node_id,
            parent_id,
            name,
            transform,
            content,
            material_id,
            metadata,
        } => {
            document.scene.nodes_mut().push(crate::document::SceneNode {
                id: node_id.clone(),
                name,
                parent_id,
                transform,
                material_id,
                content,
                metadata,
                visible: true,
            });
            diff.node_added(node_id);
        }
        Operation::NodeUpdate {
            node_id,
            name,
            transform,
            visible,
            metadata,
        } => {
            let node = node_mut(document, &node_id, operation_index, "/nodeId")?;
            if let Some(value) = name {
                node.name = value;
            }
            if let Some(value) = transform {
                node.transform = value;
            }
            if let Some(value) = visible {
                node.visible = value;
            }
            if let Some(value) = metadata {
                node.metadata = value;
            }
            diff.node_updated(node_id);
        }
        Operation::NodeReparent { node_id, parent_id } => {
            node_mut(document, &node_id, operation_index, "/nodeId")?.parent_id = parent_id;
            diff.node_updated(node_id);
        }
        Operation::NodeDelete { node_id } => {
            let index = document
                .scene
                .nodes()
                .iter()
                .position(|node| node.id == node_id)
                .ok_or_else(|| {
                    operation_error(operation_index, &node_id, "/nodeId", "node does not exist")
                })?;
            document.scene.nodes_mut().remove(index);
            diff.node_removed(node_id);
        }
        Operation::PrimitiveUpdate { node_id, primitive } => {
            let node = node_mut(document, &node_id, operation_index, "/nodeId")?;
            node.content = NodeContent::Primitive(primitive);
            diff.node_updated(node_id);
        }
        Operation::MeshReplace { node_id, mesh } => {
            let node = node_mut(document, &node_id, operation_index, "/nodeId")?;
            node.content = NodeContent::CustomMesh(mesh);
            diff.node_updated(node_id);
        }
        Operation::MaterialAssign {
            node_id,
            material_id,
        } => {
            node_mut(document, &node_id, operation_index, "/nodeId")?.material_id = material_id;
            diff.node_updated(node_id);
        }
        Operation::RelationCreate { relation } => {
            let id = relation.id.clone();
            document.relations.push(relation);
            diff.relation_added(id);
        }
        Operation::RelationUpdate {
            relation_id,
            relation_type,
            from_node_id,
            to_node_id,
            metadata,
        } => {
            let relation = document
                .relations
                .iter_mut()
                .find(|candidate| candidate.id == relation_id)
                .ok_or_else(|| {
                    operation_error(
                        operation_index,
                        &relation_id,
                        "/relationId",
                        "relation does not exist",
                    )
                })?;
            if let Some(value) = relation_type {
                relation.relation_type = value;
            }
            if let Some(value) = from_node_id {
                relation.from_node_id = value;
            }
            if let Some(value) = to_node_id {
                relation.to_node_id = value;
            }
            if let Some(value) = metadata {
                relation.metadata = value;
            }
            diff.relation_updated(relation_id);
        }
        Operation::RelationDelete { relation_id } => {
            let index = document
                .relations
                .iter()
                .position(|relation| relation.id == relation_id)
                .ok_or_else(|| {
                    operation_error(
                        operation_index,
                        &relation_id,
                        "/relationId",
                        "relation does not exist",
                    )
                })?;
            document.relations.remove(index);
            diff.relation_removed(relation_id);
        }
        Operation::ConstraintUpsert { constraint } => {
            let id = constraint.id.clone();
            if let Some(existing) = document
                .constraints
                .iter_mut()
                .find(|candidate| candidate.id == id)
            {
                *existing = constraint;
                diff.constraint_updated(id);
            } else {
                document.constraints.push(constraint);
                diff.constraint_added(id);
            }
        }
        Operation::ConstraintDelete { constraint_id } => {
            let index = document
                .constraints
                .iter()
                .position(|constraint| constraint.id == constraint_id)
                .ok_or_else(|| {
                    operation_error(
                        operation_index,
                        &constraint_id,
                        "/constraintId",
                        "constraint does not exist",
                    )
                })?;
            document.constraints.remove(index);
            diff.constraint_removed(constraint_id);
        }
    }
    Ok(())
}

fn node_mut<'a>(
    document: &'a mut Document,
    node_id: &NodeId,
    operation_index: usize,
    pointer: &str,
) -> Result<&'a mut crate::document::SceneNode, TransactionError> {
    document
        .scene
        .nodes_mut()
        .iter_mut()
        .find(|node| node.id == *node_id)
        .ok_or_else(|| operation_error(operation_index, node_id, pointer, "node does not exist"))
}

fn operation_error(
    operation_index: usize,
    object_id: impl ToString,
    pointer: &str,
    message: &str,
) -> TransactionError {
    TransactionError::OperationInvalid {
        operation_index,
        code: DiagnosticCode::ReferenceInvalid,
        object_id: Some(object_id.to_string()),
        pointer: pointer.into(),
        message: message.into(),
    }
}
