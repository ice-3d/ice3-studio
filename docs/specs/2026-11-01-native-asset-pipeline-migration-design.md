# Studio 原生 Asset Pipeline 迁移设计

> Document ID: ICE3-STUDIO-SPEC-016
>
> Status: Approved
>
> Owner: ICE3 Studio maintainers

> **Phase 状态：** Phase 1 至 Phase 5B 已实现，JT Source Import、原生 preview 和
> Studio standalone 已迁入 Studio。Toolchain repository 的最终源码清理与归档准备仍在
> Phase 5C，因此整份迁移设计继续保持 `Approved`。

## 1. 目标

把当前 Toolchain 的全部产品能力迁入 Studio，使 Studio 成为唯一的 Asset import、
prepare、partition、LOD、Tileset 和 Runtime publication 入口，并最终删除独立
Toolchain repository、Node Toolchain runtime 和 Toolchain Sidecar。

迁移必须保留格式能力、确定性、内存预算、原子发布、Diagnostic 和现有模型语义。迁移
以“功能等价、契约明确、可分阶段验收”为准，不要求第一版一次重写所有格式解析器。

## 2. 最终架构

```text
ICE3 Studio repository
├── studio-core
│   └── Document、Operation、Geometry、Material
├── studio-assets
│   └── glTF / GLB、Prepared Profile、validation、canonical writer
├── studio-pipeline
│   └── partition、LOD、semantic proxy、render batch、Tileset
├── studio-import
│   └── SourceImporter trait 与格式 registry
├── studio-project
│   └── revision、ChangeSet、cache、atomic publication
├── studio-server
│   └── JSON-RPC、auth、export orchestration
├── studio-cli
│   └── import、validate、export、serve、pack
└── apps/viewer
```

Studio 统一产品入口：

```text
source model → Studio import → Studio assets
             → Studio pipeline → Tileset
             → Studio export / Agent Console
```

最终删除：

- `ice3-toolchain` repository。
- `studio-toolchain-client`。
- Toolchain Sidecar process 和 NDJSON contract。
- Toolchain npm package。
- Toolchain Node runtime 和 TypeScript source pipeline。
- 外部 DCC authoring 与 import bridge。

## 3. 依赖策略

Studio 的建模 Kernel 继续使用 Rust。允许 Rust crate 链接或调用成熟的 C/C++ native
library，以覆盖工业格式和几何算法。

依赖必须满足：

- 构建期或显式 feature 可关闭。
- 许可证可审计并进入第三方声明。
- 不在浏览器 Runtime 中加载。
- 不通过自然语言请求动态选择库、插件或 executable。
- failure 必须映射为稳定 Studio Diagnostic。
- 同一输入、版本和配置产生确定性输出。

首阶段优先纯 Rust 或成熟 native binding；不要求为了“纯 Rust”重新实现 STEP、USD 或
JT 编译器。

## 4. 新 crate 边界

### `studio-assets`

负责 glTF/GLB 读取、Prepared Profile、accessor、mesh、material、node、metadata、
validation、canonical writing 和 source digest。

不负责空间 partition、LOD 或业务审批。

### `studio-pipeline`

负责 partition unit analysis、spatial tree、triangle/byte budgets、LOD allocation、
geometry simplification、coarse chunks、semantic proxy、render batching、Tileset
document 和 artifact validation。

不读取 project lock、不修改 Document、不处理 HTTP。

### `studio-import`

定义：

```rust
pub trait SourceImporter {
    fn id(&self) -> &'static str;
    fn extensions(&self) -> &'static [&'static str];
    fn capabilities(&self) -> SourceCapabilities;
    fn import(&self, request: &ImportRequest) -> Result<SourceScene, ImportError>;
}
```

每个格式 adapter 输出标准 source GLB 或统一的 Source Document，随后进入
`studio-assets`。adapter 不直接发布 Prepared GLB 或 Tileset。

## 5. 格式迁移策略

| 格式 | 首选实现 | 迁移阶段 |
|---|---|---|
| glTF / GLB | Rust glTF reader + 现有 Prepared profile | 1 |
| OBJ / MTL | Rust parser，保留安全 sidecar 与单位 warning | 2 |
| FBX ASCII/Binary | Studio native parser，替换外部 DCC bridge | 3 |
| USD ASCII/Binary | Studio native stage library，替换外部 DCC bridge | 3 |
| STEP AP242 | native CAD geometry kernel，替换 WASM adapter | 4 |
| IFC 4.3 | native IFC geometry engine，替换 WASM adapter | 4 |
| JT | 可选受控 native converter adapter；如无可用 parser，保持外部 converter | 5 |

允许 feature-gated dependency。默认桌面开发构建至少启用 Studio GLB、glTF 和 OBJ；
工业格式 feature 在对应阶段交付。

### 格式等价要求

- 保留原有 extension 选择与显式 adapter 选择。
- 保留层级、实例、基础颜色、Selection ID 和可序列化 metadata。
- 保留每格式现有 Diagnostic code 或提供明确迁移别名。
- 保留预算、warning、warning-as-error 和原子发布语义。
- 原 Toolchain fixture 在迁移期作为 parity corpus，不直接复制到发布 package。

## 6. 统一 CLI

最终命令面：

```text
ice3-studio import SOURCE --output FILE --asset-id ID
ice3-studio validate FILE
ice3-studio partition SOURCE --output DIRECTORY --asset-id ID
ice3-studio export tileset --project PROJECT --output DIRECTORY
ice3-studio serve ...
```

`import` 调用 `studio-import`，`validate` 调用 `studio-assets`，`partition` 和
`export tileset` 调用 `studio-pipeline`。任何 command 都不启动 Node Sidecar。

## 7. Runtime Tileset 发布

最终 `export tileset` 输出：

```text
output/
├── tileset.json
├── parts/
├── coarse/
├── bindings.json
└── export.json
```

Studio 负责 committed revision、binding aggregation、manifest、profile 和 atomic
publication；`studio-pipeline` 负责 tile/LOD 计算。输出不包含完整 source GLB。

## 8. 迁移阶段

### Phase 1：Native GLB 与 Tileset pipeline

**状态：Implemented（2026-11-01）。**

- 新建 `studio-assets` 与 `studio-pipeline`。
- 迁移 glTF/GLB source reading、Prepared Profile、validation、partition、LOD、
  coarse chunk、semantic proxy、render batch 和 Tileset。
- 增加 `export tileset`，生成 bindings 和 export manifest。
- Agent Console 可以消费 Studio 导出的 Tileset。
- 不删除 Toolchain，先用 parity tests 做双跑比较。

### Phase 2：glTF 与 OBJ

**状态：Implemented（2026-11-02）。** glTF/GLB adapter、OBJ/MTL Rust parser、CLI
`import`/`validate` 和 Toolchain OBJ adapter 退役已经交付。Toolchain 继续保留 STEP、
IFC、JT 的迁移兼容能力。

- 将 glTF/GLB importer 正式并入 `studio-import`。
- 实现 OBJ/MTL native importer。
- 删除对应 Toolchain adapter 和 Sidecar operation 依赖。

### Phase 3：FBX 与 USD

**状态：Implemented（2026-11-02）。** `studio-import` 已提供 FBX ASCII/Binary 与 USD
ASCII/Binary native adapter，解析在 Studio 自带 worker 中隔离运行。worker 实现硬
timeout、崩溃隔离、协议和 response/stderr 上限、scene budgets 与确定性 Diagnostic；
USD composition 限制在源目录内，并有递归深度和 layer 数量上限。CLI registry、parity
baseline 与负向 corpus 已交付，Toolchain 的 FBX/USD adapter、runtime dependency 和
conformance entry 已删除。

- 用 Studio native worker 替换外部 DCC bridge。
- 覆盖 ASCII/Binary、层级、实例、颜色、metadata、单位、timeout 和失败关闭。
- 删除外部 DCC 依赖和对应 npm/native bridge。

### Phase 4：STEP 与 IFC

**状态：Phase 4A STEP 与 Phase 4B IFC Implemented（2026-11-02）。**

详细设计见 `ICE3-STUDIO-SPEC-017` — [Studio 原生 STEP 与 IFC Source Import](2026-11-02-native-step-ifc-import-design.md)。

- 接入 native CAD/IFC geometry engine。
- 对齐层级、颜色、单位、Selection identity、Pset/Qto/System metadata。
- 删除 WASM adapter 和对应 Sidecar operation 依赖。

### Phase 5：JT 与 Toolchain 退役

**状态：Phase 5A JT Source Import、Phase 5B Studio standalone Implemented
（2026-11-02）；Phase 5C 待完成。**

JT converter 调用、GLB validation、Selection/metadata、shared origin、隔离 worker、
负向测试和 legacy parity 已迁入 Studio。Sidecar/client/UI/build 依赖已经删除，
Toolchain repository 的最终源码清理仍按 Spec-018 后续阶段执行。

- 将 JT converter 调用迁入 Studio Import adapter。
- 跑完整 parity corpus、release package 和负向测试。
- 删除 `studio-toolchain-client`、Sidecar、Toolchain bundling 和 scripts。
- 在确认 `ice3-studio` main 不引用后归档 Toolchain repository。

## 9. Parity 与验收

每个阶段必须提供：

- 旧 Toolchain 与新 Studio pipeline 对相同 fixture 的统计对比。
- Node/vertex/index/triangle/material/selection count 对比。
- Tileset hierarchy、bounds、URI 和 selection identity 对比。
- 确定性 digest 对比；允许的差异必须记录原因和迁移策略。
- Peak memory、wall-clock 和输出 byte budget。
- 失败输入、损坏文件、越界路径和 unsupported feature 对比。
- 文档、Schema、Diagnostic 和 CLI help 同步。

最终退役门禁：

- Studio 没有任何 `studio-toolchain-client`、Sidecar、Toolchain CLI 或 Node Asset
  runtime 依赖。
- Repository 搜索不再发现生产路径中的 Toolchain package。
- 所有 Source format 通过 Studio native importer 测试。
- Agent Console 使用 Studio 发布的 Tileset。
- Toolchain repository 可以在不损失 Studio 能力的情况下归档。

## 10. 非目标

- 不在第一阶段同时完成全部七种 Source Importer。
- 不把几何处理放进 Browser Runtime。
- 不通过 Agent prompt 动态加载 native plugin 或 executable。
- 不保留两套长期并行的 partition 实现。
- 不要求现有 Toolchain package 在退役后继续发布。

## 11. 关联文档

- `ICE3-STUDIO-ARC-001` — [Studio 架构](../architecture.md)
- `ICE3-STUDIO-SPEC-005` — [Large-Scene Authoring](2026-10-30-large-scene-authoring-design.md)
- `ICE3-STUDIO-SPEC-015` — [已废弃的 Sidecar 导出方案](2026-11-01-studio-runtime-tileset-export-design.md)
- `ICE3-DOC-CTR-001` — [Asset Preparation Contract v1](https://github.com/ice-3d/ice3-doc/blob/main/docs/contracts/asset-preparation-contract-v1.md)
- `ICE3-DOC-CTR-002` — [Toolchain Sidecar Contract v1](https://github.com/ice-3d/ice3-doc/blob/main/docs/contracts/toolchain-sidecar-contract-v1.md)
