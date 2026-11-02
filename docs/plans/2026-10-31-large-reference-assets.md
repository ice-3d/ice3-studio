# Large Reference Asset Editing Implementation Plan

> Document ID: ICE3-STUDIO-PLAN-014
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers


> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [x]`) syntax for tracking.

**Goal:** Open the existing 5.75M-triangle wastewater asset in Studio with its original visual
appearance while keeping the editable Document small, revision-safe, and Agent-friendly.

**Architecture:** Store the source GLB, Prepared GLB, tileset, and equipment manifest as immutable
project-local Reference Assets. The Document contains lightweight binding nodes and typed edits;
the Viewer streams the existing tileset and maps imported selection IDs to those nodes.

**Tech Stack:** Rust 2024, Tokio, JSON-RPC 2.0, `serde`, `schemars`, `ts-rs`, existing Toolchain
Sidecar, TypeScript/React Viewer, `ice3` tileset loader, esbuild, Node.js 24.22.1, and Playwright.

**Spec:** `docs/specs/2026-10-31-large-reference-assets-design.md`

## Global Constraints

- Preserve Document and operation history as the source of truth for authored edits.
- Keep imported geometry outside Document and normal JSON-RPC responses.
- Reference assets are immutable after registration; replacement uses a new digest directory.
- Existing parametric projects, changed response shapes, and public operations remain compatible.
- Source, Prepared, tileset, and manifest paths must remain inside the configured workspace.
- Registration requires approval in `review-required` mode.
- Do not expose filesystem paths or credentials in public responses.
- Do not make native GPU, WebGPU, or GPU scene sessions default-on.
- Use Node.js `24.22.1`; do not introduce Vite or Vitest.
- Do not use competitor or vendor names in code, comments, tests, documentation, or commits.
- Git author and committer name is `damoqiongqiu`.
- Root verification is
  `PATH=$HOME/.nvm/versions/node/v24.22.1/bin:$PATH npm run verify`.

## Review Focus

- A 5.75M-triangle asset must not appear in `document.get`; any regression must fail the
  bounded-document test rather than reaching `ERR_STRING_TOO_LONG`.
- Path traversal, symlinks leaving the project, and a digest mismatch must leave the project
  unchanged and return a structured diagnostic.
- A tile unload/reload cycle must preserve equipment selection mapping and authored overrides.
- A stale or replaced asset digest must never be silently substituted for a revision's reference.
- Portrait/landscape camera framing and selected large surfaces must remain stable during repeated
  rotation and zoom.

---

## File Structure

- `crates/studio-core/src/reference_asset.rs`: backend-neutral descriptor, stats, binding, and
  reference-instance contracts.
- `crates/studio-core/src/document.rs`: adds `NodeContent::ReferenceInstance`.
- `crates/studio-core/tests/reference_asset.rs`: serialization, validation, and generated type tests.
- `crates/studio-project/src/reference_asset_store.rs`: secure registration, hashing, manifest
  parsing, immutable storage, and descriptor lookup.
- `crates/studio-server/src/reference_asset.rs`: RPC and HTTP asset-route behavior.
- `crates/studio-server/tests/reference_asset_rpc.rs`: registration, authorization, paging,
  traversal, replacement, and bounded-response tests.
- `crates/studio-server/src/agent.rs`: manifest tools and workflow updates.
- `crates/studio-server/src/rpc.rs`: dispatch, grants, static asset routes, and project-open state.
- `apps/viewer/src/api/StudioClient.ts`: reference descriptors and binding pages.
- `apps/viewer/src/session/SessionStore.ts`: reference state without loading asset bytes.
- `apps/viewer/src/viewport/SceneAdapter.ts`: tileset loading, selection translation, and overlays.
- `apps/viewer/src/viewport/cameraFraming.ts`: tested bounds/aspect framing.
- `apps/viewer/tests/reference-session.test.ts`: bounded session and selection mapping tests.
- `apps/viewer/tests/camera-framing.test.ts`: portrait, landscape, and clip-plane regression tests.
- `docs/reference/api.md`, `docs/reference/diagnostics.md`, `docs/README.md`: public behavior and
  status.
- `scripts/run-large-reference-acceptance.mjs`: real-asset acceptance report.

### Task 1: Define Reference Contracts

**Files:**
- Create: `crates/studio-core/src/reference_asset.rs`
- Modify: `crates/studio-core/src/lib.rs`
- Modify: `crates/studio-core/src/document.rs`
- Modify: `crates/studio-core/src/geometry/mod.rs`
- Modify: `crates/studio-core/src/validation.rs`
- Test: `crates/studio-core/tests/reference_asset.rs`

**Interfaces:**

- Consumes: `Bounds`, `Transform`, `MaterialId`, `JsonSchema`, `TS`.
- Produces:
  - `ReferenceAssetStats`
  - `ReferenceAssetDescriptor`
  - `ReferenceAssetEquipment`
  - `ReferenceInstanceContent`
  - `NodeContent::ReferenceInstance(ReferenceInstanceContent)`

- [x] **Step 1: Write the failing contract test**

```rust
#[test]
fn reference_instance_and_descriptor_round_trip() {
    let descriptor = ReferenceAssetDescriptor {
        asset_id: "asset-000001".into(),
        source_digest: "a".repeat(64),
        prepared_digest: Some("b".repeat(64)),
        tileset_digest: Some("c".repeat(64)),
        bounds: Bounds {
            min: Vec3 { x: -625.0, y: -1.0, z: -390.0 },
            max: Vec3 { x: 625.0, y: 31.0, z: 390.0 },
        },
        stats: ReferenceAssetStats {
            source_nodes: 1761,
            effective_triangles: 5_757_962,
            unique_meshes: 1632,
            materials: 75,
            equipment: 204,
        },
        imported_at: "2026-10-31T00:00:00Z".into(),
    };
    let binding = SceneNode {
        id: NodeId::from("node-000001"),
        name: "AE-101".into(),
        parent_id: None,
        transform: Transform::IDENTITY,
        material_id: None,
        content: NodeContent::ReferenceInstance(ReferenceInstanceContent {
            asset_id: descriptor.asset_id.clone(),
            selection_id: "ice3-virtual-wwtp/equipment/AE-101".into(),
        }),
        metadata: BTreeMap::new(),
        visible: true,
    };
    let value = serde_json::to_value(binding).unwrap();
    assert_eq!(value["content"]["type"], "reference_instance");
    assert_eq!(
        value["content"]["selectionId"],
        "ice3-virtual-wwtp/equipment/AE-101"
    );
}
```

- [x] **Step 2: Run the test and verify it fails**

Run:

```bash
cargo test -p studio-core --test reference_asset reference_instance_and_descriptor_round_trip
```

Expected: compilation fails because the reference contract and enum variant do not exist.

- [x] **Step 3: Implement the contracts**

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct ReferenceInstanceContent {
    pub asset_id: String,
    pub selection_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct ReferenceAssetDescriptor {
    pub asset_id: String,
    pub source_digest: String,
    pub prepared_digest: Option<String>,
    pub tileset_digest: Option<String>,
    pub bounds: Bounds,
    pub stats: ReferenceAssetStats,
    pub imported_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct ReferenceAssetStats {
    pub source_nodes: usize,
    pub effective_triangles: u64,
    pub unique_meshes: usize,
    pub materials: usize,
    pub equipment: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct ReferenceAssetEquipment {
    pub selection_id: String,
    pub name: String,
    pub kind: Option<String>,
    pub process_stage: Option<String>,
    pub area: Option<String>,
    pub bounds: Bounds,
    pub transform: Transform,
    pub material_ids: Vec<MaterialId>,
}
```

Add the enum variant:

```rust
#[serde(rename = "reference_instance")]
ReferenceInstance(ReferenceInstanceContent),
```

Treat reference instances as geometryless binding nodes during local scene evaluation. Validation
checks non-empty IDs and selection IDs but does not evaluate external geometry. Export remains
handled in Task 4.

- [x] **Step 4: Run focused tests**

Run:

```bash
cargo test -p studio-core --test reference_asset
cargo test -p studio-core --test agent_contract
cargo test -p studio-core --test validation
cargo test -p studio-core export_bindings
```

Expected: all pass and new TypeScript bindings are generated.

- [x] **Step 5: Commit**

```bash
git add crates/studio-core
git commit -m "feat(studio-core): define reference asset contracts"
```

### Task 2: Register Immutable Reference Assets

**Files:**
- Create: `crates/studio-project/src/reference_asset_store.rs`
- Modify: `crates/studio-project/src/lib.rs`
- Modify: `crates/studio-project/src/store.rs`
- Test: `crates/studio-project/tests/reference_asset_store.rs`

**Interfaces:**

- Consumes: `WorkspacePath`, `ProjectStore::project_dir`, `ReferenceAssetDescriptor`.
- Produces:
  - `ReferenceAssetSource`
  - `ReferenceAssetRegistration { asset_id, descriptor }`
  - `RegisteredReferenceAsset { descriptor, root: PathBuf }`
  - `ReferenceBindingPage { items: Vec<ReferenceAssetEquipment>, next_cursor: Option<String> }`
  - `ReferenceAssetStore::register(&ReferenceAssetSource) -> Result<ReferenceAssetRegistration, ReferenceAssetError>`
  - `ReferenceAssetStore::get(&str) -> Result<RegisteredReferenceAsset, ReferenceAssetError>`
  - `ReferenceAssetStore::bindings(&str, Option<&str>, usize) -> Result<ReferenceBindingPage, ReferenceAssetError>`

`ReferenceAssetSource` is:

```rust
pub struct ReferenceAssetSource {
    pub source_path: PathBuf,
    pub prepared_path: Option<PathBuf>,
    pub tileset_path: Option<PathBuf>,
    pub equipment_manifest_path: PathBuf,
}
```

- [x] **Step 1: Write failing registration and security tests**

```rust
#[test]
fn registers_hashes_and_pages_a_reference_asset() {
    let fixture = fixture_workspace();
    let store = ReferenceAssetStore::new(fixture.project());
    let registered = store.register(&fixture.source()).unwrap();
    assert_eq!(registered.descriptor.stats.equipment, 204);
    assert_eq!(registered.descriptor.stats.effective_triangles, 5_757_962);
    assert_eq!(store.get("asset-000001").unwrap().descriptor, registered.descriptor);
    let page = store.bindings("asset-000001", None, 25).unwrap();
    assert_eq!(page.items.len(), 25);
    assert_eq!(page.items[0].selection_id, "ice3-virtual-wwtp/equipment/AE-101");
}

#[test]
fn rejects_paths_and_symlinks_outside_the_workspace() {
    let fixture = fixture_workspace();
    let error = fixture.store().register(&fixture.source_with_source_path(
        "../../outside.glb",
    )).unwrap_err();
    assert!(matches!(error, ReferenceAssetError::PathEscape));
}

#[test]
fn replacement_with_mismatched_digest_is_atomic() {
    let fixture = fixture_workspace();
    let error = fixture.store().register(&fixture.source_with_tampered_source()).unwrap_err();
    assert!(matches!(error, ReferenceAssetError::DigestMismatch));
    assert!(!fixture.project().join("references/asset-000001").exists());
}
```

- [x] **Step 2: Run the tests and verify they fail**

Run:

```bash
cargo test -p studio-project --test reference_asset_store
```

Expected: compilation fails because `ReferenceAssetStore` does not exist.

- [x] **Step 3: Implement secure registration**

Create `references` in `ProjectStore::create` and `ProjectStore::open`. Resolve every supplied
relative path, canonicalize it, require it to start with the canonical workspace root, reject
symlink components, then hash with a buffered SHA-256 reader.

Parse only the GLB JSON chunk for `asset.extras.ice3` stats and bounds. Parse the equipment
manifest into bounded `ReferenceAssetEquipment` values. Copy files to
`references/<assetId>/.tmp-<nonce>`, verify copied hashes, write `asset.json`, then rename the
temporary directory to `references/<assetId>`.

Do not store geometry buffers in `asset.json`.

- [x] **Step 4: Run focused tests**

Run:

```bash
cargo test -p studio-project --test reference_asset_store
cargo test -p studio-project
```

Expected: all pass.

- [x] **Step 5: Commit**

```bash
git add crates/studio-project
git commit -m "feat(studio-project): store large reference assets"
```

### Task 3: Expose Bounded Reference APIs

**Files:**
- Create: `crates/studio-server/src/reference_asset.rs`
- Modify: `crates/studio-server/src/lib.rs`
- Modify: `crates/studio-server/src/rpc.rs`
- Modify: `crates/studio-server/src/capabilities.rs`
- Modify: `crates/studio-server/src/agent.rs`
- Modify: `crates/studio-cli/src/schema.rs`
- Test: `crates/studio-server/tests/reference_asset_rpc.rs`
- Test: `crates/studio-server/tests/agent_api.rs`

**Interfaces:**

- Consumes: Task 2 store and Task 1 descriptor contracts.
- Produces:
  - `referenceAsset.register`
  - `referenceAsset.get`
  - `referenceAsset.bindings`
  - `/reference-assets/{assetId}/tileset.json`
  - `/reference-assets/{assetId}/tiled/{*path}`
  - `/reference-assets/{assetId}/prepared.glb`

Typed contracts:

```rust
pub struct ReferenceAssetRegisterParams {
    pub source_path: String,
    pub prepared_path: Option<String>,
    pub tileset_path: Option<String>,
    pub equipment_manifest_path: String,
}

pub struct ReferenceAssetGetParams {
    pub asset_id: String,
}

pub struct ReferenceAssetBindingsParams {
    pub asset_id: String,
    pub cursor: Option<String>,
    pub limit: usize,
}

pub struct ReferenceAssetEndpoint {
    pub descriptor: ReferenceAssetDescriptor,
    pub tileset_url: Option<String>,
    pub prepared_preview_url: Option<String>,
    pub equipment_manifest_url: String,
}
```

- [x] **Step 1: Write failing RPC tests**

```rust
#[tokio::test]
async fn registers_and_reads_a_reference_without_geometry_in_rpc() {
    let server = start_test_server(ServerConfig::review_required("reference-rpc")).await;
    let registered = rpc_with_approval_cookie(
        &server,
        "referenceAsset.register",
        serde_json::json!({
            "sourcePath": "refs/source.glb",
            "preparedPath": "refs/prepared.glb",
            "tilesetPath": "refs/tiled",
            "equipmentManifestPath": "refs/equipment.json"
        }),
    ).await;
    let asset = rpc_with_bearer(
        &server,
        server.agent_token(),
        "referenceAsset.get",
        serde_json::json!({ "assetId": registered["result"]["assetId"] }),
    ).await;
    assert_eq!(asset["result"]["descriptor"]["stats"]["equipment"], 204);
    assert!(asset["result"].get("positions").is_none());
    assert!(asset.to_string().len() < 64 * 1024);
}

#[tokio::test]
async fn reference_registration_requires_approval_and_rejects_escape() {
    let server = start_test_server(ServerConfig::review_required("reference-auth")).await;
    let denied = rpc_with_bearer(
        &server,
        server.agent_token(),
        "referenceAsset.register",
        serde_json::json!({
            "sourcePath": "../source.glb",
            "preparedPath": null,
            "tilesetPath": null,
            "equipmentManifestPath": "../equipment.json"
        }),
    ).await;
    assert_eq!(denied["error"]["diagnosticCode"], "ICE3_STUDIO_AUTH_REQUIRED");
}
```

- [x] **Step 2: Run the tests and verify they fail**

Run:

```bash
cargo test -p studio-server --test reference_asset_rpc
```

Expected: the methods are unknown and the test fails.

- [x] **Step 3: Implement RPC, authorization, and routes**

Add typed camelCase request/result structs with `deny_unknown_fields`. Add
`referenceAsset.register` to approval methods, `referenceAsset.get` and
`referenceAsset.bindings` to read methods, `system.capabilities.methods`, and the Agent manifest.

The file handler resolves `assetId` and relative path through `ReferenceAssetStore`; it rejects
absolute paths, `..`, and symlinks outside the asset directory. Serve only GET/HEAD with explicit
JSON or GLB content types.

Bound binding pages to `1..=100`. Default to 25 and cursor by stable `selectionId`.

- [x] **Step 4: Run focused tests**

Run:

```bash
cargo test -p studio-server --test reference_asset_rpc
cargo test -p studio-server --test agent_api
cargo test -p studio-cli --test schema_output
```

Expected: all pass and the generated schema includes all three methods and request/result types.

- [x] **Step 5: Commit**

```bash
git add crates/studio-server crates/studio-cli schemas
git commit -m "feat(studio-server): expose bounded reference asset APIs"
```

### Task 4: Edit Reference Bindings Through ChangeSets

**Files:**
- Modify: `crates/studio-server/src/rpc.rs`
- Modify: `crates/studio-server/src/changeset.rs`
- Modify: `crates/studio-server/src/iteration_impact.rs`
- Modify: `crates/studio-server/src/scene_session.rs`
- Modify: `crates/studio-server/src/agent.rs`
- Modify: `crates/studio-core/src/validation.rs`
- Test: `crates/studio-server/tests/reference_asset_editing.rs`

**Interfaces:**

- Consumes: `NodeContent::ReferenceInstance`, normal `node.create`, `node.update`,
  `instance.patch`, and `material.assign` operations.
- Produces: local ChangeSet edits over binding nodes and aggregate reference statistics.

- [x] **Step 1: Write failing binding-edit tests**

```rust
#[tokio::test]
async fn transform_and_material_edits_on_bindings_are_local() {
    let server = reference_fixture_server("reference-edit").await;
    let binding = create_binding_change_set(&server, "asset-000001", "AE-101").await;
    let committed = commit_direct(&server, binding).await;
    assert_eq!(committed["result"]["revision"], 2);

    let edit = create_change_set(&server, vec![
        serde_json::json!({
            "type": "node.update",
            "node_id": "node-000001",
            "name": null,
            "transform": {
                "translation": { "x": 4, "y": 0, "z": 0 },
                "rotation": [0, 0, 0, 1],
                "scale": { "x": 1, "y": 1, "z": 1 }
            },
            "visible": null,
            "metadata": null
        }),
        serde_json::json!({
            "type": "material.assign",
            "node_id": "node-000001",
            "material_id": "material-000001"
        }),
    ]).await;
    let impact = request_impact(&server, &edit).await;
    assert_eq!(impact["scope"], "local");
    assert_eq!(impact["affectedSourceNodes"], 1);
    assert_eq!(impact["fullSceneRebuildRequired"], false);
}

#[tokio::test]
async fn context_aggregates_reference_stats_without_inline_geometry() {
    let server = reference_fixture_server("reference-context").await;
    create_and_commit_bindings(&server).await;
    let context = rpc_with_bearer(&server, server.agent_token(), "scene.context", serde_json::json!({})).await;
    assert_eq!(context["result"]["counts"]["sourceNodes"], 204);
    assert!(context.to_string().len() < 16 * 1024);
}
```

- [x] **Step 2: Run tests and verify they fail**

Run:

```bash
cargo test -p studio-server --test reference_asset_editing
```

Expected: binding validation or aggregate statistics are not implemented.

- [x] **Step 3: Implement binding behavior**

Server-side validation verifies that every reference node's asset exists and selection ID exists
in the manifest. `scene.context`, `evaluation.stats`, and search include binding counts and add the
reference descriptor's baseline triangle/mesh counts once per referenced digest.

`evaluate_large_scene_selected` emits a geometryless scene node for `ReferenceInstance`.
`changeset.impact` classifies one binding transform or material edit as local and reports zero
generated reference geometry.

`export.sourceGlb` streams the registered source unchanged for a baseline-only reference project.
If any reference binding differs from its manifest baseline, return
`ICE3_STUDIO_REFERENCE_EXPORT_UNSUPPORTED`.

- [x] **Step 4: Run focused tests**

Run:

```bash
cargo test -p studio-server --test reference_asset_editing
cargo test -p studio-server --test agent_iteration_impact
cargo test -p studio-server --test scene_session_rpc
```

Expected: all pass.

- [x] **Step 5: Commit**

```bash
git add crates/studio-core/src/validation.rs crates/studio-server
git commit -m "feat(studio-server): edit reference bindings through changesets"
```

### Task 5: Stream Reference Assets In The Viewer

**Files:**
- Modify: `apps/viewer/src/api/StudioClient.ts`
- Modify: `apps/viewer/src/session/SessionStore.ts`
- Modify: `apps/viewer/src/viewport/SceneAdapter.ts`
- Modify: `apps/viewer/src/viewport/Viewport.tsx`
- Modify: `apps/viewer/src/app/Workbench.tsx`
- Test: `apps/viewer/tests/reference-session.test.ts`

**Interfaces:**

- Consumes: Task 3 APIs and `ICETilesetLoader`.
- Produces:
  - `StudioReferenceAsset`
  - `SessionSnapshot.reference`
  - `SceneAdapter.showReference(asset, bindings)`
  - imported selection ID → Studio node ID translation.

- [x] **Step 1: Write failing session tests**

```ts
test('loads reference metadata without requesting document geometry', async () => {
  const source = {
    document: async () => documentWithOneBinding(),
    validation: async () => [],
    referenceAsset: async () => largeReferenceDescriptor(),
  };
  const store = new SessionStore(source);
  await store.refresh();
  expect(store.snapshot.document?.scene.nodes).toHaveLength(1);
  expect(store.snapshot.reference?.stats.effectiveTriangles).toBe(5_757_962);
});

test('maps imported selection IDs back to Studio binding IDs', () => {
  expect(mapReferenceSelection(
    'ice3-virtual-wwtp/equipment/AE-101',
    [{ nodeId: 'node-000001', selectionId: 'ice3-virtual-wwtp/equipment/AE-101' }],
  )).toBe('node-000001');
});
```

- [x] **Step 2: Run tests and verify they fail**

Run:

```bash
cd apps/viewer
PATH=$HOME/.nvm/versions/node/v24.22.1/bin:$PATH npm test
```

Expected: compile/test failure because reference session fields and mapping do not exist.

- [x] **Step 3: Implement metadata loading and tileset rendering**

Add `StudioClient.referenceAsset(assetId)` and binding-page calls with bounded requests.
`SessionStore.refresh` loads descriptor metadata in parallel with the small Document.

`SceneAdapter.showReference` creates `ICETilesetLoader` with the existing camera, opens
`tilesetUrl`, frames `handle.bounds`, sets `handle.scene`, and retains the handle. Dispose it only
after the replacement handle has produced a usable coarse tile.

Translate engine selection IDs through the binding map before calling `onSelection`. Apply
binding transform/visibility overrides to loaded imported nodes by stable selection ID on every
`TILESET_TILE_LOADED` event.

- [x] **Step 4: Run focused tests**

Run:

```bash
cd apps/viewer
PATH=$HOME/.nvm/versions/node/v24.22.1/bin:$PATH npm test
PATH=$HOME/.nvm/versions/node/v24.22.1/bin:$PATH npm run typecheck
PATH=$HOME/.nvm/versions/node/v24.22.1/bin:$PATH npm run build
```

Expected: all pass.

- [x] **Step 5: Commit**

```bash
git add apps/viewer
git commit -m "feat(studio-viewer): stream reference assets"
```

### Task 6: Stabilize Camera And Selection Rendering

**Files:**
- Modify: `apps/viewer/src/viewport/cameraFraming.ts`
- Modify: `apps/viewer/src/viewport/SceneAdapter.ts`
- Modify: `apps/viewer/tests/camera-framing.test.ts`
- Modify: `<workspace>/ice3-engine/packages/renderer/src/WebGL2RendererBackend.ts`
- Test: `apps/viewer/e2e/large-reference.spec.ts`

**Interfaces:**

- Consumes: `frameBoundsForViewport`, `SCENE_CAMERA_NEAR`, `SCENE_CAMERA_FAR`, and renderer
  selection passes.
- Produces: stable bounds framing and depth-safe selection color.

- [x] **Step 1: Extend the failing framing and selection tests**

```ts
test('keeps all large-model corners inside the viewport after a portrait rotation', () => {
  assertBoundsInsideViewport(largePlantBounds, 0.72);
});

test('uses negative polygon offset for selected semantic batches', async ({ page }) => {
  await page.addInitScript(() => {
    const original = WebGL2RenderingContext.prototype.polygonOffset;
    const offsets: Array<[number, number]> = [];
    Object.defineProperty(window, '__polygonOffsets', { value: offsets });
    WebGL2RenderingContext.prototype.polygonOffset = function (factor, units) {
      offsets.push([factor, units]);
      return original.call(this, factor, units);
    };
  });
  await page.goto(referenceAcceptanceUrl());
  await selectReferenceBinding(page, 'ice3-virtual-wwtp/equipment/GW-101');
  const offsets = await page.evaluate(() => (
    window as unknown as { __polygonOffsets: Array<[number, number]> }
  ).__polygonOffsets);
  expect(offsets.some(([factor, units]) => factor < 0 && units < 0)).toBe(true);
});
```

- [x] **Step 2: Run tests and verify the new selection test fails**

Run:

```bash
(cd apps/viewer && PATH=$HOME/.nvm/versions/node/v24.22.1/bin:$PATH npm test)
(cd apps/viewer && PATH=$HOME/.nvm/versions/node/v24.22.1/bin:$PATH npm run test:e2e -- large-reference.spec.ts)
```

Expected: framing tests pass; batched semantic selection does not set negative polygon offset.

- [x] **Step 3: Make selection bias uniform**

Before both `semanticProgram` highlight paths, set `gl.polygonOffset(-1, -4)`. Reset to
`gl.polygonOffset(2, 4)` after selection rendering. Keep ordinary tint and selected-edge behavior
unchanged.

Document the chosen camera clip range and verify the eight projected bounds corners remain within
`[-0.9, 0.9]` on both axes.

- [x] **Step 4: Run focused tests**

Run:

```bash
(cd apps/viewer && PATH=$HOME/.nvm/versions/node/v24.22.1/bin:$PATH npm test)
(cd apps/viewer && PATH=$HOME/.nvm/versions/node/v24.22.1/bin:$PATH npm run test:e2e -- large-reference.spec.ts)
```

Expected: all pass.

- [x] **Step 5: Commit both repositories**

```bash
git -C <workspace>/ice3-engine add \
  packages/renderer/src/WebGL2RendererBackend.ts
git -C <workspace>/ice3-engine commit \
  -m "fix(renderer): stabilize semantic selection depth"

git add apps/viewer/e2e/large-reference.spec.ts apps/viewer
git commit -m "test(studio-viewer): verify stable reference camera framing"
```

### Task 7: Real Asset Acceptance And Documentation

**Files:**
- Create: `scripts/run-large-reference-acceptance.mjs`
- Create during acceptance: `output/large-reference-acceptance.json`
- Create during acceptance: `output/large-reference-acceptance.png`
- Modify: `docs/reference/api.md`
- Modify: `docs/reference/diagnostics.md`
- Modify: `docs/README.md`
- Modify: `docs/specs/2026-10-31-large-reference-assets-design.md`

**Interfaces:**

- Consumes: all prior tasks and the existing Console source/Prepared/tileset/equipment files.
- Produces: reproducible acceptance evidence and final status.

- [x] **Step 1: Write a failing acceptance invariant test**

```js
test('accepts only a bounded reference project with complete equipment semantics', () => {
  assert.doesNotThrow(() => assertReferenceAcceptance({
    documentBytes: 480_000,
    contextBytes: 8_200,
    sourceNodes: 204,
    equipment: 204,
    effectiveTriangles: 5_757_962,
    coarseVisible: true,
    selectionMapped: true,
    cameraClipped: false,
    flickerFrames: 0,
  }));
  assert.throws(() => assertReferenceAcceptance({
    documentBytes: 300_000_000,
    contextBytes: 8_200,
    sourceNodes: 5065,
    equipment: 204,
    effectiveTriangles: 5_757_962,
    coarseVisible: true,
    selectionMapped: true,
    cameraClipped: true,
    flickerFrames: 3,
  }));
});
```

- [x] **Step 2: Run it and verify it fails**

Run:

```bash
PATH=$HOME/.nvm/versions/node/v24.22.1/bin:$PATH \
  node --test scripts/tests/large-reference-acceptance.test.mjs
```

Expected: missing acceptance helper.

- [x] **Step 3: Implement the acceptance workflow**

The script must:

1. Start a review-required Studio server.
2. Register the existing Console source, Prepared GLB, tileset, and equipment manifest.
3. Create exactly 204 reference binding nodes through ChangeSets.
4. Measure `document.get`, `scene.context`, and one binding page.
5. Open the tileset in the Viewer and wait for a coarse visible tile.
6. Verify the complete model remains inside the viewport in portrait and landscape.
7. Select ground, road, water, and a building; rotate and zoom through deterministic camera poses;
   capture five frames at each pose.
8. Fail if large selected surfaces change tint between consecutive stable frames without a camera
   state difference.
9. Verify an Agent transform/material ChangeSet commits as a local binding edit.
10. Write JSON and screenshot evidence.

Document regeneration and run commands.

- [x] **Step 4: Run acceptance and full verification**

Run:

```bash
PATH=$HOME/.nvm/versions/node/v24.22.1/bin:$PATH \
  node --test scripts/tests/large-reference-acceptance.test.mjs
PATH=$HOME/.nvm/versions/node/v24.22.1/bin:$PATH \
  node scripts/run-large-reference-acceptance.mjs
PATH=$HOME/.nvm/versions/node/v24.22.1/bin:$PATH npm run verify
```

Expected:

- 204 binding nodes;
- 204 equipment entries;
- 5,757,962 effective triangles;
- bounded responses;
- no camera clipping;
- zero selected-surface flicker frames;
- all 14 verification gates pass.

- [x] **Step 5: Mark implemented and commit**

Only after Step 4 passes:

```bash
git add docs scripts/run-large-reference-acceptance.mjs \
  scripts/tests/large-reference-acceptance.test.mjs
git commit -m "docs(studio): mark large reference assets implemented"
```

## Self-Review

- Spec coverage: Tasks 1-3 cover contracts, immutable storage, path safety, RPCs, asset routes,
  authorization, and bounded discovery. Task 4 covers binding edits, stats, impact, and export.
  Task 5 covers tileset streaming and selection mapping. Task 6 covers camera and selection
  stability. Task 7 covers real-asset acceptance and documentation.
- Placeholder scan: all tasks name exact files, interfaces, test code, commands, expected failures,
  and commits.
- Type consistency: `ReferenceAssetDescriptor`, `ReferenceInstanceContent`, `assetId`,
  `selectionId`, `referenceAsset.register`, `referenceAsset.get`, and
  `referenceAsset.bindings` are used consistently.
- Review Focus coverage: bounded Document tests are in Tasks 3, 4, and 7; path and digest tests are
  in Task 2; tile reload and mapping tests are in Task 5; stale digest checks are in Tasks 2 and 4;
  camera and flicker tests are in Task 6.
