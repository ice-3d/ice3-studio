use std::collections::BTreeMap;

use serde::Serialize;

pub const ENDPOINT_EXCLUSION_M: f32 = 1.5;
pub const DEFAULT_MAX_SUPPORT_SPACING_M: f32 = 6.0;
pub const SUPPORT_PROJECTION_TOLERANCE_M: f32 = 1.0;
pub const TURN_THRESHOLD_RADIANS: f32 = 15.0_f32.to_radians();
pub const MAX_OBSTACLE_OWNERS: usize = 50_000;
pub const MAX_ROUTE_SEGMENTS: usize = 50_000;
pub const MAX_TRIANGLE_PAIRS: usize = 1_000_000;
pub const MAX_COLLISIONS_PER_ROUTE: usize = 32;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteSafetyStats {
    pub obstacle_owners: usize,
    pub route_segments: usize,
    pub broad_phase_candidates: usize,
    pub narrow_phase_pairs: usize,
    pub triangle_pairs: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) enum ObstacleClass {
    Solid,
    Ignored,
    ClearanceSurface { clearance: f32 },
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn obstacle_class(
    metadata: &BTreeMap<String, String>,
) -> Result<ObstacleClass, &'static str> {
    match metadata.get("routeObstacle").map(String::as_str) {
        None | Some("solid") => Ok(ObstacleClass::Solid),
        Some("ignored") => Ok(ObstacleClass::Ignored),
        Some("clearance-surface") => {
            let clearance = metadata
                .get("routeClearance")
                .map(|value| value.parse::<f32>())
                .transpose()
                .map_err(|_| "routeClearance must be a finite non-negative number")?
                .unwrap_or(0.0);
            if clearance.is_finite() && clearance >= 0.0 {
                Ok(ObstacleClass::ClearanceSurface { clearance })
            } else {
                Err("routeClearance must be a finite non-negative number")
            }
        }
        Some(_) => Err("routeObstacle must be solid, ignored, or clearance-surface"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routing_limits_and_defaults_are_stable() {
        assert_eq!(DEFAULT_MAX_SUPPORT_SPACING_M, 6.0);
        assert_eq!(ENDPOINT_EXCLUSION_M, 1.5);
        assert_eq!(MAX_OBSTACLE_OWNERS, 50_000);
        assert_eq!(MAX_ROUTE_SEGMENTS, 50_000);
        assert_eq!(MAX_TRIANGLE_PAIRS, 1_000_000);
    }

    #[test]
    fn metadata_parser_rejects_unknown_obstacle_values() {
        let mut metadata = BTreeMap::new();
        metadata.insert("routeObstacle".into(), "mystery".into());
        assert_eq!(
            obstacle_class(&metadata),
            Err("routeObstacle must be solid, ignored, or clearance-surface"),
        );
    }
}
