# ICE3 Studio 架构

> Document ID: ICE3-STUDIO-ARC-001
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

ICE3 Studio 是本地优先的 AI-native authoring workspace。自然语言理解和 Agent 规划留在
Studio 外部；Studio 通过公开 API 接收类型化 Operation，把提案、人工修改、求值、审阅和
持久化组织为可重放的 Document revision。

## 边界

```text
外部 Agent / 用户
        |
        | JSON-RPC 2.0
        v
Studio Server（仅 loopback）
        |
        +---- studio-core
        |     Document、Operation、校验、几何、GLB
        |
        +---- studio-assets
        |     glTF/GLB、Prepared Profile、accessor、source digest
        |
        +---- studio-pipeline
        |     partition、LOD、coarse、Tileset、render batch
        |
        +---- studio-import
        |     glTF/GLB、OBJ/MTL、FBX、USD、STEP、IFC、JT、SourceScene
        |
        +---- studio-project
        |     revision、锁、ChangeSet、缓存
        |
        +---- studio-gpu
        |     实验性 wgpu 几何求值
        |
        +---- revision 事件 ----> Web Viewer
```

Studio Core 不依赖 Viewer、Engine、Toolchain、Node.js 或模型供应商 SDK。Node.js 只用于
Viewer 构建和历史 parity 脚本。Runtime preview、Asset preparation、partition、LOD 和
Tileset 全部由 Studio 原生 crate 完成，不启动 Node Asset runtime 或 Toolchain Sidecar。

## 组件

### `studio-core`

拥有稳定 Document 类型、通用 Operation、primitive/sweep Geometry、Material、relation、
constraint、Diagnostic、Document diff 和标准 GLB 序列化。它没有文件、进程、网络或 UI
访问权限。

`large_scene` 模块拥有 Component Definition、Instance、InstanceSet、参数解析、端口感知
Connection 和 CPU 参考展开。派生 Geometry 和 identity 必须确定性生成，行业术语只能进入
metadata 或案例资产。

Material 使用 RGBA base color、opacity、可选 edge color 和可选 `directional`、`flat` 或
`water` shading。opacity 小于 1 时 GLB 导出为 alpha blend；shading 写入
`extras.ice3.shading`。

### `studio-project`

拥有项目路径、写锁、原子 revision snapshot、undo/redo、ChangeSet、内容寻址 Geometry
cache 和项目完整性检查。新 revision 只有在 snapshot 持久化完成后才对外可见。

### `studio-assets`

读取 GLB/glTF、解析多 buffer 与 accessor、验证 `ice3-prepared` profile，并在允许的
base directory 内解析外部资源。Draco primitive 在读取阶段物化为普通 accessor；绝对
路径、URI scheme、`..`、symlink escape、畸形 chunk 和越界 accessor 均失败关闭。

### `studio-import`

定义格式无关的 `SourceImporter`、registry 和 `SourceScene`。当前 adapter：

- `gltf`：`.gltf`、`.glb`，保留层级、共享 Mesh、Material、extras 和 Selection。
- `obj`：`.obj`，Rust parser 支持 OBJ face、负索引、多边形三角化、object/group 和
  同目录 MTL。
- `fbx`：`.fbx`，native decoder 在隔离 worker 中保留 ASCII/Binary 层级、transform、
  shared Mesh、颜色、实例和有界 custom properties。
- `usd`：`.usda`、`.usdc`，native stage 在隔离 worker 中保留 prim path、层级、
  instance/prototype、Geometry、颜色和有界 metadata。
- `step`：`.step`、`.stp`，原生 AP242 reader 在隔离 worker 中保留 product/assembly
  层级、occurrence 矩阵、shared Mesh、逐面颜色、Selection 和单位归一。
- `ifc`：`.ifc`，原生 IFC4X3 parser/geometry pipeline 在隔离 worker 中保留空间层级、
  产品、Pset/Qto/System、shared instance、颜色、Selection、单位和 bounded metadata。
- `jt`：`.jt`，Studio isolated worker 调用 host-owned converter，验证标准 GLB 后保留
  层级、共享实例、材质、JT metadata、Selection 和 shared source origin。

adapter 不写 Tileset；`studio-assets::canonical` 把 `SourceScene` 写成 Prepared GLB。
重要安全边界：外部资源只能读取源文件目录内文件。FBX/USD/STEP/IFC/JT worker 使用硬 timeout、
进程崩溃隔离、response/stderr 上限和 post-decode scene budgets；USD composition 额外
限制递归深度、layer 数量、绝对路径、URI、`..` 和 symlink escape。任何超时、崩溃、
协议污染或越界资源都失败关闭。

### `studio-pipeline`

拥有 partition unit 分析、确定性的空间切分、meshopt LOD、coarse chunk、可选 semantic
proxy、静态 render batch 和 Tileset document。共享 Mesh 的实例保持同一 atomic partition
unit；LOD 保留稳定 selection identity，简化失败时保留原几何。

### `studio-gpu`

拥有本地 `wgpu` adapter/device、生命周期、受预算约束的 buffer 和 WGSL kernel、CPU/GPU
parity 以及执行证据。它不读取项目、不提供 HTTP、不批准 revision，也不替代浏览器渲染。
CPU 是权威求值路径；Native GPU 和 resident geometry 保持实验性、默认关闭。

### `studio-server`

拥有 loopback HTTP/WebSocket 边界、认证、参数校验、单事务串行、持久化和 revision 事件。
阻塞文件系统与 native preview 工作使用 Tokio blocking worker，避免阻塞 health 和事件流。
Compute service 负责 GPU 内存、并发、优先级、取消、健康检查和 CPU fallback。

### Native preview

`PreviewAdapter` 直接消费 Studio Core 求值得到的 GLB：

```text
Studio Server
└── PreviewAdapter
    └── Studio Core evaluated GLB
        └── cache/preview/<revision>.glb
```

committed preview 使用原子发布并返回文件 SHA-256。candidate source 位于
`cache/candidates/<digest>-source.glb`，candidate preview 位于
`cache/preview/candidates/<digest>.glb`；缓存响应重新读取实际字节计算 digest。
不存在外部优化器 fallback、Sidecar 探测或 capability 协商。

### `studio-cli`

启动本地 Server，生成确定性的 JSON Schema/TypeScript declaration，提供 project pack
和 unpack、Source Import/validate，并从 committed revision 发布
`agent-console-v1` Runtime bundle。

### Web Viewer

只消费公开 API 和 revision 事件，不直接修改 Document。默认打开 `Agent` tab；在同一个
三栏工作台中提供 Agent 上下文、3D Scene、Objects、Inspect、Changes 和 Evidence。

Objects 搜索会保留匹配项祖先并显示匹配数。Selection 以完整业务实体或 source object
为单位；Reference Asset 的派生 selection 会映射回同一 binding。用户点击对象后先更新
Agent 焦点，只有点击 `Inspect selected object` 才进入人工参数编辑。

Agent Gateway 不可用时界面显示离线状态，但对象搜索、参数检查、candidate review 和人工
提交保持可用。Viewer 不自带模型凭据、Prompt 或 Agent policy。

## 真源与派生值

可序列化 `Document` 加 operation history 是持久真源。Component Definition、Instance、
InstanceSet 和 Connection Intent 都是 source data。Triangle Mesh、part identity、world
bounds、Diagnostic、GLB、runtime preview、Tileset、coarse tile 和 binding manifest
都是派生值。

每次成功事务只增加一个 revision。稳定 object ID 在 rename、reload、undo 和 redo 后保持
不变。Operation history 与 revision snapshot 共同提供可重放和可审计状态。

Component parameter 和 part binding 使用 untagged wire format：标量直接写数字，Vector3
直接写 `{x,y,z}`，Color 直接写四元素数组，Enumeration 直接写字符串。参数引用使用
`{param:"name"}`，派生标量使用 `{expr:"radius - wall / 2"}`。

## 大规模场景与 GPU

CPU revision-scoped scene cache 默认开启，并复用已解析的 scene index。实验性 Native GPU
层增加 resident topology、instance buffer、稳定 handle 和增量求值，但当前设备上没有稳定
的性能 crossover，因此默认仍然使用 CPU。

Preview packet 只有在 host 明确开启时发布。客户端验证完整 packet 后原子复用未变化的
buffer，失败时回退到 canonical GLB。GPU scene visibility 也提供 CPU fallback；GPU
handle、device generation 和 buffer slot 不能泄漏为公共 object identity。

静态 Schema 固化 37 个 JSON-RPC 方法；运行中的 Server 另外公布
`evaluation.waitScene` 和 `evaluation.visibleInstances`，共 39 个方法。后者属于实验性
能力，调用前应读取 `system.capabilities`。

## Agent capability

`system.agentManifest` 描述可用 tool、当前 token authority 和调用上限；
`scene.context` 返回固定大小摘要；`scene.search` 提供 revision-bound 分页结果；
`evaluation.query` 只读取显式列出的派生 Geometry。所有读接口复用同一 Document 和缓存。

Agent proposal 必须经过 ChangeSet。`changeset.impact` 返回 local、targeted、broad、
full_rebuild 或 unavailable 的门禁与缓存证据，但不会发布 candidate Scene。提交后使用
`evaluation.stats` 读取 committed revision 的真实执行结果。

## 行业分层

Core primitive 和 Operation 保持行业中立。工业概念使用 metadata、relation、component
assembly 和案例资产表达。`wastewater-mini` 是 11 节点确定性契约 fixture；默认 authoring
工程是 `projects/wastewater-plant`；大型程序化工业 fixture 用于验证组件、实例、连接、
缓存和性能和规模边界。

Studio 启动时扫描 `projects/*/project.json`，默认加载修改时间最新的工程；时间相同时选择
更高 `currentRevision`。权威能力清单位于
[能力清单](reference/capabilities.md)，当前文件与代码冲突时以代码和该清单为准。

## 安全边界

- HTTP 和 WebSocket 只绑定 `127.0.0.1`。
- 写请求需要进程级 bearer token。
- ChangeSet commit 需要审批 Cookie，并与 candidate digest 绑定。
- 项目和 Resource 限制在配置的 workspace。
- Operation 不能提交路径、URL、shell command、脚本或任意表达式。
- JT converter 只来自 host-owned `ICE3_JT_CONVERTER`；API 请求不能选择 executable、
  参数、环境变量或 shell fragment。
- Diagnostic 不包含凭据或 workspace 外路径。

这套边界是单用户本地工具的安全模型，不是多租户 remote deployment 模型。

## 尚未实现

以下能力在产品范围但尚未交付：

- Boolean geometry，包括 cut、union 和 intersect。
- Free-form curve 与 curve-driven sweep。
- Sculpting 和有机网格编辑。
- 协作编辑与并发多用户 session。
- 云租户、账号和远程部署。
- 大型静态 Asset 作为直接建模输入。
- 跨项目持久 GPU buffer pool。

Runtime LOD、空间 partition 和 GLB Runtime Asset 发布已由 Studio 原生 crate 实现。
glTF/GLB、OBJ/MTL、FBX、USD、STEP、IFC 与 JT Source Import 已迁入 Studio。
`studio-import` 的 JT adapter 在 isolated worker 中调用 host-owned converter，验证
标准 GLB，并在 `f64` 共享原点转换后生成 `SourceScene`。Toolchain 不再拥有 Source
Import 生产 adapter 或外部 DCC bridge。Sidecar/client/UI 已退役；Toolchain repository
的最终源码清理与归档准备仍属于 Phase 5C。

## 关联文档

- `ICE3-STUDIO-RM-001` — [Studio README](../README.md)
- `ICE3-STUDIO-REF-001` — [能力清单](reference/capabilities.md)
- `ICE3-STUDIO-REF-002` — [API Reference](reference/api.md)
- `ICE3-STUDIO-SPEC-014` — [Agent 主导工作台设计](specs/2026-11-01-agent-led-workbench-design.md)
- `ICE3-STUDIO-SPEC-016` — [Studio 原生 Asset Pipeline 迁移](specs/2026-11-01-native-asset-pipeline-migration-design.md)
- `ICE3-DOC-ADR-006` — [Agent 主导的 Studio 工作台](https://github.com/ice-3d/ice3-doc/blob/main/docs/decisions/0006-agent-led-studio-workbench.md)
