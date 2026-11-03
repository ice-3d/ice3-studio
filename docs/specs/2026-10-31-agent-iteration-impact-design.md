# Agent Iteration Impact Analysis

> Document ID: ICE3-STUDIO-SPEC-010
>
> Status: Historical
>
> Owner: ICE3 Studio maintainers

> **历史状态：** 本文保留已完成设计过程的审计记录，不代表当前生产依赖；当前能力以
> [Asset Pipeline](../reference/asset-pipeline.md) 和 [能力清单](../reference/capabilities.md)
> 为准。

## 1. Summary

AI-driven modeling is iterative by nature. An Agent will make a small change,
inspect the result, correct it, and repeat. The public API therefore needs to tell
the Agent how a proposed ChangeSet will affect the scene before approval, not only
whether the final Document validates.

This design adds `changeset.impact`, a proposal-level analysis method that:

- evaluates the current ChangeSet using existing transaction semantics;
- reports whether the change is local, targeted, or requires a full rebuild;
- reports the number of affected source and derived nodes;
- executes the existing incremental scene planner against the candidate;
- reports reused geometry, generated geometry, cache hits, and cache misses;
- returns machine-readable reasons for the affected areas.

The existing `changeset.evaluate`, `changeset.commit`, document response shapes,
and ChangeSet persistence format remain compatible.

## 2. Goals

- Let an Agent determine whether an iterative edit is narrowly scoped before
  requesting human approval.
- Detect accidental broad or full-scene invalidation early.
- Reuse the same ChangeSet validation, candidate digest, and scene dependency
  rules used by the existing evaluation and commit path.
- Report exact cache reuse and geometry generation counts for the candidate on
  the current host, subject to configured cache policy.
- Keep responses bounded; impact analysis returns counts and reasons rather than
  complete geometry or affected-node arrays.
- Preserve the Document as the only durable source of truth. Scene analysis and
  cache entries remain derived state.

## 3. Non-Goals

- Committing, approving, or discarding a ChangeSet.
- Replacing `changeset.evaluate` or `changeset.getCandidate`.
- Returning screenshots, visual diffs, collision reports, or camera data.
- Guaranteeing that a later commit has identical cache counters if the cache is
  evicted or the device changes between analysis and commit.
- Giving an Agent direct access to the scene cache or GPU-resident buffers.
- Defining natural-language planning, prompts, or Agent policy.

## 4. Constraints

- The method requires proposal authority in `review-required` mode.
- The method never mutates the committed Document or advances its revision.
- Existing public RPC response shapes remain unchanged.
- Candidate evaluation uses existing atomic operation validation.
- Impact analysis must obey ChangeSet version and revision checks.
- A blocked or stale ChangeSet returns structured impact status instead of a
  partial scene forecast.
- Impact analysis may populate content-addressed derived cache entries, but it
  may not publish a revision, scene event, or renderer scene update.
- All new request and response types use camelCase JSON and `deny_unknown_fields`
  where applicable.

## 5. Method Contract

### 5.1 Method

`changeset.impact`

### 5.2 Request

```text
ChangeSetImpactParams {
  changeSetId: string
  expectedVersion: number
}
```

### 5.3 Response

```text
ChangeSetImpactResult {
  changeSetId: string
  version: number
  baseRevision: number
  status: ChangeSetStatus
  candidateDigest: string | null
  diagnosticCount: number
  impact: IterationImpact
}
```

```text
IterationImpact {
  scope: "local" | "targeted" | "broad" | "full_rebuild" | "unavailable"
  fullSceneRebuildRequired: boolean
  sourceNodes: ChangeImpactCounts
  derivedNodes: ChangeImpactCounts
  relations: ChangeImpactCounts
  components: ChangeImpactCounts
  materials: ChangeImpactCounts
  affectedSourceNodes: number
  affectedDerivedNodes: number
  reusedGeometry: number
  generatedGeometry: number
  cacheHits: number
  cacheMisses: number
  reasons: IterationImpactReason[]
}

ChangeImpactCounts {
  added: number
  updated: number
  removed: number
}
```

`IterationImpactReason` is one of:

```text
node_content
node_transform
instance_parameters
material_assignment
component_definition
relation_topology
constraint
full_rebuild
validation_blocked
```

Reasons are deduplicated and sorted in the order shown. They describe causes, not
individual affected objects. The existing `changeset.evaluate` remains the API
for complete added/updated/removed ID lists when a client needs them.

## 6. Impact Semantics

### 6.1 Source Changes

The impact analyzer reads the candidate `DocumentDiff` produced by existing
ChangeSet evaluation:

- source node counts come from added, updated, and removed node IDs;
- relation, component, and material counts come from their matching diff arrays;
- these counts describe durable source changes only.

### 6.2 Derived Impact

The analyzer uses the existing `SceneSourceIndex` dependency rules:

- a derivative diff with a known set of affected source nodes is a targeted
  analysis;
- a diff that cannot use the incremental planner requires a full rebuild;
- affected derived nodes are counted from the candidate scene fragment produced
  by `build_scene_patch_with_scene`;
- added, updated, and removed derived counts compare candidate fragment IDs with
  the previous evaluated scene.

No derived IDs are returned in this phase.

### 6.3 Geometry And Cache

The analyzer runs the same selected-scene evaluator used by commit, using the
current scene cache and evaluator policy. It reports:

- `reusedGeometry`: geometry already available without evaluation;
- `generatedGeometry`: geometry evaluated for the candidate;
- `cacheHits` and `cacheMisses`: exact counters returned by that analysis pass.

Analysis may write reusable candidate geometry into the content-addressed cache.
It must not merge the candidate scene into `scene_snapshot`, publish a scene patch,
or emit a revision event.

### 6.4 Scope Classification

Classification is deterministic:

- `unavailable`: candidate status is `blocked` or `stale`, so no scene forecast
  is produced;
- `full_rebuild`: the existing dependency planner cannot select affected source
  nodes;
- `local`: no component, relation, or constraint changes; at most one source node
  changes; and no full rebuild is required;
- `broad`: incremental analysis succeeds, the change does not meet `local`, and
  the dependency closure affects more than 1,000 derived nodes;
- `targeted`: all remaining incremental changes.

`fullSceneRebuildRequired` is `true` only for `full_rebuild`. It is `false` for
`local`, `targeted`, and `broad`. It is `false` for `unavailable`; callers must
consult `scope` rather than treating unavailable as a successful small edit.

## 7. Evaluation And State

`changeset.impact` evaluates the ChangeSet as part of the request and persists the
same candidate status, digest, and diagnostics as `changeset.evaluate`. This makes
it usable as the normal Agent evaluation step:

```text
add/remove operations
      |
      v
changeset.impact
      |
      +-- candidateDigest
      +-- validation diagnostics count
      +-- local/targeted/broad classification
      +-- cache and geometry forecast
      |
      v
human approval and changeset.commit
```

An invalid `expectedVersion` fails with the existing version-conflict contract.
A stale base returns `status: "stale"` and `scope: "unavailable"`.
A blocked candidate returns `status: "blocked"`, `diagnosticCount > 0`, and
`scope: "unavailable"`.

A successful analysis returns the same `candidateDigest` that
`changeset.commit` will verify.

## 8. Authorization

`changeset.impact` is proposal-scoped because it evaluates and persists
ChangeSet state. In review-required mode:

- a proposal token can call it;
- a read-only client cannot call it;
- an approval capability can call it;
- an autonomous writer can call it.

It never grants commit or direct-write authority.

## 9. Manifest

Add `changeset.impact` to `system.agentManifest`:

```text
name: changeset.impact
sideEffect: proposal
requiresApproval: false
idempotency: none
inputSchemaRef: ...#/types/ChangeSetImpactParams
outputSchemaRef: ...#/types/ChangeSetImpactResult
```

Update the `propose` workflow:

```text
changeset.create
changeset.addOperations
changeset.impact
```

`changeset.evaluate` remains advertised for clients that need the complete diff.

## 10. Error And Bound Rules

- Unknown ChangeSet: existing project not-found contract.
- Version mismatch: existing operation-invalid conflict.
- Stale project revision: `status: "stale"`, `scope: "unavailable"`.
- Blocked candidate: `status: "blocked"`, `scope: "unavailable"`.
- Responses contain counts only; no source, derived, or geometry payload arrays.
- The method must not exceed the existing RPC body limit; requests contain only
  ChangeSet identity and expected version.
- Candidate cache writes must use the existing content-addressed limits and
  eviction policy.

## 11. Performance Requirements

On the existing detailed wastewater fixture:

- a single material or instance patch must not require a full scene rebuild;
- an unbound parameter change must reuse all unaffected part geometry;
- a local material or transform-only analysis must report zero generated geometry
  when the required cache entries are available;
- adding one pump and one connection must generate only the new pump and route
  geometry, not regenerate unrelated plant geometry;
- impact analysis must produce deterministic counts for an unchanged candidate
  and cache state.

Cache counters are host and eviction dependent. Tests assert invariants, not
machine-specific latency.

## 12. Testing

### 12.1 Core And Server

- A material-only ChangeSet reports `scope: "local"`, one updated source node, no
  component/relation change, and no generated geometry after cache warm-up.
- A transform and unbound-parameter patch reports `scope: "local"`, one updated
  source node, cache reuse, and zero generated geometry.
- A new pump plus connection reports `scope: "targeted"`, two added source nodes,
  one added relation, and generated geometry limited to the new elements.
- A component-definition change reports every direct instance through the existing
  dependency closure.
- An unsupported relation/constraint change reports `full_rebuild`.
- A validation-blocked candidate reports `unavailable` and never emits a partial
  scene forecast.
- Stale revision and version conflicts preserve existing diagnostics.
- Proposal, read-only, approval, and autonomous manifest permissions match
  dispatch-time authorization.
- `changeset.impact` returns the same candidate digest as the existing evaluator.
- Existing ChangeSet methods and response shapes remain unchanged.

### 12.2 Agent Acceptance

After implementation, use the current AI Agent against an existing detailed
wastewater project to:

1. read `system.agentManifest` and confirm `changeset.impact` is available;
2. make a material-only edit and verify a local impact report;
3. modify one instance transform and unbound parameter and verify no geometry
   regeneration;
4. add one local pump and pipe and verify only local geometry generation;
5. reject at least one deliberately invalid connection and confirm an unavailable
   impact report;
6. approve and commit each valid ChangeSet;
7. compare reported metrics with post-commit `evaluation.stats`;
8. record wall-clock time, request bytes, cache hits, generated geometry, and
   whether any unrelated derived nodes changed.

## 13. Documentation

Update:

- API reference with request, response, scope, and cache semantics;
- ChangeSet review guide with the iterative Agent workflow;
- Agent manifest documentation and generated JSON Schema and TypeScript types;
- diagnostics documentation for unavailable impact states;
- the detailed wastewater validation report with measured local-change evidence.

## 14. Acceptance Criteria

The feature is complete when:

- all existing 14 verification gates pass;
- the new focused tests pass;
- existing ChangeSet response shapes remain unchanged;
- the current AI Agent completes repeated material, instance, topology, and failed
  validation iterations against an existing large model;
- every successful local iteration reports no full rebuild;
- reported cache and generated-geometry counts match the post-commit scene
  statistics for the same cache state.
