# Studio Runtime Tileset 导出设计

> Document ID: ICE3-STUDIO-SPEC-015
>
> Status: Deprecated
>
> Owner: ICE3 Studio maintainers

> 状态说明：本方案由
> [Studio 原生 Asset Pipeline 迁移](2026-11-01-native-asset-pipeline-migration-design.md)
> 取代。Studio 将直接实现 Asset pipeline，不再保留 Toolchain Sidecar 作为运行时依赖；
> 本文仅保留早期导出方案记录。

## 1. 目标

让 Studio 成为 Runtime Asset 发布的唯一产品入口。用户或 Agent 只操作 Studio command，
Studio 从当前 Document revision 生成 Prepared GLB，调用固定版本 Toolchain Sidecar 完成
partition/LOD，校验输出并原子发布可供 Agent Console 使用的 Tileset bundle。

Toolchain 仍是分块、LOD 和 Tileset 算法的执行实现，不作为最终用户必须单独操作的工程
步骤。

## 2. 边界

```text
Studio project revision
        ↓
Studio source GLB export
        ↓
Toolchain Sidecar asset.partition
        ↓
Studio validation + bindings manifest
        ↓
Atomic runtime asset publication
        ↓
Agent Console
```

- Studio Core 不复制 Toolchain 的 partition 或 LOD 实现。
- Studio Server 的 Toolchain client 继续负责进程、协议、timeout 和路径边界。
- 导出只读取 committed revision；ChangeSet candidate 不能直接发布。
- Agent Console 本阶段不修改。
- 发布失败不得留下部分目录，也不得替换上一个成功版本。

## 3. 产品接口

新增 CLI 子命令：

```bash
ice3-studio export tileset \
  --project ./projects/wastewater-plant \
  --output ../ice3-agent-console/public/assets/wastewater-plant \
  --profile agent-console-v1
```

可选参数：

| 参数 | 默认 | 行为 |
|---|---|---|
| `--toolchain` | `STUDIO_TOOLCHAIN_CLI` 或 host 管理包 | 指定受信任 Sidecar entry |
| `--force` | false | 允许替换已有 output directory |

`--profile agent-console-v1` 固定以下 partition 配置：

- `maxTriangles = 60000`
- `maxBytes = 67108864`
- `lod = true`
- `lodRatio = 0.5`
- `lodAllocation = adaptive`
- `lodMaxTriangles = 120000`
- `lodMinUnitTriangles = 8`
- `lodMaxMemoryBytes = 536870912`
- `coarseChunkMaxBytes = 4194304`
- `semanticProxy = off`
- `renderBatch = static`

Profile 是一份受版本控制的发布策略。V1 CLI 不接受任意原始 Toolchain option 覆盖。

## 4. 输出 Bundle

```text
output/
├── tileset.json
├── parts/
│   ├── part-000.glb
│   └── ...
├── coarse/
│   └── ...
├── bindings.json
└── export.json
```

不发布完整 source GLB。source GLB 和 Sidecar partition staging 只存在于 Studio project
`cache/` 中；最终 Bundle 只保留流式 Tile、manifest 和 provenance。

### `bindings.json`

```json
{
  "schemaVersion": 1,
  "assetId": "wastewater-plant",
  "revision": 1,
  "bindings": [
    {
      "selectionId": "wastewater-plant/equipment/AE-101",
      "tag": "AE-101",
      "name": "AE-101 好氧池 A",
      "stage": "biological",
      "area": "Train1",
      "bounds": {
        "min": [118.99, 0.11, 28.5],
        "max": [139.81, 6.75, 43.5]
      },
      "nodeSelectionIds": [
        "node-000119/ae-wall",
        "node-000119/ae-water"
      ],
      "metadata": {
        "stage": "biological",
        "train": "Train1"
      }
    }
  ]
}
```

Binding 由 evaluated Scene Node metadata 聚合：

- `metadata.tag` 是业务实体键。
- 同一 tag 的所有派生 Node 合并为一个 binding。
- `nodeSelectionIds` 用于将 Agent Console 的逻辑 Selection 展开到实际 GLB Node。
- `bounds` 是聚合后的 world-space bounds。
- `area` 优先取 `metadata.train`，否则取 `metadata.stage`。
- metadata 只保留可序列化字符串，不返回本地路径或凭据。

### `export.json`

```json
{
  "schemaVersion": 1,
  "kind": "ice3-runtime-tileset",
  "projectId": "wastewater-plant",
  "revision": 1,
  "assetId": "wastewater-plant",
  "profile": "agent-console-v1",
  "sourceDigest": "sha256:<64-hex>",
  "tilesetDigest": "sha256:<64-hex>",
  "fileCount": 42,
  "totalBytes": 12345678,
  "toolchain": {
    "contractVersion": "1.0.0",
    "packageVersion": "0.2.0"
  },
  "partition": {
    "maxTriangles": 60000,
    "maxBytes": 67108864,
    "lod": true,
    "lodRatio": 0.5,
    "lodAllocation": "adaptive",
    "lodMaxTriangles": 120000,
    "lodMinUnitTriangles": 8,
    "lodMaxMemoryBytes": 536870912,
    "coarseChunkMaxBytes": 4194304,
    "semanticProxy": "off",
    "renderBatch": "static"
  }
}
```

`tilesetDigest` 对按 path 排序的 Tileset 文件内容和 `bindings.json` 一起计算，不包含
`export.json` 自身。`export.json` 不包含时间戳，因此相同 project revision、profile 和
Toolchain version 会生成相同内容。

## 5. 执行流程

1. CLI 打开指定 project directory，读取当前 committed Document。
2. 调用 Studio 现有 GLB exporter，在 `cache/preview/` 生成 source GLB，并取得 source digest。
3. 启动或复用 `ToolchainService`，确认 Sidecar state 为 ready，并确认
   `asset.partition` capability 可用。
4. 在 project `cache/toolchain/runtime-tilesets/` 中调用 `asset.partition`。
5. 验证返回的 Tileset 和每个 GLB 文件都在 managed cache 内，并校验清单中的 file hash。
6. 从 evaluated Scene metadata 生成 `bindings.json`。
7. 在 output parent 创建唯一 staging directory，复制 partition files、Tileset 和
   manifests。
8. 计算 bundle digest，写入 `export.json`，完成最终校验。
9. 原子发布 staging directory；只有全部成功后才替换目标目录。

## 6. 失败行为

- Toolchain 未配置、不可用、版本不兼容或不具备 partition capability：命令失败，不生成
  final output。
- Sidecar timeout、协议错误、任务拒绝或输出越界：命令失败，保留上一个成功 Bundle。
- source GLB export 或 Scene evaluation 失败：命令失败，不调用 partition。
- binding 聚合发现重复 tag、无效 bounds 或空 nodeSelectionIds：命令失败。
- Tileset 引用缺失文件、hash 不匹配或包含 cache 外路径：命令失败。
- output 已存在且未设置 `--force`：命令失败。
- `--force` 替换失败：恢复 previous output。

## 7. 安全边界

- Sidecar executable 必须来自显式 `--toolchain`、环境变量或 host-managed manifest。
- Sidecar 只能读写 project cache；最终 output 复制由 Studio host CLI 完成。
- CLI 不把 output path 传给 Toolchain Sidecar。
- Tileset、binding 和 export manifest 不包含绝对 source path、session token、
  approval token 或 Agent 凭据。
- 发布不修改 Document、ChangeSet、revision 或 approval state。

## 8. 测试

- CLI parser 覆盖 `export tileset`、默认 profile、`--force` 和缺失 project/output。
- GLB source digest 与 Document revision 一致。
- Toolchain 未配置时明确失败且没有 output。
- capability 缺失、Sidecar failed 和 malicious path response 失败关闭。
- 相同 revision/profile/toolchain version 产生相同 `export.json`、bindings 和 Tileset bytes。
- binding 聚合把同一 tag 的多个派生 Node 合并为一个设备，bounds 覆盖全部 Node。
- 每个 Tileset GLB 和 manifest 在最终 Bundle 中存在，并匹配文件 hash。
- 已有 output 在未使用 `--force` 时保持不变。
- `--force` 的 staging 失败恢复 previous output。
- Chrome 不参与本规格；下一阶段 Agent Console 集成时再执行 HTTP/Tileset 加载与 E2E。

## 9. 非目标

- 不在 Studio Core 中重写空间 partition、LOD 或 Tileset。
- 不调用 Blender。
- 不发布 uncommitted ChangeSet。
- 不修改 Agent Console。
- 不提供任意 partition option 透传。
- 不增加远程上传、对象存储或 CDN 发布。

## 10. 关联文档

- `ICE3-STUDIO-ARC-001` — [Studio 架构](../architecture.md)
- `ICE3-STUDIO-REF-005` — [Toolchain Sidecar](../reference/toolchain-sidecar.md)
- `ICE3-STUDIO-SPEC-014` — [Agent 主导工作台](2026-11-01-agent-led-workbench-design.md)
- `ICE3-DOC-CTR-002` — [Toolchain Sidecar Contract v1](https://github.com/ice-3d/ice3-doc/blob/main/docs/contracts/toolchain-sidecar-contract-v1.md)
