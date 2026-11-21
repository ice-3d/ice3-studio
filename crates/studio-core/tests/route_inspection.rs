mod support;

use std::collections::BTreeMap;

use studio_core::ids::Revision;
use studio_core::large_scene::route_inspection::{RouteInspectionLimits, inspect_route};

fn definitions(
    document: &studio_core::document::Document,
) -> BTreeMap<
    studio_core::ids::ComponentId,
    &studio_core::large_scene::component::ComponentDefinition,
> {
    document
        .components
        .iter()
        .map(|definition| (definition.id.clone(), definition))
        .collect()
}

#[test]
fn inspects_the_formal_route_and_resolved_endpoints() {
    let mut document = support::route_collision_document();
    document.revision = Revision::new(1);
    let result = inspect_route(
        &document,
        &definitions(&document),
        "L-001",
        Some(1),
        RouteInspectionLimits::default(),
    )
    .unwrap();

    assert_eq!(result.revision, 1);
    assert_eq!(result.line_id, "L-001");
    assert_eq!(result.segments.len(), 1);
    let segment = &result.segments[0];
    assert_eq!(segment.node_id, "node-000003");
    assert_eq!(segment.from.port, "outlet");
    assert_eq!(segment.to.port, "inlet");
    assert_eq!(segment.from.position.x, 5.0);
    assert_eq!(segment.to.position.x, 7.0);
    assert_eq!(segment.points.first(), Some(&segment.from_position_array()));
    assert_eq!(segment.points.last(), Some(&segment.to_position_array()));
    assert!(!result.truncated.segments);
    assert!(!result.truncated.points);
}
