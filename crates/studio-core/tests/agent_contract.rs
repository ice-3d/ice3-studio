use std::collections::BTreeMap;

use studio_core::agent_contract::{
    AgentLimits, EvaluationInclude, EvaluationQueryRequest, SceneSearchField, SceneSearchRequest,
    SceneSearchScope,
};

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
