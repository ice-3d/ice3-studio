# Studio Web Workbench UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Use checkbox (`- [ ]`) syntax for tracking.

> Document ID: ICE3-STUDIO-PLAN-003
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

**Goal:** Replace the minimal Viewer with a generic AI-native Workbench that displays committed
and candidate models, stages form edits, shows Agent activity, and supports digest-bound approval
without direct canvas editing.

**Architecture:** React components consume the ChangeSet API from the preceding plan and mount the
existing Engine through one persistent read-only adapter. A custom external-store layer separates
session, review, selection, and Agent state. esbuild builds the browser bundle; Rust Studio Server
serves the bundle, API, WebSocket, schemas, and previews from one loopback origin.

**Tech Stack:** React 19.3.1, React DOM 19.3.1, TypeScript 7.0.2, esbuild 0.28.2,
Playwright 1.64.1, Node's built-in test runner, Rust Studio Server, existing ICE3 Engine and
Toolchain.

**Spec:** [`ICE3-STUDIO-SPEC-002`](../specs/2026-10-30-studio-web-workbench-design.md)

**Prerequisite plan:** [`ICE3-STUDIO-PLAN-002`](2026-10-30-studio-changeset-api.md)

## Global Constraints

- The viewport is read-only and contains no transform controls or modeling tools.
- Every model mutation is a typed Operation submitted through the public API.
- The main UI must render any project and cannot depend on wastewater-specific names or schema.
- React and browser code must not own validation, persistence, candidate construction, or revision
  semantics.
- Studio Web must not contain model credentials, prompt policy, or provider SDKs.
- Approval binds to `changeSetId`, `expectedVersion`, and `approvedCandidateDigest`.
- The Engine canvas remains mounted across revision, proposal, selection, and Agent updates.
- Build with esbuild, typecheck with `tsc --noEmit`, unit-test with `node:test`, and browser-test
  with Playwright; do not add a general-purpose frontend development server or test framework.
- Workbench assets and API are served from the same loopback origin.
- The visual system uses the tokens from spec section 17.
- Text contrast must meet WCAG AA, keyboard focus must be visible, and color cannot be the only
  state signal.
- Git author and committer must be `damoqiongqiu <damoqiongqiu@126.com>`.

## Review Focus

- A domain-neutral project renders with no wastewater labels: navigation, forms, and empty states
  must be generated only from generic project data.
- A user clicks or focuses geometry in the canvas: camera and selection may change, but Document
  and ChangeSet Operation counts must remain unchanged.
- An Agent update arrives while a proposal is open: the canvas must remain mounted and stale
  ChangeSet data must not replace the active candidate.
- Agent Gateway is unavailable or fails mid-stream: current viewing, form staging, diagnostics,
  and approval remain usable.
- A candidate changes after review but before approval: commit must be disabled locally and the
  server must reject the old digest.

---

### Task 1: Replace the Viewer Toolchain and Serve One Same-Origin App

**Files:**

- Modify: `apps/viewer/package.json`
- Modify: `apps/viewer/package-lock.json`
- Modify: `apps/viewer/tsconfig.json`
- Delete: `apps/viewer/vite.config.ts`
- Create: `apps/viewer/esbuild.mjs`
- Create: `apps/viewer/esbuild-tests.mjs`
- Modify: `apps/viewer/index.html`
- Create: `apps/viewer/src/main.tsx`
- Delete: `apps/viewer/src/main.ts`
- Modify: `apps/viewer/playwright.config.ts`
- Modify: `scripts/run-viewer-e2e-server.mjs`
- Modify: `crates/studio-server/Cargo.toml`
- Modify: `Cargo.toml`
- Modify: `crates/studio-server/src/rpc.rs`
- Modify: `crates/studio-server/tests/rpc_contract.rs`

**Interfaces:**

- Consumes: compiled Workbench assets and existing Studio HTTP routes.
- Produces: `ServerConfig.viewer_dir: Option<PathBuf>`.
- Produces: `npm run build`, `npm run dev`, `npm run typecheck`, `npm test`, and
  `npm run test:e2e` without a general-purpose frontend development server.
- Produces: `STUDIO_VIEWER_DIR` static hosting and `/assets/*` file responses from Studio Server.

- [ ] **Step 1: Write a failing same-origin asset test**

```rust
#[tokio::test]
async fn serves_built_workbench_and_api_from_one_origin() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("assets")).unwrap();
    std::fs::write(
        root.path().join("index.html"),
        "<!doctype html><script src=\"/assets/app.js\"></script>",
    )
    .unwrap();
    std::fs::write(root.path().join("assets/app.js"), "console.log('workbench')").unwrap();
    let server = start_test_server(ServerConfig {
        viewer_dir: Some(root.path().to_path_buf()),
        ..ServerConfig::for_tests("generic-project")
    }).await;

    let index = reqwest::get(server.base_url()).await.unwrap();
    assert!(index.text().await.unwrap().contains("/assets/app.js"));
    let asset = reqwest::get(format!("{}/assets/app.js", server.base_url()))
        .await
        .unwrap();
    assert!(asset.text().await.unwrap().contains("workbench"));
}
```

- [ ] **Step 2: Verify RED**

Run: `cargo test -p studio-server --test rpc_contract serves_built_workbench_and_api_from_one_origin`

Expected: compilation fails because the viewer-serving test helper and routes do not exist.

- [ ] **Step 3: Add Rust static serving**

Add `tower-http` with its filesystem feature. Store an optional viewer directory in the server
state through `ServerConfig.viewer_dir`, then add:

```rust
let router = if let Some(viewer_dir) = viewer_dir {
    router
        .route_service(
            "/assets/{*path}",
            ServeDir::new(viewer_dir.join("assets")),
        )
        .fallback_service(ServeFile::new(viewer_dir.join("index.html")))
} else {
    router
};
```

API, schema, WebSocket, and preview routes take precedence over the HTML fallback. A missing viewer
directory returns the existing JSON health API but a plain `404` for page assets.

- [ ] **Step 4: Write the esbuild scripts**

```js
// apps/viewer/esbuild.mjs
import { context, build } from 'esbuild';
import { copyFile, mkdir } from 'node:fs/promises';

const watch = process.argv.includes('--watch');
const options = {
  entryPoints: ['src/main.tsx'],
  outdir: 'dist/assets',
  bundle: true,
  splitting: true,
  format: 'esm',
  target: ['chrome120', 'firefox120', 'safari17'],
  sourcemap: true,
  minify: !watch,
  jsx: 'automatic',
  loader: {
    '.woff2': 'file',
  },
};

await mkdir('dist/assets', { recursive: true });
await copyFile('index.html', 'dist/index.html');
if (watch) {
  const watcher = await context(options);
  await watcher.watch();
  console.log('Workbench watch build active');
} else {
  await build(options);
}
```

Generate `apps/viewer/dist/test-dist` entries with `esbuild-tests.mjs`; then run
`node --test test-dist/*.test.mjs`. This keeps tests on `node:test` without a TypeScript test
framework.

Change the HTML entry to `<script type="module" src="/assets/main.js"></script>`. Keep no frontend
development-server client in the HTML.

- [ ] **Step 5: Replace package scripts and dependencies**

Use exact versions:

```json
{
  "dependencies": {
    "ice3": "file:../../../ice3-engine/packages/ice3",
    "react": "19.3.1",
    "react-dom": "19.3.1"
  },
  "devDependencies": {
    "@playwright/test": "1.64.1",
    "esbuild": "0.28.2",
    "typescript": "7.0.2"
  },
  "scripts": {
    "build": "node esbuild.mjs",
    "dev": "node esbuild.mjs --watch",
    "typecheck": "tsc --noEmit",
    "test": "node esbuild-tests.mjs && node --test test-dist/*.test.mjs",
    "test:e2e": "npm run build && playwright test",
    "verify": "npm run typecheck && npm test && npm run build"
  }
}
```

Remove the deleted frontend development server dependency and Vite-specific TypeScript types.

- [ ] **Step 6: Run the focused test and production build**

Run: `cargo test -p studio-server --test rpc_contract serves_built_workbench_and_api_from_one_origin`

Run: `npm --prefix apps/viewer run typecheck`

Run: `npm --prefix apps/viewer run build`

Expected: all pass and `apps/viewer/dist/index.html` plus `assets/` exist.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock apps/viewer crates/studio-server scripts
git commit -m "build(studio): serve react workbench with esbuild"
```

---

### Task 2: Build the Workbench Shell and Session Store

**Files:**

- Create: `apps/viewer/src/app/Workbench.tsx`
- Create: `apps/viewer/src/app/WorkbenchHeader.tsx`
- Create: `apps/viewer/src/app/studioTokens.css`
- Create: `apps/viewer/src/api/StudioClient.ts`
- Create: `apps/viewer/src/session/SessionStore.ts`
- Create: `apps/viewer/src/session/SessionProvider.tsx`
- Create: `apps/viewer/src/shared/ExternalStore.ts`
- Create: `apps/viewer/tests/session-store.test.ts`
- Modify: `apps/viewer/src/main.tsx`
- Modify: `apps/viewer/index.html`
- Delete: `apps/viewer/src/api.ts`
- Delete: `apps/viewer/src/panel.ts`
- Delete: `apps/viewer/styles.css`

**Interfaces:**

- Consumes: `/health`, `document.get`, `validation.run`, and revision WebSocket events.
- Produces: `StudioClient` with typed `health`, `document`, `validation`, `changesets`, and
  `subscribe`.
- Produces: `SessionStore` snapshot `{ project, revision, document, diagnostics, connection }`.
- Produces: `SessionStore.refresh()`, `select(ids)`, and `applyRevision(revision)`.
- Produces: `WorkbenchHeader` and the base shell used by all later panels.

- [ ] **Step 1: Write a failing session-store test**

```ts
import assert from 'node:assert/strict';
import test from 'node:test';
import { SessionStore } from '../src/session/SessionStore.js';

test('revision events refresh the latest document without changing selection', async () => {
  let revision = 3;
  const store = new SessionStore({
    document: async () => ({
      projectId: 'generic-project',
      revision,
      scene: { nodes: [] },
    }),
    validation: async () => [],
  });
  await store.refresh();
  store.select(['node-000001']);
  revision = 4;
  await store.refresh();

  assert.equal(store.snapshot.revision, 4);
  assert.deepEqual(store.snapshot.selectionIds, ['node-000001']);
});
```

- [ ] **Step 2: Verify RED**

Run: `npm --prefix apps/viewer test -- --test-name-pattern "revision events"`

Expected: test compilation fails because `SessionStore` does not exist.

- [ ] **Step 3: Implement the external store and Workbench shell**

Use a small `ExternalStore<T>` with `getSnapshot`, `subscribe`, and `update`. Build
`SessionStore` so only a successful Document refresh replaces model data; transient Agent or
ChangeSet failures update their own state. A revision event queues one refresh and ignores stale
out-of-order responses by comparing requested generation numbers.

Implement `Workbench` with:

```tsx
export function Workbench() {
  return (
    <div className="workbench">
      <WorkbenchHeader />
      <div className="workbench__body">
        <NavigationPanel />
        <ViewportPanel />
        <ReviewDock />
      </div>
      <EvidenceDrawer />
    </div>
  );
}
```

Temporary empty components render labeled areas; later tasks replace their internals.

- [ ] **Step 4: Add layout and token CSS**

Move the spec's color, spacing, type, radius, and breakpoint values into `studioTokens.css`.
Implement the desktop grid and narrow stacked layout. Use CSS custom properties only; no inline
hardcoded colors in components.

- [ ] **Step 5: Run unit, type, and build checks**

Run: `npm --prefix apps/viewer test`

Run: `npm --prefix apps/viewer run typecheck`

Run: `npm --prefix apps/viewer run build`

Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add apps/viewer
git commit -m "feat(studio-workbench): add react application shell"
```

---

### Task 3: Add Generic Navigation and the Persistent Read-Only Viewport

**Files:**

- Create: `apps/viewer/src/navigation/NavigationPanel.tsx`
- Create: `apps/viewer/src/navigation/ObjectTree.tsx`
- Create: `apps/viewer/src/navigation/ObjectTreeStore.ts`
- Create: `apps/viewer/src/viewport/Viewport.tsx`
- Create: `apps/viewer/src/viewport/SceneAdapter.ts`
- Create: `apps/viewer/src/viewport/ViewportToolbar.tsx`
- Create: `apps/viewer/tests/object-tree.test.ts`
- Modify: `apps/viewer/src/app/Workbench.tsx`
- Modify: `apps/viewer/src/session/SessionStore.ts`
- Delete: `apps/viewer/src/scene.ts`

**Interfaces:**

- Consumes: current or candidate Document, selected IDs, and preview URLs.
- Produces: accessible generic hierarchy with search and selection.
- Produces: `SceneAdapter.mount(container)`, `show(url)`, `setSelection(ids)`, `dispose()`.
- Produces: viewport modes `current | proposal | diff`.

- [ ] **Step 1: Write failing hierarchy tests**

```ts
import assert from 'node:assert/strict';
import test from 'node:test';
import { buildObjectTree } from '../src/navigation/ObjectTree.js';

test('builds a generic hierarchy without industry assumptions', () => {
  const tree = buildObjectTree([
    { id: 'node-000001', name: 'Assembly', parentId: null, metadata: {} },
    { id: 'node-000002', name: 'Part', parentId: 'node-000001', metadata: {} },
  ]);

  assert.deepEqual(tree, [{
    id: 'node-000001',
    name: 'Assembly',
    children: [{ id: 'node-000002', name: 'Part', children: [] }],
  }]);
});
```

- [ ] **Step 2: Verify RED**

Run: `npm --prefix apps/viewer test -- --test-name-pattern "generic hierarchy"`

Expected: test compilation fails because `buildObjectTree` does not exist.

- [ ] **Step 3: Implement generic tree construction and navigation**

Build the tree by stable IDs and parent references. Preserve Document order among siblings. Search
filters names, IDs, and metadata values. Expose tree roles and keyboard behavior:

```tsx
<ul role="tree" aria-label="Project objects">
  {nodes.map((node) => (
    <li role="treeitem" aria-selected={selectedIds.includes(node.id)}>
      <button onClick={() => selection.select([node.id])}>{node.name}</button>
    </li>
  ))}
</ul>
```

The component contains no labels such as tank, pump, pipe, or plant.

- [ ] **Step 4: Implement one persistent scene adapter**

Move the existing Engine setup into `SceneAdapter`. `Viewport` mounts it once in `useEffect` with a
stable container ref and calls `show` only when the preview URL changes. The toolbar exposes
camera fit, perspective/orthographic, grid, wireframe, and observation mode. It must contain no
create, translate, rotate, scale, delete, or topology controls.

Selection arrives from Engine events and updates `SessionStore.selectionIds`; selection never
writes a ChangeSet entry.

- [ ] **Step 5: Run unit, type, and browser smoke tests**

Run: `npm --prefix apps/viewer test`

Run: `npm --prefix apps/viewer run typecheck`

Run: `npm --prefix apps/viewer run test:e2e -- --grep "canvas"`

Expected: hierarchy tests pass and the canvas remains present after a mocked revision update.

- [ ] **Step 6: Commit**

```bash
git add apps/viewer
git commit -m "feat(studio-workbench): add generic navigation and read-only viewport"
```

---

### Task 4: Compile Generic Forms into Operations

**Files:**

- Create: `apps/viewer/src/forms/FormRenderer.tsx`
- Create: `apps/viewer/src/forms/operationCompiler.ts`
- Create: `apps/viewer/src/forms/fields/NumberField.tsx`
- Create: `apps/viewer/src/forms/fields/BooleanField.tsx`
- Create: `apps/viewer/src/forms/fields/EnumField.tsx`
- Create: `apps/viewer/src/forms/fields/ColorField.tsx`
- Create: `apps/viewer/src/forms/fields/ReferenceField.tsx`
- Create: `apps/viewer/src/forms/fields/MetadataField.tsx`
- Create: `apps/viewer/tests/operation-compiler.test.ts`
- Modify: `apps/viewer/src/app/Workbench.tsx`

**Interfaces:**

- Consumes: selected `SceneNode`, `Material`, `Relation`, or `Constraint`.
- Produces: `compileFieldChanges(value, draft) -> Operation[]`.
- Produces: generic forms and field-level Diagnostic mapping.

- [ ] **Step 1: Write failing compiler tests**

```ts
import assert from 'node:assert/strict';
import test from 'node:test';
import { compileNodeChanges } from '../src/forms/operationCompiler.js';

test('compiles generic node field changes into node.update', () => {
  const operations = compileNodeChanges(
    {
      id: 'node-000001',
      name: 'Assembly',
      transform: {
        translation: { x: 0, y: 0, z: 0 },
        rotation: [0, 0, 0, 1],
        scale: { x: 1, y: 1, z: 1 },
      },
      visible: true,
      metadata: { status: 'draft' },
    },
    {
      name: 'Assembly revised',
      visible: false,
      metadata: { status: 'approved' },
    },
  );

  assert.deepEqual(operations, [{
    type: 'node.update',
    node_id: 'node-000001',
    name: 'Assembly revised',
    transform: null,
    visible: false,
    metadata: { status: 'approved' },
  }]);
});
```

- [ ] **Step 2: Verify RED**

Run: `npm --prefix apps/viewer test -- --test-name-pattern "generic node field"`

Expected: test compilation fails because `compileNodeChanges` does not exist.

- [ ] **Step 3: Implement operation compilers**

Provide focused functions for:

- `compileNodeChanges`
- `compilePrimitiveChange`
- `compileMeshChange`
- `compileMaterialChange`
- `compileRelationChange`
- `compileConstraintChange`

Each compares explicit form state with the candidate and emits no Operation for unchanged fields.
Return errors for missing references or invalid synthetic state before an API request.

- [ ] **Step 4: Implement fields and rendering**

Build the common field primitives from the spec. `FormRenderer` groups fields by generic content
union and displays a stage action:

```tsx
<form onSubmit={(event) => {
  event.preventDefault();
  void changeSets.addOperations(compileSelectedChanges());
}}>
  <PrimitiveFields value={node.content} onChange={updateDraft} />
  <MetadataField value={draft.metadata} onChange={updateMetadata} />
  <button type="submit" disabled={!changeSets.canStage}>Stage changes</button>
</form>
```

No form action calls `document.apply` or mutates a Document object directly.

- [ ] **Step 5: Run compiler and type tests**

Run: `npm --prefix apps/viewer test`

Run: `npm --prefix apps/viewer run typecheck`

Expected: every compiler type has a passing test and no UI domain constants exist.

- [ ] **Step 6: Commit**

```bash
git add apps/viewer
git commit -m "feat(studio-workbench): compile generic forms into operations"
```

---

### Task 5: Add ChangeSet Review, Proposal Preview, and Evidence

**Files:**

- Create: `apps/viewer/src/changeset/ChangeSetStore.ts`
- Create: `apps/viewer/src/changeset/ChangeSetProvider.tsx`
- Create: `apps/viewer/src/review/ReviewDock.tsx`
- Create: `apps/viewer/src/review/ChangesPanel.tsx`
- Create: `apps/viewer/src/review/InspectorPanel.tsx`
- Create: `apps/viewer/src/evidence/EvidenceDrawer.tsx`
- Create: `apps/viewer/src/evidence/OperationsPanel.tsx`
- Create: `apps/viewer/src/evidence/TimelinePanel.tsx`
- Create: `apps/viewer/src/evidence/DiagnosticsPanel.tsx`
- Create: `apps/viewer/tests/changeset-store.test.ts`
- Modify: `apps/viewer/src/app/Workbench.tsx`
- Modify: `apps/viewer/src/viewport/Viewport.tsx`
- Modify: `apps/viewer/src/api/StudioClient.ts`

**Interfaces:**

- Consumes: ChangeSet API and candidate preview URLs from the prerequisite plan.
- Produces: active ChangeSet state and review actions.
- Produces: `ChangeSetStore.acceptEvaluation(evaluation)`, `evaluate()`, `addOperations()`,
  `removeOperations()`, `markReviewed(digest)`, and `commit()`.
- Produces: `Stage`, `Remove`, `Evaluate`, `Approve`, `Revise`, and `Discard` UI actions.

- [ ] **Step 1: Write failing ChangeSet-store tests**

```ts
import assert from 'node:assert/strict';
import test from 'node:test';
import { ChangeSetStore } from '../src/changeset/ChangeSetStore.js';

const emptyDiff = () => ({
  addedNodeIds: [],
  updatedNodeIds: [],
  removedNodeIds: [],
  addedMaterialIds: [],
  updatedMaterialIds: [],
  removedMaterialIds: [],
  addedRelationIds: [],
  updatedRelationIds: [],
  removedRelationIds: [],
  addedConstraintIds: [],
  updatedConstraintIds: [],
  removedConstraintIds: [],
});

const client = {
  addOperations: async () => { throw new Error('not used'); },
  removeOperations: async () => { throw new Error('not used'); },
  evaluate: async () => ({
    id: 'changeset-0001',
    version: 2,
    status: 'ready',
    candidateDigest: 'sha256:new',
    diff: emptyDiff(),
    diagnostics: [],
    previewUrl: '/previews/candidates/sha256-new',
  }),
  commit: async () => { throw new Error('not used'); },
};

test('blocks approval when evaluation changes the digest', async () => {
  const store = new ChangeSetStore(client);
  store.acceptEvaluation({
    id: 'changeset-0001',
    version: 2,
    status: 'ready',
    candidateDigest: 'sha256:old',
    diff: emptyDiff(),
    diagnostics: [],
    previewUrl: '/previews/candidates/sha256-old',
  });

  await store.evaluate();

  assert.equal(store.snapshot.canApprove, false);
  assert.equal(store.snapshot.status, 'review-required');
});
```

- [ ] **Step 2: Verify RED**

Run: `npm --prefix apps/viewer test -- --test-name-pattern "blocks approval"`

Expected: test compilation fails because `ChangeSetStore` does not exist.

- [ ] **Step 3: Implement ChangeSet state and components**

`ChangeSetStore` tracks version, entries, candidate, digest, preview URL, diagnostics, and the last
human-reviewed digest. `canApprove` is true only when status is `ready`, version is current, and
evaluation digest equals the reviewed digest.

`ChangesPanel` lists entries with source and operation type. `OperationsPanel` preserves order.
`DiagnosticsPanel` selects the affected object when a valid object ID is present. Bind the four
viewport modes:

- Current: committed preview.
- Proposal: latest successful candidate preview.
- Diff: candidate preview with changed/error overlays.

Use the Engine's stable object IDs for selection, never screen coordinates.

- [ ] **Step 4: Handle version, stale, and preview failures**

Use expected versions for every add/remove/evaluate request. On conflict, refresh the ChangeSet and
disable approval. Keep the prior valid candidate preview on evaluation or Toolchain failure and
render a blocked state.

- [ ] **Step 5: Run unit, type, and browser checks**

Run: `npm --prefix apps/viewer test`

Run: `npm --prefix apps/viewer run typecheck`

Run: `npm --prefix apps/viewer run test:e2e -- --grep "proposal"`

Expected: proposal tests pass and current preview remains when candidate generation fails.

- [ ] **Step 6: Commit**

```bash
git add apps/viewer
git commit -m "feat(studio-workbench): review change sets and evidence"
```

---

### Task 6: Add the Agent Session Boundary and Offline State

**Files:**

- Create: `apps/viewer/src/agent/transport.ts`
- Create: `apps/viewer/src/agent/AgentStore.ts`
- Create: `apps/viewer/src/agent/AgentProvider.tsx`
- Create: `apps/viewer/src/agent/AgentPanel.tsx`
- Create: `apps/viewer/src/agent/deterministicTransport.ts`
- Create: `apps/viewer/tests/agent-store.test.ts`
- Modify: `apps/viewer/src/app/Workbench.tsx`
- Modify: `apps/viewer/src/session/SessionStore.ts`

**Interfaces:**

- Consumes: host-configured Agent transport and public ChangeSet IDs.
- Produces: `AgentTransport.submitIntent`, `cancel`, `subscribe`.
- Produces: Agent snapshots `{ connection, runId, messages, plan, linkedChangeSetId, error }`.
- Produces: deterministic local transport for tests and fixture use.

- [ ] **Step 1: Write failing Agent tests**

```ts
import assert from 'node:assert/strict';
import test from 'node:test';
import { AgentStore } from '../src/agent/AgentStore.js';
import { createDeterministicTransport } from '../src/agent/deterministicTransport.js';

test('Agent failure leaves model review available', async () => {
  const store = new AgentStore({
    connection: 'unavailable',
    submitIntent: async () => ({ runId: 'run-1' }),
    cancel: async () => {},
    subscribe: () => () => {},
  });
  await store.submit('Resize the selected assembly');
  assert.equal(store.snapshot.connection, 'unavailable');
  assert.equal(store.snapshot.reviewAvailable, true);
});

test('deterministic transport emits a linked candidate plan', async () => {
  const events = [];
  const transport = createDeterministicTransport({
    plan: 'Stage a generic node update',
    changeSetId: 'changeset-test',
  });
  const unsubscribe = transport.subscribe((event) => events.push(event));
  await transport.submitIntent({
    text: 'Adjust the assembly',
    projectId: 'generic-project',
    revision: 3,
    selectionIds: ['node-000001'],
  });
  unsubscribe();
  assert.ok(events.some((event) => event.type === 'plan.updated'));
  assert.ok(events.some((event) => event.type === 'changeset.linked'));
});
```

- [ ] **Step 2: Verify RED**

Run: `npm --prefix apps/viewer test -- --test-name-pattern "Agent failure"`

Expected: compilation fails because the Agent modules do not exist.

- [ ] **Step 3: Implement the transport and store**

Define discriminated event types:

```ts
export type AgentEvent =
  | { type: 'run.started'; runId: string }
  | { type: 'message.delta'; runId: string; text: string }
  | { type: 'plan.updated'; runId: string; summary: string }
  | { type: 'changeset.linked'; runId: string; changeSetId: string }
  | { type: 'run.finished'; runId: string }
  | { type: 'run.failed'; runId: string; diagnosticCode: string; message: string };
```

Reject late events from cancelled runs. Preserve the prior transcript and plan on connection errors.
Agent events never alter current Document, ChangeSet, selection, or viewport state directly; a
linked ChangeSet is fetched through the Studio API.

- [ ] **Step 4: Build the Agent dock**

Render connection state, transcript, current plan, linked ChangeSet, and cancel action. When
unavailable, render:

```tsx
<section aria-label="Agent unavailable">
  <p>The Agent Gateway is not connected.</p>
  <p>Model review and form editing remain available.</p>
</section>
```

The prompt field is disabled only when transport is unavailable; it never requests model credentials
or write-mode changes.

- [ ] **Step 5: Run Agent and complete viewer tests**

Run: `npm --prefix apps/viewer test`

Run: `npm --prefix apps/viewer run typecheck`

Expected: both deterministic and offline behavior pass.

- [ ] **Step 6: Commit**

```bash
git add apps/viewer
git commit -m "feat(studio-workbench): integrate generic agent sessions"
```

---

### Task 7: Complete Approval, Accessibility, and End-to-End Evidence

**Files:**

- Create: `apps/viewer/e2e/workbench.spec.ts`
- Create: `apps/viewer/e2e/generic-project.spec.ts`
- Create: `apps/viewer/e2e/agent-offline.spec.ts`
- Create: `apps/viewer/tests/accessibility-contract.test.ts`
- Modify: `apps/viewer/playwright.config.ts`
- Modify: `scripts/run-viewer-e2e-server.mjs`
- Modify: `scripts/verify.mjs`
- Modify: `README.md`
- Modify: `docs/README.md`
- Modify: `docs/guides/getting-started.md`
- Modify: `docs/architecture.md`
- Modify: `docs/specs/2026-10-30-studio-web-workbench-design.md`
- Modify: `AGENTS.md`

**Interfaces:**

- Consumes: complete API and Workbench.
- Produces: full browser acceptance, migration guide, implemented documentation, and verified
  performance baseline.

- [ ] **Step 1: Write failing E2E acceptance tests**

```ts
import { expect, test } from '@playwright/test';

test('reviews and commits a generic change without remounting the canvas', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Generic Mechanical Assembly' })).toBeVisible();
  const canvas = page.locator('canvas[data-layer="scene"]');
  await canvas.evaluate((element) => {
    (element as HTMLCanvasElement).dataset.instanceMarker = 'persistent';
  });

  await page.getByRole('button', { name: 'Stage changes' }).click();
  await expect(page.getByText('3 staged changes')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Approve & commit' })).toBeEnabled();
  await page.getByRole('button', { name: 'Approve & commit' }).click();

  await expect(page.getByText('Revision 4')).toBeVisible();
  await expect(canvas).toHaveAttribute('data-instance-marker', 'persistent');
});

test('canvas selection does not create a staged operation', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByText('0 staged changes')).toBeVisible();
  await page.locator('canvas[data-layer="scene"]').click();
  await expect(page.getByText('0 staged changes')).toBeVisible();
});
```

- [ ] **Step 2: Verify RED**

Run: `npm --prefix apps/viewer run test:e2e -- --grep "generic change"`

Expected: current UI lacks Workbench roles, form actions, approval, and revision labels.

- [ ] **Step 3: Implement the generic E2E fixture**

Create a generic mechanical assembly fixture using only generic node and material concepts. Copy it
into the E2E temporary workspace. Keep `wastewater-mini` as a compatibility fixture, not as the
default title or test data.

- [ ] **Step 4: Complete accessibility and responsive behavior**

Add automated assertions for:

- treeitem roles and keyboard focus;
- tablist/tab/tabpanel roles;
- form labels and field-level error associations;
- visible focus rings;
- 1024-pixel desktop layout;
- 390-pixel read-only layout with no horizontal page overflow.

Use reduced-motion media rules for proposal and commit transitions.

- [ ] **Step 5: Update verification and documentation**

Root verification must run:

```text
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -q -p studio-cli -- schema --output schemas --check
node scripts/run-ai-fixture.mjs --fixture fixtures/wastewater-mini
npm --prefix apps/viewer run typecheck
npm --prefix apps/viewer test
npm --prefix apps/viewer run build
npm --prefix apps/viewer run test:e2e
npm --prefix apps/viewer audit --audit-level high
```

Document generic-project usage, Agent Gateway configuration, ChangeSet approval, write modes,
candidate previews, and recovery. Mark the Workbench spec Implemented only after every gate passes.

- [ ] **Step 6: Run complete verification and benchmark**

Run: `npm run verify`

Run: `npm run benchmark`

Expected: all gates pass, performance limits remain satisfied, and no browser console errors occur.

- [ ] **Step 7: Commit**

```bash
git add apps/viewer scripts docs README.md AGENTS.md
git commit -m "feat(studio-workbench): deliver ai-native review workflow"
```

## Plan Self-Review

- Spec coverage: shell, generic navigation, read-only viewport, change modes, all generic form
  types, ChangeSet review, candidate evidence, Agent boundary/offline state, approval, responsive
  behavior, accessibility, toolchain replacement, same-origin hosting, and E2E are covered.
- Placeholder scan: no deferred implementation steps.
- Type consistency: `StudioClient`, `SessionStore`, `ChangeSetStore`, `AgentTransport`,
  `AgentEvent`, `candidateDigest`, and `approvedDigest` are defined before consumers.
- Review Focus coverage: domain leakage, canvas mutation, Agent race, Agent outage, and stale
  approval each have browser or unit tests.
- Scope check: this plan consumes the headless ChangeSet API and ends with one coherent Workbench
  UI; no Core operation is added here.
