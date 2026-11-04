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

/// 参数取值。字面量直接写值，只有引用参数时才需要包装对象。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(untagged)]
#[ts(export)]
pub enum ParameterValue {
    Number(f32),
    Vector3(Vec3),
    Color([f32; 4]),
    Enumeration(String),
}

/// 部件字段：字面量就是普通数字，引用参数写 `{ "param": "name" }`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(untagged)]
#[ts(export)]
pub enum ScalarBinding {
    Literal(f32),
    Parameter {
        param: String,
    },
    /// 参数派生量，例如 `radius - wall / 2`。可用变量是该实例的参数集合。
    Expression {
        expr: String,
    },
}

/// 向量字段：字面量写 `{ "x": 1, "y": 2, "z": 3 }`，引用参数写 `{ "param": "name" }`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(untagged)]
#[ts(export)]
pub enum Vector3Binding {
    Literal(Vec3),
    Parameter {
        param: String,
    },
    /// 逐分量给出，用于按索引算出的环形/线性布局。
    Components {
        x: ScalarBinding,
        y: ScalarBinding,
        z: ScalarBinding,
    },
}

impl Vector3Binding {
    fn is_zero(&self) -> bool {
        matches!(self, Self::Literal(value) if value.x == 0.0 && value.y == 0.0 && value.z == 0.0)
    }

    fn is_one(&self) -> bool {
        matches!(self, Self::Literal(value) if value.x == 1.0 && value.y == 1.0 && value.z == 1.0)
    }
}

/// 材质引用：字面量就是材质 id，引用参数写 `{ "param": "name" }`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(untagged)]
#[ts(export)]
pub enum MaterialBinding {
    Literal(String),
    Parameter { param: String },
}

fn unit_rotation() -> [f32; 4] {
    [0.0, 0.0, 0.0, 1.0]
}

fn is_unit_rotation(rotation: &[f32; 4]) -> bool {
    *rotation == unit_rotation()
}

fn is_true(value: &bool) -> bool {
    *value
}

/// 一次长方体挖除：中心 + 尺寸，相对部件自身坐标系。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct BoxCut {
    pub width: ScalarBinding,
    pub height: ScalarBinding,
    pub depth: ScalarBinding,
    #[serde(default = "zero_scalar")]
    pub x: ScalarBinding,
    #[serde(default = "zero_scalar")]
    pub y: ScalarBinding,
    #[serde(default = "zero_scalar")]
    pub z: ScalarBinding,
}

fn zero_scalar() -> ScalarBinding {
    ScalarBinding::Literal(0.0)
}

/// 从部件几何中减去的实体。使用 untagged 表示，已有的盒体挖除（无 `kind`
/// 字段）继续按 `Box` 解析。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(untagged)]
#[ts(export)]
pub enum PartCut {
    Box(BoxCut),
    Cylinder(CylinderCut),
    Sphere(SphereCut),
}

/// 圆柱挖除，轴向默认 `z`（穿过拉伸板/墙的孔）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct CylinderCut {
    pub radius: ScalarBinding,
    pub height: ScalarBinding,
    #[serde(default = "zero_scalar")]
    pub x: ScalarBinding,
    #[serde(default = "zero_scalar")]
    pub y: ScalarBinding,
    #[serde(default = "zero_scalar")]
    pub z: ScalarBinding,
    #[serde(default = "default_cut_segments")]
    pub segments: u32,
    /// 圆柱轴线方向：`x`、`y` 或 `z`。
    #[serde(default = "default_cut_axis")]
    pub axis: String,
}

/// 球形挖除。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct SphereCut {
    pub radius: ScalarBinding,
    #[serde(default = "zero_scalar")]
    pub x: ScalarBinding,
    #[serde(default = "zero_scalar")]
    pub y: ScalarBinding,
    #[serde(default = "zero_scalar")]
    pub z: ScalarBinding,
    #[serde(default = "default_sphere_segments")]
    pub segments: u32,
    #[serde(default = "default_sphere_rings")]
    pub rings: u32,
}

fn default_cut_segments() -> u32 {
    32
}

fn default_cut_axis() -> String {
    "z".to_string()
}

fn default_sphere_segments() -> u32 {
    24
}

fn default_sphere_rings() -> u32 {
    12
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ComponentPart {
    pub id: String,
    /// 默认值不落盘：工程文件里只出现真正携带信息的字段。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(default, skip_serializing_if = "PartTransform::is_identity")]
    pub transform: PartTransform,
    pub content: PartContent,
    /// 从部件几何中挖去的实体（相对部件中心）。支持盒、圆柱、球，
    /// 墙上的门窗洞、管道穿墙口、设备开孔都属于这一类。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subtract: Vec<PartCut>,
    /// 重复次数。求值时展开成 count 个部件，暴露 `i`（0..count-1）与 `count`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeat: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material_role: Option<MaterialBinding>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
    #[serde(default = "default_visible", skip_serializing_if = "is_true")]
    pub visible: bool,
}

fn default_visible() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct PartTransform {
    #[serde(
        default = "zero_vector",
        skip_serializing_if = "Vector3Binding::is_zero"
    )]
    pub translation: Vector3Binding,
    #[serde(default = "unit_rotation", skip_serializing_if = "is_unit_rotation")]
    pub rotation: [f32; 4],
    /// 绕 Y 轴的偏航角（度）。与 `rotation` 二选一，用它更容易表达环形朝向。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yaw: Option<ScalarBinding>,
    #[serde(default = "one_vector", skip_serializing_if = "Vector3Binding::is_one")]
    pub scale: Vector3Binding,
}

fn default_rounding_segments() -> u32 {
    4
}

fn zero_vector() -> Vector3Binding {
    Vector3Binding::Literal(Vec3::default())
}

fn one_vector() -> Vector3Binding {
    Vector3Binding::Literal(Vec3 {
        x: 1.0,
        y: 1.0,
        z: 1.0,
    })
}

impl PartTransform {
    fn is_identity(&self) -> bool {
        self.translation.is_zero()
            && is_unit_rotation(&self.rotation)
            && self.yaw.is_none()
            && self.scale.is_one()
    }
}

impl Default for PartTransform {
    fn default() -> Self {
        Self {
            translation: zero_vector(),
            rotation: unit_rotation(),
            yaw: None,
            scale: one_vector(),
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
    /// A closed 2-D profile in XY extruded along +Z (sketch → prism).
    Extrude {
        profile: Vec<[f32; 2]>,
        height: ScalarBinding,
    },
    /// A box whose 12 edges are rounded with `radius`, or bevelled when
    /// `segments` is 1.
    RoundedBox {
        width: ScalarBinding,
        height: ScalarBinding,
        depth: ScalarBinding,
        radius: ScalarBinding,
        #[serde(default = "default_rounding_segments")]
        segments: u32,
    },
    ComponentReference {
        definition_id: ComponentId,
        parameters: BTreeMap<String, ParameterValue>,
    },
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
