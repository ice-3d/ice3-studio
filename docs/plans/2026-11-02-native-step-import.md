# Studio 原生 STEP Source Import 实施计划

> Document ID: ICE3-STUDIO-PLAN-022
>
> Status: Approved
>
> Owner: ICE3 Studio maintainers

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> `superpowers:subagent-driven-development` (recommended) or
> `superpowers:executing-plans` to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal：** 用 Studio 自带 worker 中的纯 Rust AP242 reader 替换 Toolchain STEP
adapter，同时保留业务层级、实例、颜色、单位、Selection、metadata 和失败关闭语义。

**架构：** `studio-import` 增加 feature-gated `step` adapter；adapter 只生成
`SourceScene`，由现有 canonical writer 生成 Prepared GLB。CLI 通过现有 isolated
worker 执行 STEP import，Toolchain 的 STEP adapter 在 parity 通过后删除。

**技术栈：** Rust 1.98、`oxideav-step 0.0.1`、Serde、bincode 2、Cargo test、
Node.js 24.22.1（仅 parity 与 E2E）。

**Spec：** [Studio 原生 STEP 与 IFC Source Import](../specs/2026-11-02-native-step-ifc-import-design.md)

## Global Constraints

- 只承诺 STEP AP242；扩展名固定为 `.step`、`.stp`。
- 新 import 路径不得启动外部 DCC、Node、Toolchain Sidecar 或转换 executable。
- Studio Core 不链接 STEP parser；解析只在 `studio-import` worker 子进程内执行。
- adapter 只能产出 `SourceScene`；Prepared GLB 只能由 `studio-assets::canonical`
  生成。
- 默认限制固定为 1,000,000 nodes、20,000,000 triangles、512 MiB estimated decoded
  memory、120 秒硬 timeout。
- Selection ID 固定为 `<assetId>/step/node/<ordinal>`。
- 输出统一为 right-handed、Y-up、meter，并只应用一次单位/朝向转换。
- source origin 固定为 `[minX,0,minZ]`，只平移 scene root。
- 所有 warning、Diagnostic 和输出顺序必须确定。
- `step` feature 关闭时不得注册 adapter，也不得链接 `oxideav-step`。
- Git author/committer 固定为 `damoqiongqiu <damoqiongqiu@126.com>`。

## Review Focus

- assembly 中同一 part 被多个 occurrence 引用时，必须共享 mesh 而不复制几何。
- root assembly 只有子 part、自身无 shape 时，不能生成空 mesh 或破坏层级。
- 毫米/英寸等 source unit 必须只换算一次，shared origin 不能二次平移 mesh。
- damaged/truncated file、非 AP242 schema、unsupported representation 和空几何必须
  使用稳定 Diagnostic，不能发布部分输出。
- writer 前几何偏差检查必须捕获设备丢失：bounds 不超差、surface area 相对偏差不
  超过 2%、triangle ratio 位于 `0.75..=1.33`。

---

### Task 1: 固化 STEP baseline

**Files：**
- Create: `scripts/compare-step-imports.mjs`
- Create: `fixtures/parity/step-baseline-v1.json`

**Interfaces：**
- Consumes: Toolchain `packages/cli/src/main.ts import ... --adapter step`
- Produces: checked-in baseline，由后续 native parity 使用

- [ ] **Step 1: 写 baseline capture 入口**

`scripts/compare-step-imports.mjs` 先只实现 `--capture`。核心记录结构固定为：

```javascript
const baseline = {
  kind: 'ice3-step-import-baseline',
  schemaVersion: '1.0',
  fixture: '../ice3-toolchain/tests/fixtures/step/synthetic-skid.step',
  assetId: 'parity-step',
  sourceDigest,
  legacy: {
    stats: {
      nodeCount: 6,
      meshCount: 2,
      triangleCount: 188,
      vertexCount,
      instanceNodeCount,
      semanticMaterialCount: 2,
    },
    nodeNames,
    selectionIds,
    bounds,
    surfaceArea,
    materials,
    warningCodes: [],
  },
  normalizationRules: [
    'Tessellation triangles may differ while bounds, area and business identity remain equivalent.',
  ],
};
```

脚本必须以 `spawnSync('node', ['--import', 'tsx', ...])` 调用 Toolchain CLI，读取
`--report` JSON 和 Prepared GLB；不得根据 fixture 文件名硬编码 bounds 或 material。
surface area 由 triangle positions 的叉积和计算。

- [ ] **Step 2: 运行 capture**

```bash
node scripts/compare-step-imports.mjs --capture
```

Expected: 写出 `fixtures/parity/step-baseline-v1.json`，legacy triangle count 为
`188`，node count 为 `6`，mesh count 为 `2`。

- [ ] **Step 3: 增加 baseline 自检**

在脚本中追加：

```javascript
function assertBaseline(value) {
  if (value.kind !== 'ice3-step-import-baseline') throw new Error('baseline kind');
  if (value.legacy.stats.nodeCount !== 6) throw new Error('node baseline');
  if (value.legacy.stats.meshCount !== 2) throw new Error('mesh baseline');
  if (value.legacy.stats.triangleCount !== 188) throw new Error('triangle baseline');
  if (value.legacy.selectionIds.length !== 6) throw new Error('selection baseline');
  if (!(value.legacy.surfaceArea > 0)) throw new Error('surface area baseline');
}
```

再次运行 capture，Expected: exit 0，无 assertion failure。

- [ ] **Step 4: 提交**

```bash
git add scripts/compare-step-imports.mjs fixtures/parity/step-baseline-v1.json
git commit -m "test(studio): capture native STEP import baseline"
```

---

### Task 2: 添加 feature-gated STEP adapter

**Files：**
- Modify: `Cargo.toml`
- Modify: `crates/studio-import/Cargo.toml`
- Modify: `crates/studio-import/src/source.rs`
- Modify: `crates/studio-import/src/lib.rs`
- Create: `crates/studio-import/src/step/mod.rs`
- Create: `crates/studio-import/src/step/adapter.rs`
- Create: `crates/studio-import/tests/step_import.rs`

**Interfaces：**
- Consumes: `NativeImportLimits`、`ImportRequest`、`SourceScene`
- Produces:
  - `StepImporter`
  - `StepImporter::import_file(&Path, &str) -> Result<SourceScene, ImportError>`
  - `SourceFormat::Step`
  - `ImportError::SourceDiagnostic { code, message }`

- [ ] **Step 1: 写失败测试**

```rust
use std::path::PathBuf;
use studio_import::{SourceFormat, StepImporter};

#[test]
fn imports_ap242_hierarchy_and_geometry() {
    let scene = StepImporter::new()
        .import_file(&fixture("step/synthetic-skid.step"), "step-fixture")
        .unwrap();
    let names = scene.nodes.iter().map(|node| node.name.as_str()).collect::<Vec<_>>();
    assert!(names.contains(&"SyntheticSkid"));
    assert!(names.contains(&"PrimaryTank"));
    assert!(names.contains(&"TransferPump"));
    assert_eq!(scene.meshes.len(), 2);
    assert_eq!(scene.source_format, SourceFormat::Step);
    assert_eq!(
        scene.nodes.iter().filter(|node| node.mesh.is_some()).count(),
        2
    );
    assert!(scene.nodes.iter().all(|node| {
        node.id.starts_with("step-fixture/step/node/")
    }));
}

fn fixture(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../ice3-toolchain/tests/fixtures")
        .join(relative)
}
```

- [ ] **Step 2: 运行 RED**

```bash
cargo test -p studio-import --test step_import
```

Expected: compile failure，`StepImporter` 和 `SourceFormat::Step` 不存在。

- [ ] **Step 3: 添加 feature 和错误桥接**

`Cargo.toml`：

```toml
[workspace.dependencies]
oxideav-step = { version = "=0.0.1", default-features = false }
```

`crates/studio-import/Cargo.toml`：

```toml
[features]
default = ["fbx", "usd", "step"]
step = ["dep:oxideav-step"]

[dependencies]
oxideav-step = { workspace = true, optional = true }
```

`source.rs` 增加：

```rust
pub enum SourceFormat {
    Gltf,
    Obj,
    Fbx,
    Usd,
    Step,
}

impl SourceFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Gltf => "gltf",
            Self::Obj => "obj",
            Self::Fbx => "fbx",
            Self::Usd => "usd",
            Self::Step => "step",
        }
    }
}

#[error("{code}: {message}")]
SourceDiagnostic { code: String, message: String },
```

`ImportError::code()` 对 `SourceDiagnostic` 返回 `code`。`lib.rs` 增加
`#[cfg(feature = "step")] mod step;` 和 re-export。

- [ ] **Step 4: 实现 reader 与 schema gate**

`step/adapter.rs` 固定入口：

```rust
pub fn import_step(
    request: &ImportRequest,
    limits: NativeImportLimits,
) -> Result<SourceScene, ImportError> {
    let bytes = read_bounded_source(&request.path, limits.max_memory_bytes)?;
    let options = read_options(limits)?;
    let model = oxideav_step::read_step_with(&bytes, &options)
        .map_err(map_step_error)?;
    if model.schema != oxideav_step::ApSchema::Ap242 {
        return Err(ImportError::SourceDiagnostic {
            code: "ICE3_STUDIO_IMPORT_STEP_SCHEMA_UNSUPPORTED".into(),
            message: format!("unsupported STEP schema: {:?}", model.schema),
        });
    }
    project_model(&model, request.asset_id.trim(), limits)
}

fn read_bounded_source(path: &Path, max_bytes: usize) -> Result<Vec<u8>, ImportError> {
    let metadata = std::fs::metadata(path)?;
    if metadata.len() > max_bytes as u64 {
        return Err(ImportError::LimitExceeded(format!(
            "STEP input has {} bytes; limit is {max_bytes}",
            metadata.len()
        )));
    }
    std::fs::read(path).map_err(ImportError::from)
}
```

`read_options` 必须映射：

```rust
let step_limits = oxideav_step::StepLimits {
    max_input_len: limits.max_memory_bytes,
    max_instances: limits.max_nodes,
    ..Default::default()
};
let geometry_limits = oxideav_step::GeometryLimits {
    max_triangles: usize::try_from(limits.max_triangles)
        .map_err(|_| ImportError::LimitExceeded("STEP triangle budget".into()))?,
    ..Default::default()
};
let tolerance = oxideav_step::Tolerance {
    relative: 0.001,
    absolute: None,
    max_angle: 0.5,
};
```

`project_model` 按 `StepModel.roots`、`Part.children` 深度优先生成节点。共享 part
只创建一个 mesh index；每个 occurrence 生成独立 node。节点按遍历序分配
`<assetId>/step/node/<ordinal>`。先忽略 shape 细节但必须挂接 mesh index。

- [ ] **Step 5: 写最小 geometry conversion**

每个 `Shape.mesh`：

```rust
let positions = shape.mesh.positions
    .iter()
    .map(|p| [(p[0] * model.length_unit_metres) as f32,
              (p[2] * model.length_unit_metres) as f32,
              (-p[1] * model.length_unit_metres) as f32])
    .collect::<Vec<_>>();
let indices = shape.mesh.triangles
    .iter()
    .flatten()
    .map(|index| u32::try_from(*index)
        .map_err(|_| ImportError::InvalidSource("STEP index overflow".into())))
    .collect::<Result<Vec<_>, _>>()?;
```

色组先使用 shape-level colour；无颜色时固定 neutral `[0.72, 0.74, 0.76]`。
triangle/vertex budget 在构造 `SourceScene` 前检查。

- [ ] **Step 6: 运行 GREEN**

```bash
cargo test -p studio-import --test step_import
cargo test -p studio-import
```

Expected: focused test 和全 crate tests 通过。

- [ ] **Step 7: 提交**

```bash
git add Cargo.toml Cargo.lock crates/studio-import
git commit -m "feat(studio): import STEP with a native AP242 reader"
```

---

### Task 3: 完成 STEP 实例、颜色、origin 和 metadata 语义

**Files：**
- Modify: `crates/studio-import/src/step/adapter.rs`
- Create: `crates/studio-import/src/step/mapping.rs`
- Modify: `crates/studio-import/tests/step_import.rs`

**Interfaces：**
- Consumes: Task 2 `StepImporter`
- Produces:
  - shared mesh identity
  - face-colour primitive groups
  - deterministic `extras.ice3.step`
  - shared root origin

- [ ] **Step 1: 写失败测试**

```rust
#[test]
fn preserves_per_face_colors() {
    let scene = StepImporter::new()
        .import_file(&fixture("step/synthetic-skid.step"), "step-fixture")
        .unwrap();
    let colors = scene.materials.iter()
        .map(|material| material.base_color)
        .collect::<Vec<_>>();
    assert!(colors.iter().any(|color| close(*color, [0.16, 0.48, 0.72])));
    assert!(colors.iter().any(|color| close(*color, [0.92, 0.56, 0.12])));
}

#[test]
fn applies_millimeter_units_and_shared_origin_once() {
    let scene = StepImporter::new()
        .import_file(&fixture("step/synthetic-skid.step"), "step-fixture")
        .unwrap();
    let extras = &scene.asset_extras["ice3"]["step"];
    assert_eq!(extras["schema"], "ap242");
    assert_eq!(extras["unitAssumption"], "meter");
    assert!(extras["sourceOrigin"].as_array().unwrap()[1] == 0.0);
    assert_world_bounds(scene, [0.0, 0.0, 0.0], [2.01, 0.9, 0.7]);
}
```

`assert_world_bounds` 必须应用 node hierarchy matrices，比较每个轴
`max(0.0001, world_extent * 0.00001)`。

- [ ] **Step 2: 运行 RED**

```bash
cargo test -p studio-import --test step_import
```

Expected: color/origin/metadata assertions fail。

- [ ] **Step 3: 实现 per-face primitive 分组**

以 `Shape.triangle_faces` 和 `triangle_colours` 为键：

```rust
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct TriangleGroupKey {
    colour: [u32; 4],
}

fn grouped_triangles(shape: &Shape) -> BTreeMap<TriangleGroupKey, Vec<u32>> {
    let mut groups = BTreeMap::new();
    for (triangle_index, triangle) in shape.mesh.triangles.iter().enumerate() {
        let colour = shape.triangle_colours
            .get(triangle_index)
            .copied()
            .flatten()
            .or(shape.colour)
            .unwrap_or([0.72, 0.74, 0.76, 1.0]);
        groups.entry(TriangleGroupKey::from(colour))
            .or_default()
            .extend(triangle.iter().copied().map(|index| index as u32));
    }
    groups
}
```

material 由规范化 RGBA 去重，名称固定
`step/material/<ordinal>`；shape colour 和 face override 的优先级在测试中固定。

- [ ] **Step 4: 实现共享 mesh 与 occurrence 节点**

节点映射使用：

```rust
struct PartProjection {
    mesh: usize,
    children: Vec<usize>,
}

fn project_part(
    parts: &[Part],
    part_index: usize,
    parent: Option<&str>,
    state: &mut ProjectionState,
) -> Result<usize, ImportError>;
```

同一 `part_index` 的 `PartProjection.mesh` 复用；每个 `Occurrence` 生成节点并记录
occurrence transform。无 occurrence 的 part 作为 root children。

- [ ] **Step 5: 实现 unit、origin 和 metadata**

先在 `f64` 中应用 occurrence hierarchy，计算 world bounds，再决定
`sourceOrigin=[minX,0,minZ]`。所有 source mesh positions 仍保留本地坐标；root node
矩阵组合 `Y-up rotation * origin translation`，且两者只组合一次。

node extras：

```rust
json!({
    "ice3": {
        "step": {
            "schema": "ap242",
            "productId": part.product_id,
            "definitionId": part.definition,
            "occurrenceId": occurrence.id,
            "itemId": shape.item,
            "layers": shape.layers,
        }
    }
})
```

`None` 和空数组省略；warning 去重排序。

- [ ] **Step 6: 增加 shared occurrence 回归测试**

在 `mapping.rs` unit test 中构造两个 `Occurrence { part: 1, .. }` 指向同一 part：

```rust
#[test]
fn repeated_occurrences_share_one_mesh() {
    let projection = project_test_model(&[root_part(), child_part("Pump")], &[
        occurrence(0, 1),
        occurrence(0, 1),
    ]).unwrap();
    assert_eq!(projection.meshes.len(), 1);
    assert_eq!(projection.nodes.iter().filter(|node| node.mesh == Some(0)).count(), 2);
}
```

- [ ] **Step 7: 运行 GREEN**

```bash
cargo test -p studio-import --test step_import
cargo test -p studio-import
cargo clippy -p studio-import --all-targets -- -D warnings
```

Expected: all pass，warning 输出为空。

- [ ] **Step 8: 提交**

```bash
git add crates/studio-import/src/step crates/studio-import/tests/step_import.rs
git commit -m "feat(studio): preserve STEP assembly semantics"
```

---

### Task 4: 接入 CLI worker 与 STEP 失败关闭

**Files：**
- Modify: `crates/studio-cli/Cargo.toml`
- Modify: `crates/studio-cli/src/import.rs`
- Modify: `crates/studio-cli/src/main.rs`
- Modify: `crates/studio-import/src/bin/studio-import-worker.rs`
- Create: `crates/studio-cli/tests/step_import.rs`
- Modify: `scripts/compare-step-imports.mjs`

**Interfaces：**
- Consumes: `StepImporter`、`run_isolated_import`
- Produces: `--adapter auto|...|step` 和 native parity

- [ ] **Step 1: 写 CLI tests**

```rust
#[test]
fn imports_step_through_the_internal_worker() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("step.glb");
    let result = Command::new(env!("CARGO_BIN_EXE_ice3-studio"))
        .args([
            "import",
            fixture("step/synthetic-skid.step").to_str().unwrap(),
            "--output", output.to_str().unwrap(),
            "--asset-id", "step-fixture",
            "--adapter", "step",
        ])
        .output()
        .unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let parsed = read_glb(&output).unwrap();
    assert_eq!(validate_prepared_profile(&parsed).unwrap().asset_id, "step-fixture");
}

#[test]
fn truncated_step_does_not_replace_existing_output() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("step.glb");
    std::fs::write(&output, b"keep").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_ice3-studio"))
        .args([
            "import",
            fixture("step/truncated.step").to_str().unwrap(),
            "--output", output.to_str().unwrap(),
            "--asset-id", "step-truncated",
        ])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr)
        .contains("ICE3_STUDIO_IMPORT_INVALID_SOURCE"));
    assert_eq!(std::fs::read(&output).unwrap(), b"keep");
}
```

- [ ] **Step 2: 运行 RED**

```bash
cargo test -p studio-cli --test step_import
```

Expected: `step` adapter unavailable。

- [ ] **Step 3: 注册 feature、registry 和 worker**

`crates/studio-cli/Cargo.toml`：

```toml
[features]
default = ["fbx", "usd", "step"]
step = ["studio-import/step"]
```

`studio-cli/src/import.rs`：

```rust
#[cfg(feature = "step")]
use studio_import::StepImporter;

#[cfg(feature = "step")]
importers.push(Box::new(StepImporter::new()));

let imported = match selected_adapter {
    #[cfg(feature = "step")]
    "step" => run_isolated_import(&StepImporter::new(), &request, options.limits)?,
    // existing branches
};
```

registry 和 `studio-import-worker` 都在 `usd` 后注册 `step`。Clap value parser 增加
`step`。

- [ ] **Step 4: 扩展 parity 比较**

`compare-step-imports.mjs` 正常模式：

```javascript
const geometry = {
  bounds: boundsClose(native.bounds, baseline.legacy.bounds),
  surfaceArea: relativeError(native.surfaceArea, baseline.legacy.surfaceArea) <= 0.02,
  triangleRatio: ratio(native.triangleCount, baseline.legacy.triangleCount),
  triangleRatioBounded: triangleRatio >= 0.75 && triangleRatio <= 1.33,
};
if (!geometry.bounds || !geometry.surfaceArea || !geometry.triangleRatioBounded) {
  failures.push('geometry');
}
```

Selection、node names、mesh count、instance count、materials 和 warnings 仍为 exact
compare；triangle count、vertex count 和 digest 始终写入 report。

- [ ] **Step 5: 运行 GREEN**

```bash
cargo test -p studio-cli --test step_import
cargo test -p studio-cli
node scripts/compare-step-imports.mjs
```

Expected: CLI tests pass；parity `status=pass`，triangle difference 被记录，bounds 和
surface area 门禁通过。

- [ ] **Step 6: 增加负向 corpus**

`compare-step-imports.mjs` 对：

- `empty-assembly.step` -> `ICE3_STUDIO_IMPORT_EMPTY_GEOMETRY`
- `truncated.step` -> `ICE3_STUDIO_IMPORT_INVALID_SOURCE`
- valid-but-non-AP242 fixture -> `ICE3_STUDIO_IMPORT_STEP_SCHEMA_UNSUPPORTED`
- `ICE3_STUDIO_IMPORT_MAX_TRIANGLES=1` -> `ICE3_STUDIO_IMPORT_LIMIT_EXCEEDED`
- `ICE3_STUDIO_IMPORT_WORKER_TIMEOUT_MS=1` -> `ICE3_STUDIO_IMPORT_TIMEOUT`

逐项断言 exit 1、Diagnostic 存在、目标文件不存在或已有内容保持不变。

- [ ] **Step 7: 提交**

```bash
git add crates/studio-cli crates/studio-import/src/bin/studio-import-worker.rs \
  scripts/compare-step-imports.mjs fixtures/parity/step-baseline-v1.json
git commit -m "feat(studio-cli): import STEP without external bridges"
```

---

### Task 5: 退役 Toolchain STEP bridge 并完成验证

**Files：**
- Modify outside Studio: `ice3-toolchain/packages/cli/src/commands.ts`
- Modify outside Studio: `ice3-toolchain/packages/cli/package.json`
- Modify outside Studio: `ice3-toolchain/package-lock.json`
- Modify outside Studio: `ice3-toolchain/tsconfig.json`
- Modify outside Studio: `ice3-toolchain/tools/conformance/corpus.ts`
- Delete outside Studio: `ice3-toolchain/packages/step-importer/`
- Delete outside Studio: `ice3-toolchain/tests/step/`
- Delete outside Studio: `ice3-toolchain/tests/cli/importCommand.test.ts`
- Modify: `README.md`
- Modify: `AGENTS.md`
- Modify: `docs/README.md`
- Modify: `docs/architecture.md`
- Modify: `docs/reference/capabilities.md`
- Modify: `docs/reference/diagnostics.md`
- Modify: `docs/guides/getting-started.md`
- Modify: `docs/specs/2026-11-02-native-step-ifc-import-design.md`
- Modify: this plan

**Interfaces：**
- Consumes: passing native STEP parity
- Produces: Toolchain 只保留 IFC/JT，Studio 默认加载 STEP

- [ ] **Step 1: 删除 STEP production bridge**

从 Toolchain CLI lazy imports、help、package dependencies、tsconfig references 和
conformance manifest 删除 STEP。删除 `packages/step-importer` 与 `tests/step`，
但保留 `tests/fixtures/step/`。

- [ ] **Step 2: 更新文档和状态**

文档必须说明：

- Studio 原生 AP242 import、extension、Selection、metadata 和坐标系统。
- STEP worker 的 limits/timeout/failure-close。
- Toolchain 只保留 IFC、JT。
- Plan-022 和 Spec-017 的 Phase 4A milestone 标记 Implemented；Spec-017 整体仍
  Approved，因为 IFC 尚未完成。

- [ ] **Step 3: 更新 notices 和 feature tests**

Studio `THIRD_PARTY_NOTICES.md` 添加 `oxideav-step 0.0.1` MIT notice。
`crates/studio-import/tests/third_party_notices.rs` 断言该声明存在。

增加：

```bash
cargo check -p studio-cli --no-default-features
cargo test -p studio-import --no-default-features --tests
cargo tree -p studio-import --no-default-features
```

Expected: 第三种依赖树不包含 `oxideav-step`。

- [ ] **Step 4: 运行完整门禁**

Studio：

```bash
PLAYWRIGHT_CHANNEL=chrome npm run verify
node scripts/compare-asset-pipelines.mjs
node scripts/compare-source-imports.mjs
node scripts/compare-fbx-usd-imports.mjs
node scripts/compare-step-imports.mjs
```

Toolchain：

```bash
npm run verify
rg -n "step-importer|occt-import-js" packages tools package.json package-lock.json
test -f tests/fixtures/step/synthetic-skid.step
```

Expected: Studio 18/18 gates、Chrome E2E、四份 parity pass；Toolchain tests pass；
production search 无 STEP bridge；fixture 保留。

- [ ] **Step 5: 提交**

Studio：

```bash
git add README.md AGENTS.md docs THIRD_PARTY_NOTICES.md \
  crates/studio-import/tests/third_party_notices.rs
git commit -m "docs(studio): complete native STEP import phase 4a"
```

Toolchain：

```bash
git add -A
git commit -m "refactor(toolchain): retire the STEP source bridge"
```

## Plan Self-Review

- Spec coverage：层级、实例、颜色、单位、origin、Selection、metadata、worker、
  feature gate、parity 和退役都有对应 task。
- Placeholder scan：未发现占位项；所有测试与命令均有具体值。
- Type consistency：`StepImporter`、`SourceFormat::Step`、`SourceDiagnostic`、
  `project_model` 和 `project_part` 的名称跨 task 一致。
- Review Focus：多 occurrence、root-only part、unit/origin、损坏/unsupported input
  和几何偏差都由 Task 2–4 的具体测试覆盖。
