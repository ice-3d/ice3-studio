# ICE3 Studio Web Workbench Design

> Document ID: ICE3-STUDIO-SPEC-002
>
> Status: Proposed
>
> Owner: ICE3 Studio maintainers

## 1. Summary

ICE3 Studio Web Workbench is a general-purpose, AI-native review and control surface for
modeling projects. AI performs the modeling work; humans inspect results, express intent, edit
structured parameters, review proposed changes, and approve model revisions.

The 3D viewport is deliberately read-only. It is an evidence surface, not a direct-manipulation
modeling canvas. All model mutations flow through typed Operations and the public Studio API.

`wastewater-mini` remains an acceptance fixture. It must not determine navigation, vocabulary,
forms, colors, examples, hierarchy, or default behavior in the Workbench.

## 2. Goals

1. Preserve the 3D viewport as the primary evidence surface for model inspection.
2. Let humans steer AI with goals, constraints, selected-object context, and review feedback.
3. Let humans change model parameters through generated forms without editing the canvas.
4. Stage AI and human changes in one reviewable ChangeSet before commit.
5. Make candidate geometry, diffs, diagnostics, operation provenance, and revision history
   visible before approval.
6. Keep the public API as the only mutation boundary.
7. Keep model credentials, agent policy, and natural-language model execution outside Studio.
8. Work offline when the Agent Gateway is unavailable.
9. Use generic modeling and presentation contracts for every project and industry.

## 3. Non-Goals

- Direct manipulation of geometry, transforms, topology, or materials in the viewport.
- Transform gizmos, mesh painting, free-form sculpting, or draggable node graphs.
- An editable graph editor. Graph and operation views are read-only explanations of AI work.
- Embedded language models, API keys, prompt policy, or agent credentials.
- Multi-user collaboration, cloud projects, organization roles, or remote deployment.
- Industry-specific Workbench layouts, forms, terminology, icons, or workflows.
- Reworking Toolchain optimization, LOD generation, or spatial partitioning.
- Replacing the committed Document and Operation history with UI-managed state.

## 4. Product Principles

### 4.1 AI authors, humans govern

AI owns the modeling plan and complex geometry work. Humans define outcomes, inspect evidence,
edit structured parameters, request revisions, and approve commits.

### 4.2 Canvas observes, it does not edit

Pointer and keyboard interaction may navigate the camera, focus an object, select an asset, or
inspect a relationship. It must not create, move, rotate, scale, deform, or delete model data.

### 4.3 Every mutation is an Operation

AI proposals and form edits compile to the same typed Operations. The UI has no private
Document mutation route and cannot persist unvalidated fields directly.

### 4.4 Review exact candidates

Approval binds to a ChangeSet version and candidate digest. A changed candidate invalidates the
approval and requires a new review.

### 4.5 Core stays domain-neutral

The Workbench renders generic project, scene, geometry, material, relation, constraint,
metadata, operation, diagnostic, and revision concepts. Industry meaning lives in metadata and
optional presentation profiles, never in Core types or base UI behavior.

### 4.6 Quiet industrial interface

The shell uses a graphite engineering interface around a black 3D stage. Color communicates
selection, pending change, warning, or failure. Model materials retain their own realistic
appearance. Decorative gradients, glass effects, and color flooding are not used.

## 5. Primary Workflow

1. A human states an intent, references the current revision, and optionally selects objects for
   context.
2. The external Agent Gateway interprets the intent and submits Operations to a ChangeSet.
3. The human may append or remove structured form edits through the Workbench.
4. Studio evaluates the ChangeSet on a clone of its base revision, never on the current
   committed Document.
5. Studio returns a diff, validation diagnostics, a candidate Document, and a candidate preview.
6. The Workbench compares `Current`, `Proposal`, and `Diff`.
7. The human asks AI to revise, changes fields, rejects, or approves the exact candidate.
8. Studio re-evaluates the approved candidate and atomically commits one revision.
9. Revision and ChangeSet events update every connected client without replacing the canvas.

## 6. Information Architecture

### 6.1 Global Header

- ICE3 wordmark and project switcher.
- Generic breadcrumb based on project and scene hierarchy.
- Agent connection state.
- ChangeSet state and current revision.
- Global connection and write-mode status.

The header must not contain modeling tool buttons.

### 6.2 Left Navigation

The left side contains a slim application rail and a collapsible context panel.

Context views:

- `Objects`: generic scene hierarchy.
- `Relations`: typed directed relationships.
- `Materials`: project materials and assignments.
- `Collections`: computed views such as changed or invalid objects.
- `Views`: read-only Structure, Relations, and Geometry Graph modes.

Selection in this panel updates inspection context only. It does not transform geometry.

### 6.3 Center Viewport

The viewport supports:

- Camera orbit, pan, zoom, fit, and object focus.
- `Current`, `Proposal`, and `Diff` observation modes.
- Selection highlight, changed-object highlight, relation overlays, and diagnostics.
- Perspective and orthographic camera modes.
- Wireframe, bounds, grid, and orientation aids.
- Screenshot/export preview for review, without modifying the Document.

The viewport must not render transform gizmos, modeling brushes, topology handles, or creation
palettes.

### 6.4 Right Review Dock

The dock has three tabs:

- `Agent`: intent, agent activity, messages, proposed plan, and controls to request revision.
- `Inspect`: schema-driven fields for the selected object or relationship.
- `Changes`: ChangeSet entries with source, operation type, affected object, and removal action.

Agent activity must be based on real events. The UI must not fabricate reasoning steps,
percentages, or completion.

### 6.5 Bottom Evidence Drawer

Collapsible tabs:

- `Operations`: ordered ChangeSet operations and provenance.
- `Timeline`: committed revisions and ChangeSet lifecycle.
- `Diagnostics`: errors and warnings linked to objects and JSON Pointers.
- `Details`: structured inspector extension for relationships, metadata, and constraints.

### 6.6 Narrow Viewports

Below the full-workbench breakpoint:

- The viewport and review summary stack vertically.
- Form editing remains available.
- Large graph/timeline views collapse into dedicated panels.
- The layout must not hide approval status, diagnostics, or candidate revision context.

## 7. Interaction States

### 7.1 Current

The committed Document is displayed. Cyan outlines indicate selection only.

### 7.2 Agent Working

The current revision remains visible and interactive for observation. Agent status appears in the
review dock. No optimistic model mutation is displayed.

### 7.3 ChangeSet Draft

Operations are being assembled. The UI shows a pending badge and may expose incomplete
diagnostics. Commit is disabled.

### 7.4 Proposal Ready

Evaluation succeeded. The viewport can show candidate geometry and amber change highlights.
Approval actions become available with the current candidate digest.

### 7.5 Blocked

Validation failed, the Agent disconnected during a required step, or preview generation failed.
Current revision remains intact. Diagnostics identify the affected fields and objects.

### 7.6 Stale

The project revision advanced after the ChangeSet was created. Commit is blocked. The UI offers
an explicit request for AI to create a rebased ChangeSet.

### 7.7 Commit Complete

The candidate becomes the new current revision. The canvas remains mounted, the viewport switches
to the new current preview, and the ChangeSet records the resulting revision.

## 8. Generic Form Model

Forms are generated from generic Operation inputs and current/candidate Document values.

| UI change | Operation |
|---|---|
| Node transform, visibility, or metadata | `node.update` |
| Node parent | `node.reparent` |
| Primitive dimensions or type | `primitive.update` |
| Triangle mesh data | `mesh.replace` |
| Material definition | `material.upsert` |
| Node material assignment | `material.assign` |
| Relationship fields | `relation.create`, `relation.update`, or `relation.delete` |
| Constraint fields | `constraint.upsert` or `constraint.delete` |

Form behavior:

- Fields maintain dirty state against the ChangeSet candidate.
- Explicit Apply compiles changed fields into Operations.
- Validation errors bind to fields through Diagnostic pointers.
- ID and reference fields use project-scoped selectors.
- Metadata uses a generic ordered key/value editor.
- Unknown generic metadata remains editable without requiring a named schema.
- Removing an unsaved form edit removes its generated ChangeSet entry.
- There is no UI-only field persistence.

Domain profiles may later provide labels, icons, grouping, and defaults through a separate
presentation schema. They must not add write paths or change committed Document semantics.
Domain profile support is outside this MVP.

## 9. ChangeSet Domain

```text
ChangeSet
├── id
├── projectId
├── baseRevision
├── version
├── status
├── title
├── entries[]
│   ├── id
│   ├── source
│   ├── operation
│   └── createdAt
├── candidateDigest
├── diagnostics[]
├── createdAt
└── updatedAt
```

Statuses:

- `draft`: accepting Operations.
- `evaluating`: candidate build in progress.
- `ready`: valid candidate available.
- `blocked`: validation or preview failed.
- `stale`: base revision no longer current.
- `committed`: successfully committed.
- `discarded`: abandoned without modifying the project.

Rules:

- Multiple ChangeSets may exist for one project.
- Only one ChangeSet is active in a Workbench window.
- Every modification supplies `expectedVersion`.
- Adding or removing an Operation increments `version` and clears `candidateDigest`.
- Evaluation builds a candidate from the immutable base revision and all entries in order.
- The candidate Document is derived and never part of committed history until commit.
- The candidate digest covers the base digest and ordered semantic Operations.
- A failed evaluation leaves the current revision and prior valid cached preview unchanged.

## 10. Public API Additions

The existing API remains available. Workbench review uses these methods:

### 10.1 `changeset.create`

```json
{
  "baseRevision": 12,
  "title": "Resize housing"
}
```

Returns the complete ChangeSet summary with status `draft` and version `0`.

### 10.2 `changeset.list`

Returns ChangeSets for the active project ordered by `updatedAt` descending.

### 10.3 `changeset.get`

Parameters:

```json
{ "changeSetId": "changeset-000004" }
```

Returns metadata, ordered entries, latest evaluation summary, and available candidate preview.

### 10.4 `changeset.addOperations`

```json
{
  "changeSetId": "changeset-000004",
  "expectedVersion": 2,
  "entries": [
    {
      "id": "entry-000008",
      "source": {
        "kind": "human",
        "label": "Inspector"
      },
      "operation": {
        "type": "primitive.update",
        "node_id": "node-000004",
        "primitive": {
          "kind": "box",
          "width": 14.0,
          "height": 4.0,
          "depth": 8.0
        }
      }
    }
  ]
}
```

The server validates schema and expected version, appends entries, increments version, and
invalidates the candidate digest.

### 10.5 `changeset.removeOperations`

```json
{
  "changeSetId": "changeset-000004",
  "expectedVersion": 3,
  "entryIds": ["entry-000008"]
}
```

Removal preserves order of remaining entries and invalidates the candidate.

### 10.6 `changeset.evaluate`

```json
{
  "changeSetId": "changeset-000004",
  "expectedVersion": 3,
  "includeCandidate": true
}
```

Returns:

```json
{
  "changeSetId": "changeset-000004",
  "version": 3,
  "baseRevision": 12,
  "status": "ready",
  "candidateDigest": "sha256:...",
  "candidate": {},
  "diff": {},
  "diagnostics": [],
  "preview": {
    "assetId": "candidate-...",
    "url": "/previews/candidate-..."
  }
}
```

`includeCandidate` may be false for large reviews; `changeset.getCandidate` returns the derived
Document separately.

### 10.7 `changeset.getCandidate`

Returns the candidate Document for the latest successful evaluation without creating a revision.
The response includes `changeSetId`, `version`, and `candidateDigest` so the client can reject stale
or mismatched data.

### 10.8 `changeset.commit`

```json
{
  "changeSetId": "changeset-000004",
  "expectedVersion": 3,
  "approvedDigest": "sha256:...",
  "idempotencyKey": "approval-..."
}
```

Before commit, Studio re-evaluates the ChangeSet and verifies:

- The ChangeSet is still `ready`.
- `expectedVersion` matches.
- The fresh candidate digest equals `approvedDigest`.
- `baseRevision` equals the current project revision.
- The caller has approval capability.

Success returns the existing `ApplyResult` shape and records the resulting revision on the
ChangeSet. A stale base returns `ICE3_STUDIO_REVISION_CONFLICT`.

### 10.9 `changeset.discard`

Discards a non-committed ChangeSet and preserves it in audit storage with status `discarded`.

## 11. Events

The existing authenticated WebSocket gains ordered events:

- `changeset.created`
- `changeset.updated`
- `changeset.evaluated`
- `changeset.committed`
- `changeset.discarded`

Events contain IDs, versions, statuses, and digests. Clients read full state through
`changeset.get`; events do not carry full Documents or preview bytes.

Agent text, plan, and tool-status events use the Agent Gateway transport described below and do
not become committed Document state.

## 12. Persistence

Project layout gains:

```text
project/
├── changesets/
│   └── <change-set-id>.json
├── cache/
│   ├── preview/
│   └── candidates/
```

Rules:

- ChangeSet JSON is canonical and written with sibling-temp plus atomic rename.
- Entry order is stable.
- Candidate previews use content-addressed asset IDs.
- Candidate Documents are reconstructed from the base revision and entries.
- On project open, Studio validates ChangeSet schema, base snapshot, digest, and preview metadata.
- A damaged ChangeSet is reported and isolated; it cannot block loading the current revision.
- Discarded and committed ChangeSets remain available for local audit until an explicit cleanup
  policy is introduced.

## 13. Write Modes and Permissions

### 13.1 `review-required` Default

Capabilities:

- Agent proposal token: read project, create and modify ChangeSets, evaluate candidates, export
  candidate previews.
- Human approval capability: commit or discard a ChangeSet.
- Direct `document.apply`: rejected for the Agent proposal token.

The Workbench receives approval capability through a loopback-only bootstrap flow. The server
sets a short-lived `HttpOnly`, `SameSite=Strict` session cookie. Secret approval material is not
stored in local storage, page script, URLs after bootstrap, or model prompts.

### 13.2 `autonomous` Explicit Mode

Host configuration may grant a trusted automation token direct commit rights. The Workbench must
show this mode persistently and must never switch modes because an Agent request asks for it.

Unknown, missing, expired, or incorrectly scoped credentials fail with
`ICE3_STUDIO_AUTH_REQUIRED` and never mutate a ChangeSet or revision.

## 14. Agent Gateway Boundary

The Agent Gateway is an independent local service or process. It may contain model credentials
and prompt policy, which are forbidden inside Studio.

```text
Workbench
  │ intent and feedback
  ▼
Agent Gateway
  │ public Studio API only
  ▼
Studio Server
  │
  └── ChangeSet → evaluation → approval → revision
```

The Workbench consumes a generic Agent Session:

- intent submission;
- run started/finished/error;
- streamed assistant text;
- plan summary updates;
- tool-call status;
- linked ChangeSet and entry IDs;
- cancellation.

Agent tools may call public Studio methods such as `scene.query`, `changeset.create`,
`changeset.addOperations`, and `changeset.evaluate`. They must not call Viewer internals,
Engine internals, or mutate Document memory.

The first implementation includes a deterministic local Agent adapter that uses this same
transport contract and requires no credentials. The Workbench displays an unavailable Agent
state when no gateway is configured. Viewing, form edits, local ChangeSets, and approvals remain
usable.

The Workbench must not contain any user-facing control that asks Agent policy or credentials to
override server write mode.

## 15. Workbench Technology

- React and TypeScript for the Workbench application.
- esbuild for TypeScript/TSX, CSS, splitting, watch rebuilds, and production bundles.
- `tsc --noEmit` for strict type checking.
- Node's built-in test runner for isolated TypeScript helper tests executed through compiled test
  output.
- Playwright for browser and end-to-end workflows.
- CSS custom properties for design tokens; no full visual UI component suite.
- The existing Engine and Toolchain public APIs remain the only 3D runtime dependencies.
- Rust Studio Server serves the built Workbench, Studio API, WebSocket, schemas, and previews from
  one loopback origin.

Development uses an esbuild watch process that writes deterministic assets to `apps/viewer/dist`.
Rust Server serves that directory. This avoids a second frontend origin, API proxy, and separate
development server.

## 16. Frontend Structure

```text
apps/viewer/
├── src/
│   ├── app/
│   │   ├── Workbench.tsx
│   │   ├── layout/
│   │   └── routes/
│   ├── agent/
│   │   ├── AgentProvider.tsx
│   │   └── transport.ts
│   ├── api/
│   │   ├── StudioClient.ts
│   │   └── generated.ts
│   ├── changeset/
│   │   ├── ChangeSetProvider.tsx
│   │   ├── operationCompiler.ts
│   │   └── types.ts
│   ├── forms/
│   │   ├── FormRenderer.tsx
│   │   └── fields/
│   ├── viewport/
│   │   ├── Viewport.tsx
│   │   └── SceneAdapter.ts
│   ├── inspector/
│   ├── evidence/
│   ├── navigation/
│   └── styles/
├── tests/
├── e2e/
└── esbuild.mjs
```

State boundaries:

- `SessionStore`: project identity, current Document, revision, capabilities, connection.
- `ChangeSetStore`: active ChangeSet, candidate, version, digest, dirty form operations.
- `SelectionStore`: selected IDs and inspection focus.
- `AgentStore`: generic Agent Session events and connection state.
- Viewport state remains inside the Engine adapter and never serializes into Document.

The canvas instance is created once and survives revision, proposal, selection, and panel updates.

## 17. Visual Language

Working tokens:

| Token | Value | Use |
|---|---|---|
| Canvas | `#050809` | 3D viewport background |
| Shell | `#0C1417` | application and panel surfaces |
| Raised | `#172328` | selected rows and elevated controls |
| Line | `#263A40` | separators and control borders |
| Text | `#E8F2F3` | primary text |
| Muted | `#82999F` | secondary text and metadata |
| Selection | `#56C9D8` | focus and selected object |
| Proposal | `#E7A34B` | staged and changed state |
| Error | `#EB6A66` | failure and destructive state |

Typography:

- Interface copy: IBM Plex Sans with PingFang SC fallback.
- IDs, revision numbers, operation types, and tabular evidence: IBM Plex Mono.
- Minimum interactive target: 32 pixels on desktop; 44 pixels in narrow/touch layouts.
- Text contrast meets WCAG AA at its rendered size.

Interaction:

- Keyboard focus is always visible.
- State is never conveyed by color alone; labels and icons accompany color.
- Reduced-motion preferences disable nonessential transitions.
- Model materials and lighting are not tinted by shell status colors.

## 18. Performance Targets

On the current Apple Silicon development machine with release Studio Server:

- Workbench shell becomes interactive within 2 seconds after assets are served.
- Panel/tab interactions respond within 100 ms.
- Object-tree interactions remain responsive with 1,000 nodes through virtualization.
- ChangeSet apply, validation, and diff complete at p95 within 500 ms for 100 Operations.
- Candidate GLB generation remains within the existing 2-second export target.
- Runtime preview remains within the existing 10-second workflow target.
- The canvas remains mounted across revision and ChangeSet events.

## 19. Failure and Recovery

- Agent offline: current model, forms, local ChangeSets, and approval remain available.
- Agent run failure: no optimistic model mutation; error appears in Agent activity.
- Invalid form operation: server rejects the operation; field and Diagnostic pointer remain
  visible.
- ChangeSet evaluation failure: status becomes `blocked`; current revision is unchanged.
- Preview failure: previous valid preview remains; candidate IDs and diagnostics remain
  inspectable.
- Stale revision: commit is blocked; a rebase can be requested from AI.
- Candidate changes after review: approval digest mismatch blocks commit.
- Server restart: active ChangeSets and last successful candidate metadata are recoverable.
- Corrupt ChangeSet: isolated with a persistence diagnostic; current revision remains loadable.
- Duplicate commit: idempotency key returns the original result without another revision.

## 20. Testing

### 20.1 Rust Unit and Integration

- ChangeSet create/add/remove/version semantics.
- Candidate digest stability and invalidation.
- Candidate evaluation never mutates current Document.
- Approval digest and expected-version enforcement.
- Base revision conflict.
- Concurrent ChangeSet commits have one winner.
- Atomic persistence and restart recovery.
- Corrupt ChangeSet isolation.
- Proposal and approval capability enforcement.
- Event ordering and payload shape.
- Generated schema and TypeScript declarations.

### 20.2 TypeScript Unit

- Operation compiler for every generic form type.
- Field dirty-state and apply/remove behavior.
- Diagnostic-to-field mapping.
- API envelope and typed error handling.
- Agent event reducer.
- Session, ChangeSet, selection, and connection state transitions.

### 20.3 Browser E2E

- Load a generic project without wastewater-specific labels or structure.
- Select an object without mutating the model.
- Modify generic fields and stage Operations.
- Display current, candidate, and diff previews.
- Approve an exact candidate and retain the mounted canvas.
- Reject a changed or stale candidate.
- Recover after reload and server restart.
- Continue core review when Agent Gateway is unavailable.
- Verify keyboard navigation and focus visibility.
- Verify narrow read-only layout.
- Run the same workflow against `wastewater-mini` to prove fixture compatibility.

## 21. Acceptance Criteria

The Workbench is accepted when:

1. No direct canvas mutation control exists.
2. Every human model change appears as a typed ChangeSet entry.
3. AI and human entries share one candidate and one validation path.
4. Approval is bound to the exact current candidate digest.
5. Current revision remains unchanged through draft, evaluation, failure, and rejection.
6. Committed revision and event behavior remain deterministic.
7. ChangeSets survive process restart.
8. Agent-offline operation preserves viewing, forms, and approval.
9. Review-required mode prevents Agent clients from committing or bypassing approval.
10. Generic projects render without industry-specific UI assumptions.
11. The Workbench bundle builds without the removed bundler or test framework.
12. All Rust, TypeScript, browser, accessibility, schema, and performance gates pass.

## 22. Risks and Decisions

| Risk | Mitigation |
|---|---|
| Workbench slowly becomes a traditional editor | No gizmos, modeling toolbar, or editable canvas; enforce acceptance tests |
| Human and AI changes diverge | One ChangeSet and one Operation compiler |
| Large Documents overwhelm UI | Candidate payload opt-out, tree virtualization, content-addressed previews |
| Approval races | Version and digest checks immediately before commit |
| Reviewer trusts stale visuals | Preview and approval use candidate digest; stale states are explicit |
| Industry semantics leak into Core | Generic forms and no Domain Pack in MVP |
| Agent credentials leak into Studio | Separate Agent Gateway and host-only configuration |
| Multiple browser and agent writers conflict | Expected version, base revision, and single-winner commit |

## 23. Reference Decisions

- [Document as Source of Truth](../decisions/0001-document-as-source-of-truth.md)
- [Local API Boundary](../decisions/0002-local-api-boundary.md)
- [MVP Specification](2026-10-30-ai-native-studio-mvp-design.md)
- [Architecture](../architecture.md)
- [API Reference](../reference/api.md)

## 24. Open Implementation Decisions

No product or architecture decision remains open. Exact function names, React component
boundaries, schema-generation refactoring, and incremental migration steps belong to the
implementation plan.
