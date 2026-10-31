use std::collections::BTreeMap;

use studio_core::agent_contract::{
    AgentLimits, ChangeImpactCounts, EvaluationInclude, EvaluationQueryItem,
    EvaluationQueryRequest, IterationImpact, IterationImpactReason, IterationImpactScope,
    RuntimePreviewRequest, SceneSearchField, SceneSearchItem, SceneSearchRequest, SceneSearchScope,
};
use ts_rs::TS;

#[test]
fn agent_requests_serialize_with_camel_case_and_bounded_defaults() {
    let request = SceneSearchRequest {
        scope: SceneSearchScope::Derived,
        query: None,
        node_id: None,
        owner_id: None,
        parent_id: None,
        metadata: BTreeMap::new(),
        content_kind: None,
        component_definition_id: None,
        material_id: None,
        visible: None,
        relation_type: None,
        related_node_id: None,
        bounds: None,
        fields: None,
        cursor: None,
        limit: 25,
    };
    let value = serde_json::to_value(request).unwrap();
    assert_eq!(value["scope"], "derived");
    assert_eq!(value["limit"], 25);
    assert!(value.get("node_id").is_none());

    let fields = serde_json::to_value([SceneSearchField::ComponentDefinitionId]).unwrap();
    assert_eq!(fields[0], "componentDefinitionId");

    let evaluation = EvaluationQueryRequest {
        node_ids: vec!["node-000001/body".into()],
        include: vec![EvaluationInclude::Summary],
    };
    let value = serde_json::to_value(evaluation).unwrap();
    assert_eq!(value["nodeIds"][0], "node-000001/body");
    assert_eq!(AgentLimits::default().search_max_limit, 100);
}

#[test]
fn agent_request_typescript_marks_omittable_fields_optional() {
    let search = SceneSearchRequest::decl();
    assert!(search.contains("scope?: SceneSearchScope"));
    assert!(search.contains("query?: string | null"));
    assert!(search.contains("metadata?:"));
    assert!(search.contains("limit?: number"));

    let evaluation = EvaluationQueryRequest::decl();
    assert!(evaluation.contains("include?: Array<EvaluationInclude>"));

    let search_item = SceneSearchItem::decl();
    assert!(search_item.contains("name?: string"));
    assert!(search_item.contains("metadata?:"));

    let evaluation_item = EvaluationQueryItem::decl();
    assert!(evaluation_item.contains("geometryId: string | null"));
    assert!(evaluation_item.contains("positions?: Array<[number, number, number]>"));
    assert!(evaluation_item.contains("indices?: Array<number>"));

    let preview = RuntimePreviewRequest::decl();
    assert!(preview.contains("revision?: bigint | null"));
    assert!(preview.contains("candidateDigest?: string | null"));
}

#[test]
fn iteration_impact_serializes_with_counts_and_snake_case_reasons() {
    let impact = IterationImpact {
        scope: IterationImpactScope::Local,
        full_scene_rebuild_required: false,
        source_nodes: ChangeImpactCounts::default(),
        derived_nodes: ChangeImpactCounts::default(),
        relations: ChangeImpactCounts::default(),
        components: ChangeImpactCounts::default(),
        materials: ChangeImpactCounts::default(),
        affected_source_nodes: 1,
        affected_derived_nodes: 64,
        reused_geometry: 64,
        generated_geometry: 0,
        cache_hits: 64,
        cache_misses: 0,
        reasons: vec![IterationImpactReason::InstanceParameters],
    };
    let value = serde_json::to_value(impact).unwrap();

    assert_eq!(value["scope"], "local");
    assert_eq!(value["fullSceneRebuildRequired"], false);
    assert_eq!(value["affectedSourceNodes"], 1);
    assert_eq!(value["generatedGeometry"], 0);
    assert_eq!(value["reasons"][0], "instance_parameters");
}
