use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum AlphaMode {
    #[default]
    Opaque,
    Mask,
    Blend,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextureTransform {
    pub offset: [f32; 2],
    pub rotation: f32,
    pub scale: [f32; 2],
}

impl Default for TextureTransform {
    fn default() -> Self {
        Self {
            offset: [0.0, 0.0],
            rotation: 0.0,
            scale: [1.0, 1.0],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextureSampler {
    #[serde(default)]
    pub mag_filter: Option<u32>,
    #[serde(default)]
    pub min_filter: Option<u32>,
    #[serde(default = "repeat_wrap")]
    pub wrap_s: u32,
    #[serde(default = "repeat_wrap")]
    pub wrap_t: u32,
}

impl Default for TextureSampler {
    fn default() -> Self {
        Self {
            mag_filter: None,
            min_filter: None,
            wrap_s: repeat_wrap(),
            wrap_t: repeat_wrap(),
        }
    }
}

const fn repeat_wrap() -> u32 {
    10_497
}
