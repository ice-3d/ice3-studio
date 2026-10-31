use std::path::{Component, Path, PathBuf};

use crate::ToolchainClientError;

pub fn validate_workspace_path(
    workspace_root: &Path,
    relative_path: impl AsRef<Path>,
) -> Result<PathBuf, ToolchainClientError> {
    let relative_path = relative_path.as_ref();
    if relative_path.as_os_str().is_empty()
        || relative_path.is_absolute()
        || relative_path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(ToolchainClientError::protocol(
            "asset path must be relative and remain inside the workspace",
        ));
    }
    Ok(workspace_root.join(relative_path))
}
