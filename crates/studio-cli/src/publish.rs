use std::fmt;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use studio_assets::{PreparedAssetBundle, PreparedResource};

#[derive(Debug)]
pub enum PublishError {
    OutputExists,
    InvalidResource(String),
    ReportWrite(String),
    Io(std::io::Error),
    RollbackFailed {
        primary: Box<PublishError>,
        details: Vec<String>,
    },
}

impl PublishError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::OutputExists => "ICE3_STUDIO_ASSET_OUTPUT_EXISTS",
            Self::InvalidResource(_) => "ICE3_STUDIO_ASSET_INVALID_OPTIONS",
            Self::ReportWrite(_) => "ICE3_STUDIO_ASSET_REPORT_WRITE_FAILED",
            Self::Io(_) | Self::RollbackFailed { .. } => "ICE3_STUDIO_ASSET_IO",
        }
    }
}

impl fmt::Display for PublishError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutputExists => formatter.write_str("output already exists"),
            Self::InvalidResource(message) => formatter.write_str(message),
            Self::ReportWrite(message) => formatter.write_str(message),
            Self::Io(error) => write!(formatter, "{error}"),
            Self::RollbackFailed { primary, details } => {
                write!(
                    formatter,
                    "{primary}; rollback also failed: {}",
                    details.join("; ")
                )
            }
        }
    }
}

impl std::error::Error for PublishError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::RollbackFailed { primary, .. } => Some(primary.as_ref()),
            _ => None,
        }
    }
}

impl From<std::io::Error> for PublishError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

pub fn texture_namespace(output: &Path) -> Result<String, PublishError> {
    let stem = output
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| PublishError::InvalidResource("output has no file stem".into()))?;
    Ok(format!("{stem}.textures"))
}

#[derive(Debug, Default)]
struct PublicationBackups {
    output: Option<PathBuf>,
    textures: Option<PathBuf>,
    report: Option<PathBuf>,
}

pub fn publish_asset_bundle(
    bundle: &PreparedAssetBundle,
    output: &Path,
    report: Option<(&Path, &[u8])>,
    force: bool,
) -> Result<(), PublishError> {
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let texture_dir = output.with_file_name(texture_namespace(output)?);
    if report.is_some_and(|(path, _)| path == output || path == texture_dir) {
        return Err(PublishError::InvalidResource(
            "report path must differ from asset outputs".into(),
        ));
    }
    if let Some((path, _)) = report
        && let Some(parent) = path.parent()
        && path_entry_exists(parent)
        && !parent.is_dir()
    {
        return Err(PublishError::ReportWrite(
            "report parent is not a directory".into(),
        ));
    }
    if !force
        && (path_entry_exists(output)
            || path_entry_exists(&texture_dir)
            || report.is_some_and(|(path, _)| path_entry_exists(path)))
    {
        return Err(PublishError::OutputExists);
    }

    let staging = tempfile::Builder::new()
        .prefix(".ice3-studio-asset-staging-")
        .tempdir_in(parent)?;
    let staged_textures = if bundle.resources.is_empty() {
        None
    } else {
        let path = staging.path().join(&bundle.texture_uri_prefix);
        std::fs::create_dir(&path)?;
        for resource in &bundle.resources {
            write_staged_resource(&path, &bundle.texture_uri_prefix, resource)?;
        }
        Some(path)
    };
    let staged_glb = staging.path().join("asset.glb");
    let mut glb = std::fs::File::create(&staged_glb)?;
    glb.write_all(&bundle.glb)?;
    glb.sync_all()?;
    let staged_report = match report {
        Some((_, bytes)) => {
            let path = staging.path().join("asset.report.json");
            let mut file = std::fs::File::create(&path)?;
            file.write_all(bytes)?;
            file.sync_all()?;
            Some(path)
        }
        None => None,
    };

    publish_staged_with(
        staged_textures.as_deref(),
        &texture_dir,
        &staged_glb,
        output,
        report
            .zip(staged_report.as_deref())
            .map(|((target, _), staged)| (staged, target)),
        |staged, target| {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::rename(staged, target)
        },
    )?;
    Ok(())
}

fn write_staged_resource(
    root: &Path,
    expected_namespace: &str,
    resource: &PreparedResource,
) -> Result<(), PublishError> {
    let prefix = format!("{expected_namespace}/");
    let relative = resource.uri.strip_prefix(&prefix).ok_or_else(|| {
        PublishError::InvalidResource("texture URI has the wrong namespace".into())
    })?;
    let relative = Path::new(relative);
    if relative.as_os_str().is_empty()
        || relative.file_name().is_none()
        || relative.components().any(|component| {
            !matches!(component, Component::Normal(_)) || component.as_os_str().is_empty()
        })
    {
        return Err(PublishError::InvalidResource(
            "texture URI escapes its output".into(),
        ));
    }
    if format!("{:x}", Sha256::digest(&resource.bytes)) != resource.digest {
        return Err(PublishError::InvalidResource(
            "texture digest does not match its bytes".into(),
        ));
    }
    let target = root.join(relative);
    if target.exists() {
        if std::fs::read(&target)? != resource.bytes {
            return Err(PublishError::InvalidResource(
                "texture URI has conflicting bytes".into(),
            ));
        }
        return Ok(());
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::File::create(target)?;
    file.write_all(&resource.bytes)?;
    file.sync_all()?;
    Ok(())
}

fn publish_staged_with(
    staged_textures: Option<&Path>,
    texture_dir: &Path,
    staged_glb: &Path,
    output: &Path,
    staged_report: Option<(&Path, &Path)>,
    publish_report: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
) -> Result<(), PublishError> {
    let backups = backup_publication_paths(
        output,
        texture_dir,
        staged_report.as_ref().map(|(_, target)| *target),
    )?;
    let result = (|| {
        if let Some(staged) = staged_textures {
            std::fs::rename(staged, texture_dir).map_err(PublishError::Io)?;
        }
        std::fs::rename(staged_glb, output).map_err(PublishError::Io)?;
        if let Some((staged, target)) = staged_report {
            publish_report(staged, target)
                .map_err(|error| PublishError::ReportWrite(error.to_string()))?;
        }
        Ok::<(), PublishError>(())
    })();

    if let Err(primary) = result {
        let mut details = Vec::new();
        restore_published_path(output, backups.output.as_deref(), &mut details);
        restore_published_path(texture_dir, backups.textures.as_deref(), &mut details);
        if let Some((_, report)) = staged_report {
            restore_published_path(report, backups.report.as_deref(), &mut details);
        }
        if details.is_empty() {
            return Err(primary);
        }
        return Err(PublishError::RollbackFailed {
            primary: Box::new(primary),
            details,
        });
    }

    remove_optional_backup(output, backups.output.as_deref())?;
    remove_optional_backup(texture_dir, backups.textures.as_deref())?;
    if let Some((_, report)) = staged_report {
        remove_optional_backup(report, backups.report.as_deref())?;
    }
    Ok(())
}

fn backup_publication_paths(
    output: &Path,
    textures: &Path,
    report: Option<&Path>,
) -> Result<PublicationBackups, PublishError> {
    let mut backups = PublicationBackups::default();
    for (kind, path) in [
        ("textures", Some(textures)),
        ("output", Some(output)),
        ("report", report),
    ] {
        let Some(path) = path else {
            continue;
        };
        match move_optional_path_to_backup(path) {
            Ok(backup) => match kind {
                "textures" => backups.textures = backup,
                "output" => backups.output = backup,
                "report" => backups.report = backup,
                _ => unreachable!(),
            },
            Err(error) => {
                let mut details = Vec::new();
                if backups.textures.is_some() {
                    restore_published_path(textures, backups.textures.as_deref(), &mut details);
                }
                if backups.output.is_some() {
                    restore_published_path(output, backups.output.as_deref(), &mut details);
                }
                if backups.report.is_some()
                    && let Some(report) = report
                {
                    restore_published_path(report, backups.report.as_deref(), &mut details);
                }
                if details.is_empty() {
                    return Err(PublishError::Io(error));
                }
                return Err(PublishError::RollbackFailed {
                    primary: Box::new(PublishError::Io(error)),
                    details,
                });
            }
        }
    }
    Ok(backups)
}

fn move_optional_path_to_backup(path: &Path) -> Result<Option<PathBuf>, std::io::Error> {
    if !path_entry_exists(path) {
        return Ok(None);
    }
    let backup = unique_backup_path(path);
    std::fs::rename(path, &backup)?;
    Ok(Some(backup))
}

fn restore_published_path(path: &Path, backup: Option<&Path>, details: &mut Vec<String>) {
    if path.exists() {
        remove_path(path, details);
    }
    if let Some(backup) = backup
        && path.symlink_metadata().is_err()
        && let Err(error) = std::fs::rename(backup, path)
    {
        details.push(format!(
            "cannot restore {} from {}: {error}",
            path.display(),
            backup.display()
        ));
    }
}

fn remove_optional_backup(path: &Path, backup: Option<&Path>) -> Result<(), PublishError> {
    if let Some(backup) = backup {
        let mut details = Vec::new();
        remove_path(backup, &mut details);
        if let Some(detail) = details.into_iter().next() {
            return Err(PublishError::InvalidResource(detail));
        }
    }
    let _ = path;
    Ok(())
}

fn remove_path(path: &Path, details: &mut Vec<String>) {
    let result = match path.symlink_metadata() {
        Ok(metadata) if metadata.is_dir() => std::fs::remove_dir_all(path),
        Ok(_) => std::fs::remove_file(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    };
    if let Err(error) = result
        && error.kind() != std::io::ErrorKind::NotFound
    {
        details.push(format!("cannot remove {}: {error}", path.display()));
    }
}

fn path_entry_exists(path: &Path) -> bool {
    path.symlink_metadata().is_ok()
}

fn unique_backup_path(path: &Path) -> PathBuf {
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("asset");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    parent.join(format!(
        ".{name}.{}.{nonce}.asset-backup",
        std::process::id()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_report_publish_restores_glb_and_texture_directory() {
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("plant.glb");
        let textures = root.path().join("plant.textures");
        let report = root.path().join("plant.report.json");
        let stage = tempfile::tempdir_in(root.path()).unwrap();
        let staged_glb = stage.path().join("plant.glb");
        let staged_textures = stage.path().join("plant.textures");
        let staged_report = stage.path().join("plant.report.json");
        std::fs::write(&output, "old-glb").unwrap();
        std::fs::create_dir(&textures).unwrap();
        std::fs::write(textures.join("old.png"), "old-texture").unwrap();
        std::fs::write(&report, "old-report").unwrap();
        std::fs::write(&staged_glb, "new-glb").unwrap();
        std::fs::create_dir(&staged_textures).unwrap();
        std::fs::write(staged_textures.join("new.png"), "new-texture").unwrap();
        std::fs::write(&staged_report, "new-report").unwrap();

        let result = publish_staged_with(
            Some(&staged_textures),
            &textures,
            &staged_glb,
            &output,
            Some((&staged_report, &report)),
            |_, _| Err(std::io::Error::other("injected report failure")),
        );

        assert!(result.is_err());
        assert_eq!(std::fs::read_to_string(&output).unwrap(), "old-glb");
        assert_eq!(
            std::fs::read_to_string(textures.join("old.png")).unwrap(),
            "old-texture"
        );
        assert_eq!(std::fs::read_to_string(&report).unwrap(), "old-report");
    }

    #[test]
    fn staged_resources_reject_namespace_escape_and_digest_mismatch() {
        let root = tempfile::tempdir().unwrap();
        let escaped = PreparedResource {
            uri: "other/../outside.png".into(),
            digest: "a".repeat(64),
            media_type: "image/png".into(),
            bytes: vec![1],
        };
        let error = write_staged_resource(root.path(), "other", &escaped).unwrap_err();
        assert_eq!(error.code(), "ICE3_STUDIO_ASSET_INVALID_OPTIONS");

        let wrong_digest = PreparedResource {
            uri: "plant.textures/image.png".into(),
            digest: "a".repeat(64),
            media_type: "image/png".into(),
            bytes: vec![1],
        };
        let error =
            write_staged_resource(root.path(), "plant.textures", &wrong_digest).unwrap_err();
        assert_eq!(error.code(), "ICE3_STUDIO_ASSET_INVALID_OPTIONS");
    }
}
