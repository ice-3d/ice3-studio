# Agent-Friendly Modeling API Design

> Document ID: ICE3-STUDIO-SPEC-009
>
> Status: Proposed
>
> Owner: ICE3 Studio maintainers

## 1. Summary

ICE3 Studio already has a deterministic Document, atomic transactions, revision
conflicts, idempotency, structured diagnostics, persistent ChangeSets, and generated
JSON Schema and TypeScript types. These properties make the product safe for an
external Agent to operate, but the current public API is still optimized for a
client that already knows the entire low-level data model.

This design adds a bounded Agent-facing capability layer without introducing a
second source of truth or a second mutation path:

- `system.agentManifest` describes callable tools and current token permissions.
- `scene.context` returns a fixed-size project summary.
- `scene.search` provides deterministic, paged discovery over source and derived
  scene entities.
- `evaluation.query` retrieves summaries or payloads for a small explicit set of
  derived nodes.
- Sparse patch operations update only named fields instead of requiring full-object
  replacement.

All mutations continue through `document.apply` or a ChangeSet and retain existing
validation, approval, persistence, undo, revision, and export behavior.

## 2. Goals

- Let an Agent discover the available tools and its current authority without
  reading implementation source.
- Let an Agent inspect a large project with bounded responses instead of fetching
  the complete Document or every evaluated mesh.
- Let an Agent identify both source nodes and expanded component parts.
- Let an Agent modify a subset of an instance, component, connection, or material
  without reconstructing unrelated fields.
- Preserve deterministic IDs, ordering, validation, and revision semantics.
- Keep the model provider, natural-language policy, and Agent runtime outside
  Studio Core.
- Preserve every existing public API response used by current clients.

## 3. Non-Goals

- Embedding a language model, model credentials, prompts, or Agent policy.
- Adding an MCP server, GraphQL endpoint, or model-specific function-calling
  adapter in this phase.
- Returning screenshots, camera controls, collision reports, or visual diffs.
  Those belong to the subsequent Agent validation phase.
- Making WebGPU, native GPU compute, or preview packets default-on.
- Giving Agents direct access to GPU resident buffers.
- Replacing `document.get`, `scene.get`, `scene.query`, or
  `evaluation.evaluate`.
- Supporting arbitrary JSON Patch or JSON Merge Patch. Ambiguous paths would
  weaken typed validation and diagnostic quality.

## 4. Constraints

- The Document remains the only durable modeling source of truth.
- CPU owns validation, revisions, approval, persistence, and canonical GLB export.
- Derived scene data remains disposable and may be rebuilt.
- Existing mutating methods remain unavailable to review-only proposal tokens.
- New requests use camelCase JSON, reject unknown fields, and have generated JSON
  Schema and TypeScript declarations.
- Scene results use deterministic ordering by `(scope, id)`.
- A paged query may not combine results from different committed revisions.
- API responses do not include tokens, filesystem paths outside existing preview
  contracts, model credentials, or private process state.

## 5. Architecture

The Agent capability layer is a read facade over existing authoritative and derived
state plus typed sparse operations in the existing Operation enum.

```text
External Agent
    |
    +-- system.agentManifest
    +-- scene.context
    +-- scene.search ---------------+
    +-- evaluation.query -----------+
    |                               |
    |                         Document / cached scene
    |                               |
    +-- ChangeSet operations -------+
            |
            v
      Core validation + atomic transaction
            |
            v
      Persisted Document revision
```

The read methods do not create a parallel project model. `scene.context` and
`scene.search` use the current Document. Derived search and `evaluation.query`
ensure the same cached evaluated scene used by `scene.bounds` and
`evaluation.stats`.

The patch operations are variants of the existing `Operation` type. They therefore
reuse ChangeSet entries, candidate digests, transaction diffs, atomic validation,
idempotency records, undo/redo, and revision events without special cases.

## 6. Common Response Rules

Agent-facing read responses include:

- `revision`: the committed revision represented by the response.
- `projectId` where the response is project-wide.

Identifiers, facets, search items, diagnostics, and suggested actions use stable
ordering. Empty optional collections serialize as `[]`, and absent nullable values
serialize as `null`.

The implementation must not change existing endpoint result shapes. Clients with no
knowledge of this specification continue to use current methods unchanged.

## 7. Tool Manifest

### 7.1 Method

`system.agentManifest`

This is a read method. It is available in both review-required and autonomous
modes.

### 7.2 Result

```text
AgentManifest {
  apiVersion: string
  contractVersion: "1.0"
  schemaUrl: "/schemas/studio-api.schema.json"
  typesUrl: "/schemas/studio-types.ts"
  access: AgentAccess
  limits: AgentLimits
  tools: AgentTool[]
  workflows: AgentWorkflow[]
}
```

`AgentTool`:

```text
{
  name: string
  summary: string
  sideEffect: "read" | "proposal" | "commit" | "direct_write"
  requiresApproval: boolean
  idempotency: "none" | "required"
  available: boolean
  unavailableReason: string | null
  inputSchemaRef: string
  outputSchemaRef: string | null
}
```

Schema references use JSON Pointer values into the advertised schema URL.

The manifest is descriptive. It does not execute, parse, or prioritize an Agent
plan.

### 7.3 Access

```text
AgentAccess {
  canRead: boolean
  canPropose: boolean
  canCommit: boolean
  canApplyDirectly: boolean
  canDiscard: boolean
}
```

The default `review-required` mode grants read and proposal access. A proposal
token cannot commit, discard, or call `document.apply`. An explicitly configured
`autonomous` writer may receive direct write and commit access.

### 7.4 Limits

```text
AgentLimits {
  searchDefaultLimit: 25
  searchMaxLimit: 100
  evaluationMaxNodes: 32
  maxRpcBodyBytes: 67108864
}
```

Limits are returned as numbers. They are descriptive mirrors of host-enforced
values and must not be treated as authorization.

### 7.5 Workflows

The manifest advertises short machine-readable workflows:

- `inspect`: `scene.context` then `scene.search` or `evaluation.query`.
- `propose`: create a ChangeSet, add typed operations, evaluate the candidate.
- `review`: request a candidate preview and wait for human approval.
- `commit`: commit the exact approved digest when the current token is authorized.
- `recover`: refresh context after `ICE3_STUDIO_REVISION_CONFLICT`.

Workflows describe tool ordering only. They do not contain natural-language
prompts.

## 8. Scene Context

### 8.1 Method

`scene.context`

Parameters: none.

### 8.2 Result

```text
SceneContext {
  projectId: string
  revision: number
  apiVersion: string
  schemaVersion: number
  units: "meters"
  coordinateSystem: {
    handedness: "right"
    upAxis: "Y"
  }
  bounds: Bounds | null
  counts: SceneCounts
  facets: SceneFacets
  access: AgentAccess
  limits: AgentLimits
}
```

`SceneCounts`:

```text
{
  sourceNodes: number
  derivedNodes: number
  componentDefinitions: number
  instances: number
  instanceSets: number
  relations: number
  materials: number
  constraints: number
}
```

`derivedNodes` uses the cached evaluated scene and includes expanded component
parts and connection routes.

`SceneFacets` contains aggregate counts only:

```text
{
  contentKinds: [{ kind: SceneContentKind, count: number }]
  relationTypes: [{ relationType: string, count: number }]
  relationTypesTruncated: boolean
}
```

`relationTypes` is limited to 50 deterministic entries. When more distinct
relation types exist, `relationTypesTruncated` is `true`. The context contains no
cursor for facets; this phase does not claim exhaustive relation-type discovery
when the cap is reached.

`SceneContentKind` is one of `group`, `primitive`, `mesh`, `sweep`,
`component_instance`, `instance_set`, or `connection`.

The context response has a target ceiling of 16 KiB for the large industrial
acceptance fixture. It never returns complete node, material, component, or mesh
collections.

## 9. Scene Search

### 9.1 Method

`scene.search`

### 9.2 Request

```text
SceneSearchRequest {
  scope: "source" | "derived" | "both" = "source"
  query: string | null = null
  nodeId: string | null = null
  ownerId: string | null = null
  parentId: string | null = null
  metadata: Record<string, string> = {}
  contentKind: SceneContentKind | null = null
  componentDefinitionId: string | null = null
  materialId: string | null = null
  visible: boolean | null = null
  relationType: string | null = null
  relatedNodeId: string | null = null
  bounds: Bounds | null = null
  fields: SceneSearchField[] | null = null
  cursor: string | null = null
  limit: number = 25
}
```

All filters combine with AND semantics.

`query` performs a case-insensitive substring match against stable ID and display
name. Metadata filters require exact key and value matches.

`bounds` intersects world-space bounds. Scope `source` uses source node world
bounds; scope `derived` uses derived node world bounds.

`relationType` and `relatedNodeId` select source nodes participating in a matching
relation. They are invalid with scope `derived`.

`limit` must be between 1 and 100.

### 9.3 Result

```text
SceneSearchResult {
  revision: number
  items: SceneSearchItem[]
  nextCursor: string | null
}
```

Default `SceneSearchItem` fields:

```text
{
  id: string
  scope: "source" | "derived"
  kind: SceneContentKind
  name: string
  parentId: string | null
  ownerId: string | null
  visible: boolean
  componentDefinitionId: string | null
  materialId: string | null
  geometryId: string | null
  bounds: Bounds | null
  metadata: Record<string, string>
}
```

`SceneSearchField` enumerates the item fields above. An explicitly supplied
projection omits fields not selected and must not contain unknown field names.
Required identity fields `id`, `scope`, and `kind` may not be omitted.

Scope `derived` returns deterministic expanded identities such as
`<instance>/<part>` and `<connection>/route`. It does not return vertex or index
payloads.

### 9.4 Cursor And Consistency

The cursor is an opaque URL-safe string containing:

- cursor format version;
- revision;
- normalized digest of scope, filters, and projection, excluding cursor and limit;
- next deterministic offset.

An invalid cursor returns `ICE3_STUDIO_API_INVALID`. A cursor from a different
query or revision returns `ICE3_STUDIO_REVISION_CONFLICT`, and the error data
includes the expected revision and current revision.

`nextCursor` is `null` only when no later item exists.

## 10. Evaluated Geometry Query

### 10.1 Method

`evaluation.query`

### 10.2 Request

```text
EvaluationQueryRequest {
  nodeIds: string[]
  include: ("summary" | "positions" | "indices")[] = ["summary"]
}
```

- `nodeIds` contains 1 to 32 unique derived node IDs.
- `positions` and `indices` require `summary`.
- Unknown node IDs fail the whole request.
- Source-only group nodes return `geometryId: null`.
- Result items follow the order of `nodeIds`.

### 10.3 Result

```text
EvaluationQueryResult {
  revision: number
  items: EvaluationQueryItem[]
}
```

```text
EvaluationQueryItem {
  nodeId: string
  geometryId: string | null
  vertexCount: number
  indexCount: number
  bounds: Bounds | null
  topologyDigest: string | null
  attributeDigest: string | null
  positions: [number, number, number][] | null
  indices: number[] | null
}
```

The default summary does not include positions or indices. Payloads are returned
only when explicitly requested.

## 11. Sparse Patch Operations

Sparse patches use the following general rules:

- At least one patchable field must be present.
- An absent field means no change.
- Explicit `null` on a field means no change. Clearing a nullable value requires
  the operation's explicit `clear*` field.
- `setMetadata` merges supplied keys.
- `removeMetadataKeys` removes supplied keys and is idempotent for missing keys.
- Setting and removing the same metadata key in one operation is invalid.
- Set and remove lists are deduplicated in deterministic order.
- Patch operations use the same operation index and diagnostic pointer semantics
  as current operations.
- A patch object containing no patchable fields is invalid. Supplying a field
  whose value already equals current state follows normal transaction and diff
  semantics.

### 11.1 `instance.patch`

```text
InstancePatch {
  type: "instance.patch"
  node_id: string
  name: string | null
  transform: Transform | null
  visible: boolean | null
  definition_id: string | null
  set_parameters: Record<string, ParameterValue>
  remove_parameters: string[]
  set_metadata: Record<string, string>
  remove_metadata_keys: string[]
  material_id: string | null
  clear_material: boolean
}
```

Only a field present in JSON changes state.

`setParameters` replaces named values and leaves unrelated parameters untouched.
Unknown parameter names, duplicate set/remove names, and type mismatches fail
validation.

`materialId` sets a material; `null` means no change. `clearMaterial: true`
clears it. Supplying both a non-null material ID and `clearMaterial: true` is
invalid.

Child-part overrides are not patched in this phase. Callers continue to use
`instance.update` for complex override replacement.

### 11.2 `component.patch`

```text
ComponentPatch {
  type: "component.patch"
  component_id: string
  name: string | null
  set_parameter_defaults: Record<string, ParameterValue>
  set_metadata: Record<string, string>
  remove_metadata_keys: string[]
}
```

Only definition-level metadata and parameter defaults are patchable. Replacing
parts continues to use `component.update`.

### 11.3 `connection.patch`

```text
ConnectionPatch {
  type: "connection.patch"
  node_id: string
  from: InstancePortRef | null
  to: InstancePortRef | null
  medium: string | null
  diameter: number | null
  route_policy: RoutePolicy | null
  set_metadata: Record<string, string>
  remove_metadata_keys: string[]
}
```

The linked relation and endpoint references remain validated before commit.

### 11.4 `material.patch`

```text
MaterialPatch {
  type: "material.patch"
  material_id: string
  name: string | null
  base_color: [number, number, number, number] | null
  opacity: number | null
  edge_color: [number, number, number, number] | null
  clear_edge_color: boolean
}
```

`clearEdgeColor: true` clears the edge color. Supplying a non-null `edgeColor`
and `clearEdgeColor: true` together is invalid.

### 11.5 Diff And Approval

A sparse patch participates in normal candidate digest calculation and produces a
normal updated-ID diff. No new approval token, revision rule, or persistence path
is introduced.

## 12. Structured Errors

New Agent-facing errors retain the existing JSON-RPC error code and
`diagnosticCode`. Optional `error.data` adds:

```text
AgentErrorData {
  category: "validation" | "conflict" | "not_found" | "permission" |
            "limit" | "unavailable"
  retryable: boolean
  operationIndex: number | null
  objectId: string | null
  pointer: string | null
  expected: unknown | null
  actual: unknown | null
  suggestions: string[]
}
```

Examples:

- Revision conflict reports expected/current revision and suggests refreshing
  context or re-evaluating the ChangeSet.
- Invalid projection reports the field and pointer.
- Limit violation reports the requested and maximum values.
- Missing object reports object ID and operation index.
- Permission failure reports the denied capability, never a token value.

Suggestions are short declarative actions. They do not include model prompts.

## 13. Performance And Context Bounds

Acceptance uses the large industrial fixture with 426 source nodes, 1,938 derived
nodes, 233 unique meshes, and 210 connections.

- `scene.context` returns no more than 16 KiB.
- Default `scene.search` returns at most 25 items.
- A page of 25 default search items returns no more than 128 KiB.
- `evaluation.query` rejects more than 32 node IDs.
- Payload-free smoke tests do not regress existing scene statistics or revision
  correctness.
- A repeated search page uses existing cached scene data and does not rebuild all
  geometry.

These are bounded response targets, not promises about arbitrary metadata values.
The hard request-body limit remains 64 MiB.

## 14. Security And Permissions

- Proposal tokens can use all read methods.
- Proposal tokens may add patch entries to ChangeSets and evaluate candidates.
- Proposal tokens cannot directly mutate the Document or commit/discard proposals.
- Autonomous mode grants additional capabilities only when explicitly configured.
- A cursor cannot grant access beyond the current loopback session.
- Manifest and context report effective access but are not authorization checks.
- Every mutating method continues to enforce capability checks at dispatch time.

## 15. Testing

### 15.1 Core

- Each sparse patch changes only selected fields.
- Parameter type, range, metadata conflicts, and reference validation fail
  atomically.
- Empty patches and contradictory set/remove fields are rejected.
- Patch candidates have deterministic digests and diffs.
- No-op patches still satisfy the transaction's non-empty-operation requirement
  but produce the normal documented diff behavior.

### 15.2 Server Contract

- Manifest advertises every new method and accurate token access.
- Context counts match source and derived evaluation results.
- Search filters, scope, projection, ordering, limits, and next cursor are stable.
- Invalid, mismatched-query, and cross-revision cursors fail closed.
- Derived search returns expanded nodes without geometry payloads.
- Evaluation query enforces unique-node and count limits.
- Existing RPC methods and response shapes remain unchanged.

### 15.3 ChangeSet End To End

An Agent-token integration test must:

1. read the manifest and context;
2. search for an instance;
3. query a small geometry summary;
4. create a ChangeSet;
5. add an `instance.patch`;
6. evaluate and inspect the diff;
7. request a candidate preview when Toolchain is available;
8. approve and commit with human authority;
9. verify the committed revision and reloaded project.

### 15.4 Large Scene

- Context and search response byte ceilings.
- Derived search over 1,938 nodes.
- Cursor pagination through more than one page.
- Project switch invalidates cursors.
- Undo/redo and document revisions cannot yield a mixed page.

### 15.5 Regression

The existing `npm run verify` command must pass, including Rust tests, generated
schemas, Toolchain tests, Viewer tests, build, dependency audit, and browser E2E.

## 16. Documentation

Implementation updates:

- API reference with new methods, limits, cursor behavior, and patch examples.
- Diagnostics reference for Agent error data categories and recovery actions.
- Architecture boundary showing that Agent read methods never bypass the
  Document or evaluated-scene authority.
- Generated schemas and TypeScript bindings.
- A deterministic Agent workflow fixture using only public APIs.

## 17. Delivery Order

1. Agent manifest, access, limits, and error-data contracts.
2. Context, source search, and stable cursors.
3. Derived search and explicit evaluated-geometry query.
4. `instance.patch` and `material.patch`.
5. `component.patch` and `connection.patch`.
6. End-to-end Agent workflow and large-scene context benchmark.

Each step is independently testable. Experimental GPU paths remain default-off and
are not prerequisites for this capability layer.
