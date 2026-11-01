# Studio 原生 Asset Pipeline Phase 1 Implementation Plan

> Document ID: ICE3-STUDIO-PLAN-019
>
> Status: Approved
>
> Owner: ICE3 Studio maintainers

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 Studio 内实现 Rust 原生 GLB、partition、LOD、Tileset、binding manifest 和
Runtime Asset 发布，并让 Studio CLI 不再依赖 Toolchain Sidecar 就能导出 Agent Console
资产。

**Architecture:** 新增 `studio-assets` 负责 glTF/GLB 与 Prepared Profile，新增
`studio-pipeline` 负责 partition/LOD/batching/Tileset；`studio-server` 负责把 Document
revision、evaluated Scene、binding manifest 和原子发布编排成 Runtime bundle；CLI 提供
`export tileset`。旧 Toolchain 在 Phase 1 保留为 parity oracle，但新导出路径不调用它。

**Tech Stack:** Rust 1.98、`gltf` 1.4.1、`meshopt` 0.7.0、Serde、SHA-256、Cargo test、
Studio Core evaluated Scene、Node.js 24.22.1（仅 Viewer 构建和 E2E）。

**Spec:** [Studio 原生 Asset Pipeline 迁移设计](../specs/2026-11-01-native-asset-pipeline-migration-design.md)

## Global Constraints

- Phase 1 不修改 Agent Console。
- 新 `export tileset` 不启动 Node、Blender 或 Toolchain Sidecar。
- Toolchain 在 Phase 1 继续保留，仅用于 parity 对比；新导出路径不得调用它。
- Studio Core 不依赖 partition/LOD/source importer；这些能力放入独立 crate。
- `studio-assets` 与 `studio-pipeline` 不读写 Project、HTTP、ChangeSet 或审批状态。
- 发布只来自 committed revision。
- 发布 Bundle 不包含完整 source GLB。
- `agent-console-v1` profile 必须固定为：`maxTriangles=60000`、`maxBytes=67108864`、
  LOD ratio `0.5`、adaptive allocation、max triangles `120000`、coarse chunk
  `4194304`、semantic proxy off、static render batch。
- 任何失败都不得留下 partial output 或替换上一个成功 Bundle。
- Rust 1.98；Git author/committer 为 `damoqiongqiu <damoqiongqiu@126.com>`。

## Review Focus

- Studio 导出的 Prepared GLB 必须被 Rust reader 接受，且 SHA-256 与 source provenance 一致。
- 共享 Mesh 的 Node 必须保持在同一 atomic partition unit，不能拆断 Selection。
- LOD 必须保留 source identity，simplification 失败时必须保留原 Geometry。
- Tileset URI 必须相对且不能逃逸 output directory。
- 原子发布失败时必须恢复 previous output，旧 Bundle 字节不变。

## File Structure

新增：

```text
crates/studio-assets/Cargo.toml
crates/studio-assets/src/lib.rs
crates/studio-assets/src/accessor.rs
crates/studio-assets/src/glb.rs
crates/studio-assets/src/prepared.rs
crates/studio-assets/tests/prepared_profile.rs
crates/studio-assets/tests/source_reader.rs

crates/studio-pipeline/Cargo.toml
crates/studio-pipeline/src/lib.rs
crates/studio-pipeline/src/partition/analyze.rs
crates/studio-pipeline/src/partition/build.rs
crates/studio-pipeline/src/partition/mod.rs
crates/studio-pipeline/src/partition/plan.rs
crates/studio-pipeline/src/lod/mod.rs
crates/studio-pipeline/src/lod/simplify.rs
crates/studio-pipeline/src/lod/workspace.rs
crates/studio-pipeline/src/batching.rs
crates/studio-pipeline/src/tileset.rs
crates/studio-pipeline/tests/partition.rs
crates/studio-pipeline/tests/lod.rs
crates/studio-pipeline/tests/tileset.rs

crates/studio-server/src/runtime_tileset.rs
crates/studio-server/tests/runtime_tileset.rs
crates/studio-cli/tests/runtime_tileset.rs
```

修改：

```text
Cargo.toml
crates/studio-server/src/lib.rs
crates/studio-server/src/preview.rs
crates/studio-cli/src/main.rs
README.md
docs/README.md
docs/architecture.md
docs/reference/api.md
docs/reference/capabilities.md
docs/guides/getting-started.md
docs/specs/2026-11-01-native-asset-pipeline-migration-design.md
```

---

### Task 1: 建立 `studio-assets` Rust GLB layer

**Files:**

- Create: `crates/studio-assets/Cargo.toml`
- Create: `crates/studio-assets/src/lib.rs`
- Create: `crates/studio-assets/src/accessor.rs`
- Create: `crates/studio-assets/src/glb.rs`
- Create: `crates/studio-assets/src/prepared.rs`
- Create: `crates/studio-assets/tests/prepared_profile.rs`
- Create: `crates/studio-assets/tests/source_reader.rs`
- Modify: `Cargo.toml`

**Interfaces:**

- Consumes: Studio Core's `TriangleMesh`、`Bounds` and `Vec3` only.
- Produces:
  - `read_glb(path: &Path) -> Result<SourceGlb, AssetError>`
  - `read_glb_bytes(bytes: &[u8], base_dir: &Path) -> Result<SourceGlb, AssetError>`
  - `validate_prepared_profile(source: &SourceGlb) -> Result<PreparedProfile, AssetError>`
  - `SourceGlb { json, binary, base_dir, digest }`

- [ ] **Step 1: Add workspace dependencies and crate**

Add:

```toml
gltf = { version = "1.4.1", features = ["utils", "names", "extras"] }
draco3dgltf = "1.5.9"
```

Create `studio-assets` with `studio-core`, `serde`, `serde_json`, `thiserror`, `sha2`,
`gltf`.

- [ ] **Step 2: Write failing Prepared Profile tests**

Create `prepared_profile.rs` with:

```rust
#[test]
fn accepts_a_studio_prepared_glb() {
    let source = read_glb(&fixture("studio-prepared.glb")).unwrap();
    let profile = validate_prepared_profile(&source).unwrap();
    assert_eq!(profile.asset_id, "studio-prepared");
}

#[test]
fn rejects_an_unprepared_glb() {
    let error = read_glb(&fixture("plain.glb"))
        .and_then(|source| validate_prepared_profile(&source))
        .unwrap_err();
    assert_eq!(error.code(), "ICE3_ASSET_PROFILE_UNSUPPORTED");
}

#[test]
fn rejects_external_buffers_outside_base_directory() {
    let error = read_glb(&fixture("external-escape.gltf")).unwrap_err();
    assert_eq!(error.code(), "ICE3_ASSET_INVALID_GLB");
}
```

Use checked-in synthetic fixtures only.

- [ ] **Step 3: Run RED**

```bash
cargo test -p studio-assets prepared_profile
```

Expected: compile failure because the crate and APIs do not exist.

- [ ] **Step 4: Implement reader and profile validator**

Port the required behavior from:

- `ice3-toolchain/packages/toolchain-core/src/source/readSource.ts`
- `ice3-toolchain/packages/toolchain-core/src/source/readAccessor.ts`
- `ice3-toolchain/packages/toolchain-core/src/output/validatePreparedGLB.ts`

Use a custom `read_bounded_uri` resolver. Reject absolute paths, schemes, `..`, symlink
escape and missing files. Compute source SHA-256 after any compression decode.

- [ ] **Step 5: Run crate and existing Studio tests**

```bash
cargo test -p studio-assets
cargo test --workspace
```

Expected: all PASS.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock crates/studio-assets
git commit -m "feat(studio): add Rust GLB asset reader"
```

### Task 2: 迁移 partition unit analysis 与 deterministic plan

**Files:**

- Create: `crates/studio-pipeline/Cargo.toml`
- Create: `crates/studio-pipeline/src/lib.rs`
- Create: `crates/studio-pipeline/src/partition/analyze.rs`
- Create: `crates/studio-pipeline/src/partition/plan.rs`
- Create: `crates/studio-pipeline/src/partition/mod.rs`
- Create: `crates/studio-pipeline/tests/partition.rs`
- Modify: `Cargo.toml`

**Interfaces:**

- Consumes: `studio_assets::SourceGlb`.
- Produces:
  - `analyze_units(&SourceGlb) -> Result<Vec<PartitionUnit>, PipelineError>`
  - `create_partition_plan(units, max_triangles) -> PartitionPlan`
  - `PartitionUnit { id, mesh_keys, triangle_count, bounds, world_matrices }`

- [ ] **Step 1: Write failing unit tests**

```rust
#[test]
fn groups_nodes_that_share_one_mesh() {
    let source = fixture_source("shared-mesh.glb");
    let units = analyze_units(&source).unwrap();
    assert_eq!(units.len(), 1);
    assert_eq!(units[0].mesh_keys.len(), 2);
}

#[test]
fn partition_plan_is_stable_when_node_order_changes() {
    let first = create_partition_plan(
        &analyze_units(&fixture_source("ordered.glb")).unwrap(),
        500_000,
    );
    let second = create_partition_plan(
        &analyze_units(&fixture_source("reordered.glb")).unwrap(),
        500_000,
    );
    assert_eq!(first, second);
}

#[test]
fn splits_on_longest_axis_under_triangle_budget() {
    let plan = create_partition_plan(
        &analyze_units(&fixture_source("wide-plant.glb")).unwrap(),
        1_000,
    );
    assert!(plan.leaves.iter().all(|leaf| leaf.triangle_count <= 1_000));
}
```

- [ ] **Step 2: Run RED**

```bash
cargo test -p studio-pipeline --test partition
```

Expected: crate/APIs missing.

- [ ] **Step 3: Port analysis**

Port:

- `ice3-toolchain/packages/toolchain-core/src/partition/analyzePartitionUnits.ts`
- `ice3-toolchain/packages/toolchain-core/src/partition/createPartitionPlan.ts`
- `ice3-toolchain/packages/toolchain-core/src/scene/nodeMatrices.ts`

Use stable node IDs and mesh IDs for ordering. A shared mesh is atomic and cannot be split
across leaves.

- [ ] **Step 4: Run focused and full tests**

```bash
cargo test -p studio-pipeline --test partition
cargo test --workspace
```

Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock crates/studio-pipeline
git commit -m "feat(studio): add Rust partition planner"
```

### Task 3: 生成 partition GLB 与 Tileset

**Files:**

- Create: `crates/studio-pipeline/src/partition/build.rs`
- Create: `crates/studio-pipeline/src/tileset.rs`
- Create: `crates/studio-pipeline/tests/tileset.rs`
- Modify: `crates/studio-pipeline/src/partition/mod.rs`
- Modify: `crates/studio-pipeline/src/lib.rs`

**Interfaces:**

- Consumes: `PartitionPlan` and `SourceGlb`.
- Produces:
  - `build_artifact(source, options) -> PartitionArtifact`
  - `build_partition(source, plan, options) -> PartitionArtifact`（内部实现）
  - `PartitionArtifact { files, tileset, stats }`
  - `PartitionFile { relative_path, bytes, content_hash, byte_length }`

- [ ] **Step 1: Write failing tileset tests**

```rust
#[test]
fn writes_relative_tileset_content_uris() {
    let artifact = build_fixture("two-leaves.glb", 1_000);
    let json = artifact.tileset_json();
    assert!(json.contains("\"uri\":\"parts/part-000.glb\""));
    assert!(!json.contains("/Users/"));
}

#[test]
fn preserves_total_source_triangles() {
    let artifact = build_fixture("ordered.glb", 500_000);
    assert_eq!(artifact.stats.triangle_count, 4);
}

#[test]
fn rejects_an_empty_partition() {
    let error = build_fixture_error("empty.glb", 500_000);
    assert_eq!(error.code(), "ICE3_STUDIO_PARTITION_INVALID");
}
```

- [ ] **Step 2: Run RED**

```bash
cargo test -p studio-pipeline --test tileset
```

Expected: compile failure for missing build APIs.

- [ ] **Step 3: Implement baseline partition writer**

Port baseline behavior from:

- `ice3-toolchain/packages/toolchain-core/src/output/buildPreparedGLB.ts`
- `ice3-toolchain/packages/toolchain-core/src/partition/partitionAsset.ts`
- `ice3-toolchain/packages/toolchain-core/src/partition/tileset.ts`

Phase 1 baseline must preserve node IDs, selection IDs, material metadata, matrices, bounds
and deterministic JSON ordering. Use relative `parts/part-NNN.glb` URIs.

- [ ] **Step 4: Run focused and full tests**

```bash
cargo test -p studio-pipeline --test tileset
cargo test --workspace
```

Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-pipeline
git commit -m "feat(studio): write partitioned GLBs and tilesets"
```

### Task 4: 实现 Rust LOD、semantic proxy、coarse chunk 和 static batching

**Files:**

- Create: `crates/studio-pipeline/src/lod/mod.rs`
- Create: `crates/studio-pipeline/src/lod/simplify.rs`
- Create: `crates/studio-pipeline/src/lod/workspace.rs`
- Create: `crates/studio-pipeline/src/lod/semantic_proxy.rs`
- Create: `crates/studio-pipeline/src/batching.rs`
- Create: `crates/studio-pipeline/tests/lod.rs`
- Modify: `crates/studio-pipeline/src/partition/build.rs`
- Modify: `crates/studio-pipeline/src/lib.rs`

**Interfaces:**

- Consumes: baseline `PartitionUnit` geometry.
- Produces:
  - `simplify_geometry(positions, indices, options, meshopt::Simplifier) -> LodGeometry`
  - `create_semantic_proxy(samples, mode) -> Option<SemanticProxy>`
  - `create_coarse_chunks(...) -> Vec<CoarseChunk>`
  - `build_static_batches(...) -> Option<RenderBatching>`

- [ ] **Step 1: Add `meshopt = "0.7.0"` and failing LOD tests**

```rust
#[test]
fn simplification_respects_device_triangle_budget() {
    let result = simplify_fixture("dense.glb", 120);
    assert!(result.triangle_count <= 120);
    assert!(result.error_meters.is_finite());
}

#[test]
fn preserves_source_when_simplifier_rejects_geometry() {
    let result = simplify_invalid_fixture();
    assert!(result.preserved_source);
    assert_eq!(result.source_triangles, result.triangle_count);
}

#[test]
fn coarse_chunks_keep_selection_identity() {
    let artifact = build_fixture_with_lod();
    assert!(artifact.all_selection_ids().contains("node-000001/wall-a"));
}
```

- [ ] **Step 2: Run RED**

```bash
cargo test -p studio-pipeline --test lod
```

Expected: missing LOD APIs.

- [ ] **Step 3: Port LOD**

Reference:

- `packages/toolchain-core/src/lod/allocateAdaptiveLod.ts`
- `packages/toolchain-core/src/lod/allocateLodTargets.ts`
- `packages/toolchain-core/src/lod/classifyAdaptiveUnits.ts`
- `packages/toolchain-core/src/lod/createCoarseChunkPlan.ts`
- `packages/toolchain-core/src/lod/createGeometryWorkspace.ts`
- `packages/toolchain-core/src/lod/createLodSampleSchedule.ts`
- `packages/toolchain-core/src/lod/createLodUnitCurve.ts`
- `packages/toolchain-core/src/lod/simplifyGeometry.ts`
- `packages/toolchain-core/src/output/buildRenderBatches.ts`
- `packages/toolchain-core/src/lod/semanticProxy/`

Use `meshopt` for simplification. Keep proportional and adaptive allocation. A failed
simplification preserves source Geometry and records a warning. Apply source/output memory
budgets before keeping coarser geometry resident. Port the opt-in semantic proxy geometry,
fit and evaluation path; `agent-console-v1` keeps `semanticProxy=off`.

- [ ] **Step 4: Write failing batching tests**

```rust
#[test]
fn static_batching_reduces_draws_without_changing_selection() {
    let source = read_fixture("batchable.glb");
    let artifact = build_with_profile(&source, Profile::agent_console_v1());
    assert!(artifact.stats.render_batch_count > 1);
    assert_eq!(
        artifact.sorted_selection_ids(),
        source.sorted_selection_ids(),
    );
}
```

- [ ] **Step 5: Run all pipeline tests**

```bash
cargo test -p studio-pipeline
cargo test --workspace
```

Expected: all PASS.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock crates/studio-pipeline
git commit -m "feat(studio): add native LOD and batching"
```

### Task 5: Studio serialization 与 atomic Runtime bundle

**Files:**

- Create: `crates/studio-server/src/runtime_tileset.rs`
- Create: `crates/studio-server/tests/runtime_tileset.rs`
- Modify: `crates/studio-server/src/lib.rs`
- Modify: `crates/studio-server/src/preview.rs`

**Interfaces:**

- Consumes:
  - `preview::export_source_with_scene`
  - `studio_pipeline::build_artifact`
- Produces:
  - `RuntimeTilesetExporter`
  - `RuntimeTilesetExport`
  - `BindingManifest`
- Output:
  - `tileset.json`
  - `parts/`
  - `coarse/`
  - `bindings.json`
  - `export.json`

- [ ] **Step 1: Write failing binding and publication tests**

```rust
#[test]
fn groups_all_parts_sharing_a_tag_into_one_binding() {
    let manifest = fixture_binding_manifest();
    assert_eq!(manifest.bindings.len(), 1);
    assert_eq!(manifest.bindings[0].selection_id, "plant/equipment/AE-101");
    assert_eq!(manifest.bindings[0].node_selection_ids, [
        "node-000001/wall-a",
        "node-000001/wall-b",
    ]);
    assert_eq!(manifest.bindings[0].area.as_deref(), Some("Train1"));
}

#[test]
fn export_is_deterministic_and_contains_no_source_glb() {
    let first = fixture_export("first").unwrap();
    let second = fixture_export("second").unwrap();
    assert_eq!(first.tileset_digest, second.tileset_digest);
    assert!(!first.output_dir.join("source.glb").exists());
}

#[test]
fn force_publish_failure_keeps_previous_bundle() {
    let fixture = runtime_fixture("invalid-tileset");
    fixture.install_previous_bundle();
    let error = fixture.export(true).unwrap_err();
    assert!(matches!(error, RuntimeTilesetError::Pipeline(_)));
    assert_eq!(fixture.read_keep_file(), "keep");
}
```

- [ ] **Step 2: Run RED**

```bash
cargo test -p studio-server --test runtime_tileset
```

Expected: missing exporter APIs.

- [ ] **Step 3: Implement binding aggregation and manifest**

Expose `export_source_with_scene` as `pub(crate)`. Build bindings from evaluated Node
metadata as specified. Sort tags/node IDs, union world bounds, use source node display name,
and choose `area` from `train` then `stage`.

- [ ] **Step 4: Implement staging and atomic publication**

1. Write source GLB to project cache.
2. Build partition artifact entirely in memory or cache.
3. Create sibling staging directory.
4. Write Tileset/files, `bindings.json`, compute digest, write `export.json`.
5. Refuse existing output unless `force`.
6. On force, rename existing to backup and stage to output; restore on error.
7. Delete managed temporary files.

- [ ] **Step 5: Run focused and full tests**

```bash
cargo test -p studio-server --test runtime_tileset
cargo test --workspace
```

Expected: all PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/studio-server
git commit -m "feat(studio): publish native runtime tilesets"
```

### Task 6: Studio CLI `export tileset`

**Files:**

- Create: `crates/studio-cli/tests/runtime_tileset.rs`
- Modify: `crates/studio-cli/src/main.rs`

**Interfaces:**

- Consumes: `RuntimeTilesetExporter`.
- Produces: `ice3-studio export tileset`.

- [ ] **Step 1: Write failing parser and integration tests**

Unit test in `main.rs`:

```rust
#[test]
fn parses_runtime_tileset_export_command() {
    let cli = Cli::try_parse_from([
        "ice3-studio", "export", "tileset",
        "--project", "project",
        "--output", "output",
        "--profile", "agent-console-v1",
        "--force",
    ]).unwrap();
    assert!(matches!(
        cli.command,
        Command::Export {
            command: ExportCommand::Tileset { force: true, .. }
        }
    ));
}
```

Integration test runs the binary against a synthetic project and asserts all five output
items exist and no `source.glb` exists.

- [ ] **Step 2: Run RED**

```bash
cargo test -p studio-cli parses_runtime_tileset_export_command
```

Expected: export subcommands missing.

- [ ] **Step 3: Implement command**

Add nested clap:

```rust
enum Command {
    Export {
        #[command(subcommand)]
        command: ExportCommand,
    },
}

enum ExportCommand {
    Tileset {
        #[arg(long)]
        project: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value = "agent-console-v1")]
        profile: RuntimeTilesetProfileArg,
        #[arg(long)]
        force: bool,
    },
}
```

Load the current project revision, create `RuntimeTilesetExporter`, export, and print the
result JSON.

- [ ] **Step 4: Run tests**

```bash
cargo test -p studio-cli
cargo test -p studio-server
```

Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-cli
git commit -m "feat(studio-cli): export runtime tilesets"
```

### Task 7: Parity、文档和完整验证

**Files:**

- Modify: `README.md`
- Modify: `docs/README.md`
- Modify: `docs/architecture.md`
- Modify: `docs/reference/api.md`
- Modify: `docs/reference/capabilities.md`
- Modify: `docs/guides/getting-started.md`
- Modify: `docs/specs/2026-11-01-native-asset-pipeline-migration-design.md`
- Modify: `docs/plans/2026-11-01-native-asset-pipeline-phase-1.md`

**Interfaces:**

- Consumes: Rust assets/pipeline/exporter/CLI.
- Produces: parity evidence and current documentation。

- [ ] **Step 1: Add parity scripts**

Create `scripts/compare-asset-pipelines.mjs` outside production Rust:

- Run legacy Toolchain partition on a checked-in synthetic GLB.
- Run `ice3-studio export tileset`.
- Compare source/prepared stats, selection IDs, Tileset bounds, file relative paths and
  digests.
- Emit machine-readable `output/asset-pipeline-parity.json`.

- [ ] **Step 2: Run parity**

```bash
node scripts/compare-asset-pipelines.mjs
```

Expected:

- `semanticIds`, `selectionIds`, triangle totals and bounds match.
- Byte differences caused by JSON ordering are reported, not ignored.
- Exit code is non-zero for unexplained differences.

- [ ] **Step 3: Add full export verification**

```bash
cargo run -q -p studio-cli -- export tileset \
  --project ./projects/wastewater-plant \
  --output /tmp/ice3-runtime-tileset \
  --profile agent-console-v1
```

Expected:

- Output contains only `tileset.json`, `parts/`, `coarse/`, `bindings.json`,
  `export.json`.
- no `source.glb`.
- `bindings.json` contains 51 unique equipment selections.
- output digest is stable on a second run into a different directory.

- [ ] **Step 4: Update docs and Implemented status**

Document:

- `ice3-studio import` remains future Phase 2; Phase 1 covers Studio-authored GLB.
- `export tileset` does not use Toolchain Sidecar.
- output and profile semantics.
- current Toolchain remains only for parity and legacy Source Importer until later phases.

Mark Phase 1 plan and the Phase 1 milestone in Spec-016 `Implemented`. Do not mark whole
Spec-016 Implemented; later importer phases remain.

- [ ] **Step 5: Run complete verification**

```bash
node ../ice3-doc/scripts/check-docs.mjs --root .
PLAYWRIGHT_CHANNEL=chrome npm run verify
node scripts/compare-asset-pipelines.mjs
```

Expected:

- Documentation checks PASS.
- 18 Studio gates PASS.
- Parity report PASS.

- [ ] **Step 6: Commit**

```bash
git add scripts/compare-asset-pipelines.mjs
git add README.md docs/README.md docs/architecture.md
git add docs/reference/api.md docs/reference/capabilities.md
git add docs/guides/getting-started.md
git add docs/specs/2026-11-01-native-asset-pipeline-migration-design.md
git add docs/plans/2026-11-01-native-asset-pipeline-phase-1.md
git commit -m "docs(studio): complete native asset pipeline phase 1"
```
