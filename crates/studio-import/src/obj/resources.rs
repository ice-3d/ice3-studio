use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use studio_assets::read_bounded_uri;

use crate::obj::parse_mtl;
use crate::{ImportError, ImportWarning, SourceMaterial};

#[derive(Debug, Clone)]
pub struct ObjResources {
    pub obj: Vec<u8>,
    pub materials: BTreeMap<String, SourceMaterial>,
    pub warnings: Vec<ImportWarning>,
}

pub fn read_obj_resources(path: &Path) -> Result<ObjResources, ImportError> {
    let obj = std::fs::read(path)?;
    let text = std::str::from_utf8(&obj)
        .map_err(|_| ImportError::InvalidSource("OBJ is not UTF-8".into()))?;
    let root = path
        .parent()
        .ok_or_else(|| ImportError::InvalidSource("OBJ has no parent directory".into()))?;
    let mut warnings = BTreeSet::new();
    let mut libraries = BTreeSet::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let Some(reference) = line
            .strip_prefix("mtllib")
            .filter(|_| line.starts_with("mtllib"))
            .map(str::trim)
        else {
            continue;
        };
        if reference.is_empty() {
            warnings.insert(warning(
                "ICE3_STUDIO_IMPORT_OBJ_RESOURCE_IGNORED",
                "empty material library reference",
                None,
            ));
            continue;
        }
        libraries.insert(reference.replace('\\', "/"));
    }
    let mut materials = BTreeMap::new();
    for library in libraries {
        match read_bounded_uri(&library, root) {
            Ok(bytes) => {
                let parsed = parse_mtl(&bytes)?;
                if bytes
                    .split(|byte| *byte == b'\n')
                    .any(|line| line.trim_ascii_start().starts_with(b"map_"))
                {
                    warnings.insert(warning(
                        "ICE3_STUDIO_IMPORT_OBJ_TEXTURE_UNSUPPORTED",
                        "texture maps are not imported",
                        Some(&library),
                    ));
                }
                materials.extend(parsed);
            }
            Err(error) => {
                let message = error.to_string();
                let code = if message.contains("escapes")
                    || message.contains("relative")
                    || message.contains("scheme")
                {
                    "ICE3_STUDIO_IMPORT_OBJ_RESOURCE_IGNORED"
                } else {
                    "ICE3_STUDIO_IMPORT_OBJ_MATERIAL_UNAVAILABLE"
                };
                warnings.insert(warning(code, &message, Some(&library)));
            }
        }
    }
    Ok(ObjResources {
        obj,
        materials,
        warnings: warnings.into_iter().collect(),
    })
}

fn warning(code: &str, message: &str, path: Option<&str>) -> ImportWarning {
    ImportWarning {
        code: code.into(),
        message: message.into(),
        path: path.map(Into::into),
    }
}
