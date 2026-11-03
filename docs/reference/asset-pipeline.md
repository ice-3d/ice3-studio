# ICE3 Studio 资产流水线

> Document ID: ICE3-STUDIO-REF-005
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

本文描述当前生产链从 Source Asset 到 Runtime Tileset 的实际流程。架构、API 或命令
发生变化时，必须同步更新本文；已完成的迁移计划和设计规格只作为历史证据，不能替代
本文描述当前能力。

## 1. 生产边界

Studio 是 Source Import、Asset Prepare、partition、LOD、render batch 和 Tileset
发布的唯一生产入口。Engine 与 Agent Console 只消费最终 Prepared GLB 和
OGC 3D Tiles 1.1 bundle，不链接 Studio 的 Rust crate，也不在浏览器执行 source
conversion、simplification 或 tiling。

当前生产路径不启动 Toolchain Sidecar、Node Asset Runtime、Blender 或其它外部 DCC。
JT 的唯一外部例外是部署方提供的 host-owned `ICE3_JT_CONVERTER`；Studio 固定调用参数、
登记受控进程组并验证输出 GLB。

## 2. Source Import

`studio-import` 把受支持格式转换为格式无关的 `SourceScene`。当前 adapter 包括：

| Adapter | 输入 | 保留内容 |
|---|---|---|
| `gltf` | `.gltf`、`.glb` | 层级、共享 Mesh、Material、extras 和 Selection |
| `obj` | `.obj`、`.mtl` | object/group、多边形三角化、同目录 MTL 和单位 warning |
| `fbx` | ASCII/Binary `.fbx` | 层级、transform、实例、颜色和 bounded metadata |
| `usd` | `.usda`、`.usdc` | prim path、prototype、实例、Geometry 和颜色 |
| `step` | AP242 `.step`、`.stp` | product/assembly、occurrence、共享实例、面颜色和单位 |
| `ifc` | IFC4X3 `.ifc` | 空间层级、GlobalId、Pset/Qto/System、实例和单位 |
| `jt` | `.jt` | 层级、共享实例、材质、JT metadata 和受控 converter 输出 |

外部 resource 限定在 source 同目录。FBX、USD、STEP、IFC 与 JT 在隔离 worker 中执行，
并受到 node、triangle、decoded-memory、timeout、response 和 stderr 上限约束。失败关闭，
不发布部分输出。

## 3. Shared Prepare

`studio-assets::prepare` 是 `SourceScene` 到 `PreparedArtifact` 的唯一语义实现。
`import`、`build`、`inspect`、`partition` 和 `export tileset` 必须复用这一入口，不能
各自拼装 profile、metrics、budget 或 content digest。

共享 Prepare 生成：

- 标准 `ice3-prepared` GLB bytes；
- 真实 `AssetMetrics`；
- `PreparationMetadata`；
- built-in 或 custom budget result；
- BIN chunk content digest；
- warning 和稳定 failure code。

| 命令 | 默认 edges | 主要用途 |
|---|---|---|
| `import` | 关闭 | 生成 Source Import 的标准 Prepared GLB |
| `build` | 开启，crease angle 为 `30°` | 生成带确定性 render edges、budget 和可选 report 的 GLB |
| `inspect` | 只读取或求值，不发布 | 输出 source/Prepared metrics、preparation 和 budget |
| `partition` | 开启 | 直接对任意已注册 source 生成 Tiled Runtime bundle |
| `export tileset` | 由 `agent-console-v1` profile 固定 | 从 committed project revision 发布 Agent Runtime bundle |

Render edge 使用 line primitive `mode=1`、`role="edges"`、`widthPx=3` 和独立 edge
material。Edge bytes 计入 part、coarse chunk 和总 byte budget，但不计入 triangle
count。

## 4. Budget 与发布

`build` 和 `partition` 接受 `interactive-desktop-v1`、`unrestricted` 或受控 JSON
budget profile。Soft limit 生成 warning，hard limit 失败关闭；
`--warnings-as-errors` 把 warning 转为失败。

文件与目录使用 sibling staging、backup 和 atomic rename 发布：

- 默认拒绝已存在 output；
- `--force` 才替换；
- 任一 validation 或 rename 失败时恢复上一份成功输出；
- GLB 与 report 成对发布；
- Tileset 只有在所有 part 和 profile 校验通过后才整体可见。

## 5. Partition、LOD 与 Render Batch

`studio-pipeline` 先分析 atomic partition unit，再生成确定性的空间 hierarchy。共享
Mesh 的实例保持在同一 partition unit；LOD、proxy 和 batching 不改变业务设备的稳定
`selectionId`。

| 产物 | 作用 |
|---|---|
| `parts/part-NNN.glb` | Fine partition，按 camera work set 加载 |
| `coarse/*.glb` | Coarse proxy 或 chunk，提供远距离和过渡覆盖 |
| `tileset.json` | OGC 3D Tiles 1.1 explicit hierarchy、bounds、error 和 relative URI |
| `bindings.json` | 业务设备、Selection、bounds 和 metadata 的发布清单 |
| `export.json` | committed revision、profile、stats 和 content identity |

Fine、LOD、coarse、semantic proxy 和 static render batch writer 都从各自的最终
positions/indices 生成 edges。Static batch 烘焙 world transform 时保留 `COLOR_0`，
并通过 `_ICE3_SEMANTIC_INDEX` 映射原 Selection。

## 6. Identity 规则

Prepared GLB 内同时保留两类 identity：

- `extras.ice3.id` 是 partition-scoped Node ID。Tileset 发布时使用
  `<asset-id>/node/<leaf-id>/<node-id>`，避免不同 partition 复用 Node ID。
- `extras.ice3.selectionId` 是跨 LOD、proxy 和 partition 保持稳定的业务 Selection ID。

带 `metadata.tag` 的构件优先聚合为 `<asset-id>/equipment/<tag>`；没有 tag 的独立对象
使用 source owner ID 或稳定 source Node ID。一个设备的池壁、栏杆、中心筒、驱动和管线
等 components 共享同一个业务 Selection，而不是成为独立可选面片。

Runtime 合并 Tileset 时先用 partition-scoped Node ID 保持 Scene graph 唯一，再按
`selectionId` 归并 query、pick、highlight 和 focus。

## 7. `agent-console-v1`

当前 Agent Console profile 固定使用：

- 每个 fine part 最多 60,000 triangles 和 64 MiB；
- LOD ratio `0.5` 与 adaptive allocation；
- LOD ceiling 120,000 triangles；
- coarse chunk 最多 4 MiB；
- `semanticProxy=off`；
- static render batch；
- parts 与 coarse GLB 均包含 render edges。

导出命令：

```bash
cargo run -p studio-cli -- export tileset \
  --project ./projects/wastewater-plant \
  --output /tmp/ice3-runtime-tileset \
  --profile agent-console-v1
```

导出只读取 committed revision。完整 source GLB 留在 `projects/<id>/cache/preview/`，
不复制到发布目录。Agent Console 使用同目录的 `tileset.json`、`bindings.json` 和
`export.json`，网络加载按 camera work set 请求 fine/coarse content，不下载完整单体
source。

## 8. 验证

Prepare、partition 和跨工程消费使用真实产物验证：

```bash
npm run verify
npm run benchmark:prepare
node scripts/verify-prepare-engine.mjs
node --test scripts/tests/prepare-engine-contract.test.mjs
node scripts/compare-partition-imports.mjs
node --test scripts/tests/partition-contract.test.mjs
```

门禁必须覆盖：

- deterministic bytes 和 content digest；
- node、mesh、primitive、triangle、edge、material 和 Selection 统计；
- fine/LOD/coarse edge role、width 和 color；
- Tileset URI、bounds、hierarchy 和 partition-scoped Node ID；
- Engine `readPreparedGLB`、`readTileset` 与 Agent Console Tileset loading；
- p95、peak RSS、输出 bytes 和 coarse/batch budget；
- Studio 在相邻 Toolchain repository 不存在时仍可 standalone 验证。

## 9. 关联文档

- `ICE3-STUDIO-RM-001` — [Studio README](../../README.md)
- `ICE3-STUDIO-ARC-001` — [Studio 架构](../architecture.md)
- `ICE3-STUDIO-REF-001` — [能力清单](capabilities.md)
- `ICE3-STUDIO-REF-004` — [Diagnostics](diagnostics.md)
- `ICE3-STUDIO-GUIDE-004` — [Getting Started](../guides/getting-started.md)
- `ICE3-DOC-CTR-001` — [Asset Preparation Contract v1](https://github.com/ice-3d/ice3-doc/blob/main/docs/contracts/asset-preparation-contract-v1.md)
- `ICE3-AGENT-REF-002` — [Agent Console Runtime Asset 交付](https://github.com/ice-3d/ice3-agent-console/blob/main/docs/reference/asset-delivery.md)
