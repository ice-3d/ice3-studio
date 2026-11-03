use std::collections::BTreeMap;

use studio_core::document::{Document, NodeContent, SceneNode};
use studio_core::geometry::Primitive;
use studio_core::ids::{NodeId, RelationId};
use studio_core::large_scene::connection::{ConnectionIntent, InstancePortRef, RoutePolicy};
use studio_core::math::{Transform, Vec3};
use studio_core::validation::{
    DiagnosticCode, MAX_OVERLAP_DIAGNOSTICS, validate_document, validate_overlaps,
};

fn box_node(
    ordinal: u64,
    name: &str,
    x: f32,
    y: f32,
    z: f32,
    size: f32,
    parent: Option<NodeId>,
) -> SceneNode {
    SceneNode {
        id: NodeId::new("node", ordinal).unwrap(),
        name: name.into(),
        parent_id: parent,
        transform: Transform {
            translation: Vec3 { x, y, z },
            ..Transform::IDENTITY
        },
        material_id: None,
        content: NodeContent::Primitive(Primitive::Box {
            width: size,
            height: size,
            depth: size,
        }),
        metadata: BTreeMap::new(),
        visible: true,
    }
}

fn document_with(nodes: Vec<SceneNode>) -> Document {
    let mut document = Document::new("overlap-audit");
    *document.scene.nodes_mut() = nodes;
    document
}

#[test]
fn reports_a_significantly_overlapping_entity_pair() {
    let document = document_with(vec![
        box_node(1, "Filter A", 0.0, 0.0, 0.0, 2.0, None),
        box_node(2, "Filter B", 1.0, 0.0, 0.0, 2.0, None),
    ]);

    let diagnostics = validate_overlaps(&document);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    let diagnostic = &diagnostics[0];
    assert_eq!(diagnostic.code, DiagnosticCode::GeometryOverlap);
    assert_eq!(
        diagnostic.object_id.as_deref(),
        Some("node-000001|node-000002")
    );
    assert!(diagnostic.message.contains("Filter A"));
    assert!(diagnostic.message.contains("Filter B"));
    assert!(diagnostic.message.contains("x=1.00m"));

    // 事务校验路径不受影响：同一文档的常规校验不含重叠诊断。
    assert!(
        validate_document(&document)
            .iter()
            .all(|diagnostic| diagnostic.code != DiagnosticCode::GeometryOverlap)
    );
}

#[test]
fn skips_touching_slab_containment_and_ancestry() {
    let parent = box_node(1, "Assembly", 0.0, 0.0, 0.0, 4.0, None);
    let mut document = document_with(vec![
        parent.clone(),
        // 祖先-后代：子节点与父节点深度重叠，按层级链路跳过。
        box_node(
            2,
            "Inner child",
            0.5,
            0.5,
            0.5,
            1.0,
            Some(parent.id.clone()),
        ),
        // 单向包含在 Assembly 内的独立节点（嵌套，不是重合）。
        box_node(3, "Contained", -1.0, -1.0, -1.0, 1.0, None),
        // 薄板与其上方箱体：板厚 0.4（slab 过滤），箱体底面与板顶正好接触。
        box_node(4, "Slab", 20.0, 0.0, 0.0, 4.0, None),
    ]);
    document.scene.nodes_mut()[3].content = NodeContent::Primitive(Primitive::Box {
        width: 4.0,
        height: 0.4,
        depth: 4.0,
    });
    document
        .scene
        .nodes_mut()
        .push(box_node(5, "On slab", 20.0, 1.2, 0.0, 2.0, None));

    let diagnostics = validate_overlaps(&document);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn ignores_overlap_below_the_depth_threshold() {
    let document = document_with(vec![
        box_node(1, "Tank A", 0.0, 0.0, 0.0, 2.0, None),
        box_node(2, "Tank B", 1.95, 0.0, 0.0, 2.0, None),
    ]);

    assert!(validate_overlaps(&document).is_empty());
}

#[test]
fn reports_coincident_entities_as_conflicts() {
    // 双向包含（实体重合）不是嵌套，是最严重的空间冲突。
    let document = document_with(vec![
        box_node(1, "Skid A", 0.0, 0.0, 0.0, 2.0, None),
        box_node(2, "Skid B", 0.0, 0.0, 0.0, 2.0, None),
    ]);

    let diagnostics = validate_overlaps(&document);
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert!(diagnostics[0].message.contains("x=2.00m"));
}

#[test]
fn ignores_equipment_edges_protruding_from_a_large_building() {
    let mut document = document_with(vec![box_node(1, "Pump house", 0.0, 4.0, 0.0, 8.0, None)]);
    document.scene.nodes_mut()[0].content = NodeContent::Primitive(Primitive::Box {
        width: 20.0,
        height: 8.0,
        depth: 10.0,
    });
    // 室内设备，AABB 略微突出外墙：重叠体积占建筑比例远低于阈值。
    document
        .scene
        .nodes_mut()
        .push(box_node(2, "Indoor pump", 10.5, 1.0, 0.0, 2.0, None));

    let diagnostics = validate_overlaps(&document);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn skips_connection_routes() {
    let mut document = document_with(vec![
        box_node(1, "Pump A", 0.0, 0.0, 0.0, 2.0, None),
        box_node(2, "Pump B", 6.0, 0.0, 0.0, 2.0, None),
    ]);
    // 连接节点的空间穿越是路由设计行为，即使其包围盒与两端实体重叠也不审计。
    document.scene.nodes_mut().push(SceneNode {
        id: NodeId::new("node", 3).unwrap(),
        name: "Transfer pipe".into(),
        parent_id: None,
        transform: Transform::IDENTITY,
        material_id: None,
        content: NodeContent::Connection(ConnectionIntent {
            from: InstancePortRef::Instance {
                node_id: NodeId::new("node", 1).unwrap(),
                port: "outlet".into(),
            },
            to: InstancePortRef::Instance {
                node_id: NodeId::new("node", 2).unwrap(),
                port: "inlet".into(),
            },
            medium: "water".into(),
            diameter: 0.2,
            route_policy: RoutePolicy::Orthogonal,
            relation_id: RelationId::new("rel", 1).unwrap(),
            metadata: BTreeMap::new(),
        }),
        metadata: BTreeMap::new(),
        visible: true,
    });

    let diagnostics = validate_overlaps(&document);
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.object_id.as_deref() != Some("node-000001|node-000003")),
        "{diagnostics:?}"
    );
}

#[test]
fn caps_reports_and_summarizes_the_total() {
    let nodes = (1..=60)
        .map(|ordinal| box_node(ordinal, "Unit", 0.0, 0.0, 0.0, 2.0, None))
        .collect();
    let document = document_with(nodes);

    let diagnostics = validate_overlaps(&document);
    let total_pairs = 60 * 59 / 2;
    assert_eq!(diagnostics.len(), MAX_OVERLAP_DIAGNOSTICS + 1);
    let summary = diagnostics.last().unwrap();
    assert_eq!(summary.code, DiagnosticCode::GeometryOverlap);
    assert_eq!(summary.object_id, None);
    assert!(
        summary
            .message
            .contains(&format!("found {total_pairs} pairs"))
    );
}
