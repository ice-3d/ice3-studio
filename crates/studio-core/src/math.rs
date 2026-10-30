use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[ts(export)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[ts(export)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[ts(export)]
pub struct Transform {
    pub translation: Vec3,
    pub rotation: [f32; 4],
    pub scale: Vec3,
}

impl Transform {
    pub const IDENTITY: Self = Self {
        translation: Vec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale: Vec3 {
            x: 1.0,
            y: 1.0,
            z: 1.0,
        },
    };
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}
