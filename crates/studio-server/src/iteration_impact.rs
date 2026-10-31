use std::collections::BTreeSet;

use studio_core::agent_contract::{
    ChangeImpactCounts, IterationImpact, IterationImpactReason, IterationImpactScope,
};
use studio_core::diff::DocumentDiff;
use studio_core::document::Document;
use studio_core::operation::Operation;
use studio_core::validation::DiagnosticCode;

use crate::compute::ComputeEvaluator;
use crate::rpc::{RpcError, Session, ensure_scene_snapshot};
use crate::scene_session::build_scene_patch_with_scene;

pub(crate) fn analyze_impact(
    session: &mut Session,
    candidate: &Document,
    diff: &DocumentDiff,
    operations: &[Operation],
) -> Result<IterationImpact, RpcError> {
    ensure_scene_snapshot(session)?;
    let previous_scene = session
        .scene_snapshot
        .as_ref()
        .ok_or_else(|| impact_error("scene snapshot is unavailable"))?;
    let index = session
        .scene_source_index
        .as_ref()
        .ok_or_else(|| impact_error("scene source index is unavailable"))?;
    let evaluator = ComputeEvaluator::new(&session.compute);
    let built = build_scene_patch_with_scene(
        candidate,
        diff,
        previous_scene,
        index,
        &mut session.scene_cache,
        Some(&evaluator),
    )
    .map_err(|error| impact_error(&error.to_string()))?;

    let previous_ids = if let Some(affected) = &built.affected_source_nodes {
        affected
            .iter()
            .flat_map(|source| index.expanded_node_ids(source).map(str::to_owned))
            .collect::<BTreeSet<_>>()
    } else {
        previous_scene
            .nodes
            .iter()
            .map(|node| node.id.clone())
            .collect()
    };
    let current_ids = built
        .scene
        .nodes
        .iter()
        .map(|node| node.id.clone())
        .collect::<BTreeSet<_>>();
    let derived_nodes = ChangeImpactCounts {
        added: current_ids.difference(&previous_ids).count(),
        updated: current_ids.intersection(&previous_ids).count(),
        removed: previous_ids.difference(&current_ids).count(),
    };

    let source_change_count =
        diff.added_node_ids.len() + diff.updated_node_ids.len() + diff.removed_node_ids.len();
    let structural_change = !diff.added_component_ids.is_empty()
        || !diff.updated_component_ids.is_empty()
        || !diff.removed_component_ids.is_empty()
        || !diff.added_relation_ids.is_empty()
        || !diff.updated_relation_ids.is_empty()
        || !diff.removed_relation_ids.is_empty()
        || !diff.added_constraint_ids.is_empty()
        || !diff.updated_constraint_ids.is_empty()
        || !diff.removed_constraint_ids.is_empty();
    let affected_source_nodes = built
        .affected_source_nodes
        .as_ref()
        .map_or(candidate.scene.nodes().len(), BTreeSet::len);
    let scope = if built.affected_source_nodes.is_none() {
        IterationImpactScope::FullRebuild
    } else if source_change_count <= 1 && !structural_change {
        IterationImpactScope::Local
    } else if built.scene.nodes.len() > 1_000 {
        IterationImpactScope::Broad
    } else {
        IterationImpactScope::Targeted
    };

    let mut reasons = BTreeSet::new();
    if scope == IterationImpactScope::FullRebuild {
        reasons.insert(IterationImpactReason::FullRebuild);
    }
    for operation in operations {
        operation_reasons(operation, &mut reasons);
    }

    Ok(IterationImpact {
        scope,
        full_scene_rebuild_required: scope == IterationImpactScope::FullRebuild,
        source_nodes: ChangeImpactCounts {
            added: diff.added_node_ids.len(),
            updated: diff.updated_node_ids.len(),
            removed: diff.removed_node_ids.len(),
        },
        derived_nodes,
        relations: ChangeImpactCounts {
            added: diff.added_relation_ids.len(),
            updated: diff.updated_relation_ids.len(),
            removed: diff.removed_relation_ids.len(),
        },
        components: ChangeImpactCounts {
            added: diff.added_component_ids.len(),
            updated: diff.updated_component_ids.len(),
            removed: diff.removed_component_ids.len(),
        },
        materials: ChangeImpactCounts {
            added: diff.added_material_ids.len(),
            updated: diff.updated_material_ids.len(),
            removed: diff.removed_material_ids.len(),
        },
        affected_source_nodes,
        affected_derived_nodes: built.scene.nodes.len(),
        reused_geometry: built.cache_hits,
        generated_geometry: built.generated_meshes,
        cache_hits: built.cache_hits,
        cache_misses: built.cache_misses,
        reasons: reasons.into_iter().collect(),
    })
}

fn operation_reasons(operation: &Operation, reasons: &mut BTreeSet<IterationImpactReason>) {
    match operation {
        Operation::InstancePatch(patch) => {
            if patch.transform.is_some() {
                reasons.insert(IterationImpactReason::NodeTransform);
            }
            if !patch.set_parameters.is_empty() || !patch.remove_parameters.is_empty() {
                reasons.insert(IterationImpactReason::InstanceParameters);
            }
            if patch.material_id.is_some() || patch.clear_material {
                reasons.insert(IterationImpactReason::MaterialAssignment);
            }
            if patch.transform.is_none()
                && patch.set_parameters.is_empty()
                && patch.remove_parameters.is_empty()
                && patch.material_id.is_none()
                && !patch.clear_material
            {
                reasons.insert(IterationImpactReason::NodeContent);
            }
        }
        Operation::NodeUpdate { transform, .. } => {
            reasons.insert(if transform.is_some() {
                IterationImpactReason::NodeTransform
            } else {
                IterationImpactReason::NodeContent
            });
        }
        Operation::MaterialPatch(_)
        | Operation::MaterialUpsert { .. }
        | Operation::MaterialAssign { .. } => {
            reasons.insert(IterationImpactReason::MaterialAssignment);
        }
        Operation::ComponentDefine { .. }
        | Operation::ComponentUpdate { .. }
        | Operation::ComponentDelete { .. }
        | Operation::ComponentPatch(_) => {
            reasons.insert(IterationImpactReason::ComponentDefinition);
        }
        Operation::ConnectionCreate { .. }
        | Operation::ConnectionUpdate { .. }
        | Operation::ConnectionDelete { .. }
        | Operation::ConnectionPatch(_)
        | Operation::RelationCreate { .. }
        | Operation::RelationUpdate { .. }
        | Operation::RelationDelete { .. } => {
            reasons.insert(IterationImpactReason::RelationTopology);
        }
        Operation::ConstraintUpsert { .. } | Operation::ConstraintDelete { .. } => {
            reasons.insert(IterationImpactReason::Constraint);
        }
        _ => {
            reasons.insert(IterationImpactReason::NodeContent);
        }
    }
}

fn impact_error(message: &str) -> RpcError {
    RpcError::with_diagnostic(
        -32603,
        DiagnosticCode::GeometryInvalid.as_str(),
        format!("{}: {message}", DiagnosticCode::GeometryInvalid.as_str()),
        None,
    )
}
