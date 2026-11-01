//! 快照拆分：组件定义按内容寻址单独存放，快照只保留引用。
//!
//! 一个工程里组件定义往往占快照的七成以上，而组件在版本之间极少变化。
//! 把它们按内容摘要落成独立文件后，同一版本的组件在所有修订间共享，
//! 快照只剩场景、材质与引用。

use std::fs;
use std::path::Path;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use studio_core::document::Document;

use crate::ProjectError;

/// 组件文件所在目录，相对工程根。
pub const COMPONENTS_DIR: &str = "components";

/// 摘要取前 12 位十六进制，足够区分同一组件的不同版本。
const DIGEST_LEN: usize = 12;

pub struct SplitSnapshot {
    /// 拆分后的快照内容。
    pub json: Vec<u8>,
    /// 需要落盘的组件文件：相对路径与内容。
    pub components: Vec<(String, Vec<u8>)>,
}

/// 把文档拆成「快照」与「组件文件」两部分。
pub fn split(document: &Document) -> Result<SplitSnapshot, ProjectError> {
    let mut value = serde_json::to_value(document).map_err(ProjectError::Json)?;
    let mut components = Vec::new();
    let mut references = Vec::new();
    if let Some(entries) = value.get_mut("components").and_then(Value::as_array_mut) {
        for entry in entries.iter() {
            let id = entry
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let body = serde_json::to_vec(entry).map_err(ProjectError::Json)?;
            let digest = format!("{:x}", Sha256::digest(&body));
            let digest = digest[..DIGEST_LEN].to_string();
            references.push(json!({ "id": id, "digest": digest }));
            components.push((format!("{COMPONENTS_DIR}/{id}-{digest}.json"), body));
        }
    }
    value["components"] = Value::Array(references);
    let json = serde_json::to_vec(&value).map_err(ProjectError::Json)?;
    Ok(SplitSnapshot { json, components })
}

/// 从快照与组件目录还原文档。
pub fn merge(snapshot: &[u8], project_dir: &Path) -> Result<Document, ProjectError> {
    let mut value: Value = serde_json::from_slice(snapshot).map_err(ProjectError::Json)?;
    let references = value
        .get("components")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut components = Vec::with_capacity(references.len());
    for reference in &references {
        // 早期快照把定义内联在数组里；没有摘要即视为内联，直接使用。
        if reference.get("digest").is_none() {
            components.push(reference.clone());
            continue;
        }
        let id = reference
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let digest = reference
            .get("digest")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let path = project_dir
            .join(COMPONENTS_DIR)
            .join(format!("{id}-{digest}.json"));
        let body = fs::read(&path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                ProjectError::MissingComponent(format!("{id}-{digest}"))
            } else {
                ProjectError::Io(error)
            }
        })?;
        components.push(serde_json::from_slice(&body).map_err(ProjectError::Json)?);
    }
    value["components"] = Value::Array(components);
    serde_json::from_value(value).map_err(ProjectError::Json)
}

/// 写入组件文件，已存在相同摘要时跳过。
pub fn write_component_files(
    project_dir: &Path,
    files: &[(String, Vec<u8>)],
) -> Result<(), ProjectError> {
    if files.is_empty() {
        return Ok(());
    }
    let directory = project_dir.join(COMPONENTS_DIR);
    fs::create_dir_all(&directory).map_err(ProjectError::Io)?;
    for (relative, body) in files {
        let path = project_dir.join(relative);
        if path.is_file() {
            continue;
        }
        crate::store::write_atomic(&path, body)?;
    }
    Ok(())
}
