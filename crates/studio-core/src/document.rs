use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::PROJECT_SCHEMA_VERSION;
use crate::geometry::{Primitive, TriangleMesh};
use crate::ids::{ConstraintId, MaterialId, NodeId, RelationId, Revision};
use crate::math::Transform;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Document {
    pub schema_version: u32,
    pub revision: Revision,
    pub project_id: String,
    pub scene: Scene,
    pub materials: Vec<Material>,
    pub relations: Vec<Relation>,
    pub constraints: Vec<Constraint>,
    pub metadata: BTreeMap<String, String>,
}

impl Document {
    pub fn new(project_id: impl Into<String>) -> Self {
        Self {
            schema_version: PROJECT_SCHEMA_VERSION,
            revision: Revision::new(0),
            project_id: project_id.into(),
            scene: Scene::default(),
            materials: Vec::new(),
            relations: Vec::new(),
            constraints: Vec::new(),
            metadata: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[ts(export)]
pub struct Scene {
    nodes: Vec<SceneNode>,
}

impl Scene {
    pub fn nodes(&self) -> &[SceneNode] {
        &self.nodes
    }

    pub fn nodes_mut(&mut self) -> &mut Vec<SceneNode> {
        &mut self.nodes
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SceneNode {
    pub id: NodeId,
    pub name: String,
    pub parent_id: Option<NodeId>,
    pub transform: Transform,
    pub material_id: Option<MaterialId>,
    pub content: NodeContent,
    pub metadata: BTreeMap<String, String>,
    pub visible: bool,
}

impl SceneNode {
    pub fn group(id: NodeId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            parent_id: None,
            transform: Transform::IDENTITY,
            material_id: None,
            content: NodeContent::Group,
            metadata: BTreeMap::new(),
            visible: true,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum NodeContent {
    #[default]
    Group,
    Primitive(Primitive),
    CustomMesh(TriangleMesh),
    Sweep {
        points: Vec<[f32; 3]>,
        radius: f32,
        radial_segments: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Material {
    pub id: MaterialId,
    pub name: String,
    pub base_color: [f32; 4],
    pub opacity: f32,
    pub edge_color: Option<[f32; 4]>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Relation {
    pub id: RelationId,
    pub relation_type: String,
    pub from_node_id: NodeId,
    pub to_node_id: NodeId,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Constraint {
    pub id: ConstraintId,
    pub constraint_type: String,
    pub node_ids: Vec<NodeId>,
    pub metadata: BTreeMap<String, String>,
}
