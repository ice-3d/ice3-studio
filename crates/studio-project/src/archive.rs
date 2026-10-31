//! `.ice3` 工程容器。
//!
//! 工程本体是一个目录，语义层通常只有几百 KB，几何在 `references/` 下可达数百 MB。
//! 容器把语义层打成一个可传输的单文件，引用资产按需外链。
//!
//! 容器使用未压缩 tar（每块 512 字节）而不是自研二进制格式：结构公开、可用系统
//! 工具直接查看、损坏时能部分恢复，且不需要引入压缩依赖。

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MANIFEST_NAME: &str = "manifest.json";
pub const MAGIC: &str = "ICE3STUDIO";
pub const FORMAT_VERSION: u32 = 1;

/// 打包时跳过的目录：缓存可重建，引用资产体积过大且默认外链。
const EXCLUDED_PREFIXES: [&str; 2] = ["cache/", "references/"];

const BLOCK: usize = 512;

#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    #[error("archive io failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("archive json failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("archive is not an ICE3 container")]
    NotAContainer,
    #[error("unsupported archive format version {0}")]
    UnsupportedVersion(u32),
    #[error("archive is truncated")]
    Truncated,
    #[error("archive entry escapes the target directory: {0}")]
    EscapingEntry(String),
    #[error("project directory is missing project.json")]
    NotAProject,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveManifest {
    pub magic: String,
    pub format_version: u32,
    pub schema_version: u32,
    pub project_id: String,
    pub revision: u64,
    /// 容器内语义层内容的 SHA-256。
    pub digest: String,
    pub created_at: u64,
    /// 打包时排除的目录，解包后需要重新关联。
    pub excluded: Vec<String>,
}

/// 把工程目录打成单文件容器，返回写入的 manifest。
pub fn pack(project_dir: &Path, output: &Path) -> Result<ArchiveManifest, ArchiveError> {
    let project_json = project_dir.join("project.json");
    if !project_json.is_file() {
        return Err(ArchiveError::NotAProject);
    }
    let project: serde_json::Value = serde_json::from_slice(&fs::read(&project_json)?)?;
    let project_id = project
        .get("projectId")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string();
    let schema_version = project
        .get("schemaVersion")
        .and_then(|value| value.as_u64())
        .unwrap_or(0) as u32;
    let revision = project
        .get("currentRevision")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);

    let mut files = collect_files(project_dir)?;
    // 工程摘要与俯视示意图属于派生资源，随容器一起生成，便于不开编辑器时预览。
    if let Some(resources) = build_resources(project_dir, revision) {
        for (name, body) in resources {
            files.insert(name, body);
        }
    }

    let mut hasher = Sha256::new();
    for (relative, body) in &files {
        hasher.update(relative.as_bytes());
        hasher.update(body);
    }
    let digest = format!("{:x}", hasher.finalize());

    let manifest = ArchiveManifest {
        magic: MAGIC.to_string(),
        format_version: FORMAT_VERSION,
        schema_version,
        project_id,
        revision,
        digest,
        created_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_secs())
            .unwrap_or(0),
        excluded: EXCLUDED_PREFIXES
            .iter()
            .map(|prefix| prefix.trim_end_matches('/').to_string())
            .collect(),
    };

    let mut archive = Vec::new();
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
    write_entry(&mut archive, MANIFEST_NAME, &manifest_bytes)?;
    for (relative, body) in &files {
        write_entry(&mut archive, relative, body)?;
    }
    archive.extend(std::iter::repeat_n(0u8, BLOCK * 2));

    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::File::create(output)?;
    file.write_all(&archive)?;
    file.sync_all()?;
    Ok(manifest)
}

/// 从容器还原工程目录，返回容器 manifest。
pub fn unpack(archive: &Path, target: &Path) -> Result<ArchiveManifest, ArchiveError> {
    let bytes = fs::read(archive)?;
    let entries = read_entries(&bytes)?;
    let manifest_bytes = entries
        .get(MANIFEST_NAME)
        .ok_or(ArchiveError::NotAContainer)?;
    let manifest: ArchiveManifest = serde_json::from_slice(manifest_bytes)?;
    if manifest.magic != MAGIC {
        return Err(ArchiveError::NotAContainer);
    }
    if manifest.format_version != FORMAT_VERSION {
        return Err(ArchiveError::UnsupportedVersion(manifest.format_version));
    }

    fs::create_dir_all(target)?;
    for (relative, body) in &entries {
        if relative == MANIFEST_NAME {
            continue;
        }
        let path = safe_join(target, relative)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, body)?;
    }
    Ok(manifest)
}

fn collect_files(root: &Path) -> Result<BTreeMap<String, Vec<u8>>, ArchiveError> {
    let mut files = BTreeMap::new();
    collect_into(root, root, &mut files)?;
    Ok(files)
}

/// 生成 `resources/` 下的派生产物：统计摘要与俯视布局示意图。
/// 两者都是可重建的，不进入工程真相。
fn build_resources(project_dir: &Path, revision: u64) -> Option<BTreeMap<String, Vec<u8>>> {
    let path = project_dir
        .join("revisions")
        .join(format!("{revision:08}.json"));
    let bytes = fs::read(path).ok()?;
    let document: studio_core::document::Document = serde_json::from_slice(&bytes).ok()?;

    let mut areas: BTreeMap<String, usize> = BTreeMap::new();
    for node in document.scene.nodes() {
        let area = node
            .metadata
            .get("area")
            .cloned()
            .unwrap_or_else(|| "unassigned".to_string());
        *areas.entry(area).or_default() += 1;
    }

    let summary = serde_json::json!({
        "projectId": document.project_id,
        "revision": document.revision.as_u64(),
        "schemaVersion": document.schema_version,
        "nodes": document.scene.nodes().len(),
        "materials": document.materials.len(),
        "relations": document.relations.len(),
        "areas": areas,
        "generatedAt": now_seconds(),
    });

    let mut resources = BTreeMap::new();
    resources.insert(
        "resources/summary.json".to_string(),
        serde_json::to_vec_pretty(&summary).ok()?,
    );
    resources.insert(
        "resources/layout.svg".to_string(),
        layout_svg(&document).into_bytes(),
    );
    Some(resources)
}

/// 俯视示意图：每台设备画一个点，按区域着色。用于在不开编辑器的情况下判断布局。
fn layout_svg(document: &studio_core::document::Document) -> String {
    const WIDTH: f32 = 960.0;
    const HEIGHT: f32 = 620.0;
    const PADDING: f32 = 48.0;
    const PALETTE: [&str; 6] = [
        "#4f9cf9", "#f2994a", "#27ae60", "#bb6bd9", "#eb5757", "#56ccf2",
    ];

    let points: Vec<(f32, f32, String)> = document
        .scene
        .nodes()
        .iter()
        .map(|node| {
            (
                node.transform.translation.x,
                node.transform.translation.z,
                node.metadata
                    .get("area")
                    .cloned()
                    .unwrap_or_else(|| "unassigned".to_string()),
            )
        })
        .collect();

    if points.is_empty() {
        return format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{HEIGHT}" viewBox="0 0 {WIDTH} {HEIGHT}"><rect width="100%" height="100%" fill="#14181d"/><text x="24" y="36" fill="#8b949e" font-family="sans-serif" font-size="14">empty project</text></svg>"##
        );
    }

    let min_x = points.iter().fold(f32::MAX, |acc, p| acc.min(p.0));
    let max_x = points.iter().fold(f32::MIN, |acc, p| acc.max(p.0));
    let min_z = points.iter().fold(f32::MAX, |acc, p| acc.min(p.1));
    let max_z = points.iter().fold(f32::MIN, |acc, p| acc.max(p.1));
    let span_x = (max_x - min_x).max(1.0);
    let span_z = (max_z - min_z).max(1.0);
    let scale = ((WIDTH - PADDING * 2.0) / span_x).min((HEIGHT - PADDING * 2.0) / span_z);

    let mut areas: Vec<String> = points.iter().map(|p| p.2.clone()).collect();
    areas.sort();
    areas.dedup();
    let color_of = |area: &str| -> &str {
        let index = areas.iter().position(|value| value == area).unwrap_or(0);
        PALETTE[index % PALETTE.len()]
    };

    let mut body = String::new();
    for (x, z, area) in &points {
        let px = PADDING + (x - min_x) * scale;
        let py = PADDING + (z - min_z) * scale;
        body.push_str(&format!(
            r##"<circle cx="{px:.1}" cy="{py:.1}" r="4" fill="{}" fill-opacity="0.72"/>"##,
            color_of(area)
        ));
    }

    let mut legend = String::new();
    for (index, area) in areas.iter().enumerate().take(12) {
        let x = PADDING + (index % 4) as f32 * 220.0;
        let y = HEIGHT - 40.0 + (index / 4) as f32 * 16.0;
        legend.push_str(&format!(
            r##"<rect x="{x:.0}" y="{y:.0}" width="8" height="8" fill="{}"/><text x="{:.0}" y="{:.0}" fill="#c9d1d9" font-family="sans-serif" font-size="11">{area}</text>"##,
            color_of(area),
            x + 14.0,
            y + 8.0
        ));
    }

    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{HEIGHT}" viewBox="0 0 {WIDTH} {HEIGHT}"><rect width="100%" height="100%" fill="#14181d"/><text x="{PADDING}" y="28" fill="#c9d1d9" font-family="sans-serif" font-size="14">{} · revision {} · {} nodes</text>{body}{legend}</svg>"##,
        document.project_id,
        document.revision.as_u64(),
        document.scene.nodes().len()
    )
}

fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(0)
}

fn collect_into(
    root: &Path,
    directory: &Path,
    files: &mut BTreeMap<String, Vec<u8>>,
) -> Result<(), ArchiveError> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .map_err(|_| ArchiveError::NotAProject)?;
        let relative = relative.to_string_lossy().replace('\\', "/");
        if EXCLUDED_PREFIXES
            .iter()
            .any(|prefix| relative == prefix.trim_end_matches('/') || relative.starts_with(prefix))
        {
            continue;
        }
        if relative == ".lock" {
            continue;
        }
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            collect_into(root, &path, files)?;
        } else if metadata.is_file() {
            files.insert(relative, fs::read(&path)?);
        }
    }
    Ok(())
}

fn safe_join(target: &Path, relative: &str) -> Result<PathBuf, ArchiveError> {
    if relative.is_empty()
        || relative.starts_with('/')
        || relative.split('/').any(|part| part == "..")
    {
        return Err(ArchiveError::EscapingEntry(relative.to_string()));
    }
    Ok(target.join(relative))
}

fn write_entry(archive: &mut Vec<u8>, name: &str, body: &[u8]) -> Result<(), ArchiveError> {
    if name.len() > 100 {
        return Err(ArchiveError::EscapingEntry(name.to_string()));
    }
    let mut header = [0u8; BLOCK];
    header[..name.len()].copy_from_slice(name.as_bytes());
    write_octal(&mut header[100..108], 0o644);
    write_octal(&mut header[108..116], 0);
    write_octal(&mut header[116..124], 0);
    write_octal(&mut header[124..136], body.len() as u64);
    write_octal(&mut header[136..148], 0);
    header[148..156].fill(b' ');
    header[156] = b'0';
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");

    let checksum: u32 = header.iter().map(|byte| u32::from(*byte)).sum();
    write_octal(&mut header[148..156], u64::from(checksum));

    archive.extend_from_slice(&header);
    archive.extend_from_slice(body);
    let padding = (BLOCK - body.len() % BLOCK) % BLOCK;
    archive.extend(std::iter::repeat_n(0u8, padding));
    Ok(())
}

fn write_octal(field: &mut [u8], value: u64) {
    let digits = field.len() - 1;
    let text = format!("{:0>width$o}", value, width = digits);
    let bytes = text.as_bytes();
    let take = bytes.len().min(digits);
    field[..digits].fill(b'0');
    field[digits - take..digits].copy_from_slice(&bytes[bytes.len() - take..]);
    field[digits] = 0;
}

fn read_entries(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, ArchiveError> {
    let mut entries = BTreeMap::new();
    let mut offset = 0usize;
    while offset + BLOCK <= bytes.len() {
        let header = &bytes[offset..offset + BLOCK];
        if header.iter().all(|byte| *byte == 0) {
            break;
        }
        let name_end = header[..100]
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(100);
        let name = String::from_utf8_lossy(&header[..name_end]).to_string();
        let size = read_octal(&header[124..136]);
        let start = offset + BLOCK;
        let end = start
            .checked_add(size as usize)
            .ok_or(ArchiveError::Truncated)?;
        if end > bytes.len() {
            return Err(ArchiveError::Truncated);
        }
        entries.insert(name, bytes[start..end].to_vec());
        let padding = (BLOCK - (size as usize) % BLOCK) % BLOCK;
        offset = end + padding;
    }
    Ok(entries)
}

fn read_octal(field: &[u8]) -> u64 {
    let text = String::from_utf8_lossy(field);
    let trimmed = text.trim_matches(|character: char| character == '\0' || character == ' ');
    u64::from_str_radix(trimmed, 8).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_and_restores_a_project_directory() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("demo");
        fs::create_dir_all(project.join("revisions")).unwrap();
        fs::create_dir_all(project.join("history")).unwrap();
        fs::create_dir_all(project.join("cache/geometry")).unwrap();
        fs::write(
            project.join("project.json"),
            r##"{"schemaVersion":2,"projectId":"demo","currentRevision":3}"##,
        )
        .unwrap();
        fs::write(project.join("revisions/00000003.json"), "{}").unwrap();
        fs::write(project.join("history/operations.jsonl"), "{}\n").unwrap();
        // 缓存必须被排除。
        fs::write(project.join("cache/geometry/x.mesh"), "cache").unwrap();

        let archive = root.path().join("demo.ice3");
        let manifest = pack(&project, &archive).unwrap();
        assert_eq!(manifest.project_id, "demo");
        assert_eq!(manifest.revision, 3);
        assert!(archive.is_file());

        let restored = root.path().join("restored");
        let read_back = unpack(&archive, &restored).unwrap();
        assert_eq!(read_back.digest, manifest.digest);
        assert!(restored.join("project.json").is_file());
        assert!(restored.join("revisions/00000003.json").is_file());
        assert!(!restored.join("cache/geometry/x.mesh").exists());
    }

    #[test]
    fn rejects_entries_that_escape_the_target() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("out");
        assert!(safe_join(&target, "../escape").is_err());
        assert!(safe_join(&target, "/absolute").is_err());
        assert!(safe_join(&target, "safe/file.json").is_ok());
    }

    #[test]
    fn rejects_a_file_that_is_not_a_container() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("bad.ice3");
        fs::write(&path, b"not an archive").unwrap();
        assert!(matches!(
            unpack(&path, &root.path().join("out")),
            Err(ArchiveError::NotAContainer)
        ));
    }
}
