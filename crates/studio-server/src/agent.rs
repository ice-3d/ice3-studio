use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};

use sha2::{Digest, Sha256};
use studio_core::API_VERSION;
use studio_core::PROJECT_SCHEMA_VERSION;
use studio_core::agent_contract::{
    AgentIdempotency, AgentLimits, AgentManifest, AgentSideEffect, AgentTool, AgentWorkflow,
    MAX_SEARCH_LIMIT, RelationTypeCount, SceneContentKind, SceneContentKindCount, SceneContext,
    SceneCoordinateSystem, SceneCounts, SceneFacets, SceneSearchField, SceneSearchItem,
    SceneSearchRequest, SceneSearchResult, SceneSearchScope,
};
use studio_core::document::NodeContent;
use studio_core::geometry::Bounds;
use studio_core::large_scene::evaluate::evaluated_scene_bounds;
use studio_core::math::Vec3;
use studio_core::validation::DiagnosticCode;

use crate::capabilities::{RequestAuthority, agent_access};
use crate::rpc::{RpcError, Session, ensure_scene_snapshot};

const SCHEMA_URL: &str = "/schemas/studio-api.schema.json";
const TYPES_URL: &str = "/schemas/studio-types.ts";

pub(crate) fn manifest(authority: RequestAuthority) -> AgentManifest {
    let access = agent_access(authority);
    AgentManifest {
        api_version: API_VERSION.into(),
        contract_version: "1.0".into(),
        schema_url: SCHEMA_URL.into(),
        types_url: TYPES_URL.into(),
        access: access.clone(),
        limits: AgentLimits::default(),
        tools: vec![
            tool(
                "system.agentManifest",
                "Describe available modeling tools and current authority.",
                AgentSideEffect::Read,
                false,
                AgentIdempotency::None,
                access.can_read,
                "read access is unavailable",
                "AgentManifest",
                Some("AgentManifest"),
            ),
            tool(
                "scene.context",
                "Return a bounded project summary.",
                AgentSideEffect::Read,
                false,
                AgentIdempotency::None,
                access.can_read,
                "read access is unavailable",
                "SceneContext",
                Some("SceneContext"),
            ),
            tool(
                "scene.search",
                "Search source or derived scene nodes with bounded pagination.",
                AgentSideEffect::Read,
                false,
                AgentIdempotency::None,
                access.can_read,
                "read access is unavailable",
                "SceneSearchRequest",
                Some("SceneSearchResult"),
            ),
            tool(
                "evaluation.query",
                "Query summaries or payloads for a small explicit set of derived nodes.",
                AgentSideEffect::Read,
                false,
                AgentIdempotency::None,
                access.can_read,
                "read access is unavailable",
                "EvaluationQueryRequest",
                Some("EvaluationQueryResult"),
            ),
            tool(
                "changeset.create",
                "Create a persistent proposal based on a committed revision.",
                AgentSideEffect::Proposal,
                false,
                AgentIdempotency::None,
                access.can_propose,
                "proposal access is unavailable",
                "ChangeSet",
                Some("ChangeSet"),
            ),
            tool(
                "changeset.addOperations",
                "Add typed modeling operations to a proposal.",
                AgentSideEffect::Proposal,
                false,
                AgentIdempotency::None,
                access.can_propose,
                "proposal access is unavailable",
                "Operation",
                Some("ChangeSet"),
            ),
            tool(
                "changeset.evaluate",
                "Evaluate a proposal and return its diff and diagnostics.",
                AgentSideEffect::Proposal,
                false,
                AgentIdempotency::None,
                access.can_propose,
                "proposal access is unavailable",
                "ChangeSetEvaluation",
                Some("ChangeSetEvaluation"),
            ),
            tool(
                "changeset.getCandidate",
                "Return the current evaluated proposal candidate.",
                AgentSideEffect::Read,
                false,
                AgentIdempotency::None,
                access.can_read,
                "read access is unavailable",
                "ChangeSet",
                Some("ChangeSetEvaluation"),
            ),
            tool(
                "changeset.commit",
                "Commit an approved candidate digest.",
                AgentSideEffect::Commit,
                true,
                AgentIdempotency::Required,
                access.can_commit,
                "commit access is unavailable",
                "ApplyRequest",
                Some("ApplyResult"),
            ),
            tool(
                "document.apply",
                "Apply one atomic modeling transaction.",
                AgentSideEffect::DirectWrite,
                true,
                AgentIdempotency::Required,
                access.can_apply_directly,
                "direct write access is unavailable",
                "ApplyRequest",
                Some("ApplyResult"),
            ),
            operation_tool(
                "instance.patch",
                "Patch selected fields of a component instance.",
                "InstancePatch",
                access.can_propose,
            ),
            operation_tool(
                "component.patch",
                "Patch selected fields of a component definition.",
                "ComponentPatch",
                access.can_propose,
            ),
            operation_tool(
                "connection.patch",
                "Patch selected fields of a connection.",
                "ConnectionPatch",
                access.can_propose,
            ),
            operation_tool(
                "material.patch",
                "Patch selected fields of a material.",
                "MaterialPatch",
                access.can_propose,
            ),
        ],
        workflows: vec![
            workflow(
                "inspect",
                ["scene.context", "scene.search|evaluation.query"],
            ),
            workflow(
                "propose",
                [
                    "changeset.create",
                    "changeset.addOperations",
                    "changeset.evaluate",
                ],
            ),
            workflow(
                "review",
                ["changeset.getCandidate", "export.runtimePreview"],
            ),
            workflow("commit", ["changeset.commit"]),
            workflow("recover", ["scene.context", "changeset.evaluate"]),
        ],
    }
}

#[allow(clippy::too_many_arguments)]
fn tool(
    name: &str,
    summary: &str,
    side_effect: AgentSideEffect,
    requires_approval: bool,
    idempotency: AgentIdempotency,
    available: bool,
    unavailable_reason: &str,
    input: &str,
    output: Option<&str>,
) -> AgentTool {
    AgentTool {
        name: name.into(),
        summary: summary.into(),
        side_effect,
        requires_approval,
        idempotency,
        available,
        unavailable_reason: (!available).then(|| unavailable_reason.into()),
        input_schema_ref: schema_ref(input),
        output_schema_ref: output.map(schema_ref),
    }
}

fn operation_tool(name: &str, summary: &str, schema: &str, available: bool) -> AgentTool {
    tool(
        name,
        summary,
        AgentSideEffect::Proposal,
        false,
        AgentIdempotency::None,
        available,
        "proposal access is unavailable",
        schema,
        None,
    )
}

fn workflow<const N: usize>(name: &str, steps: [&str; N]) -> AgentWorkflow {
    AgentWorkflow {
        name: name.into(),
        steps: steps.into_iter().map(str::to_owned).collect(),
    }
}

fn schema_ref(value: &str) -> String {
    format!("{SCHEMA_URL}#/types/{value}")
}

pub(crate) fn context(
    session: &mut Session,
    authority: RequestAuthority,
) -> Result<SceneContext, RpcError> {
    ensure_scene_snapshot(session)?;
    let scene = session
        .scene_snapshot
        .as_ref()
        .expect("scene snapshot was ensured");
    let bounds_by_id = evaluated_scene_bounds(scene);
    let bounds = union_bounds(bounds_by_id.values().copied());

    let content_kinds = [
        SceneContentKind::Group,
        SceneContentKind::Primitive,
        SceneContentKind::Mesh,
        SceneContentKind::Sweep,
        SceneContentKind::ComponentInstance,
        SceneContentKind::InstanceSet,
        SceneContentKind::Connection,
    ]
    .into_iter()
    .filter_map(|kind| {
        let count = session
            .document
            .scene
            .nodes()
            .iter()
            .filter(|node| content_kind(&node.content) == kind)
            .count();
        (count > 0).then_some(SceneContentKindCount { kind, count })
    })
    .collect();
    let mut relation_types = session
        .document
        .relations
        .iter()
        .fold(BTreeMap::<String, usize>::new(), |mut counts, relation| {
            *counts.entry(relation.relation_type.clone()).or_default() += 1;
            counts
        })
        .into_iter()
        .map(|(relation_type, count)| RelationTypeCount {
            relation_type,
            count,
        })
        .collect::<Vec<_>>();
    relation_types.sort_by_key(|facet| (Reverse(facet.count), facet.relation_type.clone()));
    let relation_types_truncated = relation_types.len() > 50;
    relation_types.truncate(50);

    let instances = session
        .document
        .scene
        .nodes()
        .iter()
        .filter(|node| matches!(node.content, NodeContent::ComponentInstance(_)))
        .count();
    let instance_sets = session
        .document
        .scene
        .nodes()
        .iter()
        .filter(|node| matches!(node.content, NodeContent::InstanceSet(_)))
        .count();
    Ok(SceneContext {
        project_id: session.document.project_id.clone(),
        revision: session.document.revision.as_u64(),
        api_version: API_VERSION.into(),
        schema_version: PROJECT_SCHEMA_VERSION,
        units: "meters".into(),
        coordinate_system: SceneCoordinateSystem {
            handedness: "right".into(),
            up_axis: "Y".into(),
        },
        bounds,
        counts: SceneCounts {
            source_nodes: scene.stats.source_nodes,
            derived_nodes: scene.stats.expanded_nodes,
            component_definitions: session.document.components.len(),
            instances,
            instance_sets,
            relations: session.document.relations.len(),
            materials: session.document.materials.len(),
            constraints: session.document.constraints.len(),
        },
        facets: SceneFacets {
            content_kinds,
            relation_types,
            relation_types_truncated,
        },
        access: agent_access(authority),
        limits: AgentLimits::default(),
    })
}

pub(crate) fn search(
    session: &mut Session,
    request: SceneSearchRequest,
) -> Result<SceneSearchResult, RpcError> {
    if request.limit == 0 || request.limit > MAX_SEARCH_LIMIT {
        return Err(api_invalid("/limit", "limit must be between 1 and 100"));
    }
    if request.scope == SceneSearchScope::Derived {
        return Err(api_invalid(
            "/scope",
            "derived scene search is not available in this delivery step",
        ));
    }
    if request.scope == SceneSearchScope::Source && request.owner_id.is_some() {
        return Err(api_invalid(
            "/ownerId",
            "ownerId is only valid for derived scene search",
        ));
    }

    let fields = normalize_fields(request.fields.as_deref())?;
    let digest = normalize_query(&request, &fields)?;
    let revision = session.document.revision.as_u64();
    let offset = match request.cursor.as_deref() {
        Some(cursor) => decode_cursor(cursor, revision, &digest)?,
        None => 0,
    };

    ensure_scene_snapshot(session)?;
    let scene = session
        .scene_snapshot
        .as_ref()
        .expect("scene snapshot was ensured");
    let bounds_by_id = evaluated_scene_bounds(scene);
    let relation_nodes = relation_node_filter(session, &request);
    let query = request.query.as_deref().map(str::to_lowercase);
    let mut matches = session
        .document
        .scene
        .nodes()
        .iter()
        .filter(|node| {
            request
                .node_id
                .as_ref()
                .is_none_or(|node_id| node.id.as_str() == node_id)
        })
        .filter(|node| {
            request.parent_id.as_ref().is_none_or(|parent_id| {
                node.parent_id
                    .as_ref()
                    .is_some_and(|id| id.as_str() == parent_id)
            })
        })
        .filter(|node| {
            request
                .metadata
                .iter()
                .all(|(key, value)| node.metadata.get(key) == Some(value))
        })
        .filter(|node| {
            query.as_ref().is_none_or(|query| {
                node.id.as_str().to_lowercase().contains(query)
                    || node.name.to_lowercase().contains(query)
            })
        })
        .filter(|node| {
            request
                .content_kind
                .is_none_or(|kind| content_kind(&node.content) == kind)
        })
        .filter(|node| {
            request
                .component_definition_id
                .as_ref()
                .is_none_or(|id| match &node.content {
                    NodeContent::ComponentInstance(instance) => instance.definition_id == *id,
                    NodeContent::InstanceSet(set) => set.definition_id == *id,
                    _ => false,
                })
        })
        .filter(|node| {
            request
                .material_id
                .as_ref()
                .is_none_or(|id| node.material_id.as_ref() == Some(id))
        })
        .filter(|node| {
            request
                .visible
                .is_none_or(|visible| node.visible == visible)
        })
        .filter(|node| {
            relation_nodes
                .as_ref()
                .is_none_or(|nodes| nodes.contains(node.id.as_str()))
        })
        .filter(|node| {
            request.bounds.is_none_or(|bounds| {
                bounds_by_id
                    .get(node.id.as_str())
                    .copied()
                    .is_some_and(|candidate| bounds_intersect(candidate, bounds))
            })
        })
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| left.id.cmp(&right.id));

    let page_end = offset.saturating_add(request.limit).min(matches.len());
    let page = if offset < matches.len() {
        &matches[offset..page_end]
    } else {
        &[]
    };
    let evaluated_by_id = scene
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    let items = page
        .iter()
        .map(|node| {
            let selected = |field| fields.as_ref().is_none_or(|fields| fields.contains(&field));
            let component_definition_id = match &node.content {
                NodeContent::ComponentInstance(instance) => Some(instance.definition_id.clone()),
                NodeContent::InstanceSet(set) => Some(set.definition_id.clone()),
                _ => None,
            };
            SceneSearchItem {
                id: node.id.to_string(),
                scope: SceneSearchScope::Source,
                kind: content_kind(&node.content),
                name: selected(SceneSearchField::Name).then(|| node.name.clone()),
                parent_id: selected(SceneSearchField::ParentId)
                    .then(|| node.parent_id.as_ref().map(ToString::to_string))
                    .flatten(),
                owner_id: selected(SceneSearchField::OwnerId)
                    .then_some(None)
                    .flatten(),
                visible: selected(SceneSearchField::Visible).then_some(node.visible),
                component_definition_id: selected(SceneSearchField::ComponentDefinitionId)
                    .then_some(component_definition_id)
                    .flatten(),
                material_id: selected(SceneSearchField::MaterialId)
                    .then_some(node.material_id.clone())
                    .flatten(),
                geometry_id: selected(SceneSearchField::GeometryId)
                    .then(|| {
                        evaluated_by_id
                            .get(node.id.as_str())
                            .and_then(|node| node.mesh_key.clone())
                    })
                    .flatten(),
                bounds: selected(SceneSearchField::Bounds)
                    .then(|| bounds_by_id.get(node.id.as_str()).copied())
                    .flatten(),
                metadata: selected(SceneSearchField::Metadata).then(|| node.metadata.clone()),
            }
        })
        .collect();
    let next_cursor =
        (page_end < matches.len()).then(|| encode_cursor(revision, page_end, &digest));
    Ok(SceneSearchResult {
        revision,
        items,
        next_cursor,
    })
}

fn relation_node_filter(
    session: &Session,
    request: &SceneSearchRequest,
) -> Option<BTreeSet<String>> {
    if request.relation_type.is_none() && request.related_node_id.is_none() {
        return None;
    }
    let mut nodes = BTreeSet::new();
    for relation in &session.document.relations {
        if request
            .relation_type
            .as_ref()
            .is_some_and(|kind| relation.relation_type != *kind)
        {
            continue;
        }
        if request.related_node_id.as_ref().is_some_and(|node_id| {
            relation.from_node_id.as_str() != node_id && relation.to_node_id.as_str() != node_id
        }) {
            continue;
        }
        nodes.insert(relation.from_node_id.to_string());
        nodes.insert(relation.to_node_id.to_string());
    }
    Some(nodes)
}

fn normalize_fields(
    fields: Option<&[SceneSearchField]>,
) -> Result<Option<BTreeSet<SceneSearchField>>, RpcError> {
    let Some(fields) = fields else {
        return Ok(None);
    };
    let fields = fields.iter().copied().collect::<BTreeSet<_>>();
    for required in [
        SceneSearchField::Id,
        SceneSearchField::Scope,
        SceneSearchField::Kind,
    ] {
        if !fields.contains(&required) {
            return Err(api_invalid(
                "/fields",
                "fields must include id, scope, and kind",
            ));
        }
    }
    Ok(Some(fields))
}

fn normalize_query(
    request: &SceneSearchRequest,
    fields: &Option<BTreeSet<SceneSearchField>>,
) -> Result<String, RpcError> {
    let fields = fields
        .clone()
        .map(|fields| fields.into_iter().collect::<Vec<_>>());
    let bytes = serde_json::to_vec(&(
        &request.scope,
        &request.query,
        &request.node_id,
        &request.owner_id,
        &request.parent_id,
        &request.metadata,
        &request.content_kind,
        &request.component_definition_id,
        &request.material_id,
        request.visible,
        &request.relation_type,
        &request.related_node_id,
        request.bounds,
        fields,
    ))
    .map_err(|error| api_invalid("", &error.to_string()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn encode_cursor(revision: u64, offset: usize, digest: &str) -> String {
    format!("v1.{revision}.{offset}.{digest}")
}

fn decode_cursor(cursor: &str, revision: u64, digest: &str) -> Result<usize, RpcError> {
    let parts = cursor.split('.').collect::<Vec<_>>();
    if parts.len() != 4
        || parts[0] != "v1"
        || parts[3].len() != 64
        || !parts[3].bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(api_invalid("/cursor", "cursor is invalid"));
    }
    let cursor_revision = parts[1]
        .parse::<u64>()
        .map_err(|_| api_invalid("/cursor", "cursor revision is invalid"))?;
    let offset = parts[2]
        .parse::<usize>()
        .map_err(|_| api_invalid("/cursor", "cursor offset is invalid"))?;
    if cursor_revision != revision || parts[3] != digest {
        return Err(RpcError::with_diagnostic(
            -32010,
            DiagnosticCode::RevisionConflict.as_str(),
            format!(
                "{}: cursor belongs to a different revision or query",
                DiagnosticCode::RevisionConflict.as_str()
            ),
            Some(serde_json::json!({
                "category": "conflict",
                "retryable": true,
                "expected": revision,
                "actual": cursor_revision,
                "suggestions": ["restart scene.search without the cursor"]
            })),
        ));
    }
    Ok(offset)
}

fn content_kind(content: &NodeContent) -> SceneContentKind {
    match content {
        NodeContent::Group => SceneContentKind::Group,
        NodeContent::Primitive(_) => SceneContentKind::Primitive,
        NodeContent::CustomMesh(_) => SceneContentKind::Mesh,
        NodeContent::Sweep { .. } => SceneContentKind::Sweep,
        NodeContent::ComponentInstance(_) => SceneContentKind::ComponentInstance,
        NodeContent::InstanceSet(_) => SceneContentKind::InstanceSet,
        NodeContent::Connection(_) => SceneContentKind::Connection,
    }
}

fn union_bounds(bounds: impl Iterator<Item = Bounds>) -> Option<Bounds> {
    let mut bounds = bounds;
    let first = bounds.next()?;
    Some(bounds.fold(first, |left, right| Bounds {
        min: Vec3 {
            x: left.min.x.min(right.min.x),
            y: left.min.y.min(right.min.y),
            z: left.min.z.min(right.min.z),
        },
        max: Vec3 {
            x: left.max.x.max(right.max.x),
            y: left.max.y.max(right.max.y),
            z: left.max.z.max(right.max.z),
        },
    }))
}

fn bounds_intersect(left: Bounds, right: Bounds) -> bool {
    left.min.x <= right.max.x
        && left.max.x >= right.min.x
        && left.min.y <= right.max.y
        && left.max.y >= right.min.y
        && left.min.z <= right.max.z
        && left.max.z >= right.min.z
}

fn api_invalid(pointer: &str, message: &str) -> RpcError {
    RpcError::with_diagnostic(
        -32602,
        DiagnosticCode::ApiInvalid.as_str(),
        format!("{}: {message}", DiagnosticCode::ApiInvalid.as_str()),
        Some(serde_json::json!({
            "category": "validation",
            "retryable": false,
            "pointer": pointer,
            "suggestions": []
        })),
    )
}
