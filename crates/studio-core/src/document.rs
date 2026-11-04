use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::PROJECT_SCHEMA_VERSION;
use crate::geometry::{Primitive, TriangleMesh};
use crate::ids::{ConstraintId, MaterialId, NodeId, RelationId, Revision};
use crate::large_scene::component::{ComponentDefinition, ComponentInstance, InstanceSet};
use crate::large_scene::connection::ConnectionIntent;
use crate::math::Transform;
use crate::reference_asset::ReferenceInstanceContent;

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
    #[serde(default)]
    pub components: Vec<ComponentDefinition>,
    pub metadata: BTreeMap<String, String>,
    pub idempotency_keys: Vec<crate::apply::IdempotencyRecord>,
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
            components: Vec::new(),
            metadata: BTreeMap::new(),
            idempotency_keys: Vec::new(),
        }
    }

    pub fn idempotency_record(&self, key: &str) -> Option<&crate::apply::IdempotencyRecord> {
        self.idempotency_keys
            .iter()
            .find(|record| record.key == key)
    }

    pub fn migrate(mut self) -> Result<Self, DocumentSchemaError> {
        match self.schema_version {
            1 | 2 => {
                self.schema_version = PROJECT_SCHEMA_VERSION;
                Ok(self)
            }
            version => Err(DocumentSchemaError::Unsupported(version)),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DocumentSchemaError {
    #[error("unsupported document schema version {0}")]
    Unsupported(u32),
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
    ComponentInstance(ComponentInstance),
    InstanceSet(InstanceSet),
    Connection(ConnectionIntent),
    #[serde(rename = "reference_instance")]
    ReferenceInstance(ReferenceInstanceContent),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum MaterialShading {
    Directional,
    Flat,
    Water,
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
    #[serde(default)]
    pub shading: Option<MaterialShading>,
    /// Physical surface description (metallic/roughness/emissive/texture). `None`
    /// means "use the built-in dielectric defaults", which keeps projects that
    /// never cared about PBR byte-identical.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface: Option<MaterialSurface>,
}

/// Physically based surface parameters, aligned with glTF's metallic-roughness
/// model so an export needs no interpretation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct MaterialSurface {
    /// 0 = dielectric, 1 = metal.
    #[serde(default)]
    pub metallic: f32,
    /// 0 = mirror, 1 = fully diffuse.
    #[serde(default = "default_roughness")]
    pub roughness: f32,
    /// Linear RGB emitted by the surface, independent of lighting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emissive: Option<[f32; 3]>,
    /// Texture tiling in UV units per texture repeat; UVs are metres, so this is
    /// "metres per tile" inverted.
    #[serde(default = "unit_uv_scale")]
    pub uv_scale: [f32; 2],
    /// Relative path of the base-colour image, resolved against the glTF file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_color_texture: Option<String>,
}

impl MaterialSurface {
    /// Dielectric, semi-rough, untiled — what a material without a `surface`.
    pub fn dielectric() -> Self {
        Self {
            metallic: 0.0,
            roughness: default_roughness(),
            emissive: None,
            uv_scale: [1.0, 1.0],
            base_color_texture: None,
        }
    }
}

fn default_roughness() -> f32 {
    0.9
}

fn unit_uv_scale() -> [f32; 2] {
    [1.0, 1.0]
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
