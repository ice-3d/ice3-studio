use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::document::{Constraint, NodeContent, Relation};
use crate::geometry::{Primitive, TriangleMesh};
use crate::ids::{ComponentId, ConstraintId, MaterialId, NodeId, RelationId};
use crate::large_scene::component::{ComponentDefinition, ComponentInstance, InstanceSet};
use crate::large_scene::connection::ConnectionIntent;
use crate::math::Transform;
use crate::patch::{ComponentPatch, ConnectionPatch, InstancePatch, MaterialPatch};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "type", deny_unknown_fields)]
#[ts(export)]
pub enum Operation {
    #[serde(rename = "instance.patch")]
    #[ts(rename = "instance.patch")]
    InstancePatch(InstancePatch),
    #[serde(rename = "material.patch")]
    #[ts(rename = "material.patch")]
    MaterialPatch(MaterialPatch),
    #[serde(rename = "component.patch")]
    #[ts(rename = "component.patch")]
    ComponentPatch(ComponentPatch),
    #[serde(rename = "connection.patch")]
    #[ts(rename = "connection.patch")]
    ConnectionPatch(ConnectionPatch),
    #[serde(rename = "component.define")]
    #[ts(rename = "component.define")]
    ComponentDefine { definition: ComponentDefinition },
    #[serde(rename = "component.update")]
    #[ts(rename = "component.update")]
    ComponentUpdate { definition: ComponentDefinition },
    #[serde(rename = "component.delete")]
    #[ts(rename = "component.delete")]
    ComponentDelete { component_id: ComponentId },
    #[serde(rename = "instance.create")]
    #[ts(rename = "instance.create")]
    InstanceCreate {
        node_id: NodeId,
        parent_id: Option<NodeId>,
        name: String,
        transform: Transform,
        instance: ComponentInstance,
        material_id: Option<MaterialId>,
        #[serde(default)]
        metadata: BTreeMap<String, String>,
    },
    #[serde(rename = "instance.update")]
    #[ts(rename = "instance.update")]
    InstanceUpdate {
        node_id: NodeId,
        instance: ComponentInstance,
    },
    #[serde(rename = "instance.delete")]
    #[ts(rename = "instance.delete")]
    InstanceDelete { node_id: NodeId },
    #[serde(rename = "instanceSet.create")]
    #[ts(rename = "instanceSet.create")]
    InstanceSetCreate {
        node_id: NodeId,
        parent_id: Option<NodeId>,
        name: String,
        transform: Transform,
        set: InstanceSet,
        material_id: Option<MaterialId>,
        #[serde(default)]
        metadata: BTreeMap<String, String>,
    },
    #[serde(rename = "instanceSet.update")]
    #[ts(rename = "instanceSet.update")]
    InstanceSetUpdate { node_id: NodeId, set: InstanceSet },
    #[serde(rename = "instanceSet.delete")]
    #[ts(rename = "instanceSet.delete")]
    InstanceSetDelete { node_id: NodeId },
    #[serde(rename = "connection.create")]
    #[ts(rename = "connection.create")]
    ConnectionCreate {
        node_id: NodeId,
        parent_id: Option<NodeId>,
        name: String,
        transform: Transform,
        connection: ConnectionIntent,
        /// 管道材质。按介质分色时由调用方指定，缺省沿用默认。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        material_id: Option<MaterialId>,
        #[serde(default)]
        metadata: BTreeMap<String, String>,
    },
    #[serde(rename = "connection.update")]
    #[ts(rename = "connection.update")]
    ConnectionUpdate {
        node_id: NodeId,
        connection: ConnectionIntent,
    },
    #[serde(rename = "connection.delete")]
    #[ts(rename = "connection.delete")]
    ConnectionDelete { node_id: NodeId },
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
