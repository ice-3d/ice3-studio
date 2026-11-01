mod support;

use std::collections::BTreeMap;

use studio_core::document::{NodeContent, Relation, SceneNode};
use studio_core::ids::{NodeId, RelationId};
use studio_core::large_scene::cache::MemoryGeometryStore;
use studio_core::large_scene::component::ComponentPort;
use studio_core::large_scene::connection::{ConnectionIntent, InstancePortRef, RoutePolicy};
use studio_core::large_scene::evaluate::evaluate_large_scene_with_cache;
use studio_core::large_scene::route::{ResolvedPort, route_between_ports};
use studio_core::math::{Transform, Vec3};
use support::shared_pump_document;

fn port(position: [f32; 3], direction: [f32; 3], medium: &str) -> ResolvedPort {
    ResolvedPort {
        position: Vec3 {
            x: position[0],
            y: position[1],
            z: position[2],
        },
        direction: Vec3 {
            x: direction[0],
            y: direction[1],
            z: direction[2],
        },
        medium: medium.into(),
        diameter: 0.2,
    }
}

#[test]
fn an_orthogonal_route_starts_and_ends_at_the_ports() {
    let points = route_between_ports(
        &port([0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], "water"),
        &port([10.0, 2.0, 4.0], [1.0, 0.0, 0.0], "water"),
        &RoutePolicy::Orthogonal,
    )
    .unwrap();
    assert!(points.len() >= 5);
    assert_eq!(points[0], [0.0, 1.0, 0.0]);
    assert_eq!(points[points.len() - 1], [10.0, 2.0, 4.0]);
    assert!(points[1][0] < points[0][0]);
    assert!(points[points.len() - 2][0] < points[points.len() - 1][0]);
}

#[test]
fn orthogonal_route_uses_continuous_rounded_bends() {
    let points = route_between_ports(
        &port([0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], "water"),
        &port([10.0, 2.0, 4.0], [1.0, 0.0, 0.0], "water"),
        &RoutePolicy::Orthogonal,
    )
    .unwrap();

    for pair in points.windows(2) {
        let dx = pair[1][0] - pair[0][0];
        let dy = pair[1][1] - pair[0][1];
        let dz = pair[1][2] - pair[0][2];
        assert!((dx * dx + dy * dy + dz * dz).sqrt() > 1.0e-4);
    }

    let mut maximum_turn = 0.0_f32;
    for pair in points.windows(3) {
        let a = direction(pair[0], pair[1]);
        let b = direction(pair[1], pair[2]);
        let cosine = a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        maximum_turn = maximum_turn.max(cosine.clamp(-1.0, 1.0).acos());
    }
    assert!(maximum_turn < 25.0_f32.to_radians());
}

#[test]
fn orthogonal_route_drops_near_duplicate_coarse_points() {
    let points = route_between_ports(
        &port([0.0, 1.0, 0.0], [0.0, 1.0, 0.0], "water"),
        &port([10.0, 1.4, 0.00000005], [1.0, 0.0, 0.0], "water"),
        &RoutePolicy::Orthogonal,
    )
    .unwrap();
    assert_eq!(points[0], [0.0, 1.0, 0.0]);
    assert_eq!(points[points.len() - 1], [10.0, 1.4, 0.00000005]);
    assert!(points.windows(2).all(|pair| pair[0] != pair[1]));
}

#[test]
fn orthogonal_route_does_not_reverse_before_a_downward_end_port() {
    let points = route_between_ports(
        &port([0.0, 1.0, 0.0], [1.0, 0.0, 0.0], "water"),
        &port([10.0, 6.0, 0.0], [0.0, -1.0, 0.0], "water"),
        &RoutePolicy::Orthogonal,
    )
    .unwrap();
    assert_eq!(points[0], [0.0, 1.0, 0.0]);
    assert_eq!(points[points.len() - 1], [10.0, 6.0, 0.0]);
    assert!(points.iter().all(|point| point[1] >= 0.5));
}

fn direction(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    let value = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let length = (value[0] * value[0] + value[1] * value[1] + value[2] * value[2]).sqrt();
    [value[0] / length, value[1] / length, value[2] / length]
}

#[test]
fn rejects_incompatible_media_and_degenerate_routes() {
    assert!(
        route_between_ports(
            &port([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], "water"),
            &port([1.0, 0.0, 0.0], [-1.0, 0.0, 0.0], "air"),
            &RoutePolicy::Direct,
        )
        .is_err()
    );
    assert!(
        route_between_ports(
            &port([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], "water"),
            &port([0.0, 0.0, 0.0], [-1.0, 0.0, 0.0], "water"),
            &RoutePolicy::Direct,
        )
        .is_err()
    );
}

#[test]
fn moving_an_endpoint_invalidates_the_connection_mesh_key() {
    let mut document = shared_pump_document();
    let definition = &mut document.components[0];
    definition.ports = vec![
        ComponentPort {
            name: "outlet".into(),
            part_id: "body".into(),
            position: Vec3 {
                x: 1.0,
                y: 0.0,
                z: 0.0,
            },
            direction: Vec3 {
                x: 1.0,
                y: 0.0,
                z: 0.0,
            },
            medium: "water".into(),
            diameter: 0.2,
        },
        ComponentPort {
            name: "inlet".into(),
            part_id: "body".into(),
            position: Vec3 {
                x: -1.0,
                y: 0.0,
                z: 0.0,
            },
            direction: Vec3 {
                x: -1.0,
                y: 0.0,
                z: 0.0,
            },
            medium: "water".into(),
            diameter: 0.2,
        },
    ];
    let connection = ConnectionIntent {
        from: InstancePortRef::Instance {
            node_id: NodeId::from("node-000001"),
            port: "outlet".into(),
        },
        to: InstancePortRef::Instance {
            node_id: NodeId::from("node-000002"),
            port: "inlet".into(),
        },
        medium: "water".into(),
        diameter: 0.2,
        route_policy: RoutePolicy::Direct,
        relation_id: RelationId::from("relation-000001"),
        metadata: BTreeMap::new(),
    };
    document.relations.push(Relation {
        id: RelationId::from("relation-000001"),
        relation_type: "process".into(),
        from_node_id: NodeId::from("node-000001"),
        to_node_id: NodeId::from("node-000002"),
        metadata: BTreeMap::new(),
    });
    document.scene.nodes_mut().push(SceneNode {
        id: NodeId::from("node-000003"),
        name: "Pipe".into(),
        parent_id: None,
        transform: Transform::IDENTITY,
        material_id: None,
        content: NodeContent::Connection(connection),
        metadata: BTreeMap::new(),
        visible: true,
    });

    let mut cache = MemoryGeometryStore::default();
    let first = evaluate_large_scene_with_cache(&document, &mut cache).unwrap();
    let first_keys = connection_keys(&first);
    document.scene.nodes_mut()[1].transform.translation.x = 12.0;
    let second = evaluate_large_scene_with_cache(&document, &mut cache).unwrap();
    assert_ne!(first_keys, connection_keys(&second));
}

fn connection_keys(scene: &studio_core::large_scene::evaluate::EvaluatedScene) -> Vec<String> {
    scene
        .meshes
        .keys()
        .filter(|key| key.starts_with("connection-"))
        .cloned()
        .collect()
}
