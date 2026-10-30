use studio_core::large_scene::connection::RoutePolicy;
use studio_core::large_scene::route::{ResolvedPort, route_between_ports};
use studio_core::math::Vec3;

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
    assert_eq!(points.len(), 5);
    assert_eq!(points[0], [0.0, 1.0, 0.0]);
    assert_eq!(points[4], [10.0, 2.0, 4.0]);
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
