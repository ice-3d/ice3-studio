# 管线附件实体化设计

> Document ID: ICE3-STUDIO-SPEC-035
>
> Status: Proposed
>
> Owner: ICE3 Studio maintainers

## 1. 摘要

`ICE3-STUDIO-SPEC-013` 的工艺拓扑、连续路径、碰撞、道路净空和支撑门禁已经交付，但
穿墙套管、三通和异径管仍未实体化。当前管线端点可以对齐，分支仍可能从同一点叠出，
不同直径之间没有显式过渡构件，穿越墙体处也没有可识别的套管。

本设计补齐这三类附件，并保持 Studio 的通用边界：

- 资产层创建可见附件几何和真实端口；
- Core 只校验与行业无关的附件引用、端口、直径、方向和路由对齐；
- 附件继续是普通 Component Instance，不引入平行管网真源；
- 错误在 revision 持久化前失败关闭；
- 污水厂资产在不新增业务专用 Core 类型的前提下完成迁移。

成功标准是：三通、异径管和穿墙套管在 Document 中可追踪，在 GLB 中有连续可见几何，
在 Viewer 近景中无悬空、断口或错位，并受到现有路线安全与场景预算门禁保护。

## 2. 范围

### 2.1 本阶段

- 参数化三通组件；
- 参数化异径管组件；
- 参数化穿墙套管组件；
- 通用附件 metadata 与端口命名；
- 附件、连接、路由和宿主几何之间的一致性校验；
- 附件相关稳定 Diagnostic；
- 污水厂资产迁移；
- Chrome 近景视觉验收和完整仓库门禁。

### 2.2 非目标

- 自动布线、自动放置附件或自动修改 Document；
- 水力计算、管径选型、壁厚计算和压力等级校核；
- B-rep 焊接、精确相贯线、制造级倒角或管道应力分析；
- 弯头、阀门、法兰和支吊架重新实现；现有资产配件继续保留；
- 任意数量的多通接头、偏心异径管、补强圈结构计算或保温层；
- 管线以外的电缆桥架、风管和结构支撑附件；
- 把污水处理业务名称写入 Core 类型或 metadata 枚举。

## 3. 设计原则

1. **Document 是唯一真源。** 附件通过 Component Instance、端口和 metadata 表达。
2. **资产拥有几何。** Core 不生成、替换或修补附件 mesh。
3. **Core 校验事实。** Core 只检查端口存在、路由重合、方向、直径、宿主穿越和预算。
4. **显式优于推导。** 分支使用三通端口，异径使用异径管，穿墙使用套管；不以重叠管段
   伪装附件。
5. **失败关闭。** 无效引用、悬空附件或未命中宿主的套管不能被当作 warning。
6. **扩展不破坏现有调用方。** 不新增 Operation 字段，不改变现有 `connection.create`
   wire format。

## 4. 架构

```text
资产层
  ├── pipe-tee component
  ├── pipe-reducer component
  ├── wall-sleeve component
  └── asset validation
          │ Component Instance + ports + metadata
          ▼
Studio Core
  ├── resolved connection ports
  ├── route centerline
  ├── route attachment validation
  ├── host penetration validation
  └── Diagnostic[]
          │ pass / fail closed
          ▼
Document revision / evaluated geometry / GLB / Viewer
```

新增 `studio-core::large_scene::route_attachment`。它由
`validate_large_scene` 在结构校验完成后调用，并复用 `route_connection` 的正式中心线。
附件校验不读取 Viewer 状态、不启动 worker、不写磁盘。

为避免 Core 与资产层各自实现近似算法，连接端口解析从路由实现中提取为共享的
`resolved_ports` helper。附件校验必须使用与正式求值相同的端口位置、方向和直径。

## 5. Metadata 契约

附件 metadata 全部使用现有 string value 约束。

| 字段 | 三通 | 异径管 | 套管 | 约束 |
|---|---|---|---|---|
| `role` | `pipe-fitting` | `pipe-fitting` | `pipe-penetration` | 必填 |
| `fittingKind` | `tee` | `reducer` | `sleeve` | 必填 |
| `lineId` | 非空 | 非空 | 非空 | 必须匹配真实 connection |
| `hostNodeId` | 禁止 | 禁止 | 非空 | 必须匹配有效宿主节点 |
| `stage` | 可选 | 可选 | 可选 | 仅用于资产分组 |

类型未知、字段缺失、空字符串或业务角色混入附件字段时，返回
`ICE3_STUDIO_ROUTE_ATTACHMENT_INVALID`。

附件组件端口名固定：

| 组件 | 端口 |
|---|---|
| 三通 | `run-in`、`run-out`、`branch` |
| 异径管 | `large`、`small` |
| 套管 | `wall-in`、`wall-out` |

端口功能是通用机械语义，不包含道路、池体或处理工艺名称。

## 6. 三通

### 6.1 端口事实

- `run-in` 与 `run-out` 直径相同，方向相反，中心共线。
- `branch` 直径可以等于或小于主管直径。
- 三个端口的介质必须一致。
- 三条相邻 connection 必须分别引用三个不同端口。
- 任一端口不得被两条 connection 复用。

### 6.2 路由对齐

- 每条 connection 的端点世界位置与对应端口中心误差不超过统一容差。
- `run-in` 与 `run-out` 的相邻连接中心线必须沿端口轴连续。
- 支管中心线与直通轴的夹角必须在 `30°` 至 `150°` 之间。
- 直通轴线与附件实例的世界主轴平行；局部端口方向经过实例旋转后仍必须满足上述约束。

### 6.3 几何

资产层使用主管、支管、加强圈和接口法兰组合。可见接口中心必须与端口中心重合。几何可以
由多个 primitive 组成，但端口只附着到明确的 component part。

## 7. 异径管

### 7.1 端口事实

- `large` 与 `small` 直径均必须大于零且彼此不同。
- 两端口方向相反并共线。
- 两端口介质必须一致。
- 两个端口分别由一条 connection 引用。

### 7.2 兼容性

现有 connection 仍要求其直径匹配所接端口。异径管通过自身两个不同直径端口完成过渡：

```text
large connection -> reducer.large
reducer.small    -> small connection
```

不得放宽 `connection.create` 的直径匹配规则，也不得用一个连接同时表达两种直径。

### 7.3 几何

资产层生成两端法兰和锥形过渡段。大端、小端和法兰中心必须与真实端口一致。轴向过渡
长度必须为正，并在组件参数允许范围内。

## 8. 穿墙套管

### 8.1 宿主

`hostNodeId` 必须指向 Document 中一个实际 barrier 节点。barrier 可以是 primitive、
Component Instance 或 instance set 的派生 geometry，但必须参与路线实体障碍检查。
`routeObstacle=ignored` 的节点不得作为套管宿主。

### 8.2 穿越事实

- `lineId` 对应连接的真实中心线必须与宿主三角形相交。
- 仅穿过宿主 AABB、没有命中实际实体的路线不能通过。
- `wall-in` 与 `wall-out` 位于宿主两侧，端口中心必须落在宿主表面容差带内。
- 套管轴线与穿越管段轴线方向偏差不超过 `2°`。
- 套管外径必须大于管道直径，端部伸出墙体长度必须为正且不超过组件参数上限。
- 同一连接、同一宿主、同一穿越区间只能声明一个套管。

### 8.3 几何

资产层使用环形套筒、端环和可选密封法兰组合。套筒不得封闭管道通道；可见几何不能
覆盖管道颜色，也不能与管道本体形成错误的实心堵塞观感。

## 9. 容差

| 项目 | 容差 |
|---|---|
| 附件端口到连接中心线 | `max(0.02 m, diameter × 0.05)` |
| 最大位置容差 | `0.10 m` |
| 轴向方向偏差 | `2°` |
| 异径管共线夹角 | `2°` |
| 三通直通共线夹角 | `2°` |
| 套管表面中心带 | `0.15 m` |
| 宿主三角形命中 | 复用 route safety 的 `CLASH_EPSILON` |

容差在 Core 中定义为公开常量，并由测试锁定。

## 10. 与 route safety 的关系

- 附件校验使用 `route_connection`，不另写粗路由。
- 屏障穿越复用 route safety 的障碍提取和 segment-to-triangle 窄相位。
- 支吊架仍按 connection `lineId` 检查，不把三通、异径管或套管当作支撑站点。
- 附件是实际 steel 几何，对其它管线仍可构成障碍。
- 对自身 `lineId`，附件几何不作为碰撞障碍，避免套管或三通包围自身管道导致假阳性。
- 对无关 `lineId`，附件几何正常进入障碍集。

实现时可以把附件排除规则放在障碍索引阶段，但必须保留跨管线碰撞覆盖。

## 11. 诊断

新增稳定 Diagnostic code：

| Code | 含义 |
|---|---|
| `ICE3_STUDIO_ROUTE_ATTACHMENT_INVALID` | metadata、端口、引用、直径或宿主无效 |
| `ICE3_STUDIO_ROUTE_ATTACHMENT_MISALIGNED` | 附件中心线、方向或表面位置不满足容差 |
| `ICE3_STUDIO_ROUTE_PENETRATION_INVALID` | 连接未命中真实宿主、轴线错误或重复套管 |
| `ICE3_STUDIO_ROUTE_ATTACHMENT_BUDGET` | 附件或穿越检查预算耗尽 |

Diagnostic 规则：

- `object_id` 使用附件 source node ID。
- `pointer` 指向附件的 `/content/metadata` 或相关 connection。
- message 包含附件 kind、`lineId`、宿主 ID 和量化偏差。
- 每个附件最多报告 8 条附件诊断；超限汇总为预算诊断。
- Diagnostic 顺序只由 Document 顺序和稳定 ID 决定。

## 12. 预算

| 项目 | 上限 |
|---|---:|
| 附件实例总数 | 512 |
| 单附件端口数 | 3 |
| 参与者缓存条目 | 8,192 |
| 单次屏障穿越候选 | 4,096 |
| 单次附件窄相位三角形对 | 250,000 |
| 附件诊断报告 | 每附件 8 条 |

预算在构建大数组前检查。超限返回 `ICE3_STUDIO_ROUTE_ATTACHMENT_BUDGET`，不得把未检查
附件视为 clean。

附件几何继续计入现有 node、instance、triangle、mesh、material 和 encoded-byte 预算。

## 13. 资产组件

在 `scripts/wastewater-plant` 中新增可复用组件：

### 13.1 `pipe-tee`

参数：

- `runDiameter`；
- `branchDiameter`；
- `runLength`；
- `branchLength`；
- `flangeDiameter`；
- `segments`。

接口：`run-in`、`run-out`、`branch`。

### 13.2 `pipe-reducer`

参数：

- `largeDiameter`；
- `smallDiameter`；
- `length`；
- `flangeDiameter`；
- `segments`。

接口：`large`、`small`。

### 13.3 `wall-sleeve`

参数：

- `pipeDiameter`；
- `sleeveDiameter`；
- `wallThickness`；
- `extension`；
- `segments`。

接口：`wall-in`、`wall-out`。

组件参数必须满足：

- 所有尺寸必须为有限的正值；
- 分段数在现有 Studio primitive 上限内；
- `branchDiameter <= runDiameter`；
- `smallDiameter < largeDiameter`；
- `sleeveDiameter > pipeDiameter`。

## 14. 污水厂迁移

### 14.1 三通

在以下位置使用三通：

- 泵出口汇管；
- 生化配水支路；
- 消化池集气支管；
- 反冲洗供回水支路；
- 污泥回流和剩余污泥支路。

分流井和集水井继续作为工艺构筑物，不替换为裸露三通。只有在真实 pipe-to-pipe 分叉处
使用三通。

### 14.2 异径管

在以下直径变化处使用异径管：

- 泵出口到总管；
- 消化池支管到集气总管；
- 反冲洗泵出口到反冲洗主管；
- 污泥泵出口到浓缩或消化主管。

每处异径管两侧各建立一条 connection，直径分别匹配对应端口。

### 14.3 套管

在以下真实穿越处使用套管：

- 管线进入泵房、鼓风机房和设备间；
- 管线穿越混凝土池壁；
- 地下管线穿越构筑物楼板或井壁；
- 已有连接路径中段穿入实体、而不是端点正常接入设备喷嘴的位置。

资产迁移不得为了套管而把管线穿过道路面或新增违法净空。

## 15. 测试

### 15.1 Core 单元测试

- 合法三通三端口分别连接，中心线连续。
- 三通端口复用、直径不一致、方向反转和支管共线被拒绝。
- 合法异径管两侧直径不同并正确连接。
- 异径管同径、反向、错位或一侧缺失被拒绝。
- 套管命中真实墙体三角形时通过。
- 仅穿过 AABB 或偏离墙体表面时失败。
- 同一连接、宿主重复套管时失败。
- 附件预算耗尽时失败关闭。
- 附件不会对自身管线触发碰撞，但会对无关管线报告碰撞。

### 15.2 资产契约测试

- 三个附件组件的端口名、直径、方向和参数范围正确。
- 每个三通的三条 connection 使用不同端口。
- 每个异径管连接一个 strict-larger 和一个 strict-smaller 端口。
- 每个套管有真实 host，且 host 在资产中参与 barrier 检查。
- 节点、三角形和材质仍在污水厂预算内。

### 15.3 端到端测试

- 通过公开 API 重建工程只增加一次 revision。
- 非法附件计划在 revision 创建前失败，原 committed revision 不变。
- 重新打开工程后附件 ID、端口引用和 route digest 保持稳定。
- Prepared GLB 保留附件 mesh、材质和 Selection。
- Viewer preview 可加载，无控制台错误或资源配置失败。

### 15.4 Chrome 视觉验收

使用用户安装的 Google Chrome 检查：

1. 泵组出口三通和异径管；
2. 生化配水三通；
3. 消化池集气三通；
4. 泵房和池壁套管；
5. 地下管线穿越构筑物；
6. 全厂总览中附件没有异常高亮或悬空。

验收要求：

- 附件与管道、墙体和法兰连续；
- 三通没有第二根管直接插在同一端口；
- 异径过渡方向正确；
- 套管没有堵塞管道；
- 无异常断口、黑面、错位或莫名阴影；
- 未新增道路、池体核心区和其它实体碰撞。

## 16. 性能与门禁

- route safety 的 99 条连接污水厂 p95 仍不超过 `150 ms`。
- 大型工业 fixture route safety p95 仍不超过 `250 ms`。
- 附件新增后污水厂有效三角形、唯一三角形、节点、实例和材质保持在
  `ICE3-STUDIO-SPEC-012` 预算内。
- Core 附件校验额外峰值内存不超过 `32 MiB`。
- `npm run verify` 必须全部通过。
- 性能变化需要更新机器可读基线，不得静默放宽既有上限。

## 17. 兼容性与迁移

- 现有 connection、route policy、support metadata 和 Diagnostic 保持兼容。
- 现有不含附件的项目不产生新错误或新节点。
- 未知 `role` 保持现有行为；只有两个新 role 进入附件校验。
- 已有资产中的 `pipe-flange`、`pipe-collar`、`pipe-valve` 和支撑 metadata 不变。
- `SPEC-013` 在本设计全部验收通过后标记为 Implemented。

## 18. 交付顺序

1. 扩展端口解析 helper，并增加附件校验失败测试。
2. 实现三通和异径管校验。
3. 复用 route safety 几何事实实现套管穿越校验。
4. 增加 Diagnostic、预算和跨管线碰撞测试。
5. 实现三个参数化资产组件。
6. 迁移污水厂真实三通、异径管和套管。
7. 重建工程并完成 Chrome 近景验收。
8. 更新 Reference、Diagnostics、项目指南和状态文档。
9. 运行完整仓库门禁，提交并合并。

## 19. 验收标准

- 三通、异径管和穿墙套管均有真实组件、端口和可见几何。
- 所有附件引用真实 `lineId`；套管引用真实 `hostNodeId`。
- 错误附件在提交前失败关闭，Diagnostic 可定位节点和 JSON Pointer。
- 附件不对自身管线产生碰撞假阳性，但跨管线碰撞仍被检查。
- 污水厂资产通过现有工艺、道路、净空、支撑、场景预算和新的附件测试。
- Prepared GLB、partition 和 Viewer preview 保留附件几何与材质。
- Google Chrome 关键近景无断口、堵管、悬空或错误阴影。
- 文档状态与实际实现一致，完整仓库门禁通过。
