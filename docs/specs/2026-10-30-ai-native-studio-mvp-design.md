# ICE3 Studio AI-Native Modeling MVP 设计

> Document ID: ICE3-STUDIO-SPEC-001
>
> Status: Historical
>
> Owner: ICE3 Studio maintainers

> **历史状态：** 本文保留已完成设计过程的审计记录，不代表当前生产依赖；当前能力以
> [Asset Pipeline](../reference/asset-pipeline.md) 和 [能力清单](../reference/capabilities.md)
> 为准。

## 1. 背景

ICE3 已有浏览器 Runtime、资产处理 Toolchain、工业语义 Core 和 Agent 应用，但缺少
一个能够从设计意图出发创建和修改模型的 authoring system。

现有大型工业场景的生成依赖人工维护生成代码、昂贵的几何重建和长时间视觉反馈。
这种方式可以产出可运行资产，但没有把 AI 建模需要的能力作为系统的一等公民：

- 设计意图与几何结果没有统一的可编辑模型。
- AI 无法通过稳定的通用 API 安全地多轮建模。
- 几何求值、验证、历史、预览和发布没有形成连续闭环。
- Web 界面和建模执行耦合，难以支持无界面自动化。

ICE3 Studio 从零构建自己的建模内核。它不控制或包装其他建模应用的界面，也不以生成
脚本驱动外部建模应用来隐藏核心实现。行业通用的 DCC 概念、几何处理方式和标准格式可以
作为参考或复用组件，但 Studio 必须拥有自己的文档模型、编辑操作、求值体系、历史记录、
验证和公开 API。

## 2. 目标

MVP 的目标是交付一个本地优先、API-first、AI-native 的最小完整建模系统：

1. 外部 AI 在理解自然语言后，仅通过 Studio API 创建和修改模型。
2. Studio Core 使用 Rust 实现，拥有场景、几何、材质、关系和验证能力。
3. 所有修改经过类型化事务、约束校验和确定性求值。
4. Web Viewer 展示当前 revision、对象选择、验证结果和结构差异。
5. 当前污水处理厂案例被收敛为一个可控的小型完整模块，用于端到端验收。
6. Studio 输出标准 GLB；现有 Toolchain 继续负责 Runtime 优化、LOD 和分区。

## 3. 非目标

MVP 不包含：

- 完整替代通用 DCC 的全部人工建模能力。
- 自由雕刻、动画、绑定、动力学、渲染合成或视频制作。
- 内置模型服务、模型凭据或自然语言理解。
- 云端账号、多租户、组织权限或远程协作。
- 直接编辑现有大型污水处理厂的全部 500 万级以上 Geometry。
- 在 Studio Core 内重新实现 Toolchain 的 Runtime LOD、分区和发布系统。
- 通过任意脚本、Shell 或不可校验代码修改项目。

## 4. 设计原则

### 4.1 API 是产品边界

Studio Core 必须在没有 Web UI 的情况下完成创建、查询、修改、验证、保存和导出。
外部 AI、自动化脚本和 Viewer 使用同一套公开 API，不存在只有 Viewer 能调用的私有路径。

### 4.2 AI 表达建模意图

AI 提交声明式、类型化操作，不直接持有 Rust 内存、GPU 对象或任意执行能力。每个操作都
有稳定名称、输入 Schema、输出 Schema、错误码和确定性语义。

### 4.3 人类保留关键决策

AI 可以规划、生成、验证和修复，人类负责目标确认、关键约束、视觉判断和最终发布。
Studio 不把自然语言回答当作成功证据，只以结构化验证和可重复结果为准。

### 4.4 文档是源数据，几何是派生结果

可变 Scene Graph 不能作为唯一真源。项目保存参数化文档和操作历史，Geometry 由求值器
生成并缓存。AI 的修改可做差异、重放、撤销和回归验证。

### 4.5 本地优先

MVP 默认仅监听 loopback，项目保存于本地目录，不依赖数据库、云服务或账号。API 可以
在未来承载远程客户端，但本地运行不能依赖远程控制面。

## 5. 系统边界

```text
自然语言 / 任务目标
        ↓
外部 AI Agent
        ↓ typed local API
Studio Local Server
        ↓
Rust Studio Core
  ├── project document
  ├── general modeling operations
  ├── geometry evaluation
  ├── materials and transforms
  ├── relationships and validation
  ├── revisions and operation history
  └── persistence and GLB export
        ↓
  evaluated scene / revision events
        ├──→ TypeScript Web Viewer
        └──→ source GLB
                    ↓
           ICE3 Toolchain
                    ↓
       Prepared GLB / Tileset / Engine
```

Studio Core 不依赖 Viewer、Toolchain、Runtime 或外部 AI SDK。Toolchain 是 Studio 输出
之后的独立处理阶段。

## 6. 仓库结构

```text
ice3-studio/
├── Cargo.toml
├── Cargo.lock
├── crates/
│   ├── studio-core/
│   ├── studio-project/
│   ├── studio-server/
│   └── studio-cli/
├── apps/
│   └── viewer/
├── schemas/
├── fixtures/
│   └── wastewater-mini/
├── docs/
│   ├── decisions/
│   ├── guides/
│   ├── plans/
│   └── specs/
└── scripts/
```

### 6.1 `studio-core`

纯领域库，不执行文件 I/O、网络、进程启动或 UI 操作。包含：

- 稳定 ID、单位和数学类型。
- Document、Node、Primitive、Material 和 Relation。
- 通用建模 Operation。
- Geometry evaluator。
- 约束与验证。
- revision 内差异。
- 标准 GLB 的纯数据导出输入。

### 6.2 `studio-project`

负责项目生命周期：

- 打开、保存和关闭本地项目。
- revision 快照与操作历史。
- 内容寻址资源。
- 原子写入、恢复和完整性摘要。
- 项目锁与并发写入保护。

### 6.3 `studio-server`

提供本地 JSON-RPC API 和事件流：

- HTTP JSON-RPC 2.0 请求。
- WebSocket revision 与 validation 事件。
- loopback 绑定、短期本地会话令牌和工作区路径限制。
- 将 API 请求转换为 Studio Core transaction。
- 受控调用 GLB 导出和外部 Toolchain 编排命令。

### 6.4 `studio-cli`

无界面使用入口：

- 启动本地 server。
- 初始化项目。
- 运行验证和导出。
- 重放 operation history。
- 执行确定性检查。

MVP 可以先让 server 负责导出编排；Toolchain 不进入 `studio-core`。

### 6.5 `apps/viewer`

TypeScript 薄客户端，使用 Studio API 获取 revision 信息、对象结构和诊断，通过标准
GLB 预览展示当前求值结果。Viewer 不直接修改 document，也不自行推断工业语义。

## 7. 数据模型

### 7.1 Project

```text
Project
├── schemaVersion
├── projectId
├── currentRevision
├── document
├── operationHistory
├── resources
└── metadata
```

所有 ID 在项目生命周期内稳定且不因显示名称变化而改变。

### 7.2 Document

```text
Document
├── revision
├── scene
├── materials
├── relations
├── constraints
└── metadata
```

Document 使用 SI 单位、右手坐标系和 Y-up。长度 API 输入统一为米。角度使用弧度。

### 7.3 Node

```text
Node
├── id
├── name
├── parentId
├── transform
├── materialId
├── content
├── metadata
└── visibility
```

`content` 在 MVP 中是以下判别联合之一：

- `group`
- `primitive`
- `custom_mesh`
- `sweep`

工业设备、池体或管线不是 Geometry 的操作码，它们通过 metadata、关系和 Assembly
组合表达，避免 Core 被单一行业写死。

### 7.4 Primitive

MVP 支持：

- Box
- Plane
- Cylinder
- Sphere
- TriangleMesh
- Polyline Sweep

所有 Primitive 参数和派生 Geometry 必须有限、单位明确且满足索引范围。

### 7.5 Material

MVP Material 包含稳定 ID、名称、基于 sRGB 的基础颜色、不透明度和可选边线颜色。
材质引用不复制到 Node 内容中。

### 7.6 Relation

MVP Relation 是通用有向关系，包含：

- `id`
- `type`
- `fromNodeId`
- `toNodeId`
- `metadata`

管线连接和设备端口由 Relation 表达。具体工业 relation type 由上层规约设置，不进入
Studio Core 的固定枚举。

## 8. 公共 API

### 8.1 传输

MVP 使用 JSON-RPC 2.0 over HTTP，并在独立 WebSocket 上发布 revision 事件。所有方法
共享一个本地端点，减少 AI 对多个 URL 和 REST 语义的依赖。

API 方法分为：

| 命名空间 | 方法 |
|---|---|
| `system` | `health`, `capabilities` |
| `project` | `create`, `open`, `get`, `save`, `close` |
| `document` | `get`, `apply`, `undo`, `redo`, `history` |
| `scene` | `get`, `query`, `bounds` |
| `validation` | `run` |
| `evaluation` | `evaluate` |
| `export` | `sourceGlb`, `runtimePreview` |

### 8.2 `document.apply`

所有写操作通过一个事务入口提交：

```json
{
  "projectId": "wastewater-mini",
  "expectedRevision": 12,
  "dryRun": false,
  "idempotencyKey": "ai-batch-0042",
  "operations": []
}
```

返回值包含：

```json
{
  "previousRevision": 12,
  "revision": 13,
  "changedNodeIds": [],
  "changedMaterialIds": [],
  "changedRelationIds": [],
  "diff": [],
  "diagnostics": [],
  "stats": {}
}
```

规则：

- `expectedRevision` 不匹配时拒绝事务。
- 相同 `idempotencyKey` 不重复产生 revision。
- `dryRun` 执行完整校验和求值，但不保存操作或变化。
- operations 作为一个原子批次提交。
- 任一 operation 失败时，全部 operations 丢弃。
- API 不返回部分成功。

### 8.3 Operation

MVP Operation 包括：

| Operation | 作用 |
|---|---|
| `node.create` | 创建 group、primitive、mesh 或 sweep Node |
| `node.update` | 更新名称、变换、可见性或 metadata |
| `node.reparent` | 改变父子关系 |
| `node.delete` | 删除无外部引用 Node |
| `primitive.update` | 修改参数化 Primitive 参数 |
| `mesh.replace` | 替换自有 TriangleMesh 数据 |
| `material.upsert` | 创建或修改 Material |
| `material.assign` | 给 Node 分配 Material |
| `relation.create` | 创建有向关系 |
| `relation.update` | 更新关系 metadata |
| `relation.delete` | 删除关系 |
| `constraint.upsert` | 创建或修改空间约束 |
| `constraint.delete` | 删除空间约束 |

每个 Operation 使用稳定 `operationId` 和判别 `type`。未知 Operation、额外字段和无效
单位必须被拒绝，不能静默忽略。

### 8.4 查询

`scene.get` 返回当前 revision 的完整可序列化场景。

`scene.query` 支持按 Node ID、parent、metadata、Relation 和 bounds 查询。结果有稳定
排序，不依赖 HashMap 遍历顺序。

### 8.5 Schema

每个公开 method、operation、diagnostic 和资源都由 Rust 类型生成：

- JSON Schema
- AI tool definition
- TypeScript declaration

生成文件位于 `schemas/`，CI 检查生成文件是否与 Rust 类型一致。

## 9. 事务与求值

提交流程固定为：

1. 校验 method 参数和 API schema。
2. 检查项目锁、`expectedRevision` 和 `idempotencyKey`。
3. 克隆当前 Document 的临时工作副本。
4. 按顺序应用 Operation，并验证每个中间引用。
5. 验证 Document：类型、单位、引用、循环、网格、关系和空间约束。
6. 增量求值变更子树和必要 Geometry cache。
7. 计算确定性 diff、统计和摘要。
8. `dryRun` 在此返回；否则原子写入新 revision。
9. 发布 revision event。

失败在步骤 1–7 中发生时，当前 revision 和磁盘项目保持不变。

几何求值必须可增量执行。未变化的子树直接复用已求值 Geometry；修改 Node 只失效其后代
和依赖相关关系的派生数据。

MVP 的默认空间验证只拒绝失效几何、零尺寸 Primitive、非法变换和循环层级。对象之间的
重叠不默认报错，因为工业 Assembly 和装饰部件允许有意穿插。只有显式
`no_overlap` constraint 覆盖的 Node 集合才执行 bounds 冲突检查。

## 10. 持久化

项目目录结构：

```text
project/
├── project.json
├── revisions/
│   ├── 00000001.json
│   └── 00000002.json
├── history/
│   └── operations.jsonl
├── resources/
│   └── <content-digest>
└── cache/
    └── preview/
```

要求：

- `project.json` 保存 schema、current revision、项目锁摘要和元数据。
- revision snapshot 是规范化 JSON，不包含临时 ID 或路径。
- history 只追加，不就地重写。
- 大资源通过内容摘要引用。
- 写入使用同目录临时文件与原子 rename。
- 打开项目时验证 schema、revision 链和资源摘要。
- 损坏的未完成 revision 不替换最后一个有效 revision。

项目路径必须位于启动时配置的工作区根目录内。API 不接受任意绝对路径或父目录穿越。

## 11. GLB 导出与 Viewer

Studio Core 生成标准 GLB 所需的数据：

- Scene hierarchy
- triangulated Geometry
- transforms
- Material colors
- stable node and relation metadata

MVP 不把 Studio 专属格式写入 GLB。Viewer 预览路径为：

1. 当前 revision 请求 `export.runtimePreview`。
2. Studio 生成临时 source GLB。
3. 本地编排器调用现有 Toolchain 生成 Prepared GLB。
4. Web Viewer 使用现有 Engine 加载 Prepared GLB。
5. revision event 触发 Viewer 切换到新预览。

Toolchain 处理失败时，Viewer 保留上一份有效预览，并显示结构化诊断。Studio Core 不
启动 Node.js 进程；编排由 `studio-server` 的独立 adapter 或 MVP build script 完成。

后续版本可以增加增量 Geometry transport，但不改变 Document API 和模型身份。

## 12. Web Viewer

Viewer 提供：

- 3D 场景展示、相机控制和对象选择。
- 项目结构树、Node 属性和 Relation 查询结果。
- 当前 revision、验证状态和诊断。
- 操作日志和 dry-run 差异。
- 手动触发重新加载和导出。

Viewer 不提供自由代码输入框，不直接调用 Toolchain，不持有 AI 凭据。API 调试界面只能
调用公开 Studio method。

## 13. 安全边界

- HTTP 和 WebSocket 默认只绑定 `127.0.0.1`。
- 启动时生成短期本地会话令牌；写 API 必须携带令牌。
- 项目访问限制在配置的 workspace root。
- Operation 不接受文件路径、URL、Shell、脚本或任意表达式。
- 外部资源导入在 MVP 后单独设计，必须经过专用 importer。
- Toolchain adapter 只接受 Studio 生成并验证通过的 staging 输出。
- AI 会话、凭据、用户名和本地路径不写入 GLB metadata。

## 14. Diagnostic

错误使用稳定 code、message、operation ID、node ID、JSON Pointer 和 details。

MVP code 至少包括：

| Code | 含义 |
|---|---|
| `ICE3_STUDIO_API_INVALID` | JSON-RPC 或 method 参数无效 |
| `ICE3_STUDIO_API_VERSION_UNSUPPORTED` | API 版本不兼容 |
| `ICE3_STUDIO_AUTH_REQUIRED` | 缺少有效本地会话令牌 |
| `ICE3_STUDIO_PROJECT_NOT_FOUND` | 项目不存在或不可访问 |
| `ICE3_STUDIO_PROJECT_LOCKED` | 项目已被其他写入者占用 |
| `ICE3_STUDIO_REVISION_CONFLICT` | expected revision 已过期 |
| `ICE3_STUDIO_OPERATION_INVALID` | Operation 类型或输入无效 |
| `ICE3_STUDIO_REFERENCE_INVALID` | Node 或 Material 引用无效 |
| `ICE3_STUDIO_GEOMETRY_INVALID` | 网格或 Primitive 无效 |
| `ICE3_STUDIO_CONSTRAINT_FAILED` | 关系或空间约束失败 |
| `ICE3_STUDIO_BUDGET_EXCEEDED` | 项目限制超限 |
| `ICE3_STUDIO_EXPORT_FAILED` | source GLB 导出失败 |
| `ICE3_STUDIO_RUNTIME_PREVIEW_FAILED` | Toolchain 预览失败 |
| `ICE3_STUDIO_PERSISTENCE_FAILED` | 项目原子写入失败 |

Diagnostic 不包含堆栈、凭据或工作区外路径。

## 15. 验证与测试

### 15.1 Rust

- Document、Operation、Geometry 和 Material 单元测试。
- 索引、有限值、单位、循环、删除引用和边界条件测试。
- 确定性 ID、排序、revision digest 和 golden GLB 测试。
- 随机合法 Operation 序列的属性测试。
- 100 次 Operation 后撤销与重放一致性测试。

### 15.2 API

- JSON-RPC 参数校验和稳定错误码。
- `dryRun` 不写文档或 history。
- 相同 idempotency key 只提交一次。
- revision 冲突和双写入锁。
- 工作区路径穿越和无效令牌拒绝。
- WebSocket revision 事件顺序。

### 15.3 端到端

使用一个不依赖真实模型凭据的确定性 AI 测试客户端：

1. 读取 `system.capabilities` 和生成 Schema。
2. 创建小型污水模块。
3. 提交错误操作并读取 Diagnostic。
4. 使用 `dryRun` 修正冲突。
5. 提交有效事务。
6. 撤销、重做和重新打开项目。
7. 导出 GLB，经 Toolchain 处理后在 Viewer 展示。
8. 校验 reload 后 revision、Node ID 和 Geometry digest 一致。

## 16. MVP 验收场景

唯一初始 fixture 为 `wastewater-mini`。确定性 AI 客户端通过 Studio API 创建：

- 1 个地坪。
- 2 个曝气池。
- 2 个二沉池。
- 2 台泵。
- 至少 4 条连接关系及对应 Sweep 管线。
- 至少 6 个 Material。

验收必须满足：

- 全程不使用 Studio 私有 UI 操作或直接修改网格内存。
- 所有写操作都通过一个或多个 `document.apply` 事务完成。
- 至少一个错误操作被结构性拒绝且不改变 revision。
- `dryRun` 与正式提交产生相同验证和 diff。
- 撤销后重新求值与上一 revision 字节一致。
- 项目关闭后重新打开，稳定 ID、关系和 revision 不变。
- Viewer 显示当前 revision，并能选择和检查所有设备和管线。
- GLB 经现有 Toolchain 处理后可以加载，Runtime Selection ID 保持稳定。

## 17. 性能目标

目标设备为当前开发用 Apple Silicon Mac，使用 release build，记录结果：

- 本地 Server 冷启动到 `system.health` 成功不超过 3 秒。
- 打开 100 Node 级 fixture 不超过 1 秒。
- 100 个 Operation 的 apply、validation 和 diff p95 不超过 500 ms。
- `wastewater-mini` 增量求值 p95 不超过 300 ms。
- source GLB 导出不超过 2 秒。
- Viewer 从 revision event 到新预览可交互不超过 10 秒，Toolchain 处理单独记录。

性能测试输出机器可读 JSON，不使用单次手工观感作为结论。

## 18. MVP 里程碑

### M0：工程与 API 骨架

- Rust workspace 和 crate 边界。
- JSON-RPC server 生命周期。
- Schema 生成和 API integration harness。

### M1：Document 与通用几何

- 稳定 ID、单位、Node、Material、Relation。
- Box、Plane、Cylinder、Sphere、TriangleMesh 和 Sweep。
- 参数化求值和基础空间验证。

### M2：事务与持久化

- `document.apply`、`dryRun`、undo/redo。
- revision、operation history 和原子项目目录。
- 冲突、幂等和恢复测试。

### M3：导出与 Viewer

- source GLB 导出。
- Toolchain preview adapter。
- TypeScript Viewer 展示 revision、结构、选择和诊断。

### M4：AI 驱动验收

- `wastewater-mini` 确定性 AI 客户端。
- 错误修复、并发冲突、reload 和性能测试。
- 完整端到端验收和文档。

## 19. 风险与约束

| 风险 | 约束和对策 |
|---|---|
| 第一版 Geometry 能力过大 | 只实现通用 Primitive、TriangleMesh 和 Sweep；布尔、NURBS、雕刻后置 |
| AI 产生大量无效操作 | 强类型 Schema、dry-run、稳定 Diagnostic、事务拒绝和 revision 冲突 |
| Viewer 反向侵入 Core | Viewer 只消费 API；Core 不依赖浏览器或前端类型 |
| 预览链路过慢 | 增量求值、Geometry cache、标准 GLB 和现有 Toolchain；先测后优化 |
| 项目文件损坏 | revision 链、内容摘要、临时写入和原子 rename |
| 本地 API 被其他进程滥用 | loopback、短期令牌、workspace root 和受限 Operation |
| Studio 变成污水处理厂专用工具 | 通用 Core 与工业 assembly/metadata 分层 |

## 20. 已确认决策

- Studio Core 使用 Rust，不使用 Node.js。
- API-first，Web Viewer 只做展示和审查。
- 外部 AI 负责自然语言理解，Studio 不内置模型或凭据。
- Document 和操作历史是源数据，Geometry 是派生结果。
- 通用 3D Primitive 是核心，工业对象通过组合和 metadata 构建。
- 本地优先，HTTP JSON-RPC + WebSocket 事件。
- 标准 GLB 作为互操作输出，现有 Toolchain 负责 Runtime 优化。
- `wastewater-mini` 是唯一 MVP 验收 fixture。
