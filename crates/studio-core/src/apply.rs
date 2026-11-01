use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use ts_rs::TS;

use crate::diff::{DiffBuilder, DocumentDiff};
use crate::document::{Document, Material, NodeContent};
use crate::ids::{ComponentId, MaterialId, NodeId, RelationId, Revision};
use crate::large_scene::component::PartContent;
use crate::large_scene::connection::ConnectionIntent;
use crate::operation::Operation;
use crate::patch::{ComponentPatch, ConnectionPatch, InstancePatch, MaterialPatch};
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
    pub changed_component_ids: Vec<ComponentId>,
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
        let mut changed_component_ids = diff.added_component_ids.clone();
        changed_component_ids.extend(diff.updated_component_ids.iter().cloned());
        changed_component_ids.extend(diff.removed_component_ids.iter().cloned());
        changed_component_ids.sort();
        changed_component_ids.dedup();
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
            changed_component_ids,
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

pub struct OperationEvaluation {
    pub document: Document,
    pub diff: DocumentDiff,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn evaluate_operations(
    base: &Document,
    operations: &[Operation],
) -> Result<OperationEvaluation, TransactionError> {
    let mut candidate = base.clone();
    let mut diff = DiffBuilder::default();
    for (index, operation) in operations.iter().cloned().enumerate() {
        apply_operation(&mut candidate, operation, &mut diff, index)?;
    }
    let diagnostics = validate_document(&candidate);
    Ok(OperationEvaluation {
        document: candidate,
        diff: diff.finish(),
        diagnostics,
    })
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

    let evaluation = evaluate_operations(document, &request.operations)?;
    if !evaluation.diagnostics.is_empty() {
        return Err(TransactionError::ValidationFailed(evaluation.diagnostics));
    }
    if request.dry_run {
        return Ok(ApplyResult::dry_run(document.revision, evaluation.diff));
    }

    let previous_revision = document.revision;
    let mut candidate = evaluation.document;
    candidate.revision = previous_revision.next();
    let result = ApplyResult::committed(previous_revision, candidate.revision, evaluation.diff);
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
        Operation::InstancePatch(patch) => {
            apply_instance_patch(document, patch, diff, operation_index)?;
        }
        Operation::MaterialPatch(patch) => {
            apply_material_patch(document, patch, diff, operation_index)?;
        }
        Operation::ComponentPatch(patch) => {
            apply_component_patch(document, patch, diff, operation_index)?;
        }
        Operation::ConnectionPatch(patch) => {
            apply_connection_patch(document, patch, diff, operation_index)?;
        }
        Operation::ComponentDefine { definition } => {
            let id = definition.id.clone();
            if document
                .components
                .iter()
                .any(|candidate| candidate.id == id)
            {
                return Err(operation_error(
                    operation_index,
                    &id,
                    "/definition/id",
                    "component definition already exists",
                ));
            }
            document.components.push(definition);
            diff.component_added(id);
        }
        Operation::ComponentUpdate { definition } => {
            let id = definition.id.clone();
            let existing = document
                .components
                .iter_mut()
                .find(|candidate| candidate.id == id)
                .ok_or_else(|| {
                    operation_error(
                        operation_index,
                        &id,
                        "/definition/id",
                        "component definition does not exist",
                    )
                })?;
            *existing = definition;
            diff.component_updated(id);
        }
        Operation::ComponentDelete { component_id } => {
            ensure_component_unreferenced(document, &component_id, operation_index)?;
            let index = document
                .components
                .iter()
                .position(|candidate| candidate.id == component_id)
                .ok_or_else(|| {
                    operation_error(
                        operation_index,
                        &component_id,
                        "/componentId",
                        "component definition does not exist",
                    )
                })?;
            document.components.remove(index);
            diff.component_removed(component_id);
        }
        Operation::InstanceCreate {
            node_id,
            parent_id,
            name,
            transform,
            instance,
            material_id,
            metadata,
        } => {
            ensure_new_node(document, &node_id, operation_index)?;
            document.scene.nodes_mut().push(crate::document::SceneNode {
                id: node_id.clone(),
                name,
                parent_id,
                transform,
                material_id,
                content: NodeContent::ComponentInstance(instance),
                metadata,
                visible: true,
            });
            diff.node_added(node_id);
        }
        Operation::InstanceUpdate { node_id, instance } => {
            let node = node_mut(document, &node_id, operation_index, "/nodeId")?;
            let NodeContent::ComponentInstance(current) = &mut node.content else {
                return Err(operation_error(
                    operation_index,
                    &node_id,
                    "/nodeId",
                    "node is not a component instance",
                ));
            };
            *current = instance;
            diff.node_updated(node_id);
        }
        Operation::InstanceDelete { node_id } => {
            delete_node(document, &node_id, operation_index, diff)?;
        }
        Operation::InstanceSetCreate {
            node_id,
            parent_id,
            name,
            transform,
            set,
            material_id,
            metadata,
        } => {
            ensure_new_node(document, &node_id, operation_index)?;
            document.scene.nodes_mut().push(crate::document::SceneNode {
                id: node_id.clone(),
                name,
                parent_id,
                transform,
                material_id,
                content: NodeContent::InstanceSet(set),
                metadata,
                visible: true,
            });
            diff.node_added(node_id);
        }
        Operation::InstanceSetUpdate { node_id, set } => {
            let node = node_mut(document, &node_id, operation_index, "/nodeId")?;
            let NodeContent::InstanceSet(current) = &mut node.content else {
                return Err(operation_error(
                    operation_index,
                    &node_id,
                    "/nodeId",
                    "node is not an instance set",
                ));
            };
            *current = set;
            diff.node_updated(node_id);
        }
        Operation::InstanceSetDelete { node_id } => {
            delete_node(document, &node_id, operation_index, diff)?;
        }
        Operation::ConnectionCreate {
            node_id,
            parent_id,
            name,
            transform,
            connection,
            material_id,
            metadata,
        } => {
            ensure_new_node(document, &node_id, operation_index)?;
            ensure_connection_relation(document, &connection, operation_index, &node_id)?;
            document.scene.nodes_mut().push(crate::document::SceneNode {
                id: node_id.clone(),
                name,
                parent_id,
                transform,
                material_id: material_id.clone(),
                content: NodeContent::Connection(connection),
                metadata,
                visible: true,
            });
            diff.node_added(node_id);
        }
        Operation::ConnectionUpdate {
            node_id,
            connection,
        } => {
            ensure_connection_relation(document, &connection, operation_index, &node_id)?;
            let node = node_mut(document, &node_id, operation_index, "/nodeId")?;
            let NodeContent::Connection(current) = &mut node.content else {
                return Err(operation_error(
                    operation_index,
                    &node_id,
                    "/nodeId",
                    "node is not a connection",
                ));
            };
            *current = connection;
            diff.node_updated(node_id);
        }
        Operation::ConnectionDelete { node_id } => {
            let connection = document
                .scene
                .nodes()
                .iter()
                .find(|node| node.id == node_id)
                .and_then(|node| match &node.content {
                    NodeContent::Connection(connection) => Some(connection.clone()),
                    _ => None,
                })
                .ok_or_else(|| {
                    operation_error(
                        operation_index,
                        &node_id,
                        "/nodeId",
                        "node is not a connection",
                    )
                })?;
            if let Some(index) = document
                .relations
                .iter()
                .position(|relation| relation.id == connection.relation_id)
            {
                let relation_id = document.relations.remove(index).id;
                diff.relation_removed(relation_id);
            }
            delete_node(document, &node_id, operation_index, diff)?;
        }
        Operation::MaterialUpsert {
            material_id,
            name,
            base_color,
            opacity,
            edge_color,
            shading,
        } => {
            let material = Material {
                id: material_id.clone(),
                name,
                base_color,
                opacity,
                edge_color,
                shading,
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

fn apply_instance_patch(
    document: &mut Document,
    patch: InstancePatch,
    diff: &mut DiffBuilder,
    operation_index: usize,
) -> Result<(), TransactionError> {
    if !patch.has_input() {
        return Err(invalid_patch(
            operation_index,
            &patch.node_id,
            "",
            "patch must contain at least one field",
        ));
    }
    if patch
        .remove_parameters
        .iter()
        .any(|name| patch.set_parameters.contains_key(name))
    {
        return Err(invalid_patch(
            operation_index,
            &patch.node_id,
            "/removeParameters",
            "a parameter cannot be set and removed in one patch",
        ));
    }
    if patch
        .remove_metadata_keys
        .iter()
        .any(|key| patch.set_metadata.contains_key(key))
    {
        return Err(invalid_patch(
            operation_index,
            &patch.node_id,
            "/removeMetadataKeys",
            "a metadata key cannot be set and removed in one patch",
        ));
    }
    if patch.material_id.is_some() && patch.clear_material {
        return Err(invalid_patch(
            operation_index,
            &patch.node_id,
            "/clearMaterial",
            "materialId and clearMaterial cannot be combined",
        ));
    }

    let current_definition_id = document
        .scene
        .nodes()
        .iter()
        .find(|node| node.id == patch.node_id)
        .and_then(|node| match &node.content {
            NodeContent::ComponentInstance(instance) => Some(instance.definition_id.clone()),
            _ => None,
        })
        .ok_or_else(|| {
            invalid_patch(
                operation_index,
                &patch.node_id,
                "/nodeId",
                "node is not a component instance",
            )
        })?;
    let definition_id = patch.definition_id.clone().unwrap_or(current_definition_id);
    let definition = document
        .components
        .iter()
        .find(|definition| definition.id == definition_id)
        .ok_or_else(|| {
            invalid_patch(
                operation_index,
                &patch.node_id,
                "/definitionId",
                "component definition does not exist",
            )
        })?;
    let parameters = definition
        .parameters
        .iter()
        .map(|parameter| parameter.name.as_str())
        .collect::<BTreeSet<_>>();
    if patch
        .set_parameters
        .keys()
        .chain(&patch.remove_parameters)
        .any(|name| !parameters.contains(name.as_str()))
    {
        return Err(invalid_patch(
            operation_index,
            &patch.node_id,
            "/setParameters",
            "patch contains an unknown component parameter",
        ));
    }

    let node = node_mut(document, &patch.node_id, operation_index, "/nodeId")?;
    if let Some(name) = patch.name {
        node.name = name;
    }
    if let Some(transform) = patch.transform {
        node.transform = transform;
    }
    if let Some(visible) = patch.visible {
        node.visible = visible;
    }
    for (key, value) in patch.set_metadata {
        node.metadata.insert(key, value);
    }
    for key in patch
        .remove_metadata_keys
        .into_iter()
        .collect::<BTreeSet<_>>()
    {
        node.metadata.remove(&key);
    }
    if let Some(material_id) = patch.material_id {
        node.material_id = Some(material_id);
    } else if patch.clear_material {
        node.material_id = None;
    }

    let NodeContent::ComponentInstance(instance) = &mut node.content else {
        unreachable!("component instance checked above");
    };
    if let Some(definition_id) = patch.definition_id {
        instance.definition_id = definition_id;
    }
    for name in patch.remove_parameters.into_iter().collect::<BTreeSet<_>>() {
        instance.parameters.remove(&name);
    }
    for (name, value) in patch.set_parameters {
        instance.parameters.insert(name, value);
    }
    diff.node_updated(patch.node_id);
    Ok(())
}

fn apply_material_patch(
    document: &mut Document,
    patch: MaterialPatch,
    diff: &mut DiffBuilder,
    operation_index: usize,
) -> Result<(), TransactionError> {
    if !patch.has_input() {
        return Err(invalid_patch(
            operation_index,
            &patch.material_id,
            "",
            "patch must contain at least one field",
        ));
    }
    if patch.edge_color.is_some() && patch.clear_edge_color {
        return Err(invalid_patch(
            operation_index,
            &patch.material_id,
            "/clearEdgeColor",
            "edgeColor and clearEdgeColor cannot be combined",
        ));
    }
    let material = document
        .materials
        .iter_mut()
        .find(|material| material.id == patch.material_id)
        .ok_or_else(|| {
            operation_error(
                operation_index,
                &patch.material_id,
                "/materialId",
                "material does not exist",
            )
        })?;
    if let Some(name) = patch.name {
        material.name = name;
    }
    if let Some(base_color) = patch.base_color {
        material.base_color = base_color;
    }
    if let Some(opacity) = patch.opacity {
        material.opacity = opacity;
    }
    if let Some(edge_color) = patch.edge_color {
        material.edge_color = Some(edge_color);
    } else if patch.clear_edge_color {
        material.edge_color = None;
    }
    diff.material_updated(patch.material_id);
    Ok(())
}

fn apply_component_patch(
    document: &mut Document,
    patch: ComponentPatch,
    diff: &mut DiffBuilder,
    operation_index: usize,
) -> Result<(), TransactionError> {
    if !patch.has_input() {
        return Err(invalid_patch(
            operation_index,
            &patch.component_id,
            "",
            "patch must contain at least one field",
        ));
    }
    if patch
        .remove_metadata_keys
        .iter()
        .any(|key| patch.set_metadata.contains_key(key))
    {
        return Err(invalid_patch(
            operation_index,
            &patch.component_id,
            "/removeMetadataKeys",
            "a metadata key cannot be set and removed in one patch",
        ));
    }
    let component_id = patch.component_id.clone();
    let component = document
        .components
        .iter_mut()
        .find(|component| component.id == component_id)
        .ok_or_else(|| {
            operation_error(
                operation_index,
                &component_id,
                "/componentId",
                "component definition does not exist",
            )
        })?;
    let parameter_names = component
        .parameters
        .iter()
        .map(|parameter| parameter.name.as_str())
        .collect::<BTreeSet<_>>();
    if patch
        .set_parameter_defaults
        .keys()
        .any(|name| !parameter_names.contains(name.as_str()))
    {
        return Err(invalid_patch(
            operation_index,
            &component_id,
            "/setParameterDefaults",
            "patch contains an unknown component parameter",
        ));
    }
    if let Some(name) = patch.name {
        component.name = name;
    }
    for parameter in &mut component.parameters {
        if let Some(value) = patch.set_parameter_defaults.get(&parameter.name) {
            parameter.default = value.clone();
        }
    }
    for (key, value) in patch.set_metadata {
        component.metadata.insert(key, value);
    }
    for key in patch
        .remove_metadata_keys
        .into_iter()
        .collect::<BTreeSet<_>>()
    {
        component.metadata.remove(&key);
    }
    diff.component_updated(component_id);
    Ok(())
}

fn apply_connection_patch(
    document: &mut Document,
    patch: ConnectionPatch,
    diff: &mut DiffBuilder,
    operation_index: usize,
) -> Result<(), TransactionError> {
    if !patch.has_input() {
        return Err(invalid_patch(
            operation_index,
            &patch.node_id,
            "",
            "patch must contain at least one field",
        ));
    }
    if patch
        .remove_metadata_keys
        .iter()
        .any(|key| patch.set_metadata.contains_key(key))
    {
        return Err(invalid_patch(
            operation_index,
            &patch.node_id,
            "/removeMetadataKeys",
            "a metadata key cannot be set and removed in one patch",
        ));
    }
    let node_id = patch.node_id.clone();
    let mut connection = document
        .scene
        .nodes()
        .iter()
        .find(|node| node.id == node_id)
        .and_then(|node| match &node.content {
            NodeContent::Connection(connection) => Some(connection.clone()),
            _ => None,
        })
        .ok_or_else(|| {
            invalid_patch(
                operation_index,
                &node_id,
                "/nodeId",
                "node is not a connection",
            )
        })?;
    if let Some(from) = patch.from {
        connection.from = from;
    }
    if let Some(to) = patch.to {
        connection.to = to;
    }
    if let Some(medium) = patch.medium {
        connection.medium = medium;
    }
    if let Some(diameter) = patch.diameter {
        connection.diameter = diameter;
    }
    if let Some(route_policy) = patch.route_policy {
        connection.route_policy = route_policy;
    }
    ensure_connection_relation(document, &connection, operation_index, &node_id)?;

    let node = node_mut(document, &node_id, operation_index, "/nodeId")?;
    for (key, value) in patch.set_metadata {
        node.metadata.insert(key, value);
    }
    for key in patch
        .remove_metadata_keys
        .into_iter()
        .collect::<BTreeSet<_>>()
    {
        node.metadata.remove(&key);
    }
    node.content = NodeContent::Connection(connection);
    diff.node_updated(node_id);
    Ok(())
}

fn invalid_patch(
    operation_index: usize,
    object_id: impl ToString,
    pointer: &str,
    message: &str,
) -> TransactionError {
    TransactionError::OperationInvalid {
        operation_index,
        code: DiagnosticCode::OperationInvalid,
        object_id: Some(object_id.to_string()),
        pointer: pointer.into(),
        message: message.into(),
    }
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

fn ensure_new_node(
    document: &Document,
    node_id: &NodeId,
    operation_index: usize,
) -> Result<(), TransactionError> {
    if document
        .scene
        .nodes()
        .iter()
        .any(|node| node.id == *node_id)
    {
        return Err(operation_error(
            operation_index,
            node_id,
            "/nodeId",
            "node already exists",
        ));
    }
    Ok(())
}

fn delete_node(
    document: &mut Document,
    node_id: &NodeId,
    operation_index: usize,
    diff: &mut DiffBuilder,
) -> Result<(), TransactionError> {
    let index = document
        .scene
        .nodes()
        .iter()
        .position(|node| node.id == *node_id)
        .ok_or_else(|| {
            operation_error(operation_index, node_id, "/nodeId", "node does not exist")
        })?;
    document.scene.nodes_mut().remove(index);
    diff.node_removed(node_id.clone());
    Ok(())
}

fn ensure_component_unreferenced(
    document: &Document,
    component_id: &ComponentId,
    operation_index: usize,
) -> Result<(), TransactionError> {
    let nested = document.components.iter().any(|definition| {
        definition.parts.iter().any(|part| {
            matches!(
                &part.content,
                PartContent::ComponentReference { definition_id, .. }
                    if definition_id == component_id
            )
        })
    });
    let instantiated = document
        .scene
        .nodes()
        .iter()
        .any(|node| match &node.content {
            NodeContent::ComponentInstance(instance) => instance.definition_id == *component_id,
            NodeContent::InstanceSet(set) => set.definition_id == *component_id,
            _ => false,
        });
    if nested || instantiated {
        return Err(operation_error(
            operation_index,
            component_id,
            "/componentId",
            "component definition is still referenced",
        ));
    }
    Ok(())
}

fn ensure_connection_relation(
    document: &Document,
    connection: &ConnectionIntent,
    operation_index: usize,
    node_id: &NodeId,
) -> Result<(), TransactionError> {
    if document
        .relations
        .iter()
        .any(|relation| relation.id == connection.relation_id)
    {
        Ok(())
    } else {
        Err(operation_error(
            operation_index,
            node_id,
            "/connection/relationId",
            "connection relation does not exist",
        ))
    }
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
