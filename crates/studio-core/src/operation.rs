use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::document::{Constraint, NodeContent, Relation};
use crate::geometry::{Primitive, TriangleMesh};
use crate::ids::{ConstraintId, MaterialId, NodeId, RelationId};
use crate::math::Transform;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "type")]
#[ts(export)]
pub enum Operation {
    #[serde(rename = "node.create")]
    #[ts(rename = "node.create")]
    NodeCreate {
        node_id: NodeId,
        parent_id: Option<NodeId>,
        name: String,
        transform: Transform,
        content: NodeContent,
        material_id: Option<MaterialId>,
        #[serde(default)]
        metadata: BTreeMap<String, String>,
    },
    #[serde(rename = "node.update")]
    #[ts(rename = "node.update")]
    NodeUpdate {
        node_id: NodeId,
        name: Option<String>,
        transform: Option<Transform>,
        visible: Option<bool>,
        metadata: Option<BTreeMap<String, String>>,
    },
    #[serde(rename = "node.reparent")]
    #[ts(rename = "node.reparent")]
    NodeReparent {
        node_id: NodeId,
        parent_id: Option<NodeId>,
    },
    #[serde(rename = "node.delete")]
    #[ts(rename = "node.delete")]
    NodeDelete { node_id: NodeId },
    #[serde(rename = "primitive.update")]
    #[ts(rename = "primitive.update")]
    PrimitiveUpdate {
        node_id: NodeId,
        primitive: Primitive,
    },
    #[serde(rename = "mesh.replace")]
    #[ts(rename = "mesh.replace")]
    MeshReplace { node_id: NodeId, mesh: TriangleMesh },
    #[serde(rename = "material.upsert")]
    #[ts(rename = "material.upsert")]
    MaterialUpsert {
        material_id: MaterialId,
        name: String,
        base_color: [f32; 4],
        opacity: f32,
        edge_color: Option<[f32; 4]>,
    },
    #[serde(rename = "material.assign")]
    #[ts(rename = "material.assign")]
    MaterialAssign {
        node_id: NodeId,
        material_id: Option<MaterialId>,
    },
    #[serde(rename = "relation.create")]
    #[ts(rename = "relation.create")]
    RelationCreate { relation: Relation },
    #[serde(rename = "relation.update")]
    #[ts(rename = "relation.update")]
    RelationUpdate {
        relation_id: RelationId,
        relation_type: Option<String>,
        from_node_id: Option<NodeId>,
        to_node_id: Option<NodeId>,
        metadata: Option<BTreeMap<String, String>>,
    },
    #[serde(rename = "relation.delete")]
    #[ts(rename = "relation.delete")]
    RelationDelete { relation_id: RelationId },
    #[serde(rename = "constraint.upsert")]
    #[ts(rename = "constraint.upsert")]
    ConstraintUpsert { constraint: Constraint },
    #[serde(rename = "constraint.delete")]
    #[ts(rename = "constraint.delete")]
    ConstraintDelete { constraint_id: ConstraintId },
}
