# 干涉与净空检测（Clash / Clearance）设计

> Document ID: ICE3-STUDIO-SPEC-030
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

## 1. 背景与目标

`validation.run --includeOverlaps` 已经能报"实体包围盒互相插入"，但它只面向**源实体**
（按源节点聚合），没有净空阈值，也不暴露每一对的具体深度，Agent 无法拿来回答
"这段管廊离结构还有多少余量""哪些设备真的撞在一起"。

本设计新增只读方法 `scene.clashes`，粒度是**求值后的零件**：

- 硬碰撞（`clearance = 0`）：谁和谁撞了、侵入多深；
- 净空检查（`clearance > 0`）：谁离谁太近；
- 可选窄相位（`precise`）：用三角形求交否掉包围盒造成的假阳性。

非目标：碰撞响应与几何修正（Studio 是 AI-first 的提案-审批模型，检测只报告，不自动
修改几何）、连续碰撞检测、软体/动画干涉。

## 2. 语义

### 2.1 带符号间隙

一对零件的间隙 `gap` 是单一带符号标量：

| 情况 | `gap` | 含义 |
|---|---|---|
| 三轴都分离 | `+√(dx²+dy²+dz²)` | 两个包围盒之间的最短距离（米） |
| 三轴都重叠 | `−min(重叠量)` | **侵入深度**，负号表示穿透 |
| 恰好贴合 | `0` | 共面接触 |

用同一个数既表达间隙又表达深度，排序时"最深侵入"自然排在最前。小于 `1e-5` 的差值
归零，避免贴合面因浮点噪声在"接触/分离"之间跳动。

### 2.2 作用域

`scope`：

- `owners`（默认）——只比较**属于不同源实例**的零件。同一实例内部的零件是按设计互相
  插入的（轴在孔里、法兰在管口），把它们算成碰撞只会制造噪声；
- `parts`——同实例内部也比。用于检查组件配方本身是否自相矛盾。

### 2.3 窄相位

宽相位只比包围盒，旋转与曲面零件会误报。`precise: true` 时，对**侵入的**候选做
三角形求交（Möller–Trumbore：两个三角形各三条边对其他三角形做线段求交）：

- `confirmed: true` —— 几何真的相交；
- `confirmed: false` —— 包围盒相交但三角形不相交，**不是碰撞**，该对不返回；
- `confirmed: null` —— 没开窄相位，或三角形预算在得出结论前用尽（大网格）。

净空对（`gap > 0`）不做窄相位：网格间最近距离需要 BVH，成本与收益不成比例，包围盒
净空对"要留多少余量"这个用途已经够用。

### 2.4 预算

窄相位是唯一的重成本项，两个上限：

| 常量 | 值 | 作用 |
|---|---|---|
| `MAX_PRECISE_CANDIDATES` | 256 | 一次查询最多对多少对做窄相位 |
| `triangleBudget` | 默认 20 000，上限 4 000 000 | 单对最多比较多少组三角形 |

候选按 `gap` 升序排序**之后**才进窄相位，所以预算优先花在最严重的干涉上，而不是
sweep 的遍历顺序。预算耗尽返回 `null` 而不是猜一个结论；需要结论时把 `triangleBudget`
调大重查。

## 3. 结果

```json
{
  "pairs": [
    {
      "a": "node-000255/route",
      "b": "node-000285/route",
      "aOwner": "node-000255",
      "bOwner": "node-000285",
      "gap": -7.62,
      "penetrating": true,
      "confirmed": true
    }
  ],
  "total": 5881,
  "truncated": true,
  "clearance": 0,
  "scope": "owners",
  "precise": true,
  "checkedParts": 4914,
  "candidates": 13271
}
```

`total` 是截断前的对数，`candidates` 是宽相位产出的候选数（含被作用域过滤掉的），
`checkedParts` 是进入宽相位的零件数——三者一起说明"这份报告覆盖了什么"。

## 4. 实现

| 阶段 | 算法 | 复杂度 |
|---|---|---|
| 世界包围盒 | 父链矩阵记忆化，mesh bounds 8 角点变换 | O(节点) |
| 宽相位 | 按 `min.x` 排序后 sweep and prune，带净空扩展 | O(n log n + k) |
| 作用域过滤 | `ownerId` 比较 | O(k) |
| 排序 | 按 `gap` 升序 | O(k log k) |
| 窄相位 | 三角形两两求交，逐对预算 | O(256 × budget) |

 wastewater-plant 实测（4 914 个求值零件、348 932 三角形）：宽相位 46–54 ms；
`precise` + 默认预算约 1 s；`precise` + 4M 预算 14 s（可作为离线校验，不适合交互）。

## 5. 代码落点

| 文件 | 变更 |
|---|---|
| `crates/studio-core/src/geometry/clash.rs` | `signed_gap` / `sweep_pairs` / `triangles_intersect` / `meshes_intersect` |
| `crates/studio-core/src/large_scene/evaluate.rs` | `evaluated_world_matrices` / `evaluated_scene_bounds_with` |
| `crates/studio-core/src/agent_contract.rs` | `ClashRequest` / `ClashPair` / `ClashResult` + `build_clash_report` |
| `crates/studio-server/src/rpc.rs` | `scene.clashes` 能力与处理 |
| `crates/studio-server/src/agent.rs` | Agent 工具登记 |
| `crates/studio-cli/src/schema.rs` | 方法与类型加入固定 schema |
| `crates/studio-core/tests/clashes.rs` | 契约测试 |

## 6. 验收

- 相隔 2 m 的两个实例不报；重叠 1 m 时报一对，`gap = -1`；
- `clearance = 2.5` 时把 2 m 间隙的对报出来，`penetrating = false`；`clearance = 1.5` 时不报；
- 同实例零件默认忽略，`scope = "parts"` 时可见；
- `ownerIds` 把审计收窄到指定实例；
- 两个 45° 斜置细长盒：包围盒相交但几何分离，`precise = true` 时报 0 对，不加则报 1 对；
- 真实侵入在 `precise` 下 `confirmed = true`；
- 几何内核单测：带符号间隙、sweep 净空带、三角形相交/分离、预算耗尽返回 `None`；
- schema `--check` 通过；方法清单 41（运行时 43）。
