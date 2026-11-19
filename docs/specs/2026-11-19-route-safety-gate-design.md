# 管线工程安全门禁设计

> Document ID: ICE3-STUDIO-SPEC-033
>
> Status: Proposed
>
> Owner: ICE3 Studio maintainers

## 1. 摘要

`ICE3-STUDIO-SPEC-013` 的阶段二B仍缺少路径碰撞、道路净空、穿墙位置和支吊架间距
校验。当前“连接有效”只证明端点、介质和端口存在，不能证明管线可工程评审。

本设计先交付阶段二B的安全门禁：在 Document 写入 revision 前，对正式求值将使用的
路由中心线执行实体碰撞、净空面和支撑布局检查。设计不自动修路，也不修改 Document；
失败时返回稳定 Diagnostic，由 Agent 或人工提交修正后的 ChangeSet。

穿墙套管、三通和异径管几何是同一阶段的前置依赖，但在本设计之外单独交付。本设计先
确定路径和支撑的可验证事实，避免把工程校验与附件建模混在一次变更中。

## 2. 目标

- 在候选事务或 ChangeSet 提交前拒绝穿过实体障碍的管线。
- 检查标记为净空面的道路、硬化地坪等对象的上方净空。
- 允许管线在受控接入距离内进入起止设备，但路径后段再次穿入必须失败。
- 按连接 `lineId` 检查非埋地管线的支撑缺失、间距和转弯附近支撑。
- 所有诊断携带 connection/source node ID、障碍 node ID、JSON Pointer 和量化距离。
- 对同一 Document 产生确定性的路径、命中顺序和 Diagnostic 顺序。
- 为后续套管、三通、异径管和附件对齐提供稳定的路由事实。

## 3. 非目标

- 自动重新布线、障碍规避寻路或几何修正。
- 水力计算、管径选型、泵扬程或处理能力校核。
- B-rep 精确距离、连续碰撞检测和运动包络。
- 生成穿墙套管、三通或异径管实体；这些由后续附件设计交付。
- 把道路、池体、泵站或其它行业词汇写入 Core 类型系统。
- 取代 `scene.clashes`。前者是通用场景查询，本设计是提交事务前的路由专用门禁。

## 4. 当前缺口

| 缺口 | 当前事实 | 后果 |
|---|---|---|
| 无路径碰撞校验 | `route_connection` 只检查端点、介质和几何退化 | 管线可穿过池体、建筑或其它管线 |
| 无道路净空契约 | `routeObstacle`/`routeClearance` metadata 不存在 | 跨越道路时没有可验证的最小净空 |
| 无端点接入边界 | 起止设备本身会包住端口 | 若直接做碰撞会把正常插入误报为碰撞 |
| 支撑只生成一个中点 | 资产脚本使用最长水平段的一个中点 | 长管线缺少连续支撑，转弯和阀门附近可能无支撑 |
| 无支撑间距校验 | 资产测试只检查支撑梁是否贴近近似路径 | 管线可以缺少支撑但仍通过全部门禁 |
| 路由近似重复 | Core 与资产脚本分别实现相似的粗路由 | 校验和正式求值容易漂移 |

## 5. 架构

新增 `studio-core::large_scene::route_safety`。`validate_large_scene` 在结构校验完成后
调用它，`validate_document` 保持现有纯函数边界。

```text
Document
  ├── ConnectionIntent + resolved ports
  ├── component/instance part bounds
  ├── static primitive bounds
  └── support node metadata
          │
          ▼
  route_safety preflight
  ├── centerline from route_connection
  ├── obstacle index
  ├── broad phase
  ├── candidate-only narrow phase
  ├── clearance surfaces
  └── support layout
          │
          ▼
  Diagnostic[] → transaction/ChangeSet failure before persistence
```

安全门禁不读取 Viewer、Server 或项目缓存。它不启动进程，不写磁盘，不请求 GPU。

### 5.1 授权与提交语义

- 所有写入方式共享同一门禁：`document.apply`、dry-run、ChangeSet evaluate 和 commit。
- dry-run 执行同样的检查，但不持久化。
- 存在 error 级路由诊断时，事务或候选状态失败关闭，原 committed revision 不变。
- 门禁诊断不进入 Document，不改变节点顺序或路径内容摘要。

## 6. Metadata 契约

本设计不增加 Operation 字段，使用现有字符串 metadata。

| 对象 | 字段 | 值 | 默认/约束 |
|---|---|---|---|
| 场景节点 | `routeObstacle` | `solid` | 缺省值 |
| 场景节点 | `routeObstacle` | `ignored` | 不参与实体和净空检查 |
| 场景节点 | `routeObstacle` | `clearance-surface` | 参与上方净空检查 |
| 净空面 | `routeClearance` | 非负米数的十进制字符串 | 缺省 `0` |
| connection | `maxSupportSpacing` | 正米数的十进制字符串 | 缺省 `6` |
| 支撑横梁 | `role` | `pipe-support` | 与现有资产一致 |
| 支撑横梁 | `supportPart` | `beam` | 每个支撑站点一个 |
| connection/支撑 | `lineId` | 非空稳定字符串 | 两者必须一致 |

无效数值、未知 `routeObstacle` 值或空 `lineId` 使用现有
`ICE3_STUDIO_OPERATION_INVALID` 失败关闭。`Buried` 策略不要求可见支撑，其余策略执行
支撑布局检查。支撑节点的 `lineId` 必须匹配当前 Document 中至少一条 connection；
孤立或拼写错误的支撑 ID 同样失败关闭。

## 7. 路由事实

### 7.1 中心线

每条结构有效的 connection 调用一次现有 `route_connection`。安全检查保存：

- 与正式求值相同的采样中心线；
- 每段长度和累计弧长；
- 管线半径 `diameter / 2`；
- 起止端口位置、方向和所属 source node；
- 路线总长和方向变化位置。

安全检查不得另写粗路由算法。资产层可以为生成支撑使用相同数学规则，但最终验收以
Core 生成的中心线为准。

### 7.2 端点接入

起止 source node 在路线开始和结束的 `1.5m` 弧长范围内豁免实体碰撞。这允许管线进入
池壁、设备喷嘴或建筑接口。同一 source node 在豁免区之外再次与中心线圆管碰撞时必须
报告。

豁免只作用于端点实例本身，不影响相邻设备、结构或其它管线。

## 8. 障碍与净空

### 8.1 障碍索引

- 普通 primitive 节点使用现有世界 bounds。
- Component instance 和 instance set 解析参数、嵌套 Part 变换和实例变换，为每个
  source owner 聚合 AABB。
- Cut 使用底模和切体的保守包围盒，不因孔洞缩小 source owner 的障碍范围。
- Connection 派生管线、法兰、阀门、支架和管道夹不进入实体障碍集。
- `routeObstacle=ignored` 的节点跳过两类检查。

### 8.2 Broad phase

按管线半径扩展障碍 AABB，对中心线各段做确定性 sweep。候选按 connection 声明顺序、
段序号和障碍 node ID 排序，避免 Hash 容器决定输出。

### 8.3 Narrow phase

只对 broad-phase 命中候选计算 segment-to-triangle 最短距离。管线与三角形距离小于
等于管线半径即为碰撞。新增纯几何图元测试覆盖穿过、相切、近距未接触和退化输入。

窄相位按每个 source owner 的 `(mesh, material)` 变体缓存本次校验结果。没有候选时
不得构建完整场景网格。

### 8.4 净空面

`clearance-surface` 对象使用其 AABB 的水平 footprint。若任意中心线段的水平投影与
footprint 相交，则要求：

```text
min(centerY along segment) - pipeRadius
  >= surfaceMaxY + routeClearance
```

不跨越 footprint 的路由不执行该检查。道路和硬化地坪的 metadata 由资产层添加；Core
不识别它们的业务含义。

## 9. 支撑布局

### 9.1 站点识别

只把 `role=pipe-support`、`supportPart=beam` 的节点作为站点。按 metadata `lineId`
关联 connection。支撑位置取横梁中心的世界坐标。

### 9.2 投影

支撑中心投影到最近的中心线段，得到弧长位置。投影水平距离超过 `1m` 时报告支撑间距
诊断，防止支撑落在道路或空地上却被误认为有效。

### 9.3 间距

对每个非 Buried connection：

- 将有效支撑弧长排序，并加入路线起点 `0` 和终点 `length`；
- 任意相邻间隔大于 `maxSupportSpacing` 时报告；
- 起点到首个支撑、最后支撑到终点同样计入间隔；
- 无有效支撑时报告 `ROUTE_SUPPORT_MISSING`。

方向变化超过 `15°` 的转弯前后，至少有支撑位于 `max(1m, 4 × diameter)` 范围内。
缺失时报告支撑间距诊断，并在 message 中标记转弯位置。

## 10. 诊断

新增以下稳定诊断码：

| Code | 含义 |
|---|---|
| `ICE3_STUDIO_ROUTE_COLLISION` | 管线圆管与实体障碍相交 |
| `ICE3_STUDIO_ROUTE_CLEARANCE_INSUFFICIENT` | 净空面以上空间不足 |
| `ICE3_STUDIO_ROUTE_SUPPORT_MISSING` | 非埋地管线没有有效支撑 |
| `ICE3_STUDIO_ROUTE_SUPPORT_SPACING` | 支撑间距、离线路面或转弯覆盖不满足 |
| `ICE3_STUDIO_ROUTE_SAFETY_BUDGET` | 障碍、段或窄相位预算耗尽 |

Diagnostic 规则：

- `object_id` 使用 connection source node ID；障碍 ID 写入 message 的稳定结构化片段。
- `pointer` 指向 `/scene/nodes/<index>/content` 下的相关字段。
- message 包含以米为单位的距离、间距或超限量。
- 同一路线同类碰撞最多报告 32 条；超出部分汇总为一条预算/截断诊断，结果仍确定性。
- 预算耗尽必须返回 error，不得把未检查区域报告为 clean。

## 11. 预算

| 项 | 上限 |
|---|---:|
| 障碍 source owner 数 | 50,000 |
| 路由段数 | 50,000 |
| 单次窄相位三角形对 | 1,000,000 |
| 单条连接碰撞报告 | 32 |
| 预检额外峰值内存 | 64 MiB |

预算检查发生在构建大数组之前。所有上限在代码中定义为公开常量，并由测试锁定。

## 12. 性能

验收环境为当前 Apple Silicon 开发机：

- 99 条连接的污水厂预检 p95 不超过 `150ms`。
- 334 个源节点、560 个操作、99 条连接的大型工业 fixture 预检 p95 不超过 `250ms`。
- 无碰撞候选时不得求值完整场景网格。
- Benchmark 输出 JSON，记录连接数、障碍代理数、路由段数、候选数、窄相位对数、
  p50/p95 和额外峰值内存。

基准上限不得因实现困难而静默放宽。超过设计上限时保持 Proposed 并返回实施评审。

## 13. 测试

### 13.1 几何图元

- segment-to-AABB 相交和管线半径扩展。
- segment-to-triangle 最短距离。
- 穿过、相切、差一点接触、平行、退化三角形和预算耗尽。

### 13.2 障碍构造

- 静态 primitive、旋转 Part、嵌套组件和实例集。
- Cut 保守包围盒。
- ignored 附属件。
- 端点实例接入豁免，以及豁免区外的二次穿入。

### 13.3 净空与支撑

- 净空面从下方跨越、贴面、满足净空和不跨越。
- 无支撑、端点间距、中段间距、离线路面支撑和转弯附近缺支撑。
- Buried 豁免。
- 支撑和 connection 的 `lineId` 不一致。

### 13.4 事务与 API

- `document.apply` 和 ChangeSet evaluate 在持久化前拒绝违规路径。
- 失败后 committed revision、preview 和 ChangeSet base 不变。
- 修复后的候选只增加一个新 revision。
- Diagnostic code、object ID、pointer 和顺序稳定。
- budget 失败关闭。

### 13.5 资产与视觉

- 污水厂道路声明净空面 metadata。
- 所有非埋地管线按 `6m` 最大间距生成支撑。
- 支架、法兰、阀门和管道夹不触发自碰撞。
- 资产测试遍历全部 connection 与支撑，不抽样。
- 在 Google Chrome 中检查总览、生化池、二沉池、管廊、埋地管段和穿越建筑位置，
  截图写入 `output/`。

## 14. API 与文档

- 新诊断码使 `API_VERSION` 升至 `1.4`。
- JSON-RPC 方法和 Operation 类型不变。
- 同步各 crate TypeScript bindings，再生成 `schemas/studio-types.ts` 和汇总 Schema。
- 更新 `docs/reference/capabilities.md`、`docs/reference/diagnostics.md`、
  `docs/reference/api.md`、`projects/README.md` 和 `docs/README.md`。
- `ICE3-STUDIO-SPEC-013` 的阶段二B状态在实现验收后同步。

## 15. 资产迁移

1. 更新污水厂生成脚本，为道路和硬化面添加净空 metadata。
2. 为 connection 添加稳定 `lineId` 支撑契约；缺失时由生成器补齐。
3. 按核心路由规则生成多支撑站点，并在转弯和设备附近加密。
4. 把法兰、阀门、支架和管道夹标记为非实体障碍。
5. 通过公开 API 在临时 workspace 重建并验证。
6. 临时验证通过后才更新 `projects/wastewater-plant`。

禁止手工修改 `projects/*/revisions/*.json`、派生 GLB 或 cache。生成工程替换历史时，
必须在实施报告中明确记录 revision 变化。

## 16. 交付顺序

1. 几何距离图元与预算。
2. 障碍代理、端点豁免与 broad/narrow phase。
3. 净空面检查。
4. 支撑布局检查。
5. Diagnostic、Schema 和 API `1.4`。
6. 污水厂 metadata 与多支撑资产生成。
7. 性能基线、Chrome 验收和文档收口。

每一步先写失败测试，再写最小实现。Core 门禁和资产迁移不得在同一个未经测试的提交中
混合启用。

## 17. 风险

| 风险 | 缓解 |
|---|---|
| AABB 误报或漏报 | 候选窄相位；Cut 保守包围盒；测试旋转和凹体 |
| 全场景障碍构建过慢 | 两阶段索引；只对候选构建网格；硬预算 |
| 端点接入豁免过宽 | 固定弧长窗口；豁免区外二次命中仍报错 |
| 支撑投影歧义 | 最近段确定性选择；超过 1m 视为无效 |
| 默认 6m 不适用所有资产 | connection metadata 可显式覆盖 |
| 道路语义泄漏进 Core | 只消费通用净空 metadata |
| 资产重建丢失历史 | 先临时 workspace 验证；实施报告记录 revision 变化 |

## 18. 接受标准

- 违规路径在事务和 ChangeSet 持久化前失败，原 revision 不变。
- 污水厂全部非埋地管线满足支撑布局，埋地管线不要求地面支撑。
- 没有被忽略的实体障碍被管线穿过。
- 道路与硬化面满足声明净空。
- 全部新增几何、Core、API、资产、性能和 Chrome 门禁通过。
- Diagnostic、Schema、文档和 `API_VERSION=1.4` 一致。
- 性能达到第 12 节上限，预算耗尽失败关闭。

## 19. 关联文档

- `ICE3-STUDIO-SPEC-013` — [工艺拓扑与工程管线校正](2026-11-01-wastewater-process-piping-correction-design.md)
- `ICE3-STUDIO-SPEC-030` — [干涉与净空检测](2026-11-03-clash-detection-design.md)
- `ICE3-STUDIO-REF-001` — [能力清单](../reference/capabilities.md)
- `ICE3-STUDIO-REF-004` — [Diagnostics](../reference/diagnostics.md)
- `ICE3-STUDIO-GUIDE-003` — [污水处理厂工程](../../projects/README.md)
