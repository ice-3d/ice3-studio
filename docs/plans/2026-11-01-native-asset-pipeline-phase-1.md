# Studio 原生 Asset Pipeline Phase 1 Implementation Plan

> Document ID: ICE3-STUDIO-PLAN-019
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

## 1. Implementation Outcome

Phase 1 已实现。Studio 新增 `studio-assets`、`studio-pipeline` 和
`studio-server::runtime_tileset`，CLI 提供 `export tileset`。真实
`projects/wastewater-plant` revision 1 导出了 4 个 fine partition、4 个 coarse chunk、
132,124 unique triangles、51 个设备绑定，并保持稳定 tileset/bundle digest。Phase 1 不调用
Toolchain Sidecar；Toolchain 只用于 `scripts/compare-asset-pipelines.mjs` 双跑对照。

> **执行说明：** 本计划已按 `executing-plans` 在单一实现上下文中逐任务执行；所有步骤均以
> 测试先行和 checkbox 状态记录。

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

## 2. Global Constraints

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

## 3. Review Focus

- Studio 导出的 Prepared GLB 必须被 Rust reader 接受，且 SHA-256 与 source provenance 一致。
- 共享 Mesh 的 Node 必须保持在同一 atomic partition unit，不能拆断 Selection。
- LOD 必须保留 source identity，simplification 失败时必须保留原 Geometry。
- Tileset URI 必须相对且不能逃逸 output directory。
- 原子发布失败时必须恢复 previous output，旧 Bundle 字节不变。

## 4. File Structure

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

### 4.1 Task 1: 建立 `studio-assets` Rust GLB layer

**Files:**

- 新增：`crates/studio-assets/Cargo.toml`
- 新增：`crates/studio-assets/src/lib.rs`
- 新增：`crates/studio-assets/src/accessor.rs`
- 新增：`crates/studio-assets/src/glb.rs`
- 新增：`crates/studio-assets/src/prepared.rs`
- 新增：`crates/studio-assets/tests/prepared_profile.rs`
- 新增：`crates/studio-assets/tests/source_reader.rs`
- 修改：`Cargo.toml`

**Interfaces:**

- Consumes: Studio Core's `TriangleMesh`、`Bounds` and `Vec3` only.
- Produces:
  - `read_glb(path: &Path) -> Result<SourceGlb, AssetError>`
  - `read_glb_bytes(bytes: &[u8], base_dir: &Path) -> Result<SourceGlb, AssetError>`
  - `validate_prepared_profile(source: &SourceGlb) -> Result<PreparedProfile, AssetError>`
  - `SourceGlb { json, binary, base_dir, digest }`

- [x] **Step 1: Add workspace dependencies and crate**

Add:

```toml
gltf = { version = "1.4.1", features = ["utils", "names", "extras"] }
draco3dgltf = "1.5.9"
```

Create `studio-assets` with `studio-core`, `serde`, `serde_json`, `thiserror`, `sha2`,
`gltf`.

- [x] **Step 2: Write failing Prepared Profile tests**

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

- [x] **Step 3: Run RED**

```bash
cargo test -p studio-assets prepared_profile
```

预期：crate 和 API 尚不存在，因此编译失败。

- [x] **Step 4: Implement reader and profile validator**

Port the required behavior from:

- `ice3-toolchain/packages/toolchain-core/src/source/readSource.ts`
- `ice3-toolchain/packages/toolchain-core/src/source/readAccessor.ts`
- `ice3-toolchain/packages/toolchain-core/src/output/validatePreparedGLB.ts`

使用自定义 `read_bounded_uri` resolver，拒绝绝对路径、URI scheme、`..`、symlink escape
和缺失文件；压缩解码完成后再计算 source SHA-256。

- [x] **Step 5: 运行 crate 与现有 Studio 测试**

```bash
cargo test -p studio-assets
cargo test --workspace
```

Expected: all PASS.

- [x] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock crates/studio-assets
git commit -m "feat(studio): add Rust GLB asset reader"
```

### 4.2 Task 2: 迁移 partition unit analysis 与 deterministic plan

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

- [x] **Step 1: Write failing unit tests**

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

- [x] **Step 2: Run RED**

```bash
cargo test -p studio-pipeline --test partition
```

Expected: crate/APIs missing.

- [x] **Step 3: Port analysis**

Port:

- `ice3-toolchain/packages/toolchain-core/src/partition/analyzePartitionUnits.ts`
- `ice3-toolchain/packages/toolchain-core/src/partition/createPartitionPlan.ts`
- `ice3-toolchain/packages/toolchain-core/src/scene/nodeMatrices.ts`

使用稳定 node ID 和 mesh ID 排序。共享 Mesh 是 atomic unit，不能在 leaf 之间拆开。

- [x] **Step 4: Run focused and full tests**

```bash
cargo test -p studio-pipeline --test partition
cargo test --workspace
```

Expected: all PASS.

- [x] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock crates/studio-pipeline
git commit -m "feat(studio): add Rust partition planner"
```

### 4.3 Task 3: 生成 partition GLB 与 Tileset

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

- [x] **Step 1: Write failing tileset tests**

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

- [x] **Step 2: Run RED**

```bash
cargo test -p studio-pipeline --test tileset
```

Expected: compile failure for missing build APIs.

- [x] **Step 3: Implement baseline partition writer**

Port baseline behavior from:

- `ice3-toolchain/packages/toolchain-core/src/output/buildPreparedGLB.ts`
- `ice3-toolchain/packages/toolchain-core/src/partition/partitionAsset.ts`
- `ice3-toolchain/packages/toolchain-core/src/partition/tileset.ts`

Phase 1 baseline 必须保留 node ID、selection ID、material metadata、matrix、bounds 和确定性
JSON 顺序，并使用相对 `parts/part-NNN.glb` URI。

- [x] **Step 4: Run focused and full tests**

```bash
cargo test -p studio-pipeline --test tileset
cargo test --workspace
```

Expected: all PASS.

- [x] **Step 5: Commit**

```bash
git add crates/studio-pipeline
git commit -m "feat(studio): write partitioned GLBs and tilesets"
```

### 4.4 Task 4: 实现 Rust LOD、semantic proxy、coarse chunk 和 static batching

**Files:**

- 新增：`crates/studio-pipeline/src/lod/mod.rs`
- 新增：`crates/studio-pipeline/src/lod/simplify.rs`
- 新增：`crates/studio-pipeline/src/lod/workspace.rs`
- 新增：`crates/studio-pipeline/src/lod/semantic_proxy.rs`
- 新增：`crates/studio-pipeline/src/batching.rs`
- 新增：`crates/studio-pipeline/tests/lod.rs`
- 修改：`crates/studio-pipeline/src/partition/build.rs`
- 修改：`crates/studio-pipeline/src/lib.rs`

**Interfaces:**

- Consumes: baseline `PartitionUnit` geometry.
- Produces:
  - `simplify_geometry(positions, indices, options, meshopt::Simplifier) -> LodGeometry`
  - `create_semantic_proxy(samples, mode) -> Option<SemanticProxy>`
  - `create_coarse_chunks(...) -> Vec<CoarseChunk>`
  - `build_static_batches(...) -> Option<RenderBatching>`

- [x] **Step 1: Add `meshopt = "0.7.0"` and failing LOD tests**

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

- [x] **Step 2: Run RED**

```bash
cargo test -p studio-pipeline --test lod
```

Expected: missing LOD APIs.

- [x] **Step 3: Port LOD**

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

使用 `meshopt` 做简化，保留 proportional 与 adaptive allocation。简化失败时保留 source
Geometry 并记录 warning；保留 coarse Geometry 前执行 input/output memory budget。实现
opt-in semantic proxy、fit 和 evaluation；`agent-console-v1` 固定
`semanticProxy=off`。

- [x] **Step 4: Write failing batching tests**

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

- [x] **Step 5: Run all pipeline tests**

```bash
cargo test -p studio-pipeline
cargo test --workspace
```

Expected: all PASS.

- [x] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock crates/studio-pipeline
git commit -m "feat(studio): add native LOD and batching"
```

### 4.5 Task 5: Studio serialization 与 atomic Runtime bundle

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

- [x] **Step 1: 编写失败的 binding 与 publication 测试**

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

- [x] **Step 2: Run RED**

```bash
cargo test -p studio-server --test runtime_tileset
```

Expected: missing exporter APIs.

- [x] **Step 3: Implement binding aggregation and manifest**

把 `export_source_with_scene` 暴露为 `pub(crate)`。按规范从 evaluated Node metadata 构建
binding，排序 tag/node ID，合并 world bounds，使用 source node display name，并依次从
`train`、`stage` 选择 `area`。

- [x] **Step 4: Implement staging and atomic publication**

1. 把 source GLB 写入 project cache。
2. 在 memory 或 cache 中完整构建 partition artifact。
3. 创建 sibling staging directory。
4. 写入 Tileset/files 和 `bindings.json`，计算 digest，再写 `export.json`。
5. 除非 `force`，否则拒绝已有输出。
6. `force` 时先把旧目录 rename 为 backup，再把 staging rename 为 output；失败时恢复。
7. 删除受管理的临时文件。

- [x] **Step 5: Run focused and full tests**

```bash
cargo test -p studio-server --test runtime_tileset
cargo test --workspace
```

Expected: all PASS.

- [x] **Step 6: Commit**

```bash
git add crates/studio-server
git commit -m "feat(studio): publish native runtime tilesets"
```

### 4.6 Task 6: Studio CLI 导出 Tileset

**Files:**

- 新增：`crates/studio-cli/tests/runtime_tileset.rs`
- 修改：`crates/studio-cli/src/main.rs`

**Interfaces:**

- Consumes: `RuntimeTilesetExporter`.
- Produces: `ice3-studio export tileset`.

- [x] **Step 1: 编写失败的 parser 与 integration 测试**

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

Integration test 对 synthetic project 运行 binary，断言五类输出均存在，且不存在
`source.glb`。

- [x] **Step 2: Run RED**

```bash
cargo test -p studio-cli parses_runtime_tileset_export_command
```

Expected: export subcommands missing.

- [x] **Step 3: Implement command**

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

读取 current project revision，创建 `RuntimeTilesetExporter`，执行 export 并打印 result
JSON。

- [x] **Step 4: Run tests**

```bash
cargo test -p studio-cli
cargo test -p studio-server
```

Expected: all PASS.

- [x] **Step 5: Commit**

```bash
git add crates/studio-cli
git commit -m "feat(studio-cli): export runtime tilesets"
```

### 4.7 Task 7: Parity、文档和完整验证

**Files:**

- 修改：`README.md`
- 修改：`docs/README.md`
- 修改：`docs/architecture.md`
- 修改：`docs/reference/api.md`
- 修改：`docs/reference/capabilities.md`
- 修改：`docs/guides/getting-started.md`
- 修改：`docs/specs/2026-11-01-native-asset-pipeline-migration-design.md`
- 修改：`docs/plans/2026-11-01-native-asset-pipeline-phase-1.md`

**Interfaces:**

- 消费：Rust assets/pipeline/exporter/CLI。
- 产出：parity evidence 与当前文档。

- [x] **Step 1: Add parity scripts**

Create `scripts/compare-asset-pipelines.mjs` outside production Rust:

- 对 checked-in synthetic GLB 运行旧 Toolchain partition。
- 运行 `ice3-studio export tileset`。
- 比较 source/prepared stats、selection ID、Tileset bounds、相对路径和 digest。
- 输出机器可读的 `output/asset-pipeline-parity.json`。

- [x] **Step 2: Run parity**

```bash
node scripts/compare-asset-pipelines.mjs
```

Expected:

- `semanticIds`、`selectionIds`、triangle total 和 bounds 一致。
- JSON 顺序导致的 byte difference 必须被报告，不能忽略。
- 存在无法解释的差异时 exit code 非零。

- [x] **Step 3: Add full export verification**

```bash
cargo run -q -p studio-cli -- export tileset \
  --project ./projects/wastewater-plant \
  --output /tmp/ice3-runtime-tileset \
  --profile agent-console-v1
```

Expected:

- 输出只包含 `tileset.json`、`parts/`、`coarse/`、`bindings.json` 和 `export.json`。
- 不包含 `source.glb`。
- `bindings.json` 包含 51 个唯一 equipment selection。
- 第二次输出到不同目录时 digest 保持一致。

- [x] **Step 4: Update docs and Implemented status**

Document:

- `ice3-studio import` 仍属于 Phase 2；Phase 1 只覆盖 Studio-authored GLB。
- `export tileset` 不调用 Toolchain Sidecar。
- 说明 output 与 profile semantics。
- 后续阶段完成前，当前 Toolchain 只保留 parity 和 legacy Source Importer 用途。

把 Phase 1 plan 和 Spec-016 中的 Phase 1 milestone 标记为 `Implemented`。整份 Spec-016
仍保持未完成，因为后续 importer 阶段尚未交付。

- [x] **Step 5: Run complete verification**

```bash
node ../ice3-doc/scripts/check-docs.mjs --root .
PLAYWRIGHT_CHANNEL=chrome npm run verify
node scripts/compare-asset-pipelines.mjs
```

预期：

- Documentation checks 通过。
- 18 道 Studio gate 通过。
- Parity report 通过。

- [x] **Step 6: Commit**

```bash
git add scripts/compare-asset-pipelines.mjs
git add README.md docs/README.md docs/architecture.md
git add docs/reference/api.md docs/reference/capabilities.md
git add docs/guides/getting-started.md
git add docs/specs/2026-11-01-native-asset-pipeline-migration-design.md
git add docs/plans/2026-11-01-native-asset-pipeline-phase-1.md
git commit -m "docs(studio): complete native asset pipeline phase 1"
```
