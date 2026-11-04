//! CSV projection of a bill of materials.
//!
//! The BOM itself is computed by [`crate::agent_contract::build_bom`]; this
//! module only turns the tabular result into a stable, spreadsheet-friendly
//! text form so that the CLI and downstream tooling share one layout.

use crate::agent_contract::BomResult;

const CSV_HEADER: &str = "key,label,count,owners,volumeM3,minX,minY,minZ,maxX,maxY,maxZ";

/// Render a BOM as RFC 4180 CSV with a `\n` line ending.
pub fn bom_csv(result: &BomResult) -> String {
    let mut text = String::from(CSV_HEADER);
    text.push('\n');
    for row in &result.rows {
        let bounds = row
            .bounds
            .map(|bounds| {
                [
                    bounds.min.x,
                    bounds.min.y,
                    bounds.min.z,
                    bounds.max.x,
                    bounds.max.y,
                    bounds.max.z,
                ]
            })
            .unwrap_or([f32::NAN; 6]);
        text.push_str(&escape(&row.key));
        text.push(',');
        text.push_str(&escape(&row.label));
        text.push(',');
        text.push_str(&row.count.to_string());
        text.push(',');
        text.push_str(&row.owners.to_string());
        text.push(',');
        text.push_str(&format_volume(row.volume));
        for value in bounds {
            text.push(',');
            if value.is_finite() {
                text.push_str(&format_volume(value));
            }
        }
        text.push('\n');
    }
    text
}

fn format_volume(value: f32) -> String {
    let text = format!("{value:.6}");
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() || trimmed == "-" {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

fn escape(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}
