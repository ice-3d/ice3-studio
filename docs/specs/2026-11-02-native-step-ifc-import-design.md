# Studio 原生 STEP 与 IFC Source Import 设计

> Document ID: ICE3-STUDIO-SPEC-017
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

> **Phase 状态：** Phase 4A STEP 与 Phase 4B IFC 均已实现（2026-11-02）。

**目标：** 在 Studio 内原生支持 STEP AP242 与 IFC4X3 Source Import，替换 Toolchain 的
WASM adapter，同时保留层级、实例、颜色、单位、Selection identity、Pset/Qto/System
metadata、资源预算、确定性输出和原子发布语义。

**上级设计：** [Studio 原生 Asset Pipeline 迁移](2026-11-01-native-asset-pipeline-migration-design.md)

## 1. 背景

Phase 1 至 Phase 3 已把 glTF/GLB、OBJ/MTL、FBX 和 USD 的 Source Import 迁入
Studio。Phase 4A 完成时，Toolchain 只保留 IFC 和 JT 的迁移兼容能力。

STEP 与 IFC 不能直接复用文本 parser 方案：

- STEP AP242 需要解析 B-rep、装配关系和曲面三角化。
- IFC4X3 需要解析空间层级、产品关系、属性集、数量集、系统和参数化几何。
- 两者都必须在有限资源内失败关闭，不能把第三方解析器的崩溃或越界分配传播到
  Studio Core。

详细实现拆为两个可独立合并的里程碑：

- Phase 4A：STEP AP242。
- Phase 4B：IFC4X3。

拆分原因是两者的引擎、metadata 模型和退役验收互相独立。任一里程碑完成即可独立
替换对应 Toolchain adapter，不需要等待另一项。

## 2. 成功标准

- `ice3-studio import SOURCE --output FILE --asset-id ID --adapter step|ifc` 可在没有
  Toolchain、Node、浏览器 WASM 或外部 DCC runtime 的环境中完成导入。
- Studio 原生输出通过 Prepared GLB validation，并保留现有业务 Selection 和基础
  material 语义。
- STEP assembly/product 层级、实例、逐面颜色和单位与退役前语义一致。
- IFC 空间层级、产品属性、Pset、Qto、System、GlobalId 和共享几何实例与退役前语义
  一致。
- 两种格式都通过旧 Toolchain baseline、业务语义 parity、几何偏差门禁、负向 corpus
  和确定性测试。
- Studio Core 不链接 STEP/IFC parser；解析只在 `studio-import` worker 子进程内执行。
- Studio 默认开发构建支持两种格式，关闭 `step`/`ifc` feature 时依赖图不包含对应
  parser 或 geometry engine。
- 每个里程碑完成后，Toolchain 对应 adapter、依赖和 conformance entry 可以独立删除。

## 3. 非目标

- 不扩展到 STEP AP203/AP214/AP201 的额外产品承诺；迁移验收以当前 AP242 fixture
  和 Toolchain 既有能力为准。
- 不扩展到 IFC2X3、IFC4、IFC4X1 或 IFC4X2；当前产品入口继续只接受 IFC4X3。
- 不把 STEP/IFC 几何处理放进浏览器 Runtime。
- 不根据 Agent 自然语言请求动态加载 parser、plugin 或 converter executable。
- 不保留长期并行的 STEP/IFC Toolchain adapter。
- 不在本阶段迁移 JT 或完成 Toolchain repository 归档。
- 不提供 CAD 编辑能力；本阶段只做 Source Import 和 Prepared GLB 生成。

## 4. 总体架构

```text
ice3-studio executable
└── studio-cli import
    ├── glTF/OBJ: in-process studio-import
    └── FBX/USD/STEP/IFC: isolated import worker
        ├── WorkerRequest
        ├── adapter-specific parser and source mapping
        ├── SourceScene
        └── WorkerResponse
            ↓
studio-assets::canonical
            ↓
Prepared GLB validation
            ↓
atomic output publication
```

共同边界：

- `studio-import` 新增 default-on、可独立关闭的 `step` 与 `ifc` Cargo features。
- STEP/IFC parser 不进入 `studio-core`、`studio-assets` 或浏览器 bundle。
- 两个 adapter 只产出 `SourceScene`。
- `studio-assets::canonical` 继续是 Prepared GLB 的唯一 writer。
- CLI registry 固定顺序扩展为 `gltf`、`obj`、`fbx`、`usd`、`step`、`ifc`。
- `--adapter` 接受 `auto|gltf|obj|fbx|usd|step|ifc`。
- 所有输入在 worker 内解析，父进程负责硬超时、崩溃隔离、response/stderr 上限和
  失败关闭。

## 5. 依赖决策

### 5.1 STEP

采用纯 Rust AP242 reader `oxideav-step 0.0.1`，关闭其 Scene3D registry 输出功能，
只使用 std-only `StepModel`：

- 支持 AP242/AP214/AP203 物理文件、产品结构、实例、STEP B-rep、曲面裁剪、
  颜色、透明度和单位。
- 输出 `Part`、`Occurrence`、`Shape`、`TriMesh`，可保留 product/occurrence 身份。
- MIT 许可；不引入 C/C++ runtime 或外部 executable。
- 现有 `synthetic-skid.step` 冒烟结果：3 个产品、1 个 root occurrence、208 个
  triangle、毫米单位，并正确恢复 `PrimaryTank`、`TransferPump` 和两种颜色。

禁止直接调用该 crate 的 glTF writer。Studio 只消费其 `StepModel` 并映射到统一
`SourceScene`，避免出现第二套 canonical output。

### 5.2 IFC

采用纯 Rust `ifc-lite-core 27.1.0` 与 `ifc-lite-processing 27.1.0`：

- core 提供 IFC STEP parser、schema registry、实体解码、单位和关系读取。
- processing 提供空间/产品 metadata extraction 和原生 geometry pipeline。
- 支持 IFC4X3、空间结构、properties、quantities、systems、materials、openings 和
  `IfcMappedItem` instance。
- MPL-2.0 许可；不修改其 source 文件，并将完整许可和 notice 纳入 Studio 第三方声明。
- 现有 `synthetic-plant.ifc` 冒烟结果：IFC4X3、75 个 entity、2 个 geometry product、
  24 个 triangle、毫米单位、`Primary Pump`/`Buffer Tank` 名称和颜色正确。

禁止调用该 crate 的 Node/WASM wrapper 或 CLI；Studio 只使用 Rust API。

## 6. 公共 Source Import 契约

### 6.1 Feature 与 registry

```toml
[features]
default = ["fbx", "usd", "step", "ifc"]
step = ["dep:oxideav-step"]
ifc = ["dep:ifc-lite-core", "dep:ifc-lite-processing"]
```

`studio-cli` 的同名 features 透传依赖。关闭 feature 时：

- adapter 不注册；
- explicit adapter 返回 `ICE3_STUDIO_IMPORT_ADAPTER_UNAVAILABLE`；
- `cargo tree` 不包含对应 parser/geometry crate。

### 6.2 Source format

`SourceFormat` 增加 `Step` 与 `Ifc`，序列化值分别为 `step` 和 `ifc`。

两种 adapter 都输出：

- canonical SourceScene materials；
- face/primitive 级材质拆分；
- 稳定 node/mesh/material 名称；
- finite positions、合法 indices 和合法 material index；
- BTree 顺序或源序号决定的确定性 warning；
- asset extras 中的格式、schema 和坐标系统信息。

### 6.3 Selection identity

STEP：

```text
<assetId>/step/node/<ordinal>
```

`ordinal` 按最终 `SourceScene.nodes` 的顺序分配，root、occurrence 和独立 part 都必须
先形成稳定节点顺序。

IFC：

```text
<assetId>/ifc/<GlobalId>
```

GlobalId 缺失时使用：

```text
<assetId>/ifc/express/<expressId>
```

重复 GlobalId 时使用：

```text
<assetId>/ifc/<GlobalId>/express/<expressId>
```

并产生确定性 `ICE3_STUDIO_IMPORT_IFC_DUPLICATE_GLOBAL_ID` warning。

### 6.4 单位、朝向和 origin

- 输出统一为米、right-handed、Y-up。
- 格式单位换算只应用一次。
- 旋转/手性转换只应用一次。
- world bounds 在 `f64` 中计算，`SourceScene` 使用共享
  `sourceOrigin=[minX,0,minZ]`。
- 只平移 scene root，不修改每个 source mesh 的局部 positions。
- asset extras 记录原始单位、sourceOrigin 和坐标转换策略。

## 7. Phase 4A：STEP 设计

**状态：Implemented（2026-11-02）。** Studio 原生读取 AP242，解析在 isolated
worker 中运行；Toolchain STEP adapter、WASM 依赖和 conformance entry 已退役，
synthetic fixtures 保留为 parity corpus。

### 7.1 Adapter

```rust
pub struct StepImporter;

impl SourceImporter for StepImporter {
    fn id(&self) -> &'static str { "step" }
    fn extensions(&self) -> &'static [&'static str] { &[".step", ".stp"] }
    fn capabilities(&self) -> SourceCapabilities { /* full hierarchy + instances + colors */ }
    fn import(&self, request: &ImportRequest) -> Result<SourceScene, ImportError>;
}
```

使用 `oxideav_step::ReadOptions`，把 `NativeImportLimits` 映射为 parser、part 和
triangle budgets；tessellation tolerance 固定为 relative `0.001`。

### 7.2 层级和实例映射

- `StepModel.roots` 生成顶层 occurrence nodes。
- `Part.children` 递归展开 occurrence tree。
- 同一个 `Part` 被多个 occurrence 引用时共享一个 `SourceMesh`。
- 没有 occurrence 引用的 part 作为 root 下的独立节点保留。
- occurrence `transform` 转成 canonical `[f32;16]`；父/子矩阵只组合一次。
- STEP 为空、只有 wireframe 或最终无 triangle 时返回
  `ICE3_STUDIO_IMPORT_EMPTY_GEOMETRY`。

### 7.3 Geometry、材质和 metadata

- 每个 `Part` 的 `Shape.mesh` 形成一个 `SourceMesh`。
- 依据 `triangle_faces` 与 `triangle_colours` 拆成 primitive 组，每组一个 canonical
  material。
- 无 triangle override 时使用 shape colour；仍无颜色时使用统一的 neutral material。
- 保留 authored normals；没有 normals 时交给 canonical writer 生成。
- node extras 写入：

```json
{
  "ice3": {
    "step": {
      "schema": "ap242",
      "productId": "...",
      "definitionId": 1,
      "occurrenceId": 2,
      "itemId": 3,
      "layers": []
    }
  }
}
```

空字段省略，数组和 warning 排序稳定。

### 7.4 失败和 warning

- 非 AP242 或损坏 physical file：`ICE3_STUDIO_IMPORT_INVALID_SOURCE`
- 可解析但 schema 不是 AP242：`ICE3_STUDIO_IMPORT_STEP_SCHEMA_UNSUPPORTED`
- 无 triangle geometry：`ICE3_STUDIO_IMPORT_EMPTY_GEOMETRY`
- budget 超限：`ICE3_STUDIO_IMPORT_LIMIT_EXCEEDED`
- library 报告 unsupported representation item：每个 keyword 产生一个
  `ICE3_STUDIO_IMPORT_STEP_UNSUPPORTED_ENTITY` warning
- worker timeout/crash/protocol：沿用现有 worker Diagnostic

## 8. Phase 4B：IFC 设计

**状态：Implemented（2026-11-02）。** Studio 原生读取 IFC4X3，解析在 isolated worker
中运行；Toolchain IFC adapter、runtime dependency 和 conformance entry 已退役，
synthetic fixtures 保留为 parity corpus。

### 8.1 两阶段读取

同一份源字节在 worker 中读取两次，但只读一次磁盘：

1. `extract_data_model(bytes)`：空间树、产品、properties、quantities、relationships、
   materials 和 units。
2. `process(bytes, ProcessingRequest::full_pass().retain_meshes(true).instancing(true))`：
   几何、颜色、openings、instance records 和 coordinate frame。

两次读取共享同一 byte buffer。metadata 和 geometry 以 Express ID 关联。

### 8.2 Schema 和单位门禁

- 只接受 `IFC4X3` schema family。
- 不支持 schema 返回 `ICE3_STUDIO_IMPORT_IFC_SCHEMA_UNSUPPORTED`。
- 缺失、冲突、零值或非有限长度单位返回
  `ICE3_STUDIO_IMPORT_IFC_UNIT_UNSUPPORTED`。
- length unit scale 必须明确存在，不能静默假设 1 米。
- geometry 和 placement 使用已换算的米制坐标。

### 8.3 层级与实体

- 空间树 root 为 `IfcProject`。
- Site、Building、Storey、Space 与 aggregate/containment 关系形成 canonical hierarchy。
- 每个空间节点保留 IFC type、name 和 Express ID。
- 有 geometry 但没有空间归属的产品放在 project 下的“unassigned products”稳定容器。
- 同源 geometry map 复用 `SourceMesh`；`InstanceRecord` 生成独立 occurrence node，
  并共享 template mesh。

### 8.4 metadata

产品 node extras 写入 `extras.ice3.ifc`：

```json
{
  "schema": "IFC4X3",
  "expressId": 18,
  "globalId": "...",
  "ifcType": "IfcPump",
  "name": "Primary Pump",
  "objectType": "...",
  "predefinedType": "...",
  "tag": "...",
  "spatialPath": "Project/Site/Building/Storey",
  "propertySets": [],
  "quantitySets": [],
  "systems": []
}
```

约束：

- propertyPset、quantitySet 按名称和 entity ID 稳定排序。
- property value 只保留 string、finite number、bool 和浅层 primitive array。
- 深层对象、递归引用、EntityRef 和非有限 number 省略，并产生聚合 warning。
- 单个 metadata 数组最多 256 项，depth 最多 8。

### 8.5 Geometry 和材质

- 每个 geometry mesh 对应产品 node。
- `MeshData.origin + positions` 在 `f64` 中形成 world positions。
- color 和 material name 形成 canonical material；同色 material 合并。
- opening/boolean geometry 由原生 pipeline 处理。
- instance template 只生成一个 mesh；occurrence transform 使用格式提供的
  template-relative matrix。
- surface texture 不进入第一阶段；产生确定性 warning。

### 8.6 失败和 warning

- malformed/truncated IFC：`ICE3_STUDIO_IMPORT_INVALID_SOURCE`
- unsupported schema：`ICE3_STUDIO_IMPORT_IFC_SCHEMA_UNSUPPORTED`
- unsupported/conflicting unit：`ICE3_STUDIO_IMPORT_IFC_UNIT_UNSUPPORTED`
- no renderable geometry：`ICE3_STUDIO_IMPORT_EMPTY_GEOMETRY`
- resource budget：`ICE3_STUDIO_IMPORT_LIMIT_EXCEEDED`
- unsupported surface texture、camera、light、skeleton、volume 和非 IFC4X3 schema：
  确定性的 `ICE3_STUDIO_IMPORT_IFC_UNSUPPORTED_RESOURCE` warning

## 9. 资源、安全与确定性

- 沿用默认限制：1,000,000 nodes、20,000,000 triangles、512 MiB estimated decoded
  memory、120 秒硬 timeout。
- STEP parser 和 IFC metadata/geometry 在分配最终 `SourceScene` 之前执行可计算的
  budget preflight。
- worker 的 response、stderr、protocol 和 crash 限制继续生效。
- 所有 parser warning 排序后输出。
- 不按 HashMap iteration order 生成节点、材质或 warning。
- 相同 bytes、同一 crate 版本和同一配置产生字节稳定的 Source/Prepared GLB。
- worker 失败、timeout、格式校验失败或 metadata 错误不得替换已有输出。

## 10. CLI 和发布

CLI：

```text
ice3-studio import SOURCE --output FILE --asset-id ID \
  [--adapter auto|gltf|obj|fbx|usd|step|ifc]
```

隐藏 worker 入口继续使用：

```text
ice3-studio --internal-import-worker REQUEST_PATH
```

非 force 发布使用 `persist_noclobber`；force 发布只在完整 validation 成功后替换
目标文件。

## 11. 测试与 parity

每个里程碑独立提供：

- 旧 Toolchain baseline：
  - STEP：`synthetic-skid.step`、`empty-assembly.step`、`truncated.step`
  - IFC：`synthetic-plant.ifc` 及现有 negative fixtures
- native adapter 单元和 integration tests；
- Prepared GLB validation；
- Node/vertex/triangle/material/instance count；
- Selection IDs、bounds、warnings 和 material RGBA；
- ASCII/deterministic digest；
- 负向输入不得发布输出，已有输出不得被替换；
- `cargo check` / `cargo test` / `cargo tree` 的 feature-isolation evidence；
- Chrome E2E 和全仓 18 道 verification gates。

允许的输出差异包括 tessellation topology、vertex welding、accessor layout、
prepared edge generation 和 byte digest；这些差异必须显式报告。

阻断比较：

- node/mesh count 和业务实体名称一致。
- instance count、Selection IDs 和 material identity 一致。
- bounds 每个分量的绝对误差不超过 `max(0.0001 m, 0.00001 × 该轴 extent)`。
- native/legacy surface area 相对误差不超过 `2%`。
- native/legacy triangle count ratio 必须位于 `0.75..=1.33`；ratio 只作为异常
  tessellation 或几何丢失的护栏，不作为精确拓扑契约。
- 任何设备、空间节点、实例、材质或 geometry-bearing entity 丢失都失败关闭。

非阻断但必须报告：

- triangle count 的具体差异和 ratio。
- vertex count。
- index/accessor ordering。
- GLB byte digest。

## 12. 第三方与许可

- `oxideav-step 0.0.1`：MIT。
- `ifc-lite-core 27.1.0` 与 `ifc-lite-processing 27.1.0`：MPL-2.0。
- 未修改的 MPL source 文件仍受 MPL-2.0 约束；Studio 不改变其文件级许可。
- `THIRD_PARTY_NOTICES.md` 必须包含完整许可、notice、版本和 feature gate。
- 发布 package 不得包含 Node/WASM parser、外部 DCC 或 Toolchain adapter。

## 13. 迁移与退役

Phase 4A：

1. 固化 STEP baseline。
2. 实现 native STEP adapter、worker/CLI 接入和 parity。
3. 删除 Toolchain STEP adapter、依赖和 conformance entry。
4. Toolchain 在 Phase 4A 后保留 IFC、JT，等待 Phase 4B 退役 IFC。

Phase 4B：

1. 固化 IFC baseline。
2. 实现 native IFC metadata/geometry adapter、worker/CLI 接入和 parity。
3. 删除 Toolchain IFC adapter、依赖和 conformance entry。
4. Toolchain 只保留 JT。

两个 milestone 分别可合并，但不得在 native parity 通过前删除对应旧 adapter。

## 14. 验收

Phase 4A 完成条件：

- Studio 原生 `step` adapter 可用。
- STEP AP242 层级、实例、颜色、单位和 metadata parity 通过。
- STEP Toolchain bridge 和 WASM dependency 删除。
- `step` feature 关闭时不链接 STEP parser。

Phase 4B 完成条件：

- Studio 原生 `ifc` adapter 可用。
- IFC4X3 空间层级、产品、Pset/Qto/System、实例和颜色 parity 通过。
- IFC Toolchain bridge 和 WASM dependency 删除。
- `ifc` feature 关闭时不链接 IFC parser/geometry engine。

Phase 4 整体完成条件：

- Studio 默认 registry 为 `gltf`、`obj`、`fbx`、`usd`、`step`、`ifc`。
- Toolchain conformance 只剩 JT。
- 全仓 verification、STEP/IFC parity、负向 corpus 和文档检查通过。
- 不依赖 Toolchain、Node、WASM Source Import、外部 DCC 或动态 plugin。

## 15. 关联文档

- `ICE3-STUDIO-SPEC-016` — [Studio 原生 Asset Pipeline 迁移](2026-11-01-native-asset-pipeline-migration-design.md)
- `ICE3-STUDIO-PLAN-021` — [Phase 3 实施计划](../plans/2026-11-02-native-asset-pipeline-phase-3.md)
- `ICE3-STUDIO-PLAN-022` — [Phase 4A STEP 实施计划](../plans/2026-11-02-native-step-import.md)
- `ICE3-STUDIO-PLAN-023` — [Phase 4B IFC 实施计划](../plans/2026-11-02-native-ifc-import.md)
- `ICE3-STUDIO-ARC-001` — [Studio 架构](../architecture.md)
- `ICE3-STUDIO-REF-001` — [能力清单](../reference/capabilities.md)
- `ICE3-STUDIO-REF-004` — [Diagnostics](../reference/diagnostics.md)
