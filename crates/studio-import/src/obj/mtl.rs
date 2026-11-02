use std::collections::BTreeMap;

use serde_json::json;

use crate::{ImportError, SourceMaterial};

pub fn parse_mtl(bytes: &[u8]) -> Result<BTreeMap<String, SourceMaterial>, ImportError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| ImportError::InvalidSource("MTL is not UTF-8".into()))?;
    let mut materials = BTreeMap::<String, SourceMaterial>::new();
    let mut current = None::<String>;
    for (line_index, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut fields = line.split_whitespace();
        let statement = fields.next().unwrap_or("");
        match statement {
            "newmtl" => {
                let name = line
                    .strip_prefix("newmtl")
                    .expect("statement")
                    .trim()
                    .to_owned();
                if name.is_empty() {
                    return Err(invalid(line_index, "material name is empty"));
                }
                materials
                    .entry(name.clone())
                    .or_insert_with(|| SourceMaterial {
                        id: name.clone(),
                        name: name.clone(),
                        base_color: [1.0; 3],
                        opacity: 1.0,
                        extras: json!({"ice3": {"id": name}}),
                    });
                current = Some(name);
            }
            "Kd" => {
                let material = material_mut(&mut materials, &current, line_index)?;
                material.base_color = [
                    parse_unit(fields.next(), line_index)?,
                    parse_unit(fields.next(), line_index)?,
                    parse_unit(fields.next(), line_index)?,
                ];
            }
            "d" => {
                material_mut(&mut materials, &current, line_index)?.opacity =
                    parse_unit(fields.next(), line_index)?;
            }
            "Tr" => {
                material_mut(&mut materials, &current, line_index)?.opacity =
                    1.0 - parse_unit(fields.next(), line_index)?;
            }
            _ => {}
        }
    }
    Ok(materials)
}

fn material_mut<'a>(
    materials: &'a mut BTreeMap<String, SourceMaterial>,
    current: &Option<String>,
    line: usize,
) -> Result<&'a mut SourceMaterial, ImportError> {
    let name = current
        .as_ref()
        .ok_or_else(|| invalid(line, "material property appears before newmtl"))?;
    materials
        .get_mut(name)
        .ok_or_else(|| invalid(line, "material is missing"))
}

fn parse_unit(value: Option<&str>, line: usize) -> Result<f32, ImportError> {
    let value = value
        .and_then(|value| value.parse::<f32>().ok())
        .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
        .ok_or_else(|| invalid(line, "MTL value must be between zero and one"))?;
    Ok(value)
}

fn invalid(line: usize, message: &str) -> ImportError {
    ImportError::InvalidSource(format!("MTL line {}: {message}", line + 1))
}
