# Studio Asset Prepare Core 实施计划

> Document ID: ICE3-STUDIO-PLAN-027
>
> Status: Draft
>
> Owner: ICE3 Studio maintainers

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> `superpowers:subagent-driven-development` (recommended) or
> `superpowers:executing-plans` to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal：** 建立 Studio 唯一的 Asset Prepare 实现，补齐真实 edges、metrics、
built-in/custom budget、版本化 report，以及 `build`、`inspect` CLI。

**架构：** `studio-assets` 负责 SourceScene 到 PreparedArtifact 的全部准备语义；
`studio-cli` 只负责参数解析、source import 和原子文件发布。现有 `import` 改走同一
Prepare 实现，但默认关闭 edges；新增 `build` 默认开启 edges。

**技术栈：** Rust 1.98、Serde、glam、sha2、clap、现有 `studio-assets` /
`studio-cli` / `studio-import` crate、Node.js 24.22.1（仅一次性 legacy baseline）。

**Spec：** [Studio Asset Prepare 与 Toolchain 功能等价](../specs/2026-11-02-studio-asset-prepare-parity-design.md)

## Global Constraints

- 不恢复 Toolchain、Node Asset Runtime、外部 DCC、Blender、`bpy` 或 shell bridge。
- Prepared GLB 只能由 `studio-assets::prepare` 语义路径生成。
- `import` 默认 `generate_edges = false`；`build` 默认 `generate_edges = true`。
- 默认 crease angle 为 `30°`，合法值 `0..=180`。
- Edge generation 使用 `round(position * 100000)` welding，boundary + crease。
- Edge primitive 固定 `mode = 1`、`role = "edges"`、`widthPx = 3`。
- 默认 edge color 为 `#e6f2ff`；material 的 `edgeColor` 或 `edges=false` 优先。
- `interactive-desktop-v1`、`unrestricted` 和 custom budget 必须可验证。
- hard budget、source、edge、profile 或 report 失败不得发布任何新文件。
- 所有输出必须 byte-deterministic；Diagnostic 不得包含绝对路径或 converter stderr。
- Legacy Toolchain 只允许在一次性 baseline capture 中读取，普通测试不得依赖它。
- Git author/committer 固定为 `damoqiongqiu <damoqiongqiu@126.com>`。

## Review Focus

- 共面三角形不能被错误生成 face diagonal；开放边界、硬边和共享顶点必须稳定。
- Edge material、line primitive 和 index accessor 必须能被 Engine 实际读取，而不只是
  通过 Studio 自己生成的浅层统计。
- Soft budget 只 warning，hard budget 必须阻止 GLB/report 发布；
  `--warnings-as-errors` 必须把 soft warning 转成失败。
- GLB/report 必须成对原子发布，任一 rename 失败不能留下新 GLB 或半份 report。
- Source Import 与 build 必须共享同一 PrepareArtifact，不能重新出现两套 metrics、
  budget 或 Preparation metadata。

---

### Task 1: 固化 legacy Prepare baseline

**文件：**
- Create: `fixtures/source-import/gltf/prepare-cube.gltf`
- Create: `fixtures/parity/prepare-baseline-v1.json`
- Create: `scripts/compare-prepare-imports.mjs`
- Create: `scripts/tests/prepare-baseline.test.mjs`

**接口：**
- 消费：archived Toolchain source at `ICE3_TOOLCHAIN_ARCHIVE` only during explicit
  `--capture`.
- 产出：immutable Studio baseline for default edges, skip edges, crease variants,
  metrics, budget and report.

- [ ] **Step 1: 写 baseline 失败测试**

`scripts/tests/prepare-baseline.test.mjs`：

```javascript
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';

test('prepare baseline pins edges, metrics and budget', async () => {
  const baseline = JSON.parse(await readFile(
    new URL('../../fixtures/parity/prepare-baseline-v1.json', import.meta.url),
    'utf8',
  ));
  assert.equal(baseline.kind, 'ice3-prepare-baseline');
  assert.equal(baseline.default.edgesPrepared, true);
  assert.equal(baseline.default.stats.triangleCount, 12);
  assert.equal(baseline.default.stats.edgeSegmentCount, 12);
  assert.equal(baseline.default.stats.materialCount, 5);
  assert.equal(baseline.default.edgeRole, 'edges');
  assert.equal(baseline.default.edgeWidthPx, 3);
  assert.equal(baseline.skipEdges.stats.edgeSegmentCount, 0);
  assert.equal(baseline.crease90.stats.edgeSegmentCount, 12);
  assert.equal(baseline.budget.profile, 'interactive-desktop-v1');
  assert.equal(baseline.budget.status, 'pass');
});
```

Run:

```bash
node --test scripts/tests/prepare-baseline.test.mjs
```

预期：FAIL because baseline does not exist.

- [ ] **Step 2: 准备 cube source**

从合成 cube fixture 生成 `fixtures/source-import/gltf/prepare-cube.gltf`。只在实现
阶段读取归档 Toolchain fixture；生成后 Studio baseline 测试只读取本地文件。

验证：

```bash
node -e "const fs=require('fs');const j=JSON.parse(fs.readFileSync('fixtures/source-import/gltf/prepare-cube.gltf'));if(j.meshes.length!==1)process.exit(1)"
sha256sum fixtures/source-import/gltf/prepare-cube.gltf
```

预期：valid glTF 2.0，包含一个 12-triangle cube。

- [ ] **Step 3: 实现一次性 capture**

`scripts/compare-prepare-imports.mjs --capture`：

- 要求 `ICE3_TOOLCHAIN_ARCHIVE` 指向归档仓库；
- 在该仓库 build 后运行 `ice3-tool build`；
- 分别 capture 默认、`--skip-edges`、`--crease-angle 90`；
- 读取输出 GLB、profile 和 report；
- 记录 sourceDigest、stats、edge role/width/color、budget 和 output digest；
- 写稳定排序 JSON。

捕获前验证归档是 synthetic corpus，不包含客户路径。

- [ ] **Step 4: capture 并重复校验**

```bash
ICE3_TOOLCHAIN_ARCHIVE=/Users/felix/.Trash/ice3-toolchain-retired-20261102 \
  node scripts/compare-prepare-imports.mjs --capture
node scripts/compare-prepare-imports.mjs --verify-baseline
node --test scripts/tests/prepare-baseline.test.mjs
```

预期：baseline 写出；连续 capture 的 normalized digest 相同；测试 PASS。

- [ ] **Step 5: 禁止普通测试访问旧仓库**

```bash
rg -n "ice3-toolchain|ice3-tool build" scripts/tests fixtures/parity \
  --glob '!prepare-baseline-v1.json'
```

预期：无输出。只有 capture 脚本可要求外部 archive env。

- [ ] **Step 6: 提交**

```bash
git add scripts/compare-prepare-imports.mjs scripts/tests/prepare-baseline.test.mjs \
  fixtures/source-import/gltf/prepare-cube.gltf fixtures/parity/prepare-baseline-v1.json
git commit -m "test(studio): capture legacy asset prepare baseline"
```

---

### Task 2: 建立 AssetMetrics、Budget 与 Report 类型

**文件：**
- Create: `crates/studio-assets/src/metrics.rs`
- Create: `crates/studio-assets/src/budget.rs`
- Create: `crates/studio-assets/src/report.rs`
- Modify: `crates/studio-assets/src/lib.rs`
- Create: `crates/studio-assets/tests/asset_budget.rs`

**接口：**
- 产出：
  - `AssetMetrics`
  - `BudgetProfile`, `BudgetLimit`, `BudgetResult`, `BudgetStatus`
  - `PreparationMetadata`
  - `AssetBuildReport`
  - `BudgetProfile::interactive_desktop_v1()`
  - `BudgetProfile::unrestricted()`
  - `BudgetProfile::from_json(&str) -> Result<Self, AssetError>`
  - `evaluate_budget(AssetMetrics, &BudgetProfile) -> BudgetResult`
  - `PreparationMetadata::edges(crease_angle_degrees: f32) -> Self`
  - `AssetBuildReport::new(command, source_digest, content_digest, asset_id, metrics, preparation, budget, warnings) -> Self`

- [ ] **Step 1: 写 budget 失败测试**

`crates/studio-assets/tests/asset_budget.rs`：

```rust
use studio_assets::{AssetMetrics, BudgetProfile, BudgetStatus, evaluate_budget};

fn metrics(triangles: u64) -> AssetMetrics {
    AssetMetrics {
        encoded_bytes: 0,
        json_chunk_bytes: 0,
        bin_chunk_bytes: 0,
        node_count: 1,
        max_node_depth: 0,
        mesh_count: 1,
        primitive_count: 2,
        triangle_count: triangles,
        edge_segment_count: 12,
        material_count: 3,
        selection_group_count: 1,
    }
}

#[test]
fn classifies_soft_and_hard_limits_inclusively() {
    let profile = BudgetProfile::interactive_desktop_v1();
    assert_eq!(evaluate_budget(metrics(5_000_000), &profile).status, BudgetStatus::Pass);
    assert_eq!(evaluate_budget(metrics(5_000_001), &profile).status, BudgetStatus::Warning);
    assert_eq!(evaluate_budget(metrics(20_000_001), &profile).status, BudgetStatus::Error);
}

#[test]
fn parses_a_custom_budget_and_rejects_invalid_limits() {
    let valid = BudgetProfile::from_json(r#"{
      "id":"custom",
      "limits":[{
        "id":"triangleCount","label":"Triangles",
        "softLimit":1000,"hardLimit":2000
      }]
    }"#).unwrap();
    assert_eq!(valid.id(), "custom");

    for invalid in [
        r#"{"id":"x","limits":[{"id":"triangleCount","softLimit":2,"hardLimit":1}]}"#,
        r#"{"id":"x","limits":[{"id":"bogus","softLimit":1,"hardLimit":2}]}"#,
        r#"{"id":"x","limits":[]}"#,
    ] {
        assert_eq!(
            BudgetProfile::from_json(invalid).unwrap_err().code(),
            "ICE3_STUDIO_ASSET_INVALID_BUDGET"
        );
    }
}
```

- [ ] **Step 2: 写 report determinism 测试**

```rust
#[test]
fn report_is_versioned_and_deterministic() {
    let report = AssetBuildReport::new(
        "build",
        "sha256:source",
        "sha256:content",
        "asset",
        metrics(12),
        PreparationMetadata::edges(30.0),
        evaluate_budget(metrics(12), &BudgetProfile::interactive_desktop_v1()),
        vec![warning("B", "second"), warning("A", "first")],
    );
    let first = serde_json::to_vec(&report).unwrap();
    let second = serde_json::to_vec(&report).unwrap();
    assert_eq!(first, second);
    assert_eq!(report.kind, "ice3-asset-build-report");
    assert_eq!(report.schema_version, "1.0");
    assert_eq!(report.warnings[0].code, "A");
}
```

- [ ] **Step 3: 运行 RED**

```bash
cargo test -p studio-assets --test asset_budget
```

预期：编译失败，因为类型尚不存在。

- [ ] **Step 4: 实现类型**

`AssetError` 增加：

```rust
#[error("invalid asset budget: {0}")]
InvalidBudget(String),
#[error("asset budget exceeded")]
BudgetExceeded,
#[error("asset report serialization failed: {0}")]
Report(serde_json::Error),
```

Budget status 使用：

```rust
pub enum BudgetStatus { Pass, Warning, Error }
```

Custom budget 必须有至少一个 limit；unknown fields 使用
`#[serde(deny_unknown_fields)]` 拒绝。

Report warnings 在构造时按 `(code, message, path)` 排序并去重。

`PreparationMetadata` 固定字段：

```rust
pub struct PreparationMetadata {
    pub runtime_conversion_required: bool,
    pub units_normalized: bool,
    pub matrix_transforms_baked: bool,
    pub required_compression_decoded: bool,
    pub edges_prepared: bool,
    pub selection_groups_prepared: bool,
    pub crease_angle_degrees: f32,
}
```

- [ ] **Step 5: 运行 GREEN**

```bash
cargo test -p studio-assets --test asset_budget
cargo clippy -p studio-assets --all-targets -- -D warnings
```

预期：PASS。

- [ ] **Step 6: 提交**

```bash
git add crates/studio-assets
git commit -m "feat(studio-assets): define metrics budget and reports"
```

---

### Task 3: 实现确定性 render edges

**文件：**
- Create: `crates/studio-assets/src/edges.rs`
- Modify: `crates/studio-assets/src/lib.rs`
- Create: `crates/studio-assets/tests/render_edges.rs`

**接口：**
- 产出：
  - `generate_edge_indices(positions: &[[f32; 3]], indices: &[u32], crease_angle_degrees: f32) -> Result<Vec<u32>, AssetError>`
  - `edge_color_from_material(extras: &serde_json::Value) -> Result<[f32; 4], AssetError>`
  - `material_suppresses_edges(extras: &serde_json::Value) -> bool`

- [ ] **Step 1: 写边界、硬边和 determinism 测试**

`crates/studio-assets/tests/render_edges.rs`：

```rust
use studio_assets::generate_edge_indices;

#[test]
fn emits_boundary_edges_for_one_triangle() {
    let positions = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    let edges = generate_edge_indices(&positions, &[0, 1, 2], 30.0).unwrap();
    assert_eq!(edges.len(), 6);
}

#[test]
fn drops_coplanar_shared_edges_and_keeps_cube_boundaries() {
    let (positions, indices) = unit_quad();
    let edges = generate_edge_indices(&positions, &indices, 30.0).unwrap();
    assert_eq!(edges, vec![0, 1, 1, 3, 3, 2, 2, 0]);
}

#[test]
fn repeated_generation_is_byte_stable() {
    let (positions, indices) = unit_cube();
    let first = generate_edge_indices(&positions, &indices, 30.0).unwrap();
    let second = generate_edge_indices(&positions, &indices, 30.0).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.len(), 24);
}

#[test]
fn rejects_out_of_range_and_non_finite_inputs() {
    for error in [
        generate_edge_indices(&[[0.0; 3]], &[0, 1, 2], 30.0).unwrap_err(),
        generate_edge_indices(
            &[[0.0, f32::NAN, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            &[0, 1, 2],
            30.0,
        ).unwrap_err(),
    ] {
        assert_eq!(error.code(), "ICE3_STUDIO_ASSET_INVALID_GEOMETRY");
    }
}

fn unit_cube() -> (Vec<[f32; 3]>, Vec<u32>) { /* 8 vertices, 12 triangles */ }
fn unit_quad() -> (Vec<[f32; 3]>, Vec<u32>) { /* two coplanar triangles */ }
```

- [ ] **Step 2: 写 material color/suppression 测试**

```rust
#[test]
fn resolves_edge_color_and_material_suppression() {
    let custom = serde_json::json!({"ice3":{"edgeColor":"#10203080"}});
    assert_eq!(
        studio_assets::edge_color_from_material(&custom).unwrap(),
        [16.0 / 255.0, 32.0 / 255.0, 48.0 / 255.0, 128.0 / 255.0]
    );
    let default = serde_json::json!({});
    assert_eq!(
        studio_assets::edge_color_from_material(&default).unwrap(),
        [0.9019608, 0.9490196, 1.0, 1.0]
    );
    let suppressed = serde_json::json!({"ice3":{"edges":false}});
    assert!(studio_assets::material_suppresses_edges(&suppressed));
}
```

- [ ] **Step 3: 运行 RED**

```bash
cargo test -p studio-assets --test render_edges
```

预期：编译失败，因为 edge API 尚不存在。

- [ ] **Step 4: 实现算法**

使用：

```rust
type WeldedVertex = (i64, i64, i64);
type EdgeKey = (u32, u32);

let welded = BTreeMap<WeldedVertex, u32>::new();
let edges = BTreeMap<EdgeKey, usize>::new();
let records = Vec<EdgeRecord>::new();
```

`records` 保留首次出现顺序；最终按 records 顺序复制 edge indices。

规则：

- quantized coordinate：`(value * 100_000.0).round() as i64`；
- 非有限值、非法 index、退化三角形返回 invalid geometry；
- face normal 长度为 0 时不参与 crease 判断；
- boundary edge `count == 1` 或任意相邻 normal dot <= cosine threshold 时输出；
- crease angle 被 clamp 到 `0..=180`；
- 无 edge 时返回空 Vec，不报 empty geometry。

- [ ] **Step 5: 运行 GREEN**

```bash
cargo test -p studio-assets --test render_edges
cargo clippy -p studio-assets --all-targets -- -D warnings
```

预期：PASS；cube 输出 12 edge segments / 24 indices。

- [ ] **Step 6: 提交**

```bash
git add crates/studio-assets
git commit -m "feat(studio-assets): generate deterministic render edges"
```

---

### Task 4: 引入 PrepareArtifact 并让 canonical writer 输出真实 edges

**文件：**
- Create: `crates/studio-assets/src/prepare.rs`
- Create: `crates/studio-assets/tests/prepare_artifact.rs`
- Modify: `crates/studio-assets/src/canonical.rs`
- Modify: `crates/studio-assets/src/lib.rs`
- Modify: `crates/studio-assets/src/metrics.rs`
- Modify: `crates/studio-assets/src/prepared.rs`

**接口：**
- 消费：Task 2 `AssetMetrics/BudgetProfile/BudgetResult/AssetBuildReport`
- 消费：Task 3 edge generator/color/suppression
- 产出：
  - `PrepareOptions`
  - `PrepareArtifact`
  - `prepare_canonical(scene, options) -> Result<PrepareArtifact, AssetError>`
  - unchanged public `write_prepared_glb(scene, PreparedGlbOptions) -> Result<Vec<u8>, AssetError>`

- [ ] **Step 1: 写 PrepareArtifact 测试**

```rust
#[test]
fn default_prepare_emits_edges_and_real_metrics() {
    let scene = triangle_scene("asset");
    let artifact = prepare_canonical(&scene, PrepareOptions {
        asset_id: "asset".into(),
        source_format: "gltf".into(),
        source_digest: "abc".into(),
        crease_angle_degrees: 30.0,
        generate_edges: true,
        budget: BudgetProfile::interactive_desktop_v1(),
    }).unwrap();

    assert_eq!(artifact.metrics.triangle_count, 1);
    assert_eq!(artifact.metrics.edge_segment_count, 3);
    assert_eq!(artifact.metrics.primitive_count, 2);
    assert!(artifact.preparation.edges_prepared);
    assert_eq!(artifact.budget.status, BudgetStatus::Pass);

    let parsed = read_glb_bytes(&artifact.bytes, std::env::temp_dir()).unwrap();
    let primitives = parsed.json["meshes"][0]["primitives"].as_array().unwrap();
    assert_eq!(primitives[1]["mode"], 1);
    assert_eq!(primitives[1]["extras"]["ice3"]["role"], "edges");
    assert_eq!(primitives[1]["extras"]["ice3"]["widthPx"], 3);
}

#[test]
fn skip_edges_removes_edge_material_and_reports_zero() {
    let artifact = prepare_with_edges(false);
    assert_eq!(artifact.metrics.edge_segment_count, 0);
    assert!(!artifact.preparation.edges_prepared);
    assert!(artifact.metrics.material_count < prepare_with_edges(true).metrics.material_count);
}
```

- [ ] **Step 2: 写 budget/atomic validation 测试**

```rust
#[test]
fn hard_budget_returns_no_publishable_artifact() {
    let error = prepare_canonical(&triangle_scene("asset"), prepare_options_with_budget(
        BudgetProfile::from_json(r#"{
          "id":"tiny",
          "limits":[{
            "id":"triangleCount","label":"Triangles",
            "softLimit":0,"hardLimit":0
          }]
        }"#).unwrap(),
    )).unwrap_err();
    assert_eq!(error.code(), "ICE3_STUDIO_ASSET_BUDGET_EXCEEDED");
}
```

- [ ] **Step 3: 运行 RED**

```bash
cargo test -p studio-assets --test prepare_artifact
```

预期：编译失败，因为 PrepareArtifact 尚不存在。

- [ ] **Step 4: 重构 canonical writer**

拆分而不复制逻辑：

- `build_canonical_parts(scene, options) -> (json, binary, metrics, world_bounds)`
- `prepare_canonical` 调用并完成 budget/report/digest。
- `write_prepared_glb` 调用 `prepare_canonical`，使用 `generate_edges=false` 和
  `interactive_desktop_v1`。

Edge output：

- 每个 triangle primitive 之后追加对应 line primitive；
- line primitive 使用同一 POSITION accessor 和新的 edge index accessor；
- 每个实际使用 material 有且只有一个 edge material；
- material 无 triangle 或 suppression 时不创建 edge material；
- 在 source material/edge material 后固定追加 `default`、`default edges` 和
  `vertex colors` 三个 reserved material，以保持 legacy material index parity；
- material count、primitive count、edge count 来自最终 JSON。

- [ ] **Step 5: 更新 Prepared validation**

`validate_prepared_profile` 增加：

- `edgesPrepared` 必须由 preparation options 和 stats 一致；
- edge line primitive 的 role/width/accessor/material 合法；
- triangle count 只统计 mode 4，edge count 只统计 mode 1；
- stats 与实际 accessor 数一致。

- [ ] **Step 6: 运行 GREEN**

```bash
cargo test -p studio-assets
cargo clippy -p studio-assets --all-targets -- -D warnings
```

预期：全 PASS；现有 writer determinism 测试保持通过。

- [ ] **Step 7: 提交**

```bash
git add crates/studio-assets
git commit -m "feat(studio-assets): emit prepared artifacts with render edges"
```

---

### Task 5: 让 import 使用共享 Prepare 并增加 build/inspect CLI

**文件：**
- Modify: `crates/studio-cli/src/import.rs`
- Create: `crates/studio-cli/src/prepare_command.rs`
- Modify: `crates/studio-cli/src/lib.rs`
- Modify: `crates/studio-cli/src/main.rs`
- Modify: `crates/studio-cli/Cargo.toml`
- Create: `crates/studio-cli/tests/prepare_command.rs`

**接口：**
- 消费：`prepare_canonical`, `PrepareOptions`, `AssetBuildReport`
- 产出：
  - `run_build(PrepareCommandOptions) -> Result<PrepareCommandOutput, PrepareCommandError>`
  - `run_inspect(InspectOptions) -> Result<AssetBuildReport, PrepareCommandError>`
  - CLI commands `build`, `inspect`

- [ ] **Step 1: 写 build CLI 测试**

```rust
#[test]
fn build_emits_edges_report_and_budget() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("cube.glb");
    let report = root.path().join("cube.report.json");
    let result = Command::new(env!("CARGO_BIN_EXE_studio-cli"))
        .args([
            "build", fixture("prepare-cube.gltf").to_str().unwrap(),
            "--output", output.to_str().unwrap(),
            "--asset-id", "cube",
            "--report", report.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));

    let parsed = studio_assets::read_glb(&output).unwrap();
    assert_eq!(parsed.json["asset"]["extras"]["ice3"]["stats"]["edgeSegmentCount"], 12);
    let report: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&report).unwrap()
    ).unwrap();
    assert_eq!(report["kind"], "ice3-asset-build-report");
    assert_eq!(report["budget"]["status"], "pass");
}
```

- [ ] **Step 2: 写 import/build/inspect 差异测试**

```rust
#[test]
fn import_has_no_edges_while_build_does() {
    let imported = run_json("import", fixture("prepare-cube.gltf"), None);
    let built = run_json("build", fixture("prepare-cube.gltf"), None);
    assert_eq!(imported["metrics"]["edgeSegmentCount"], 0);
    assert_eq!(built["metrics"]["edgeSegmentCount"], 12);
}

#[test]
fn inspect_source_and_prepared_use_the_same_fields() {
    let output = build_fixture();
    let source = inspect(fixture("prepare-cube.gltf"));
    let prepared = inspect(&output);
    assert_eq!(source["metrics"]["triangleCount"], prepared["metrics"]["triangleCount"]);
    assert_eq!(source["metrics"]["edgeSegmentCount"], prepared["metrics"]["edgeSegmentCount"]);
}
```

- [ ] **Step 3: 写失败/原子发布测试**

- hard budget -> `ICE3_STUDIO_ASSET_BUDGET_EXCEEDED`，output/report 均不存在；
- soft budget + `--warnings-as-errors` -> `ICE3_STUDIO_ASSET_BUDGET_WARNING`；
- report 父目录为 file -> `ICE3_STUDIO_ASSET_REPORT_WRITE_FAILED`，output 不存在；
- existing output 无 `--force` -> `ICE3_STUDIO_ASSET_OUTPUT_EXISTS`；
- `--force` 成功替换 output/report；
- invalid crease、negative/double budget limit -> stable invalid diagnostic。

- [ ] **Step 4: 运行 RED**

```bash
cargo test -p studio-cli --test prepare_command
```

预期：Clap rejects `build`/`inspect`.

- [ ] **Step 5: 实现 CLI 和共享 import**

`ImportOptions` 增加：

```rust
pub prepare_options: PrepareInvocationOptions,
```

或提取共享 `ImportThenPrepare` service，避免 `import` 与 `build` 复制 source import。

`build`：

```rust
prepare_canonical(
    &scene,
    PrepareOptions {
        asset_id,
        source_format: scene.source_format.as_str().into(),
        source_digest,
        crease_angle_degrees,
        generate_edges: !skip_edges,
        budget,
    },
)
```

`inspect`：

- Prepared GLB 直接读取 profile/stats/preparation/budget；
- Source 执行同一 import + prepare，但不发布；
- 输出 `AssetBuildReport`。

- [ ] **Step 6: 实现原子发布**

同一目录使用两个 `NamedTempFile`：

1. 写 report 并 sync；
2. 写 GLB 并 sync；
3. report persist-noclobber 或 force；
4. GLB persist-noclobber 或 force；
5. 第二步失败时删除本次 report 新文件，不能破坏旧 output。

测试旧 output/report bytes 在失败后保持不变。

- [ ] **Step 7: 运行 GREEN**

```bash
cargo test -p studio-cli --test prepare_command
cargo test -p studio-cli
cargo test -p studio-assets
```

预期：PASS。

- [ ] **Step 8: 提交**

```bash
git add crates/studio-cli crates/studio-assets
git commit -m "feat(studio-cli): build and inspect prepared assets"
```

---

### Task 6: 完成 Prepare parity 和 Phase 6A 文档

**文件：**
- Modify: `scripts/compare-prepare-imports.mjs`
- Modify: `scripts/tests/prepare-baseline.test.mjs`
- Modify: `scripts/verify.mjs`
- Modify: `README.md`
- Modify: `docs/architecture.md`
- Modify: `docs/reference/capabilities.md`
- Modify: `docs/reference/diagnostics.md`
- Modify: `docs/guides/getting-started.md`
- Modify: `docs/specs/2026-11-02-studio-asset-prepare-parity-design.md`
- Modify: `docs/plans/2026-11-02-studio-prepare-core.md`

**接口：**
- 消费：passing build/inspect + baseline
- 产出：Studio-native parity report and Phase 6A status

- [ ] **Step 1: 实现正常 parity 模式**

脚本：

- `ICE3_STUDIO_BIN` 指向当前 debug/release build；
- build source 两次到临时目录；
- 比较 baseline metrics、edge role/width/color、budget、bounds 和 selection；
- 两次 GLB byte digest 必须相同；
- 写 `output/prepare-parity.json`；
- `status=fail` 时 exit 1。

- [ ] **Step 2: 加入 verify**

`scripts/verify.mjs` 增加：

```javascript
{
  label: 'Prepare baseline contract',
  command: 'node',
  args: ['--test', 'scripts/tests/prepare-baseline.test.mjs'],
},
{
  label: 'Prepare native parity',
  command: 'node',
  args: ['scripts/compare-prepare-imports.mjs'],
},
```

- [ ] **Step 3: 更新文档**

必须说明：

- `import` 默认无 edges；
- `build` 默认 edges；
- edge primitive/material/profile 契约；
- budget/report 与原子发布；
- `ICE3_STUDIO_ASSET_*` Diagnostic；
- Phase 6A Implemented，Phase 6 总体仍 Approved；
- no Blender / no Toolchain。

- [ ] **Step 4: 运行门禁**

```bash
cargo test -p studio-assets
cargo test -p studio-cli
node scripts/compare-prepare-imports.mjs
node --test scripts/tests/prepare-baseline.test.mjs
PLAYWRIGHT_CHANNEL=chrome npm run verify
```

预期：全部 PASS，prepare parity `status=pass`。

- [ ] **Step 5: 提交**

```bash
git add README.md docs scripts/verify.mjs scripts/compare-prepare-imports.mjs \
  scripts/tests/prepare-baseline.test.mjs
git commit -m "test(studio): prove native asset prepare parity"
```

## Plan Self-Review

- Spec coverage：shared Prepare、edges、metrics、budget、report、build、inspect、
  import integration、parity、diagnostics 和文档均有 task。
- Placeholder scan：没有 TODO/TBD 或未定义 API。
- Type consistency：PrepareOptions、PrepareArtifact、AssetMetrics、BudgetProfile、
  BudgetResult、AssetBuildReport 和 CLI 名称跨 task 一致。
- Review Focus：coplanar/crease、Engine edge consumption、budget fail-close、
  GLB/report 原子发布和 import/build 语义分叉都有具体测试。
