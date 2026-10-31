use studio_core::API_VERSION;
use studio_core::agent_contract::{
    AgentIdempotency, AgentLimits, AgentManifest, AgentSideEffect, AgentTool, AgentWorkflow,
};

use crate::capabilities::{RequestAuthority, agent_access};

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
