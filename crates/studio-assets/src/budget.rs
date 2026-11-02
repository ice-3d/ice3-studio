use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{AssetError, AssetMetrics, AssetResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BudgetMetric {
    EncodedBytes,
    NodeCount,
    MaxNodeDepth,
    MeshCount,
    PrimitiveCount,
    TriangleCount,
    EdgeSegmentCount,
    MaterialCount,
    SelectionGroupCount,
}

impl BudgetMetric {
    pub const fn value(self, metrics: AssetMetrics) -> u64 {
        match self {
            Self::EncodedBytes => metrics.encoded_bytes,
            Self::NodeCount => metrics.node_count,
            Self::MaxNodeDepth => metrics.max_node_depth,
            Self::MeshCount => metrics.mesh_count,
            Self::PrimitiveCount => metrics.primitive_count,
            Self::TriangleCount => metrics.triangle_count,
            Self::EdgeSegmentCount => metrics.edge_segment_count,
            Self::MaterialCount => metrics.material_count,
            Self::SelectionGroupCount => metrics.selection_group_count,
        }
    }

    pub const fn check_id(self) -> &'static str {
        match self {
            Self::NodeCount => "nodes",
            Self::MeshCount => "meshes",
            Self::PrimitiveCount => "primitives",
            Self::TriangleCount => "triangles",
            Self::EdgeSegmentCount => "edges",
            Self::SelectionGroupCount => "selectionGroups",
            Self::EncodedBytes | Self::MaxNodeDepth | Self::MaterialCount => self.metric_id(),
        }
    }

    pub const fn metric_id(self) -> &'static str {
        match self {
            Self::EncodedBytes => "encodedBytes",
            Self::NodeCount => "nodeCount",
            Self::MaxNodeDepth => "maxNodeDepth",
            Self::MeshCount => "meshCount",
            Self::PrimitiveCount => "primitiveCount",
            Self::TriangleCount => "triangleCount",
            Self::EdgeSegmentCount => "edgeSegmentCount",
            Self::MaterialCount => "materialCount",
            Self::SelectionGroupCount => "selectionGroupCount",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BudgetLimit {
    pub id: BudgetMetric,
    pub label: String,
    pub soft_limit: u64,
    pub hard_limit: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BudgetProfile {
    id: String,
    limits: Vec<BudgetLimit>,
}

impl BudgetProfile {
    pub fn interactive_desktop_v1() -> Self {
        Self {
            id: "interactive-desktop-v1".into(),
            limits: vec![
                limit(BudgetMetric::NodeCount, "Nodes", 25_000, 50_000),
                limit(BudgetMetric::MeshCount, "Meshes", 5_000, 15_000),
                limit(BudgetMetric::PrimitiveCount, "Primitives", 10_000, 30_000),
                limit(
                    BudgetMetric::TriangleCount,
                    "Triangles",
                    5_000_000,
                    20_000_000,
                ),
                limit(
                    BudgetMetric::EdgeSegmentCount,
                    "Edge segments",
                    10_000_000,
                    40_000_000,
                ),
                limit(
                    BudgetMetric::EncodedBytes,
                    "Encoded bytes",
                    134_217_728,
                    536_870_912,
                ),
                limit(
                    BudgetMetric::SelectionGroupCount,
                    "Selection groups",
                    20_000,
                    100_000,
                ),
            ],
        }
    }

    pub fn unrestricted() -> Self {
        Self {
            id: "unrestricted".into(),
            limits: Vec::new(),
        }
    }

    pub fn from_json(value: &str) -> AssetResult<Self> {
        let profile: Self = serde_json::from_str(value)
            .map_err(|error| AssetError::InvalidBudget(error.to_string()))?;
        profile.validate()?;
        Ok(profile)
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn limits(&self) -> &[BudgetLimit] {
        &self.limits
    }

    fn validate(&self) -> AssetResult<()> {
        if self.id.trim().is_empty() || self.limits.is_empty() {
            return Err(invalid_budget("id and limits must not be empty"));
        }
        let mut metrics = BTreeSet::new();
        for limit in &self.limits {
            if limit.label.trim().is_empty()
                || limit.soft_limit > limit.hard_limit
                || !metrics.insert(limit.id)
            {
                return Err(invalid_budget("limit entry is invalid or duplicated"));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BudgetStatus {
    Pass,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetCheck {
    pub id: String,
    pub metric: BudgetMetric,
    pub value: u64,
    pub soft_limit: u64,
    pub hard_limit: u64,
    pub status: BudgetStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetResult {
    pub profile: String,
    pub status: BudgetStatus,
    pub warning_count: u64,
    pub error_count: u64,
    pub checks: Vec<BudgetCheck>,
}

pub fn evaluate_budget(metrics: AssetMetrics, profile: &BudgetProfile) -> BudgetResult {
    let checks = profile
        .limits
        .iter()
        .map(|limit| {
            let value = limit.id.value(metrics);
            let status = if value > limit.hard_limit {
                BudgetStatus::Error
            } else if value > limit.soft_limit {
                BudgetStatus::Warning
            } else {
                BudgetStatus::Pass
            };
            BudgetCheck {
                id: limit.id.check_id().into(),
                metric: limit.id,
                value,
                soft_limit: limit.soft_limit,
                hard_limit: limit.hard_limit,
                status,
            }
        })
        .collect::<Vec<_>>();
    let warning_count = checks
        .iter()
        .filter(|check| check.status == BudgetStatus::Warning)
        .count() as u64;
    let error_count = checks
        .iter()
        .filter(|check| check.status == BudgetStatus::Error)
        .count() as u64;
    BudgetResult {
        profile: profile.id().into(),
        status: if error_count > 0 {
            BudgetStatus::Error
        } else if warning_count > 0 {
            BudgetStatus::Warning
        } else {
            BudgetStatus::Pass
        },
        warning_count,
        error_count,
        checks,
    }
}

fn limit(id: BudgetMetric, label: &str, soft_limit: u64, hard_limit: u64) -> BudgetLimit {
    BudgetLimit {
        id,
        label: label.into(),
        soft_limit,
        hard_limit,
    }
}

fn invalid_budget(message: &str) -> AssetError {
    AssetError::InvalidBudget(message.into())
}
