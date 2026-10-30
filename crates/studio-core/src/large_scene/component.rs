use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::geometry::TriangleMesh;
use crate::ids::{ComponentId, MaterialId};
use crate::math::{Transform, Vec3};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ComponentDefinition {
    pub id: ComponentId,
    pub name: String,
    pub parameters: Vec<ComponentParameter>,
    pub parts: Vec<ComponentPart>,
    pub ports: Vec<ComponentPort>,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ComponentParameter {
    pub name: String,
    pub value_type: ParameterType,
    pub default: ParameterValue,
    pub unit: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum ParameterType {
    Number {
        minimum: Option<f32>,
        maximum: Option<f32>,
    },
    Vector3,
    Color,
    Enumeration {
        values: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
#[ts(export)]
pub enum ParameterValue {
    Number(f32),
    Vector3(Vec3),
    Color([f32; 4]),
    Enumeration(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ComponentPart {
    pub id: String,
    pub parent_id: Option<String>,
    pub transform: PartTransform,
    pub content: PartContent,
    pub material_role: Option<MaterialBinding>,
    pub metadata: BTreeMap<String, String>,
    pub visible: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct PartTransform {
    pub translation: Vector3Binding,
    pub rotation: [f32; 4],
    pub scale: Vector3Binding,
}

impl Default for PartTransform {
    fn default() -> Self {
        Self {
            translation: Vector3Binding::Literal(Vec3::default()),
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: Vector3Binding::Literal(Vec3 {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum PartContent {
    Group,
    Box {
        width: ScalarBinding,
        height: ScalarBinding,
        depth: ScalarBinding,
    },
    Plane {
        width: ScalarBinding,
        depth: ScalarBinding,
    },
    Cylinder {
        radius: ScalarBinding,
        height: ScalarBinding,
        segments: u32,
    },
    Sphere {
        radius: ScalarBinding,
        segments: u32,
        rings: u32,
    },
    CustomMesh {
        mesh: TriangleMesh,
    },
    Sweep {
        points: Vec<[f32; 3]>,
        radius: ScalarBinding,
        radial_segments: u32,
    },
    ComponentReference {
        definition_id: ComponentId,
        parameters: BTreeMap<String, ParameterValue>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
#[ts(export)]
pub enum ScalarBinding {
    Literal(f32),
    Parameter(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
#[ts(export)]
pub enum Vector3Binding {
    Literal(Vec3),
    Parameter(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
#[ts(export)]
pub enum MaterialBinding {
    Literal(String),
    Parameter(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ComponentPort {
    pub name: String,
    pub part_id: String,
    pub position: Vec3,
    pub direction: Vec3,
    pub medium: String,
    pub diameter: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ComponentInstance {
    pub definition_id: ComponentId,
    pub parameters: BTreeMap<String, ParameterValue>,
    pub overrides: BTreeMap<String, PartOverride>,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct PartOverride {
    pub material_id: Option<MaterialId>,
    pub visible: Option<bool>,
    pub metadata: Option<BTreeMap<String, String>>,
    pub parameters: BTreeMap<String, ParameterValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum InstanceLayout {
    Linear {
        origin: Vec3,
        direction: Vec3,
        spacing: f32,
        count: u32,
    },
    Grid {
        origin: Vec3,
        x_direction: Vec3,
        z_direction: Vec3,
        rows: u32,
        columns: u32,
        row_spacing: f32,
        column_spacing: f32,
    },
    Radial {
        centre: Vec3,
        radius: f32,
        start_angle_radians: f32,
        count: u32,
    },
    Path {
        placements: Vec<Transform>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct InstanceSet {
    pub definition_id: ComponentId,
    pub layout: InstanceLayout,
    pub shared_parameters: BTreeMap<String, ParameterValue>,
    pub item_overrides: BTreeMap<u32, BTreeMap<String, ParameterValue>>,
    pub metadata: BTreeMap<String, String>,
}
