use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceFormat {
    Gltf,
    Obj,
    Fbx,
    Usd,
}

impl SourceFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Gltf => "gltf",
            Self::Obj => "obj",
            Self::Fbx => "fbx",
            Self::Usd => "usd",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceMaterial {
    pub id: String,
    pub name: String,
    pub base_color: [f32; 3],
    pub opacity: f32,
    #[serde(with = "json_value")]
    pub extras: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourcePrimitive {
    pub positions: Vec<[f32; 3]>,
    pub normals: Option<Vec<[f32; 3]>>,
    pub colors: Option<Vec<[f32; 3]>>,
    pub indices: Vec<u32>,
    pub material: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceMesh {
    pub id: String,
    pub primitives: Vec<SourcePrimitive>,
    #[serde(with = "json_value")]
    pub extras: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceNode {
    pub id: String,
    pub name: String,
    pub parent: Option<String>,
    pub matrix: [f32; 16],
    pub mesh: Option<usize>,
    pub material: Option<usize>,
    #[serde(with = "json_value")]
    pub extras: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportWarning {
    pub code: String,
    pub message: String,
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceScene {
    pub asset_id: String,
    pub source_format: SourceFormat,
    pub nodes: Vec<SourceNode>,
    pub meshes: Vec<SourceMesh>,
    pub materials: Vec<SourceMaterial>,
    pub warnings: Vec<ImportWarning>,
    #[serde(with = "json_value")]
    pub asset_extras: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceCapabilities {
    pub hierarchy: bool,
    pub vertex_colors: bool,
    pub material_colors: bool,
    pub instances: bool,
    pub semantic_metadata: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportRequest {
    pub path: PathBuf,
    pub asset_id: String,
    pub requested_adapter: Option<String>,
}

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("unsupported source: {0}")]
    Unsupported(String),
    #[error("source adapter is unavailable: {0}")]
    AdapterUnavailable(String),
    #[error("source importer registry is invalid: {0}")]
    RegistryInvalid(String),
    #[error("source is invalid: {0}")]
    InvalidSource(String),
    #[error("source contains no geometry")]
    EmptyGeometry,
    #[error("source resource was ignored: {0}")]
    ResourceIgnored(String),
    #[error("source import limit exceeded: {0}")]
    LimitExceeded(String),
    #[error("source import worker timed out")]
    WorkerTimeout,
    #[error("source import worker protocol is invalid: {0}")]
    WorkerProtocolInvalid(String),
    #[error("{message}")]
    WorkerReported { code: String, message: String },
    #[error("source import worker failed: {0}")]
    WorkerFailed(String),
    #[error("source import worker output exceeded its limit: {0}")]
    WorkerOutputTooLarge(String),
    #[error("source I/O failed")]
    Io(#[from] std::io::Error),
}

impl ImportError {
    pub fn code(&self) -> &str {
        match self {
            Self::Unsupported(_) => "ICE3_STUDIO_IMPORT_UNSUPPORTED",
            Self::AdapterUnavailable(_) => "ICE3_STUDIO_IMPORT_ADAPTER_UNAVAILABLE",
            Self::RegistryInvalid(_) => "ICE3_STUDIO_IMPORT_REGISTRY_INVALID",
            Self::InvalidSource(_) => "ICE3_STUDIO_IMPORT_INVALID_SOURCE",
            Self::EmptyGeometry => "ICE3_STUDIO_IMPORT_EMPTY_GEOMETRY",
            Self::ResourceIgnored(_) => "ICE3_STUDIO_IMPORT_RESOURCE_IGNORED",
            Self::LimitExceeded(_) => "ICE3_STUDIO_IMPORT_LIMIT_EXCEEDED",
            Self::WorkerTimeout => "ICE3_STUDIO_IMPORT_TIMEOUT",
            Self::WorkerProtocolInvalid(_) => "ICE3_STUDIO_IMPORT_WORKER_PROTOCOL_INVALID",
            Self::WorkerReported { code, .. } => code,
            Self::WorkerFailed(_) => "ICE3_STUDIO_IMPORT_WORKER_FAILED",
            Self::WorkerOutputTooLarge(_) => "ICE3_STUDIO_IMPORT_WORKER_OUTPUT_TOO_LARGE",
            Self::Io(_) => "ICE3_STUDIO_IMPORT_IO",
        }
    }
}

pub trait SourceImporter: Send + Sync {
    fn id(&self) -> &'static str;
    fn extensions(&self) -> &'static [&'static str];
    fn capabilities(&self) -> SourceCapabilities;
    fn import(&self, request: &ImportRequest) -> Result<SourceScene, ImportError>;
}

mod json_value {
    use serde::{Deserialize, Deserializer, Serializer};
    use serde_json::Value;

    pub fn serialize<S>(value: &Value, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let encoded = serde_json::to_string(value).map_err(serde::ser::Error::custom)?;
        serializer.serialize_str(&encoded)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        serde_json::from_str(&encoded).map_err(serde::de::Error::custom)
    }
}
