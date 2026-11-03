//! `scene.measure` contract: centre distance and AABB clearance.

use std::collections::BTreeMap;

use studio_core::agent_contract::{MeasurePoint, MeasureRequest, measure_request};
use studio_core::geometry::Bounds;
use studio_core::math::Vec3;

fn bounds(min: [f32; 3], max: [f32; 3]) -> Bounds {
    Bounds {
        min: Vec3 {
            x: min[0],
            y: min[1],
            z: min[2],
        },
        max: Vec3 {
            x: max[0],
            y: max[1],
            z: max[2],
        },
    }
}

fn node(id: &str) -> MeasurePoint {
    MeasurePoint {
        node_id: Some(id.to_string()),
        point: None,
    }
}

fn point(position: [f32; 3]) -> MeasurePoint {
    MeasurePoint {
        node_id: None,
        point: Some(position),
    }
}

#[test]
fn measures_centre_distance_and_clearance_between_nodes() {
    let mut map = BTreeMap::new();
    map.insert("a".to_string(), bounds([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]));
    map.insert("b".to_string(), bounds([3.0, 0.0, 0.0], [4.0, 1.0, 1.0]));
    let request = MeasureRequest {
        from: node("a"),
        to: node("b"),
    };
    let result = measure_request(&request, &map).expect("measure");
    assert!(
        (result.distance - 3.0).abs() < 1e-5,
        "distance {}",
        result.distance
    );
    assert!(
        (result.clearance - 2.0).abs() < 1e-5,
        "clearance {}",
        result.clearance
    );
    assert!((result.from[0] - 0.5).abs() < 1e-5);
    assert!((result.to[0] - 3.5).abs() < 1e-5);
}

#[test]
fn overlapping_bounds_have_zero_clearance() {
    let mut map = BTreeMap::new();
    map.insert("a".to_string(), bounds([0.0, 0.0, 0.0], [2.0, 2.0, 2.0]));
    map.insert("b".to_string(), bounds([1.0, 1.0, 1.0], [3.0, 3.0, 3.0]));
    let request = MeasureRequest {
        from: node("a"),
        to: node("b"),
    };
    let result = measure_request(&request, &map).expect("measure");
    assert_eq!(result.clearance, 0.0);
    assert!(result.distance > 0.0);
}

#[test]
fn measures_between_points() {
    let request = MeasureRequest {
        from: point([0.0, 0.0, 0.0]),
        to: point([3.0, 4.0, 0.0]),
    };
    let result = measure_request(&request, &BTreeMap::new()).expect("measure");
    assert!((result.distance - 5.0).abs() < 1e-5);
    assert!((result.clearance - 5.0).abs() < 1e-5);
}

#[test]
fn node_to_point_uses_distance_as_clearance() {
    let mut map = BTreeMap::new();
    map.insert("a".to_string(), bounds([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]));
    let request = MeasureRequest {
        from: node("a"),
        to: point([0.5, 0.5, 5.0]),
    };
    let result = measure_request(&request, &map).expect("measure");
    assert!((result.distance - 4.5).abs() < 1e-5);
    assert!((result.clearance - 4.5).abs() < 1e-5);
}

#[test]
fn unknown_node_and_ambiguous_points_are_rejected() {
    let request = MeasureRequest {
        from: node("missing"),
        to: point([0.0, 0.0, 0.0]),
    };
    assert!(measure_request(&request, &BTreeMap::new()).is_err());

    let ambiguous = MeasureRequest {
        from: MeasurePoint {
            node_id: Some("a".into()),
            point: Some([0.0, 0.0, 0.0]),
        },
        to: point([1.0, 1.0, 1.0]),
    };
    assert!(measure_request(&ambiguous, &BTreeMap::new()).is_err());

    let empty = MeasureRequest {
        from: MeasurePoint {
            node_id: None,
            point: None,
        },
        to: point([1.0, 1.0, 1.0]),
    };
    assert!(measure_request(&empty, &BTreeMap::new()).is_err());
}

#[test]
fn request_json_matches_the_contract() {
    let request: MeasureRequest =
        serde_json::from_str(r#"{"from":{"nodeId":"node-000001"},"to":{"point":[1,2,3]}}"#)
            .expect("request parses");
    assert_eq!(request.from.node_id.as_deref(), Some("node-000001"));
    assert_eq!(request.to.point, Some([1.0, 2.0, 3.0]));
}
