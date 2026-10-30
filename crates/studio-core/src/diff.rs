use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::{ConstraintId, MaterialId, NodeId, RelationId};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DocumentDiff {
    pub added_node_ids: Vec<NodeId>,
    pub updated_node_ids: Vec<NodeId>,
    pub removed_node_ids: Vec<NodeId>,
    pub added_material_ids: Vec<MaterialId>,
    pub updated_material_ids: Vec<MaterialId>,
    pub removed_material_ids: Vec<MaterialId>,
    pub added_relation_ids: Vec<RelationId>,
    pub updated_relation_ids: Vec<RelationId>,
    pub removed_relation_ids: Vec<RelationId>,
    pub added_constraint_ids: Vec<ConstraintId>,
    pub updated_constraint_ids: Vec<ConstraintId>,
    pub removed_constraint_ids: Vec<ConstraintId>,
}

#[derive(Debug, Default)]
pub(crate) struct DiffBuilder {
    added_nodes: BTreeSet<NodeId>,
    updated_nodes: BTreeSet<NodeId>,
    removed_nodes: BTreeSet<NodeId>,
    added_materials: BTreeSet<MaterialId>,
    updated_materials: BTreeSet<MaterialId>,
    removed_materials: BTreeSet<MaterialId>,
    added_relations: BTreeSet<RelationId>,
    updated_relations: BTreeSet<RelationId>,
    removed_relations: BTreeSet<RelationId>,
    added_constraints: BTreeSet<ConstraintId>,
    updated_constraints: BTreeSet<ConstraintId>,
    removed_constraints: BTreeSet<ConstraintId>,
}

impl DiffBuilder {
    pub(crate) fn node_added(&mut self, id: NodeId) {
        self.added_nodes.insert(id);
    }

    pub(crate) fn node_updated(&mut self, id: NodeId) {
        self.updated_nodes.insert(id);
    }

    pub(crate) fn node_removed(&mut self, id: NodeId) {
        self.removed_nodes.insert(id);
    }

    pub(crate) fn material_added(&mut self, id: MaterialId) {
        self.added_materials.insert(id);
    }

    pub(crate) fn material_updated(&mut self, id: MaterialId) {
        self.updated_materials.insert(id);
    }

    pub(crate) fn relation_added(&mut self, id: RelationId) {
        self.added_relations.insert(id);
    }

    pub(crate) fn relation_updated(&mut self, id: RelationId) {
        self.updated_relations.insert(id);
    }

    pub(crate) fn relation_removed(&mut self, id: RelationId) {
        self.removed_relations.insert(id);
    }

    pub(crate) fn constraint_added(&mut self, id: ConstraintId) {
        self.added_constraints.insert(id);
    }

    pub(crate) fn constraint_updated(&mut self, id: ConstraintId) {
        self.updated_constraints.insert(id);
    }

    pub(crate) fn constraint_removed(&mut self, id: ConstraintId) {
        self.removed_constraints.insert(id);
    }

    pub(crate) fn finish(self) -> DocumentDiff {
        DocumentDiff {
            added_node_ids: self.added_nodes.into_iter().collect(),
            updated_node_ids: self.updated_nodes.into_iter().collect(),
            removed_node_ids: self.removed_nodes.into_iter().collect(),
            added_material_ids: self.added_materials.into_iter().collect(),
            updated_material_ids: self.updated_materials.into_iter().collect(),
            removed_material_ids: self.removed_materials.into_iter().collect(),
            added_relation_ids: self.added_relations.into_iter().collect(),
            updated_relation_ids: self.updated_relations.into_iter().collect(),
            removed_relation_ids: self.removed_relations.into_iter().collect(),
            added_constraint_ids: self.added_constraints.into_iter().collect(),
            updated_constraint_ids: self.updated_constraints.into_iter().collect(),
            removed_constraint_ids: self.removed_constraints.into_iter().collect(),
        }
    }
}
