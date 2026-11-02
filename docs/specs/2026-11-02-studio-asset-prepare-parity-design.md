# Studio Asset Prepare 与 Toolchain 功能等价设计

> Document ID: ICE3-STUDIO-SPEC-019
>
> Status: Approved
>
> Owner: ICE3 Studio maintainers

> **阶段状态：** Phase 1 至 Phase 5 已完成，Phase 6A 已完成。Phase 6 是迁移后的
> 产品能力收口，目标是在不恢复 Toolchain、Node Asset Runtime、外部 DCC 或 Blender
> 的前提下，让 Studio 具备原 Toolchain 的完整资产准备能力。Phase 6B/6C 仍为 Approved，
> 因此 Spec 019 总体尚未标记 Implemented。

**目标：** 在 Studio 内建立唯一、共享的 Asset Prepare pipeline，覆盖确定性 render
edges、crease angle、Prepared metrics、built-in/custom budget、版本化 report、
`build`、`inspect`、直接 source `partition` 以及 Runtime Tileset edge 输出。

**前置设计：**

- [Studio 原生 Asset Pipeline 迁移](2026-11-01-native-asset-pipeline-migration-design.md)
- [Studio 原生 JT Import 与 Toolchain 最终退役](2026-11-02-native-jt-toolchain-retirement-design.md)

## 1. 背景

Phase 5 完成后，Studio 已拥有：

- glTF/GLB、OBJ/MTL、FBX、USD、STEP AP242、IFC4X3 和 JT Source Import；
- Prepared GLB canonical writer；
- committed/candidate preview；
- partition、LOD、semantic proxy、render batch、coarse chunk 和 Runtime Tileset；
- Agent-first Viewer 和 ChangeSet 审查。

Studio 的生产代码、测试和依赖不再调用 Blender。JT 仍使用组织批准的 host-owned
converter，这是已有的受控例外。

但旧 Toolchain 的下列能力尚未全部形成 Studio 公共入口：

- `build`：edge generation、crease angle、budget 和 report；
- `inspect`：source 与 Prepared GLB 的稳定统计；
- 直接对任意 source 执行带完整 LOD/batching 选项的 `partition`；
- Prepared GLB line primitive 和 edge material；
- metrics、budget、report 与实际产物的一致性。

Engine 仍实际消费 `mode = 1` line primitive、`role = "edges"`、`widthPx` 和 material
`edgeColor`。因此不能把 render edges 当作可忽略的旧渲染细节。

## 2. 成功标准

- 同一 glTF source 经 Studio `build` 后，node、mesh、primitive、triangle、edge、
  material 和 Selection 统计与旧 Toolchain baseline 等价。
- Studio 生成的带 edges Prepared GLB 可由 Engine validator 和 reader 实际读取。
- `build` 支持 built-in/custom budget、report、warnings-as-error 和原子发布。
- `inspect` 对 source 或 Prepared GLB 输出稳定的机器可读统计。
- Studio 可直接对任意受支持 source 执行 `partition`，完整暴露现有 Studio LOD、
  semantic proxy、render batch 和 coarse chunk 选项。
- `export tileset` 与直接 `partition` 共用同一 `AssetProfile` 和 edge 配置。
- Studio 的生产路径、发布包和测试都不依赖 Blender、Toolchain、Node Asset Runtime
  或外部 DCC。
- 现有七种 Source Import、preview、Agent Console Tileset 和 API 行为不回归。

## 3. 非目标

- 不恢复 Toolchain package、Sidecar、Node import adapter 或旧 CLI executable。
- 不恢复 Blender、`bpy`、Python authoring、DCC bridge 或任意 shell 脚本。
- 不改变 JT 的受控 converter exception；本项目不新增其它外部转换器。
- 不实现布尔几何、自由曲线、雕刻或新的材质模型。
- 不改变 Document、Operation、ChangeSet 或 Selection 的业务语义。
- 不承诺旧 Toolchain 的包名、npm 入口或 `ICE3_TOOL_*` Diagnostic 字面值长期存在；
  只保留 CLI 参数、产物语义和可审计迁移映射。
- 不在本次收口中把 render edges 替换成 shader 或后处理边线。

## 4. 总体架构

```text
Source format
    │
    ▼
studio-import ──> SourceScene
                     │
                     ▼
               studio-assets::prepare
                 ├── edge generation
                 ├── crease-angle handling
                 ├── Prepared Profile / metadata
                 ├── asset metrics
                 ├── budget evaluation
                 └── canonical Prepared GLB
                     │
          ┌──────────┴──────────┐
          ▼                     ▼
CLI build/import        studio-pipeline
inspect/report          partition / LOD / Tileset
```

边界规则：

- `SourceScene` 仍是格式无关的 source contract。
- `studio-assets::prepare` 是 Prepared GLB 的唯一语义实现。
- `studio-cli` 只负责参数、文件发布和命令编排。
- `studio-pipeline` 的 fine/coarse output 通过共享 edge generator 生成 line primitive。
- `export tileset` 与 `partition` 通过同一个 `AssetProfile` 选择 LOD、batching、
  budget 和 edge 行为。

## 5. 共享 Prepare API

`studio-assets` 增加以下 public contract：

```rust
pub struct PrepareOptions {
    pub asset_id: String,
    pub source_format: String,
    pub source_digest: String,
    pub crease_angle_degrees: f32,
    pub generate_edges: bool,
    pub budget: BudgetProfile,
}

pub struct PrepareArtifact {
    pub bytes: Vec<u8>,
    pub metrics: AssetMetrics,
    pub preparation: PreparationMetadata,
    pub budget: BudgetResult,
    pub content_digest: String,
    pub warnings: Vec<PrepareWarning>,
}
```

要求：

- `PrepareOptions` 只接受受控配置，不接受任意 executable、plugin 或脚本路径。
- `PrepareArtifact` 在内存中完成验证后才允许调用者发布。
- `write_prepared_glb` 保留为薄封装，内部必须调用同一 Prepare 实现。
- `import`、`build`、`partition` 和 `export tileset` 不得再各自拼装 profile 或统计。
- asset extras 中的 profile、metrics、preparation、budget 和 digest 必须来自同一
  `PrepareArtifact`。

## 6. Render Edge 语义

### 6.1 生成算法

对每个 triangle primitive：

1. 使用 `round(position * 100000)` 量化后焊接相同位置顶点。
2. 按 welded vertex pair 聚合边，保留首次出现顺序。
3. 计算非退化三角形 face normal。
4. 以下边进入 edge index：
   - 只被一个 triangle 使用的 boundary edge；
   - 相邻 face normal 夹角不小于 crease angle 的 edge。
5. 退化法线的处理必须确定，不得产生 NaN 或随机顺序。

默认 crease angle 固定为 `30°`，合法范围 `0..=180`。`--skip-edges` 或 source 的
per-material suppression 可以使 edge segment 为零。

### 6.2 Output primitive

每个带 triangle 的 source material 生成一个对应 edge material：

```json
{
  "name": "<material> edges",
  "pbrMetallicRoughness": {
    "baseColorFactor": [0.90196, 0.94902, 1.0, 1.0],
    "metallicFactor": 0,
    "roughnessFactor": 1
  },
  "extras": {
    "ice3": {
      "id": "<assetId>/material/<index>/edges"
    }
  }
}
```

Edge primitive：

```json
{
  "attributes": {"POSITION": 0},
  "indices": 1,
  "mode": 1,
  "material": 1,
  "extras": {
    "ice3": {
      "role": "edges",
      "widthPx": 3
    }
  }
}
```

颜色规则：

- `material.extras.ice3.edgeColor` 为合法 6/8 位 CSS hex 时使用该颜色；
- 否则使用 `#e6f2ff`；
- `material.extras.ice3.edges = false` 时该 material 不生成 edge primitive；
- edge material 必须参与 material count、primitive count 和 encoded byte budget。
- `--skip-edges` 只移除 line primitive 和 edge segment；为保持 legacy material
  index parity，实际使用的非 suppression source materials 仍保留对应 edge
  material。
- 为保持 legacy parity，始终保留 `default`、`default edges` 和 `vertex colors`
  三个 reserved material；即使当前 source 没有使用。其顺序固定排在 source
  material/edge material 之后。

### 6.3 索引和确定性

- 顶点数不超过 `65535` 时使用 `u16` index，否则使用 `u32`。
- Welding、edge 首次出现顺序、material 顺序、accessor 顺序和 JSON 顺序必须确定。
- 相同 SourceScene、PrepareOptions 和 Studio version 必须产生 byte-identical GLB。

## 7. Metrics、Budget 与 Preparation Metadata

### 7.1 Metrics

```rust
pub struct AssetMetrics {
    pub encoded_bytes: u64,
    pub json_chunk_bytes: u64,
    pub bin_chunk_bytes: u64,
    pub node_count: u64,
    pub max_node_depth: u64,
    pub mesh_count: u64,
    pub primitive_count: u64,
    pub triangle_count: u64,
    pub edge_segment_count: u64,
    pub material_count: u64,
    pub selection_group_count: u64,
}
```

所有值从最终 GLB JSON/BIN 和 SourceScene identity 计算，不得由 CLI 猜测或固定填写。

### 7.2 Budget

内置 profile：

- `interactive-desktop-v1`
- `unrestricted`

`interactive-desktop-v1` 保持原阈值：

| Metric | Soft | Hard |
|---|---:|---:|
| Nodes | 25,000 | 50,000 |
| Meshes | 5,000 | 15,000 |
| Primitives | 10,000 | 30,000 |
| Triangles | 5,000,000 | 20,000,000 |
| Edge segments | 10,000,000 | 40,000,000 |
| Encoded bytes | 134,217,728 | 536,870,912 |
| Selection groups | 20,000 | 100,000 |

自定义 budget 从受控 JSON 文件读取，schema 为：

```json
{
  "id": "project-profile",
  "limits": [
    {
      "id": "triangleCount",
      "label": "Triangles",
      "softLimit": 1000000,
      "hardLimit": 2000000
    }
  ]
}
```

规则：

- metric id 只能来自 `AssetMetrics` 的非 chunk-byte 字段；
- soft/hard 必须是非负 integer，且 soft 不得大于 hard；
- 重复 metric、未知字段取值或非法文件返回稳定 Diagnostic；
- hard 超限时不发布 output/report；
- soft warning 默认发布，并在 `--warnings-as-errors` 时失败。

### 7.3 Preparation metadata

Prepared asset extras 至少包含：

```json
{
  "profile": "ice3-prepared",
  "profileVersion": "1.0",
  "contractVersion": "1.0",
  "assetId": "...",
  "sourceFormat": "...",
  "sourceDigest": "sha256:...",
  "contentDigest": {"algorithm": "sha256", "value": "...", "covers": "binary-chunk"},
  "bounds": {"min": [0, 0, 0], "max": [1, 1, 1]},
  "stats": {},
  "preparation": {
    "runtimeConversionRequired": false,
    "unitsNormalized": true,
    "matrixTransformsBaked": false,
    "requiredCompressionDecoded": true,
    "edgesPrepared": true,
    "selectionGroupsPrepared": true,
    "options": {"creaseAngleDegrees": 30, "edges": true}
  },
  "budget": {}
}
```

## 8. Report 契约

`--report FILE` 输出：

```json
{
  "kind": "ice3-asset-build-report",
  "schemaVersion": "1.0",
  "command": "build",
  "sourceDigest": "sha256:...",
  "contentDigest": "sha256:...",
  "assetId": "...",
  "metrics": {},
  "preparation": {},
  "budget": {},
  "warnings": []
}
```

要求：

- warnings 按 code、message 和 path 排序；
- report 不包含 source 绝对路径、临时路径、用户名或 converter stderr；
- report 与 GLB 使用同一次 PrepareArtifact；
- report 写入和 GLB rename 失败时都不得留下部分文件；
- `--json` 控制 stdout 紧凑 JSON，未指定时使用稳定 pretty JSON。

## 9. CLI 契约

### 9.1 `import`

```text
ice3-studio import SOURCE --output FILE --asset-id ID
  [--adapter auto|gltf|obj|fbx|usd|step|ifc|jt]
  [--warnings-as-errors]
  [--force]
```

保持现有行为并改走共享 Prepare：

- 默认 `generate_edges = false`；
- 仍写出完整 Prepared Profile、真实 metrics 和 budget；
- 不生成 render edges。

### 9.2 `build`

```text
ice3-studio build SOURCE --output FILE --asset-id ID
  [--adapter auto|gltf|obj|fbx|usd|step|ifc|jt]
  [--crease-angle 30]
  [--skip-edges]
  [--budget interactive-desktop-v1|unrestricted|BUDGET.json]
  [--report REPORT.json]
  [--warnings-as-errors]
  [--force]
  [--json]
```

默认：

- `generate_edges = true`；
- `crease_angle_degrees = 30`；
- budget profile 为 `interactive-desktop-v1`。

### 9.3 `inspect`

```text
ice3-studio inspect SOURCE
  [--adapter auto|gltf|obj|fbx|usd|step|ifc|jt]
  [--crease-angle 30]
  [--skip-edges]
  [--budget interactive-desktop-v1|unrestricted|BUDGET.json]
  [--json]
```

- Prepared GLB：读取 profile 中的 metrics、preparation 和 budget；
- Source：执行不发布的临时 Prepare，并输出同一字段；
- 空 Geometry、invalid source 和 budget error 使用稳定 Diagnostic；
- inspect 不创建 cache、preview 或工程 revision。

### 9.4 `partition`

```text
ice3-studio partition SOURCE --output DIRECTORY --asset-id ID
  [--adapter auto|gltf|obj|fbx|usd|step|ifc|jt]
  [--max-triangles 500000]
  [--max-bytes 33554432]
  [--crease-angle 30]
  [--skip-edges]
  [--budget interactive-desktop-v1|unrestricted|BUDGET.json]
  [--render-batch off|static]
  [--lod]
  [--lod-ratio 0.15]
  [--lod-allocation proportional|adaptive]
  [--lod-max-triangles 250000]
  [--lod-min-unit-triangles 8]
  [--lod-curve-samples 8]
  [--lod-max-memory-bytes 67108864]
  [--semantic-proxy off|auto]
  [--coarse-chunk-max-bytes MAX_BYTES]
  [--warnings-as-errors]
  [--force]
  [--json]
```

约束：

- `--semantic-proxy` 需要 `--lod`；
- `semantic-proxy=auto` 需要 `adaptive` allocation；
- curve samples 为 `4..=32`；
- min unit triangles 不得大于 max triangles；
- `--coarse-chunk-max-bytes` 默认等于 `--max-bytes`；
- 默认不替换已有输出目录，显式 `--force` 才允许；
- fine、LOD、coarse 和 render batch GLB 都必须生成 edges 并进行 Prepared validation。

### 9.5 `export tileset`

保持现有项目入口：

```text
ice3-studio export tileset --project PROJECT --output DIRECTORY
  [--profile agent-console-v1]
  [--force]
```

该 profile 增加 edge preparation 配置，并把同一 `AssetProfile` 传给 pipeline。

## 10. Studio Pipeline 集成

`AssetProfile` 扩展为：

```rust
pub struct AssetProfile {
    pub id: String,
    pub max_triangles: u64,
    pub max_bytes: usize,
    pub crease_angle_degrees: f32,
    pub generate_edges: bool,
    pub lod_ratio: f64,
    pub lod_allocation: LodAllocationMode,
    pub lod_max_triangles: u64,
    pub lod_min_unit_triangles: u64,
    pub lod_max_memory_bytes: usize,
    pub coarse_chunk_max_bytes: usize,
    pub semantic_proxy: SemanticProxyMode,
    pub render_batch: RenderBatchMode,
}
```

Fine/LOD/coarse output 必须：

- 用共享 Rust edge generator 生成 line primitive；
- 保留 source material 的 edge color 和 suppression；
- 把 edge bytes 纳入 `max_bytes`、coarse chunk 和总 budget；
- 在 part profile 中写入真实 `edgeSegmentCount` 和 `edgesPrepared`；
- 保持 shared topology 的 extent 和 edge generation 一致；
- 不把 line primitive 计入 triangle limit，但计入 edge/byte limit。

## 11. Diagnostics 与兼容

Studio 新 Diagnostic 使用：

- `ICE3_STUDIO_ASSET_INVALID_OPTIONS`
- `ICE3_STUDIO_ASSET_INVALID_BUDGET`
- `ICE3_STUDIO_ASSET_BUDGET_WARNING`
- `ICE3_STUDIO_ASSET_BUDGET_EXCEEDED`
- `ICE3_STUDIO_ASSET_INVALID_PROFILE`
- `ICE3_STUDIO_ASSET_OUTPUT_EXISTS`
- `ICE3_STUDIO_ASSET_INVALID_GEOMETRY`
- `ICE3_STUDIO_ASSET_REPORT_WRITE_FAILED`
- `ICE3_STUDIO_ASSET_TILESET_INVALID`

旧 `ICE3_TOOL_*` 只用于 baseline 和迁移映射，不得出现在新的 Studio 生产输出。

兼容矩阵：

| Toolchain behavior | Studio behavior |
|---|---|
| `build` | `build`，参数语义与默认值兼容 |
| `inspect` | `inspect`，source/Prepared 同一字段 |
| `validate` | `validate`，验证 Prepared profile |
| `partition` | `partition`，支持全部 Studio source |
| `--json` | 保留 |
| `--skip-edges` | 保留 |
| `--budget` | 保留，使用 Studio parser |
| `--report` | 保留，使用 Studio schema |
| Toolchain package | 不恢复 |
| Blender/DCC | 不恢复 |

## 12. 错误与原子性

- Source import、edge generation、budget、GLB validation 任一失败：不发布 GLB。
- report 生成或写入失败：不发布 GLB/report 中任何新文件。
- `partition` 先写同父目录 staging；全部 GLB、Tileset 和 metadata 验证后再 rename。
- `--force` replacement 失败时恢复旧目录；不得留下 merge directory。
- 并发 writer 仍由现有 project/workspace lock 或 output no-clobber 规则保护。
- 所有 Diagnostic 稳定、无随机 ID、无时间戳、无客户路径。

## 13. 性能与资源

- Edge generation 复杂度目标为 `O(triangles + unique_edges)`。
- 默认沿用 Studio import 的 1,000,000 nodes、20,000,000 triangles 和 512 MiB
  estimated decoded memory。
- edge segment 超过 hard budget 时失败关闭。
- 对 large-industrial fixture 记录 build/partition 的 wall-clock、peak memory、
  output bytes、triangle/edge counts。
- 与 Toolchain legacy baseline 比较时，Studio wall-clock p95 不得超过旧实现两倍；
  超过时必须在 spec/plan 中记录原因和后续优化，不得静默放宽。
- Engine/Agent Console 继续只消费 Prepared GLB/Tileset，不进入 prepare 实现。

## 14. 测试与验收

### 14.1 Edge parity

- Cube boundary edges 与 face diagonals；
- 开放网格 boundary edges；
- 双面/闭合网格 crease；
- 0°、30°、60°、90° 和 180° crease；
- 重复位置、退化三角形和不连续索引；
- per-material `edges=false`；
- edge color default/custom；
- u16/u32 index 切换；
- deterministic accessor、indices 和 GLB digest。

### 14.2 Legacy parity

在归档 Toolchain 仍可读时生成 Studio-local baseline，记录：

- source digest；
- stats；
- node/mesh/material/selection；
- bounds 和 surface area；
- edgeSegmentCount；
- edge material color/role/width；
- budget result；
- report schema；
- deterministic output digest。

后续测试不得依赖相邻 Toolchain repository 或 Blender。

### 14.3 CLI

- `build` 默认 edges 和 `--skip-edges`；
- custom/built-in budget；
- warning/error/`--warnings-as-errors`；
- report + GLB 原子发布；
- `inspect` source/Prepared；
- `partition` 的全部 LOD/batching 参数和非法组合；
- `partition --force` 恢复旧输出；
- 任意 supported source importer 的 build/inspect/partition；
- no-default build 在不支持工业格式时不注册对应 adapter。

### 14.4 Engine

- Studio 生成的带 edge Prepared GLB 通过 Engine profile validator；
- Engine reader 恢复 line primitive、edge color、width metadata 和 selection；
- 不带 edges 的 Studio import 输出仍可通过 validator；
- `export tileset` 的 part/coarse GLB 可被 Agent Console 加载。

### 14.5 全量门禁

- Rust format、clippy、tests；
- schema 和 TypeScript bindings；
- Viewer unit/typecheck/build；
- Chrome E2E；
- 七种 Source Import parities；
- large-industrial fixture；
- Studio standalone 且相邻 Toolchain repository 不存在；
- dependency audit 为 0 high/critical vulnerability。

## 15. 实施阶段

### Phase 6A：共享 Prepare 与 build（Implemented）

- 实现 AssetMetrics、BudgetProfile、edges、report 和 PrepareArtifact。
- 让 `import` 改走共享 Prepare，默认 edges 关闭。
- 增加 `build` 和 `inspect`。
- 使用旧 Toolchain baseline 完成 build/edge parity。

Phase 6A 已由 Plan 027 交付。`node scripts/compare-prepare-imports.mjs` 连续构建两次
Studio GLB，验证字节确定性，并比较 Toolchain baseline 的 stats、bounds、业务节点身份、
Selection fallback、edge role/width/color、budget 和 report 语义。

### Phase 6B：Pipeline 与 Runtime Tileset

- 扩展 AssetProfile。
- fine/LOD/coarse 生成真实 edges、metrics 和 budget。
- 增加直接 `partition`。
- `export tileset` 改用统一 profiles。

### Phase 6C：文档、性能与最终验收

- Engine 兼容测试。
- large-industrial 性能和资源基线。
- CLI/API/诊断文档同步。
- 最终 full verify、Chrome E2E 和 Studio standalone。
- 明确记录“Studio 已完成 Toolchain 产品能力替代，且不依赖 Blender”。

## 16. 风险与控制

| 风险 | 控制 |
|---|---|
| Edge generation 造成内存膨胀 | 共享 hard limit、逐 primitive 处理、关闭时真实 metrics |
| Import/build 语义分叉 | 二者只能调用同一个 PrepareArtifact 实现 |
| Partition 丢失 edges 或统计错误 | part/coarse 均生成并按实际 GLB 计算 |
| 旧 CLI 参数被悄悄改变 | baseline + CLI contract tests 固定默认值和非法组合 |
| Legacy parity 依赖已删除 Toolchain | 一次性生成本地 baseline，之后运行测试不访问旧仓库 |
| Edge bytes 突破 partition budget | edges 计入 maxBytes、coarse chunk 和总预算 |
| 性能回退 | large fixture wall-clock、memory、bytes 和 p95 门禁 |
| 误称无 Blender 依赖 | 依赖树、源码扫描及 standalone verify 同时检查 |

## 17. 关联文档

- `ICE3-STUDIO-SPEC-016` — [Studio 原生 Asset Pipeline 迁移](2026-11-01-native-asset-pipeline-migration-design.md)
- `ICE3-STUDIO-SPEC-018` — [Studio 原生 JT Import 与 Toolchain 最终退役](2026-11-02-native-jt-toolchain-retirement-design.md)
- `ICE3-STUDIO-ARC-001` — [Studio 架构](../architecture.md)
- `ICE3-STUDIO-REF-001` — [能力清单](../reference/capabilities.md)
- `ICE3-DOC-CTR-001` — [Asset Preparation Contract v1](https://github.com/ice-3d/ice3-doc/blob/main/docs/contracts/asset-preparation-contract-v1.md)
