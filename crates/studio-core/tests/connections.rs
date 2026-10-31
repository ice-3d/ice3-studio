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
    assert!(points[points.len() - 2][0] > points[points.len() - 1][0]);
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
