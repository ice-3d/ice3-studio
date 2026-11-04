use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::document::{Constraint, MaterialShading, MaterialSurface, NodeContent, Relation};
use crate::geometry::{Primitive, TriangleMesh};
use crate::ids::{ComponentId, ConstraintId, MaterialId, NodeId, RelationId};
use crate::large_scene::component::{ComponentDefinition, ComponentInstance, InstanceSet};
use crate::large_scene::connection::ConnectionIntent;
use crate::math::Transform;
use crate::patch::{ComponentPatch, ConnectionPatch, InstancePatch, MaterialPatch};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "type", deny_unknown_fields, rename_all_fields = "camelCase")]
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
    ComponentDelete {
        #[serde(alias = "component_id")]
        component_id: ComponentId,
    },
    #[serde(rename = "instance.create")]
    #[ts(rename = "instance.create")]
    InstanceCreate {
        #[serde(alias = "node_id")]
        node_id: NodeId,
        #[serde(alias = "parent_id")]
        parent_id: Option<NodeId>,
        name: String,
        transform: Transform,
        instance: ComponentInstance,
        #[serde(alias = "material_id")]
        material_id: Option<MaterialId>,
        #[serde(default)]
        metadata: BTreeMap<String, String>,
    },
    #[serde(rename = "instance.update")]
    #[ts(rename = "instance.update")]
    InstanceUpdate {
        #[serde(alias = "node_id")]
        node_id: NodeId,
        instance: ComponentInstance,
    },
    #[serde(rename = "instance.delete")]
    #[ts(rename = "instance.delete")]
    InstanceDelete {
        #[serde(alias = "node_id")]
        node_id: NodeId,
    },
    #[serde(rename = "instanceSet.create")]
    #[ts(rename = "instanceSet.create")]
    InstanceSetCreate {
        #[serde(alias = "node_id")]
        node_id: NodeId,
        #[serde(alias = "parent_id")]
        parent_id: Option<NodeId>,
        name: String,
        transform: Transform,
        set: InstanceSet,
        #[serde(alias = "material_id")]
        material_id: Option<MaterialId>,
        #[serde(default)]
        metadata: BTreeMap<String, String>,
    },
    #[serde(rename = "instanceSet.update")]
    #[ts(rename = "instanceSet.update")]
    InstanceSetUpdate {
        #[serde(alias = "node_id")]
        node_id: NodeId,
        set: InstanceSet,
    },
    #[serde(rename = "instanceSet.delete")]
    #[ts(rename = "instanceSet.delete")]
    InstanceSetDelete {
        #[serde(alias = "node_id")]
        node_id: NodeId,
    },
    #[serde(rename = "connection.create")]
    #[ts(rename = "connection.create")]
    ConnectionCreate {
        #[serde(alias = "node_id")]
        node_id: NodeId,
        #[serde(alias = "parent_id")]
        parent_id: Option<NodeId>,
        name: String,
        transform: Transform,
        connection: ConnectionIntent,
        /// 管道材质。按介质分色时由调用方指定，缺省沿用默认。
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            alias = "material_id"
        )]
        material_id: Option<MaterialId>,
        #[serde(default)]
        metadata: BTreeMap<String, String>,
    },
    #[serde(rename = "connection.update")]
    #[ts(rename = "connection.update")]
    ConnectionUpdate {
        #[serde(alias = "node_id")]
        node_id: NodeId,
        connection: ConnectionIntent,
    },
    #[serde(rename = "connection.delete")]
    #[ts(rename = "connection.delete")]
    ConnectionDelete {
        #[serde(alias = "node_id")]
        node_id: NodeId,
    },
    #[serde(rename = "node.create")]
    #[ts(rename = "node.create")]
    NodeCreate {
        #[serde(alias = "node_id")]
        node_id: NodeId,
        #[serde(alias = "parent_id")]
        parent_id: Option<NodeId>,
        name: String,
        transform: Transform,
        content: NodeContent,
        #[serde(alias = "material_id")]
        material_id: Option<MaterialId>,
        #[serde(default)]
        metadata: BTreeMap<String, String>,
    },
    #[serde(rename = "node.update")]
    #[ts(rename = "node.update")]
    NodeUpdate {
        #[serde(alias = "node_id")]
        node_id: NodeId,
        name: Option<String>,
        transform: Option<Transform>,
        visible: Option<bool>,
        metadata: Option<BTreeMap<String, String>>,
    },
    #[serde(rename = "node.reparent")]
    #[ts(rename = "node.reparent")]
    NodeReparent {
        #[serde(alias = "node_id")]
        node_id: NodeId,
        #[serde(alias = "parent_id")]
        parent_id: Option<NodeId>,
    },
    #[serde(rename = "node.delete")]
    #[ts(rename = "node.delete")]
    NodeDelete {
        #[serde(alias = "node_id")]
        node_id: NodeId,
    },
    #[serde(rename = "primitive.update")]
    #[ts(rename = "primitive.update")]
    PrimitiveUpdate {
        #[serde(alias = "node_id")]
        node_id: NodeId,
        primitive: Primitive,
    },
    #[serde(rename = "mesh.replace")]
    #[ts(rename = "mesh.replace")]
    MeshReplace {
        #[serde(alias = "node_id")]
        node_id: NodeId,
        mesh: TriangleMesh,
    },
    #[serde(rename = "material.upsert")]
    #[ts(rename = "material.upsert")]
    MaterialUpsert {
        #[serde(alias = "material_id")]
        material_id: MaterialId,
        name: String,
        #[serde(alias = "base_color")]
        base_color: [f32; 4],
        opacity: f32,
        #[serde(alias = "edge_color")]
        edge_color: Option<[f32; 4]>,
        #[serde(default)]
        shading: Option<MaterialShading>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        surface: Option<MaterialSurface>,
    },
    #[serde(rename = "material.assign")]
    #[ts(rename = "material.assign")]
    MaterialAssign {
        #[serde(alias = "node_id")]
        node_id: NodeId,
        #[serde(alias = "material_id")]
        material_id: Option<MaterialId>,
    },
    #[serde(rename = "relation.create")]
    #[ts(rename = "relation.create")]
    RelationCreate { relation: Relation },
    #[serde(rename = "relation.update")]
    #[ts(rename = "relation.update")]
    RelationUpdate {
        #[serde(alias = "relation_id")]
        relation_id: RelationId,
        #[serde(alias = "relation_type")]
        relation_type: Option<String>,
        #[serde(alias = "from_node_id")]
        from_node_id: Option<NodeId>,
        #[serde(alias = "to_node_id")]
        to_node_id: Option<NodeId>,
        metadata: Option<BTreeMap<String, String>>,
    },
    #[serde(rename = "relation.delete")]
    #[ts(rename = "relation.delete")]
    RelationDelete {
        #[serde(alias = "relation_id")]
        relation_id: RelationId,
    },
    #[serde(rename = "constraint.upsert")]
    #[ts(rename = "constraint.upsert")]
    ConstraintUpsert { constraint: Constraint },
    #[serde(rename = "constraint.delete")]
    #[ts(rename = "constraint.delete")]
    ConstraintDelete {
        #[serde(alias = "constraint_id")]
        constraint_id: ConstraintId,
    },
}
