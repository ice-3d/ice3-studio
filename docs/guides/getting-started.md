# ICE3 Studio 快速开始

> Document ID: ICE3-STUDIO-GUIDE-004
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

本指南启动本地 Studio Server、加载最新工程、使用 Agent 主导的 Workbench，并通过公开 API
完成一次 ChangeSet 审阅。Rust 负责 Document、Operation、校验、revision、持久化和 GLB
导出；浏览器 Viewer 只通过公开 API 读取和显示。

## 前置条件

- Rust 1.98 与 Cargo。
- Node.js 24.22.1、npm 11 或更高版本。
- 支持 WebGL2 的浏览器；WebGPU 为显式实验开关。

首次安装 Viewer 依赖：

```bash
npm --prefix apps/viewer ci
```

## 启动默认工程

`npm run dev` 扫描 `projects/*/project.json`，选择修改时间最新的工程；时间相同时选择
`currentRevision` 更高的工程。当前默认工程是 `projects/wastewater-plant`。

```bash
npm --prefix apps/viewer run build
npm run dev
```

命令在 `127.0.0.1:8149` 启动 Server，打印 project、revision 和 Viewer URL，随后持续
运行，直到按下 `Ctrl+C`。可用 `STUDIO_PROJECTS_DIR` 更换 projects root，或用
`STUDIO_PORT` 更换端口。

## Workbench

Workbench 保留 Objects、3D Scene 和 Review dock 三栏：

- `Agent` 是默认 tab，显示 Gateway 状态、project、revision 和焦点 object。
- 在 Objects 或 3D Scene 中选择对象会把完整业务实体设为 Agent 焦点。
- 点击 `Inspect selected object` 后才进入人工参数编辑。
- `Changes` 显示 candidate，Evidence bar 显示 Operations、Timeline 和 Diagnostics。

Agent Gateway 离线时仍可使用对象搜索、属性检查、ChangeSet review 和人工提交。界面不会
伪造已连接状态。

## 小型契约 fixture

`wastewater-mini` 是一个 11 节点、约 16 KB 的确定性 fixture，用于快速 API、E2E 和
benchmark 验证，不是默认展示 Asset：

```bash
npm run dev:mini
```

服务只绑定 `127.0.0.1`，默认端口 `8120`。`--native-gpu cpu` 是 release 默认值；
`auto` 和强制 `gpu` 只应在目标设备通过 GPU benchmark 后实验。

## 前端 watch

保持 Server 运行，在另一个 terminal 构建 Viewer：

```bash
npm --prefix apps/viewer run dev
```

watch build 更新 `apps/viewer/dist`，重新加载浏览器 tab 后生效。Revision event 会触发
Document 和 preview 刷新，但不会替换 canvas，因此 Selection 和 camera 状态可以保留。

## 通过 API 修改模型

模型写入使用 `POST /rpc` 和 JSON-RPC 2.0。写请求需要 bearer token、`expectedRevision`
和幂等键：

```json
{
  "jsonrpc": "2.0",
  "id": "add-pump",
  "method": "document.apply",
  "params": {
    "expectedRevision": 2,
    "dryRun": false,
    "idempotencyKey": "add-pump-001",
    "operations": [
      {
        "type": "node.create",
        "node_id": "node-000012",
        "parent_id": null,
        "name": "Standby Pump",
        "transform": {
          "translation": { "x": 0, "y": 1, "z": 0 },
          "rotation": [0, 0, 0, 1],
          "scale": { "x": 1, "y": 1, "z": 1 }
        },
        "content": {
          "type": "primitive",
          "kind": "cylinder",
          "radius": 1.2,
          "height": 1.8,
          "segments": 20
        },
        "material_id": null,
        "metadata": { "role": "pump" }
      }
    ]
  }
}
```

`dryRun: true` 执行同样的校验、求值和 diff，但不持久化。一个无效 Operation 会拒绝整批
修改，不会留下部分 revision。

## Review ChangeSet

AI proposal 和人工表单修改都进入 ChangeSet。默认 `review-required` 模式下，Agent token
可以创建、编辑和求值候选，但不能提交：

1. `changeset.create { baseRevision, title }`
2. `changeset.addOperations { changeSetId, expectedVersion, entries }`
3. `changeset.evaluate { changeSetId, expectedVersion }`
4. `changeset.impact { changeSetId, expectedVersion }`
5. `changeset.commit { changeSetId, expectedVersion, approvedDigest, idempotencyKey }`

Commit 必须携带审批 Cookie，并且 `approvedDigest` 必须与重新求值得到的 candidate digest
一致。ChangeSet 不是 `ready`、基线 revision 已变化或只提供 Agent token 时，提交失败。

## 验证

```bash
npm run verify
```

根级门禁覆盖 Rust format、lint、test、generated Schema、asset contract、large fixture、
Viewer unit、typecheck、build、Chrome E2E 和依赖审计。

性能基线：

```bash
npm run benchmark
```

结果写入 `output/studio-mvp-benchmark.json`；超过设计上限会使 command 失败。

## 导出 Runtime Tileset

Studio 直接从 committed revision 生成 Agent Console Runtime Asset：

```bash
cargo run -p studio-cli -- export tileset \
  --project ./projects/wastewater-plant \
  --output /tmp/ice3-runtime-tileset \
  --profile agent-console-v1
```

输出固定为 `tileset.json`、`parts/`、`coarse/`、`bindings.json` 和 `export.json`。
完整 source GLB 只写入项目缓存，不进入发布目录。发布目录已存在时默认拒绝；显式
`--force` 时才原子替换，失败会恢复上一份成功 bundle。

`agent-console-v1` 使用 60,000 triangles/part、64 MiB/part、0.5 LOD ratio、adaptive
allocation、120,000 triangles LOD ceiling、4 MiB coarse chunk 和 static render batch。
命令不调用 Node、外部 DCC 或资产守护进程。CLI 可以在 Studio Server 持有 writer
lock 时读取 committed revision，但不会修改工程。

原生 asset pipeline 契约检查：

```bash
node scripts/compare-asset-pipelines.mjs
```

脚本连续执行两次 Studio 导出，校验 triangle/node/Selection、Tileset bounds、相对 URI
和字节确定性；机器可读结果写入 `output/asset-pipeline-parity.json`。

## 导入 Source Asset

glTF/GLB、OBJ/MTL、FBX、USD、STEP、IFC 与 JT 由 Studio 原生导入：

```bash
cargo run -p studio-cli -- import plant.obj \
  --output /tmp/plant.glb \
  --asset-id plant \
  --adapter obj

cargo run -p studio-cli -- import plant.fbx \
  --output /tmp/plant-fbx.glb \
  --asset-id plant-fbx \
  --adapter fbx

cargo run -p studio-cli -- import plant.usdc \
  --output /tmp/plant-usd.glb \
  --asset-id plant-usd \
  --adapter usd

cargo run -p studio-cli -- import skid.step \
  --output /tmp/skid-step.glb \
  --asset-id skid-step \
  --adapter step

cargo run -p studio-cli -- import plant.ifc \
  --output /tmp/plant-ifc.glb \
  --asset-id plant-ifc \
  --adapter ifc

ICE3_JT_CONVERTER=/path/to/approved-jt-converter \
  cargo run -p studio-cli -- import assembly.jt \
    --output /tmp/assembly-jt.glb \
    --asset-id assembly-jt \
    --adapter jt

cargo run -p studio-cli -- validate /tmp/plant.glb
```

OBJ 假定单位为米；`mtllib` 和纹理引用只能访问源文件同目录。绝对路径、URI scheme、
`..` 和 symlink escape 不会读取。输出先完成 Prepared GLB 校验，再以 sibling temporary
file 原子发布。失败、warning-as-error 或已有 output 且未指定 `--force` 时不会替换文件。

FBX/USD/STEP/IFC/JT 在 Studio 自带的隔离 worker 中解析，不需要安装外部 DCC。worker 硬超时会终止
进程；crash、协议污染、response/stderr 超限或 scene budget 越界都不会发布 output。
默认预算为 1,000,000 nodes、20,000,000 triangles、512 MiB estimated decoded memory
和 120 秒；内存值由分配前预检与 response 后记账共同执行，不等同于进程 RSS 上限。
FBX/USD/STEP/IFC/JT 默认由 Cargo features 启用，也可以分别关闭。
USD 的 references、sublayers 和 payloads 只能访问源文件目录，并受 composition depth
与 layer 总数限制。STEP 只接受 AP242，并保留装配/产品层级、共享实例、面颜色、
Selection、单位换算和源 origin。
IFC 只接受 IFC4X3，并保留空间层级、GlobalId Selection、Pset/Qto/System、共享实例、
颜色和源 origin；unsupported resource 使用确定性 warning。
JT 使用部署方提供的 `ICE3_JT_CONVERTER`，默认 timeout 为 300 秒，并由外层 worker
额外提供 5 秒清理宽限。Studio 固定 converter 参数、验证 512 MiB 上限内的 GLB，并保留
层级、共享实例、材质、JT metadata、Selection 和 shared source origin。

## 停止与恢复

使用 `Ctrl+C` 停止 Server。Revision snapshot 在事件发布前原子写入；写入过程中断时保留
上一个可加载 revision。死进程遗留的 project lock 会在下一次 open 时恢复。

## 关联文档

- `ICE3-STUDIO-RM-001` — [Studio README](../../README.md)
- `ICE3-STUDIO-ARC-001` — [Studio 架构](../architecture.md)
- `ICE3-STUDIO-REF-002` — [API Reference](../reference/api.md)
- `ICE3-STUDIO-REF-003` — [ChangeSet 审阅](../reference/change-sets.md)
- `ICE3-STUDIO-SPEC-014` — [Agent 主导工作台](../specs/2026-11-01-agent-led-workbench-design.md)
