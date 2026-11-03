use std::collections::BTreeMap;

use studio_core::agent_contract::{
    AgentLimits, ChangeImpactCounts, ComponentDefinitionSummary, ComponentGetParams,
    ComponentGetResult, ComponentPartKind, ComponentPartSummary, EvaluationInclude,
    EvaluationQueryItem, EvaluationQueryRequest, IterationImpact, IterationImpactReason,
    IterationImpactScope, MAX_PART_SUMMARIES, RuntimePreviewRequest, SceneSearchField,
    SceneSearchItem, SceneSearchRequest, SceneSearchScope,
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

#[test]
fn component_get_request_serializes_with_camel_case_and_rejects_unknown_fields() {
    let request = ComponentGetParams {
        component_id: "component-000001".into(),
        include_parts: Some(true),
    };
    let value = serde_json::to_value(&request).unwrap();
    assert_eq!(value["componentId"], "component-000001");
    assert_eq!(value["includeParts"], true);
    assert!(value.get("component_id").is_none());

    let rejected = serde_json::from_value::<ComponentGetParams>(serde_json::json!({
        "componentId": "component-000001",
        "includePartsAndPorts": true
    }));
    assert!(rejected.is_err());

    let with_parts = serde_json::from_value::<ComponentGetParams>(serde_json::json!({
        "componentId": "component-000001",
        "includeParts": true
    }));
    assert!(with_parts.is_ok());
    assert_eq!(with_parts.unwrap().include_parts, Some(true));

    let empty = serde_json::from_value::<ComponentGetParams>(serde_json::json!({
        "componentId": ""
    }));
    assert!(empty.is_ok());
}

#[test]
fn component_get_result_serializes_summary_with_snake_case_part_kinds() {
    let result = ComponentGetResult {
        revision: 2,
        component: ComponentDefinitionSummary {
            id: "component-000001".into(),
            name: "Pump station".into(),
            parameters: vec![],
            ports: vec![],
            parts: vec![
                ComponentPartSummary {
                    id: "body".into(),
                    parent_id: None,
                    kind: ComponentPartKind::Box,
                    referenced_definition_id: None,
                    repeat: None,
                    visible: true,
                },
                ComponentPartSummary {
                    id: "cap".into(),
                    parent_id: Some("body".into()),
                    kind: ComponentPartKind::ComponentReference,
                    referenced_definition_id: Some("component-000002".into()),
                    repeat: Some(30),
                    visible: true,
                },
            ],
            part_count: 2,
            parts_truncated: false,
            instance_count: 4,
            metadata: BTreeMap::new(),
        },
        definition: None,
    };
    let value = serde_json::to_value(&result).unwrap();

    assert_eq!(value["revision"], 2);
    let component = &value["component"];
    assert_eq!(component["id"], "component-000001");
    assert_eq!(component["partCount"], 2);
    assert_eq!(component["partsTruncated"], false);
    assert_eq!(component["instanceCount"], 4);
    assert_eq!(component["metadata"], serde_json::json!({}));
    assert_eq!(component["parts"][0]["kind"], "box");
    assert_eq!(component["parts"][1]["kind"], "component_reference");
    assert_eq!(
        component["parts"][1]["referencedDefinitionId"],
        "component-000002"
    );
    assert_eq!(component["parts"][1]["repeat"], 30);
    // 摘要模式：definition 键不出现（skip_serializing_if）。
    assert_eq!(value.get("definition"), None);
}

#[test]
fn component_get_typescript_marks_summary_fields_required() {
    let params = ComponentGetParams::decl();
    assert!(params.contains("componentId: string"));

    let summary = ComponentDefinitionSummary::decl();
    assert!(summary.contains("partCount: number"));
    assert!(summary.contains("partsTruncated: boolean"));
    assert!(summary.contains("instanceCount: number"));
    assert!(summary.contains("parameters: Array<ComponentParameter>"));
    assert!(summary.contains("ports: Array<ComponentPort>"));

    let part_summary = ComponentPartSummary::decl();
    assert!(part_summary.contains("kind: ComponentPartKind"));
    assert!(part_summary.contains("referencedDefinitionId: ComponentId | null"));

    assert_eq!(MAX_PART_SUMMARIES, 100);
}
