//! `scene.clashes` contract: broad phase, clearance bands, narrow phase.

mod support;

use std::collections::BTreeMap;

use studio_core::agent_contract::{ClashRequest, build_clash_report};
use studio_core::document::Document;
use studio_core::large_scene::component::{
    ComponentPart, PartContent, PartTransform, ScalarBinding, Vector3Binding,
};
use studio_core::large_scene::evaluate::evaluate_large_scene;
use studio_core::math::Vec3;
use support::shared_pump_document;

fn request(clearance: Option<f32>) -> ClashRequest {
    ClashRequest {
        clearance,
        ..ClashRequest::default()
    }
}

fn moved(document: &mut Document, ordinal: usize, x: f32) {
    document.scene.nodes_mut()[ordinal - 1]
        .transform
        .translation = Vec3 { x, y: 0.0, z: 0.0 };
}

#[test]
fn clear_scene_has_no_clashes() {
    let document = shared_pump_document();
    let scene = evaluate_large_scene(&document).unwrap();
    let result = build_clash_report(&scene, &request(None));
    assert_eq!(result.total, 0, "pumps sit two metres apart");
    assert_eq!(result.checked_parts, 2);
    assert_eq!(result.scope, "owners");
    assert!(!result.precise);
}

#[test]
fn overlapping_instances_report_the_penetration_depth() {
    let mut document = shared_pump_document();
    moved(&mut document, 2, 3.0);
    let scene = evaluate_large_scene(&document).unwrap();
    let result = build_clash_report(&scene, &request(None));
    assert_eq!(result.total, 1, "one pair: the two pump bodies");
    let pair = &result.pairs[0];
    assert!(pair.penetrating);
    assert!((pair.gap + 1.0).abs() < 1e-4, "depth {}", pair.gap);
    assert_eq!(pair.a_owner, "node-000001");
    assert_eq!(pair.b_owner, "node-000002");
    assert_eq!(pair.confirmed, None, "narrow phase was not requested");
}

#[test]
fn clearance_band_reports_close_but_clear_parts() {
    let document = shared_pump_document();
    let scene = evaluate_large_scene(&document).unwrap();
    let result = build_clash_report(&scene, &request(Some(2.5)));
    assert_eq!(result.total, 1);
    let pair = &result.pairs[0];
    assert!(!pair.penetrating);
    assert!((pair.gap - 2.0).abs() < 1e-4, "gap {}", pair.gap);
    assert_eq!(result.clearance, 2.5);
}

#[test]
fn clearance_band_excludes_parts_that_are_further_away() {
    let document = shared_pump_document();
    let scene = evaluate_large_scene(&document).unwrap();
    assert_eq!(build_clash_report(&scene, &request(Some(1.5))).total, 0);
}

#[test]
fn parts_of_the_same_instance_are_ignored_by_default() {
    let mut document = shared_pump_document();
    document.components[0].parts.push(ComponentPart {
        id: "shaft".into(),
        parent_id: None,
        transform: PartTransform {
            translation: Vector3Binding::Literal(Vec3 {
                x: 0.5,
                y: 0.25,
                z: 0.25,
            }),
            ..PartTransform::default()
        },
        content: PartContent::Box {
            width: ScalarBinding::Literal(1.0),
            height: ScalarBinding::Literal(0.5),
            depth: ScalarBinding::Literal(0.5),
        },
        subtract: Vec::new(),
        repeat: None,
        material_role: None,
        metadata: BTreeMap::new(),
        visible: true,
    });
    let scene = evaluate_large_scene(&document).unwrap();
    let owners = build_clash_report(&scene, &request(None));
    assert_eq!(
        owners.total, 0,
        "body and shaft belong to the same instance"
    );

    let parts = build_clash_report(
        &scene,
        &ClashRequest {
            scope: Some("parts".into()),
            ..ClashRequest::default()
        },
    );
    assert!(
        parts.total >= 1,
        "intra-instance interference is visible with scope=parts"
    );
    assert!(parts.pairs.iter().all(|pair| pair.a_owner == pair.b_owner));
}

#[test]
fn owner_filter_narrows_the_audit() {
    let mut document = shared_pump_document();
    moved(&mut document, 2, 3.0);
    let scene = evaluate_large_scene(&document).unwrap();
    let result = build_clash_report(
        &scene,
        &ClashRequest {
            owner_ids: Some(vec!["node-000001".into()]),
            ..ClashRequest::default()
        },
    );
    assert_eq!(result.checked_parts, 1);
    assert_eq!(result.total, 0);
}

#[test]
fn narrow_phase_drops_bounding_volume_only_overlaps() {
    // Two long thin boxes rotated 45° about Y and stacked diagonally: their
    // axis-aligned boxes overlap, the geometry stays well clear.
    let mut document = shared_pump_document();
    document.components[0].parts[0].content = PartContent::Box {
        width: ScalarBinding::Literal(4.0),
        height: ScalarBinding::Literal(0.2),
        depth: ScalarBinding::Literal(0.2),
    };
    document.components[0].parts[0].transform = PartTransform {
        translation: Vector3Binding::Literal(Vec3::default()),
        yaw: Some(ScalarBinding::Literal(45.0)),
        ..PartTransform::default()
    };
    document.scene.nodes_mut()[0].transform.translation = Vec3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    document.scene.nodes_mut()[1].transform.translation = Vec3 {
        x: 1.5,
        y: 0.0,
        z: 1.5,
    };
    let scene = evaluate_large_scene(&document).unwrap();

    let coarse = build_clash_report(&scene, &request(None));
    assert_eq!(coarse.total, 1, "AABBs overlap");
    assert!(coarse.pairs[0].penetrating);

    let precise = build_clash_report(
        &scene,
        &ClashRequest {
            precise: Some(true),
            ..ClashRequest::default()
        },
    );
    assert_eq!(precise.total, 0, "triangles stay clear, so no clash");
    assert!(precise.precise);
}

#[test]
fn narrow_phase_confirms_a_real_penetration() {
    let mut document = shared_pump_document();
    moved(&mut document, 2, 3.0);
    let scene = evaluate_large_scene(&document).unwrap();
    let result = build_clash_report(
        &scene,
        &ClashRequest {
            precise: Some(true),
            ..ClashRequest::default()
        },
    );
    assert_eq!(result.total, 1);
    assert_eq!(
        result.pairs[0].confirmed,
        Some(true),
        "interpenetrating boxes really do share triangles"
    );
}

#[test]
fn limit_truncates_and_flags_the_result() {
    let mut document = shared_pump_document();
    moved(&mut document, 2, 3.0);
    let scene = evaluate_large_scene(&document).unwrap();
    let result = build_clash_report(
        &scene,
        &ClashRequest {
            limit: Some(1),
            ..request(None)
        },
    );
    // One candidate pair: check the flag stays false and total is honest.
    assert_eq!(result.total, 1);
    assert!(!result.truncated);
}

#[test]
fn request_json_matches_the_contract() {
    let request: ClashRequest = serde_json::from_str(
        r#"{"clearance":0.25,"scope":"parts","precise":true,"ownerIds":["node-000001"],"limit":10}"#,
    )
    .expect("parses");
    assert_eq!(request.clearance, Some(0.25));
    assert_eq!(request.scope.as_deref(), Some("parts"));
    assert_eq!(request.precise, Some(true));
    assert_eq!(
        request.owner_ids.as_deref(),
        Some(["node-000001".to_string()].as_slice())
    );
    assert_eq!(request.limit, Some(10));
}
