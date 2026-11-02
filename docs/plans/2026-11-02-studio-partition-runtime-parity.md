# Studio Partition 与 Runtime Tileset 功能等价实施计划

> Document ID: ICE3-STUDIO-PLAN-028
>
> Status: Approved
>
> Owner: ICE3 Studio maintainers

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> `superpowers:subagent-driven-development` (recommended) or
> `superpowers:executing-plans` to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal：** 让 Studio 直接 `partition` 和项目 `export tileset` 共享同一 AssetProfile，
在 fine、LOD、coarse 和 render-batch 产物中生成真实 edges、metrics 与 budget。

**架构：** `studio-pipeline` 接收扩展后的 `AssetProfile`，复用
`studio-assets::prepare` 的 edge generator。`studio-cli` 负责任意 source import
和兼容参数解析；`RuntimeTilesetExporter` 只调用同一 pipeline profile。

**技术栈：** Rust 1.98、Serde、glam、现有 `studio-assets` / `studio-pipeline` /
`studio-server` / `studio-cli`、Cargo tests。

**Spec：** [Studio Asset Prepare 与 Toolchain 功能等价](../specs/2026-11-02-studio-asset-prepare-parity-design.md)

## Global Constraints

- 依赖 Plan-027 的共享 edge generator、metrics、budget 和 PrepareArtifact。
- `partition` 接受任意已注册 Studio source adapter。
- 默认 `maxTriangles=500000`、`maxBytes=33554432`、crease `30°`、edges on。
- default render batch 为 `off`；default LOD 为 off；LOD ratio `0.15`。
- default LOD allocation 为 `proportional`；LOD max triangles `250000`；
  min unit triangles `8`；curve samples `8`；LOD memory `67108864`。
- `semantic-proxy=auto` 只允许 adaptive allocation。
- Fine/LOD/coarse 产物必须包含真实 edge primitive 和 metrics。
- Edge bytes 计入 part/chunk byte budget；line primitive 不计入 triangle count。
- Partition 默认 no-clobber；只有 `--force` 可替换。
- 任何失败不得留下 partial Tileset、GLB、metadata 或临时目录。
- 不依赖 Blender、Toolchain、Node Asset Runtime 或外部 DCC。
- Git author/committer 固定为 `damoqiongqiu <damoqiongqiu@126.com>`。

## Review Focus

- 共享 mesh 的多个 node 必须保持同一 atomic partition unit，edges 不能改变该规则。
- LOD/coarse 简化后 edges 必须基于简化几何生成，不能沿用 source edge indices。
- Edge bytes 必须真实影响 maxBytes、coarse chunk 和 total budget。
- `--force` 替换失败必须恢复旧输出目录，不能留下合并目录。
- Direct `partition` 与 `export tileset` 必须对同一输入、profile 和 edges 配置得到
  相同语义与确定性布局。

---

### Task 1: 扩展 AssetProfile 并接入共享 edge generator

**文件：**
- Create: `crates/studio-pipeline/src/profile.rs`
- Modify: `crates/studio-pipeline/src/lib.rs`
- Modify: `crates/studio-pipeline/src/tileset.rs`
- Modify: `crates/studio-pipeline/Cargo.toml`
- Create: `crates/studio-pipeline/tests/asset_profile.rs`

**接口：**
- 消费：`studio_assets::generate_edge_indices`,
  `studio_assets::edge_color_from_material`,
  `studio_assets::material_suppresses_edges`
- 产出：
  - `AssetProfile` with `crease_angle_degrees` and `generate_edges`
  - `AssetProfile::agent_console_v1()`
  - `AssetProfile::direct_partition(...)`

- [ ] **Step 1: 写 profile 失败测试**

```rust
#[test]
fn agent_console_profile_prepares_edges() {
    let profile = studio_pipeline::AssetProfile::agent_console_v1();
    assert!(profile.generate_edges);
    assert_eq!(profile.crease_angle_degrees, 30.0);
}

#[test]
fn direct_profile_uses_legacy_defaults() {
    let profile = studio_pipeline::AssetProfile::direct_partition(500_000, 33_554_432);
    assert_eq!(profile.max_triangles, 500_000);
    assert_eq!(profile.max_bytes, 33_554_432);
    assert_eq!(profile.render_batch, studio_pipeline::RenderBatchMode::Off);
    assert!(profile.generate_edges);
}

#[test]
fn adaptive_semantic_proxy_is_rejected_for_proportional_allocation() {
    let mut profile = studio_pipeline::AssetProfile::agent_console_v1();
    profile.lod_allocation = studio_pipeline::LodAllocationMode::Proportional;
    profile.semantic_proxy = studio_pipeline::SemanticProxyMode::Auto;
    assert!(profile.validate().is_err());
}
```

- [ ] **Step 2: 运行 RED**

```bash
cargo test -p studio-pipeline --test asset_profile
```

预期：编译失败，因为 profile 模块和字段尚不存在。

- [ ] **Step 3: 移动并扩展 profile**

从 `tileset.rs` 移出 `AssetProfile`、`RenderBatchMode` 到 `profile.rs`，避免继续扩展
大文件。`lib.rs` re-export。

`direct_partition` 接受所有旧参数并执行与 Spec-019 相同的 validation。

- [ ] **Step 4: 在 partition writer 中生成 edges**

新增共享 helper：

```rust
pub(crate) fn append_edges_to_primitive(
    json: &mut serde_json::Value,
    binary: &mut Vec<u8>,
    positions_accessor: usize,
    positions: &[[f32; 3]],
    indices: &[u32],
    material_index: usize,
    edge_material_index: usize,
    crease_angle_degrees: f32,
) -> Result<u32, PipelineError>;
```

该 helper：

- 调用共享 edge generator；
- 追加 edge index bufferView/accessor；
- 追加 line primitive；
- 返回 edge segment count。

- [ ] **Step 5: 加 profile validation 和测试**

校验：

- `max-triangles` / `max-bytes` 必须大于 0；
- crease 范围必须为 `0..=180`；
- LOD ratio 必须在 `(0,1)` 内；
- min 不得大于 max；
- curve samples 必须在 `4..=32`；
- semantic-proxy 需要 LOD + adaptive。

- [ ] **Step 6: 运行 GREEN**

```bash
cargo test -p studio-pipeline --test asset_profile
cargo test -p studio-pipeline
cargo clippy -p studio-pipeline --all-targets -- -D warnings
```

预期：PASS。

- [ ] **Step 7: 提交**

```bash
git add crates/studio-pipeline
git commit -m "feat(studio-pipeline): unify asset profiles and edge config"
```

---

### Task 2: 在 fine、LOD、coarse part GLB 中生成真实 edges

**文件：**
- Modify: `crates/studio-pipeline/src/partition/build.rs`
- Modify: `crates/studio-pipeline/src/tileset.rs`
- Create: `crates/studio-pipeline/tests/partition_edges.rs`

**接口：**
- 消费：Task 1 `AssetProfile` validation and edge writer
- 产出：PartitionArtifact part files with line primitives and accurate profile stats

- [ ] **Step 1: 写 fine partition edge 测试**

```rust
#[test]
fn fine_partition_glb_contains_edge_line_primitives() {
    let source = read_glb(fixture("gltf/prepare-cube.gltf")).unwrap();
    let profile = AssetProfile::direct_partition(500_000, 33_554_432);
    let artifact = build_with_profile(&source, "partition-cube", &profile).unwrap();
    let part = artifact.files.iter()
        .find(|file| file.relative_path.starts_with("parts/"))
        .unwrap();
    let parsed = read_glb_bytes(&part.bytes, std::env::temp_dir()).unwrap();
    let primitives = parsed.json["meshes"][0]["primitives"].as_array().unwrap();
    assert!(primitives.iter().any(|primitive| {
        primitive["mode"] == 1
            && primitive["extras"]["ice3"]["role"] == "edges"
    }));
    assert!(parsed.json["asset"]["extras"]["ice3"]["stats"]["edgeSegmentCount"]
        .as_u64().unwrap() > 0);
}
```

- [ ] **Step 2: 写 LOD/coarse edge 测试**

```rust
#[test]
fn lod_and_coarse_outputs_generate_edges_from_their_own_geometry() {
    let artifact = partition_with_lod_and_edges();
    for file in artifact.files.iter().filter(|file| {
        file.relative_path.starts_with("parts/")
            || file.relative_path.starts_with("coarse/")
    }) {
        let parsed = read_glb_bytes(&file.bytes, std::env::temp_dir()).unwrap();
        let stats = &parsed.json["asset"]["extras"]["ice3"]["stats"];
        assert!(stats["edgeSegmentCount"].as_u64().unwrap() > 0);
        assert_eq!(
            parsed.json["asset"]["extras"]["ice3"]["preparation"]["edgesPrepared"],
            true
        );
    }
}
```

- [ ] **Step 3: 写 byte budget 测试**

创建 maxBytes 刚好不含 edges 的 profile：

- edges off 成功；
- edges on 因 part/chunk 超限失败；
- failure 不返回可发布 artifact。

- [ ] **Step 4: 运行 RED**

```bash
cargo test -p studio-pipeline --test partition_edges
```

预期：line primitive 不存在，edge stats 为 0。

- [ ] **Step 5: 更新 all part writers**

必须修改：

- `build_leaf_with_overrides`
- `build_batched_leaf`
- LOD `simplify_geometry` 输出 writer
- coarse chunk writer
- semantic proxy writer

每个 writer 在最终几何确定后生成 edges；禁止复用输入 source 的 edge accessor。

- [ ] **Step 6: 更新 PartitionStats**

增加：

```rust
pub edge_segment_count: u64,
pub edge_bytes: u64,
```

并从实际 part GLB 累加。Tileset extras、CLI report 和 export manifest 同步。

- [ ] **Step 7: 运行 GREEN**

```bash
cargo test -p studio-pipeline --test partition_edges
cargo test -p studio-pipeline
cargo clippy -p studio-pipeline --all-targets -- -D warnings
```

预期：PASS。

- [ ] **Step 8: 提交**

```bash
git add crates/studio-pipeline
git commit -m "feat(studio-pipeline): emit render edges in partition outputs"
```

---

### Task 3: 实现任意 Source 的直接 partition orchestration

**文件：**
- Create: `crates/studio-cli/src/partition.rs`
- Modify: `crates/studio-cli/src/lib.rs`
- Modify: `crates/studio-cli/src/main.rs`
- Create: `crates/studio-cli/tests/partition_command.rs`

**接口：**
- 消费：Studio import registry, shared Prepare, `AssetProfile`,
  `studio_pipeline::build_with_profile`
- 产出：
  - `PartitionOptions`
  - `partition_source(PartitionOptions) -> Result<PartitionOutput, PartitionError>`
  - `ice3-studio partition ...`

- [ ] **Step 1: 写 glTF partition 测试**

```rust
#[test]
fn partition_writes_tileset_and_part_glbs() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("dist");
    let result = run_cli([
        "partition",
        fixture("prepare-cube.gltf").to_str().unwrap(),
        "--output",
        output.to_str().unwrap(),
        "--asset-id",
        "partition-cube",
    ]);
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    assert!(output.join("tileset.json").is_file());
    assert!(std::fs::read_dir(output.join("parts")).unwrap().next().is_some());
}
```

- [ ] **Step 2: 写 adapter/auto 测试**

对 OBJ/FBX/USD/STEP/IFC/JT 各用一个小 fixture：

- `partition SOURCE` 自动选择 adapter；
- 显式 `--adapter` 正确；
- adapter 与扩展不匹配失败；
- non-source/无 geometry 不创建输出目录。

- [ ] **Step 3: 写完整 options/validation 测试**

测试默认值和非法组合：

```text
--semantic-proxy auto without --lod -> invalid
--semantic-proxy auto + proportional -> invalid
--lod-ratio 0/1 -> invalid
--lod-curve-samples 3/33 -> invalid
--lod-min-unit-triangles > --lod-max-triangles -> invalid
```

每个失败断言 `ICE3_STUDIO_ASSET_INVALID_OPTIONS`，output 不存在。

- [ ] **Step 4: 运行 RED**

```bash
cargo test -p studio-cli --test partition_command
```

预期：`partition` subcommand 不存在。

- [ ] **Step 5: 实现 orchestration**

流程：

```text
resolve adapter
  -> isolated import when required
  -> PrepareArtifact (edges per options)
  -> read prepared GLB
  -> build_with_profile
  -> validate all GLB
  -> publish staging directory atomically
```

临时 source Prepared GLB 位于同 parent staging 外的 tempdir；失败删除。

- [ ] **Step 6: 实现 no-clobber/force**

- 默认 output 已存在时返回 `ICE3_STUDIO_ASSET_OUTPUT_EXISTS`；
- force 模式在 output parent 创建 staging 和 backup；
- 发布成功时删除 backup；
- 发布失败时恢复 backup；
- 不得留下部分 output directory。

- [ ] **Step 7: 运行 GREEN**

```bash
cargo test -p studio-cli --test partition_command
cargo test -p studio-cli
```

预期：PASS。

- [ ] **Step 8: 提交**

```bash
git add crates/studio-cli crates/studio-pipeline
git commit -m "feat(studio-cli): partition arbitrary Studio sources"
```

---

### Task 4: 让 export tileset 使用统一 Profile

**文件：**
- Modify: `crates/studio-server/src/runtime_tileset.rs`
- Modify: `crates/studio-server/tests/runtime_tileset.rs`
- Modify: `crates/studio-cli/src/main.rs`
- Modify: `crates/studio-server/Cargo.toml`

**接口：**
- 消费：`AssetProfile::agent_console_v1()` and shared pipeline edges
- 产出：Runtime Tileset parts/coarse with edges, accurate stats and digest

- [ ] **Step 1: 写 runtime edge 测试**

```rust
#[test]
fn runtime_tileset_parts_include_render_edges() {
    let export = export_fixture_tileset();
    let part = export.files.iter()
        .find(|file| file.path.starts_with("parts/"))
        .unwrap();
    let bytes = std::fs::read(export.output_dir.join(&part.path)).unwrap();
    let parsed = read_glb_bytes(&bytes, std::env::temp_dir()).unwrap();
    assert!(parsed.json["meshes"].as_array().unwrap().iter().any(|mesh| {
        mesh["primitives"].as_array().unwrap().iter().any(|primitive| {
            primitive["mode"] == 1
        })
    }));
}
```

- [ ] **Step 2: 写 diagnostics stats 测试**

`DiagnosticCode`/capability reference 使用 `ICE3_STUDIO_ASSET_TILESET_INVALID`，不再
使用 Toolchain code。`ExportedStats` 增加 `edgeSegmentCount`。

- [ ] **Step 3: 运行 RED**

```bash
cargo test -p studio-server --test runtime_tileset
```

预期：part GLB 没有 line primitives。

- [ ] **Step 4: 更新 RuntimeTilesetExporter**

不再构造独立 profile：

```rust
self.profile = AssetProfile::agent_console_v1();
```

`agent_console_v1` 的 edges/LOD/batching/budget 与 direct partition 共用同一 profile
实现。

- [ ] **Step 5: 运行 GREEN**

```bash
cargo test -p studio-server --test runtime_tileset
cargo test -p studio-server
```

预期：PASS；bundle digest 稳定；binding manifest 不变。

- [ ] **Step 6: 提交**

```bash
git add crates/studio-server crates/studio-cli
git commit -m "feat(studio-server): include edges in runtime tilesets"
```

---

### Task 5: CLI/partition parity 与 Phase 6B 文档

**文件：**
- Create: `scripts/compare-partition-imports.mjs`
- Create: `scripts/tests/partition-contract.test.mjs`
- Modify: `scripts/verify.mjs`
- Modify: `README.md`
- Modify: `docs/architecture.md`
- Modify: `docs/reference/capabilities.md`
- Modify: `docs/reference/diagnostics.md`
- Modify: `docs/guides/getting-started.md`
- Modify: `docs/specs/2026-11-02-studio-asset-prepare-parity-design.md`
- Modify: `docs/plans/2026-11-02-studio-partition-runtime-parity.md`

**接口：**
- 消费：passing direct partition and Runtime Tileset edge tests
- 产出：deterministic partition parity report and Phase 6B status

- [ ] **Step 1: 写 parity contract**

`compare-partition-imports.mjs` 至少运行：

- 直接 GLB，无 LOD；
- proportional LOD；
- adaptive LOD；
- semantic proxy auto；
- static render batch；
- chunked coarse；
- edges 开启/关闭。

比较：

- part/coarse 层级；
- URI；
- bounds 边界；
- Selection；
- triangle/edge 数量；
- edge role/width/color；
- byte budget 限制；
- 确定性文件 digest。

- [ ] **Step 2: 写 contract test**

```javascript
test('partition parity proves edge and LOD semantics', async () => {
  const report = JSON.parse(await readFile(
    new URL('../../output/partition-parity.json', import.meta.url),
    'utf8',
  ));
  assert.equal(report.status, 'pass');
  assert.deepEqual(report.failures, []);
  assert.ok(report.cases.every((entry) => entry.deterministic));
  assert.ok(report.cases.some((entry) => entry.edgeSegmentCount > 0));
});
```

- [ ] **Step 3: 加入 verify**

`scripts/verify.mjs` 增加 partition contract 和 native parity；保持 Chrome E2E。

- [ ] **Step 4: 更新文档**

说明：

- `partition` 全部参数、默认值和非法组合；
- direct 与 export 共用 profile；
- fine/LOD/coarse 的 edges 和 budget；
- `--force` 恢复语义；
- Phase 6B Implemented，Phase6C pending；
- 不依赖 Blender/Toolchain。

- [ ] **Step 5: 全量验证**

```bash
cargo test -p studio-assets
cargo test -p studio-pipeline
cargo test -p studio-cli
cargo test -p studio-server
node scripts/compare-partition-imports.mjs
node --test scripts/tests/partition-contract.test.mjs
PLAYWRIGHT_CHANNEL=chrome npm run verify
```

预期：全部 PASS。

- [ ] **Step 6: 提交**

```bash
git add README.md docs scripts crates
git commit -m "test(studio): complete partition runtime parity"
```

## Plan Self-Review

- Spec coverage：AssetProfile、所有 partition 参数、fine/LOD/coarse edges、budget、
  direct partition、export tileset、atomic force 和文档均有 task。
- Placeholder scan：没有 TODO/TBD 或未定义后续项。
- Type consistency：`AssetProfile`、`PartitionStats`、`PartitionOutput`、
  `partition_source` 和 CLI flags 跨 task 一致。
- Review Focus：shared meshes、simplified geometry edges、byte budgets、force rollback
  和 direct/export consistency 都有测试。
