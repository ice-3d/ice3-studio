# 工程量清单（BOM）设计

> Document ID: ICE3-STUDIO-SPEC-029
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

## 1. 背景与目标

到 SPEC-028 为止，Studio 已经能把模型导出成 STEP / IFC / STL / OBJ / DXF，但没有任何
"这个模型里有什么、各有多少、总体积多少"的汇总能力。人只能在视口里数，Agent 也只能
逐节点 `scene.query` 再自己累加。

本设计补一条只读通道：

- **API**：`scene.bom`，Agent 直接拿结构化清单；
- **CLI**：`ice3-studio export bom`，导出 CSV 给 Excel / 造价流程；
- 两者共用 `studio_core::agent_contract::build_bom`，语义唯一。

非目标：计价（没有单价概念）、下料优化、与外部 ERP 的物料主数据匹配。

## 2. 分组语义

`BomRequest.groupBy` 决定行的分桶键，缺省 `definition`：

| 值 | 行键 | 可读名 |
|---|---|---|
| `definition` | 组件定义 id；无定义的图元节点归入 `primitive` | 组件名，找不到定义时回落为键 |
| `material` | 材质 id；未赋材质归入 `unassigned` | 材质名，找不到时回落为键 |
| `owner` | 源实例节点 id | 同键 |
| `metadata:<key>` | 该节点的 `metadata[key]`，缺失归入 `unassigned` | 同键 |

行内容：

| 字段 | 含义 |
|---|---|
| `count` | 组内**求值后**的零件节点数（`EvaluatedSceneNode` 数量，即一个实例的多个零件各算一次） |
| `owners` | 组内**去重**的源实例数 |
| `volume` | 组内零件mesh 体积之和（m³），按节点缩放的 `|sx·sy·sz|` 放大 |
| `bounds` | 组内所有零件世界包围盒的并集；组内无 bounds 时为 `null` |

`count` 与 `owners` 分开是因为二者回答不同的问题："有多少个零件要生产" vs "挂在多少个
设备实例上"。同一组件被实例化 100 次时 `count = 100×零件数`、`owners = 100`。

体积用闭合三角网格的有符号体积求和再取绝对值（

```
V = Σ (a · (b × c)) / 6
```

），对每个 mesh key 只算一次并缓存，与实例数无关。模型不是闭合网格时该值只是近似，
`bounds` 仍可用于估算。

## 3. 排序与截断

行按 `count` 降序、键升序排列，保证同一文档每次输出稳定。`limit` 缺省 200、上限
1000；被截断时 `truncated = true`，`total` 仍是截断前的行数。

## 4. API

```
POST /rpc  scene.bom
{ "groupBy": "material", "limit": 50 }
```

- 只读，`review-required` 与 `autonomous` 下均可用；`agentManifest` 中登记为 `Read`，
  参数 / 返回类型分别为 `BomRequest` / `BomResult`。
- 实现：`build_bom(document, scene, request)`，用 `evaluate_session_scene` 的求值结果，
  不返回几何，因此大场景下响应体只与行数相关。

## 5. CLI 与 CSV

```
ice3-studio export bom --project <dir> --output bom.csv [--group-by material] [--force]
```

CSV 列固定为：

```
key,label,count,owners,volumeM3,minX,minY,minZ,maxX,maxY,maxZ
```

数值保留 6 位小数并去掉尾随零；无 bounds 的行留空。含逗号、引号或换行的标签按 RFC 4180
加引号转义。命令把 `revision` / `bytes` / `rows` 打到 stdout，与其他 `export` 子命令
一致。

## 6. 代码落点

| 文件 | 变更 |
|---|---|
| `crates/studio-core/src/agent_contract.rs` | `BomRequest` / `BomRow` / `BomResult` + `build_bom` |
| `crates/studio-core/src/export/bom.rs` | `bom_csv` 投影 |
| `crates/studio-server/src/rpc.rs` | `scene.bom` 能力与处理 |
| `crates/studio-server/src/agent.rs` | Agent 工具登记 |
| `crates/studio-cli/src/main.rs` | `export bom` 子命令 |
| `crates/studio-cli/src/schema.rs` | 方法与类型加入固定 schema |
| `crates/studio-core/tests/bom.rs` | 分组、去重、体积、截断、CSV 转义 |

## 7. 验收

- `definition` 分组把两个 `2×1×1` 实例合成一行，`count=2`、`owners=2`、`volume≈4`；
- `owner` 分组成两行，每行 `count=1`；
- `material` 分组未赋材质时落到 `unassigned`；
- `metadata:<key>` 按零件 metadata 分桶；
- `limit` 截断后 `total` 不变且 `truncated=true`；
- CSV 表头固定，含逗号 / 引号的标签按 RFC 4180 转义；
- schema `--check` 通过；方法清单 40（运行时 42）。
