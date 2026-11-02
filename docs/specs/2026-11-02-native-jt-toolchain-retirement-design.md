# Studio 原生 JT Import 与 Toolchain 最终退役设计

> Document ID: ICE3-STUDIO-SPEC-018
>
> Status: Approved
>
> Owner: ICE3 Studio maintainers

> **阶段状态：** Phase 1 至 Phase 4 已实现。本规格覆盖 Phase 5：JT 迁移、Sidecar
> 退场、Studio 独立验证和 Toolchain repository 归档准备。

**目标：** 将 JT 的受控 converter bridge 和全部相关调用迁入 Studio `studio-import`，
随后删除 Studio 中的 Toolchain Sidecar、客户端、预览 fallback、配置、状态 UI 和构建
耦合，使 Studio 成为所有 Source Import、Prepared GLB、Runtime Tileset 和 Agent
交付的唯一产品入口。

**上级设计：** [Studio 原生 Asset Pipeline 迁移](2026-11-01-native-asset-pipeline-migration-design.md)

## 1. 背景

Phase 1 至 Phase 4 已将 glTF/GLB、OBJ/MTL、FBX、USD、STEP AP242 和 IFC4X3 的
Source Import 迁入 Studio。迁移后的产品路径不再通过 Toolchain 导入这些格式，但仍有
三类历史连接：

1. `.jt` 的 converter、GLB canonicalization 和 Diagnostic 仍由 Toolchain TypeScript
   package 提供。
2. Studio Server 仍链接 `studio-toolchain-client`，启动可选 Sidecar，并通过
   `system.capabilities`、`ToolchainPanel` 和预览 fallback 暴露其状态。
3. Studio 多个 importer 测试、parity 脚本、Viewer E2E、benchmark 和验证脚本仍直接
   读取 `../ice3-toolchain` 的 fixtures 或构建 Toolchain package。

因此当前并不是完整的“Studio 独立”。即使未配置 Sidecar 时编辑和预览仍可运行，测试与
发布流程仍隐含要求旧 repository 存在。

## 2. 成功标准

- 在主机没有 Node Toolchain package、没有 Sidecar、没有 Toolchain CLI 的情况下，
  `ice3-studio import SOURCE.jt --output FILE --asset-id ID` 可以使用组织批准的
  converter 生成有效 Prepared GLB。
- JT 解析仍在隔离 worker 中运行；converter 使用固定参数、硬 timeout、进程树终止、
  bounded stdout/stderr、输出大小和 GLB 完整性校验。
- JT 输出的 hierarchy、shared mesh instances、materials、Selection identity、
  object/assembly metadata、custom properties 和 shared origin 与退役前 baseline
  等价。
- Studio Server 的 committed preview 和 candidate preview 始终由 Studio Core 直接
  生成，不再尝试启动或探测外部 Sidecar。
- `system.capabilities`、公开 bindings、Viewer、本地配置和安装包不再包含 Toolchain
  状态或 executable 发现逻辑。
- Studio 的 source fixtures、parity baseline、测试、benchmark 和 `npm run verify`
  不依赖相邻 Toolchain repository。
- 关闭 `jt` feature 后，Studio 不注册 JT adapter，也不包含 converter 调用代码；默认
  开发构建启用 `jt`，以保留当前格式能力。
- Toolchain repository 不再含 Studio 可调用的生产 adapter、Sidecar package、release
  入口或 Studio 发布依赖，可以进入归档审批。

## 3. 非目标

- 不在本阶段实现 JT binary parser。JT 是迁移规格允许的唯一受控外部 converter
  exception。
- 不随 Studio 分发 converter binary，不自动下载、安装或替换 converter。
- 不允许 Agent、RPC 请求、项目文件或命令行参数任意指定 converter executable、
  参数、shell fragment 或网络服务。
- 不扩展 JT 版本、PMI、材质或几何能力；验收以当前 converter contract 和合成
  corpus 为准。
- 不保留 Sidecar 双路径、兼容代理或长期 feature switch。
- 不改变 Document、ChangeSet、Operation 或 geometry kernel 的语义。
- 不在本规格批准范围内递归删除整个 Toolchain repository。归档或删除 repository
  是完成全部退役门禁后的独立破坏性操作，必须再次取得明确确认。

## 4. 最终数据流

```text
ice3-studio import SOURCE.jt --output FILE --asset-id ID
└── studio-cli
    ├── resolve jt adapter
    ├── start isolated Studio import worker
    │   └── studio-import::jt
    │       ├── launch host-configured converter
    │       │   --input SOURCE.jt --output RAW.glb
    │       ├── validate and import RAW.glb
    │       ├── canonicalize JT semantics
    │       └── return SourceScene
    ├── studio-assets::canonical
    ├── Prepared GLB validation
    └── atomic output publication
```

preview 路径不再经过 Sidecar：

```text
Document
└── Studio evaluated Geometry
    └── export_source
        └── cache/preview/<revision>.glb
            └── Studio Runtime preview response
```

## 5. JT adapter 公共契约

### 5.1 Feature 与格式

`studio-import` 增加 default-on、无第三方 Cargo dependency 的 `jt` feature：

```toml
[features]
default = ["fbx", "usd", "step", "ifc", "jt"]
jt = []
```

`studio-cli` 同名透传。Adapter 契约为：

- id：`jt`
- extension：`.jt`
- `SourceFormat::Jt`
- CLI `--adapter` 可接受值为
  `auto|gltf|obj|fbx|usd|step|ifc|jt`
- capabilities：hierarchy、material colors、instances、semantic metadata；不承诺
  vertex colors

关闭 `jt` feature 时：

- adapter 不出现在 registry；
- `--adapter jt` 返回 `ICE3_STUDIO_IMPORT_ADAPTER_UNAVAILABLE`；
- converter 环境变量和调用实现不进入产物；
- `cargo tree` 不增加 JT 专用 parser 或 process dependency。

### 5.2 Converter 配置

Converter 只由 host-owned 环境提供：

| 变量 | 规则 |
|---|---|
| `ICE3_JT_CONVERTER` | 必填；converter executable 路径。相对路径按 Studio 进程工作目录解析。 |
| `ICE3_JT_TIMEOUT_MS` | 可选；正整数毫秒，默认 `300000`。 |

路径必须解析为现有、可执行的 regular file。缺失、不可执行或不是 file 时失败，不得从
`PATH` 隐式选择同名工具，不得根据输入内容或 Agent 请求切换 converter。

CLI 调用固定为：

```text
CONVERTER --input ABSOLUTE_SOURCE.jt --output ABSOLUTE_TEMP/raw.glb
```

不经过 shell，不透传额外参数，不把 converter stdout/stderr 写入最终 report。stdout 和
stderr 分别保留最多 64 KiB 的内部诊断窗口并持续 drain，避免子进程因 pipe 阻塞。

### 5.3 Converter 输出契约

Converter 必须输出米制、right-handed、Y-up 的标准 GLB。输出必须满足：

- 是 regular file，不能是目录或符号链接；
- 文件不得超过 512 MiB 的硬输出上限；
- 至少包含完整 GLB header；
- magic 为 `glTF`，version 为 2，declared length 与文件大小一致；
- JSON chunk 和 BIN chunk 边界合法；
- 可由 `studio-assets::read_glb` 读取；
- 最终包含至少一个三角形。

源文件、converter output 和最终 export 都位于独立临时目录。任何失败都删除临时 GLB，
并且不得修改目标 output。最终 output 继续由 Studio CLI 使用 write-then-atomic-rename
发布。

### 5.4 Link、容器与所有权

GLB 中不属于 source geometry 语义的 animations、skins、cameras 和 textures 不进入
`SourceScene`。遇到这些资源时产生稳定 warning，不解析、转换或发布对应 runtime
resource。

共享 mesh 由多个 node 引用时只保留一份 mesh，并保留 node instances。节点、mesh 和
material 顺序由 source GLB 顺序决定；metadata 和 warnings 在序列化前稳定排序。

## 6. JT 语义映射

### 6.1 节点与 Selection

导入器先确定最终 `SourceScene.nodes` 的确定性顺序，再按 ordinal 生成：

```text
<assetId>/jt/node/<ordinal>
```

Root、empty assembly node、mesh occurrence 和共享 mesh instance 都保持为独立业务节点。
空名称使用 `jt/node/<ordinal>`，不会因文件系统路径不同而变化。

### 6.2 JT metadata

Converter 可把可选输入放在源 node 的 `extras.jt`：

```json
{
  "objectId": "JT-PUMP-101",
  "assemblyPath": "/Plant/TreatmentLine/PrimaryPump",
  "customProperties": {
    "EquipmentTag": "P-101"
  }
}
```

Studio 输出节点统一写入 `extras.ice3`：

```json
{
  "selectionId": "plant/jt/node/4",
  "displayName": "PrimaryPump",
  "jt": {
    "objectType": "MESH",
    "sourceName": "PrimaryPump",
    "materialNames": ["PumpBlue"],
    "objectId": "JT-PUMP-101",
    "assemblyPath": "/Plant/TreatmentLine/PrimaryPump",
    "instanceGroup": "jt/instance/1",
    "customProperties": {
      "EquipmentTag": "P-101"
    }
  }
}
```

规则：

- `objectType` 为 `MESH` 或 `EMPTY`；
- `materialNames` 来自对应 mesh primitives，去重后排序；
- 只有同一 mesh 被两个或更多 nodes 引用时才写稳定的 `instanceGroup`；
- `objectId` 和 `assemblyPath` 只接受 string；
- custom properties 只接受 finite number、string、boolean 和受长度限制的 array；
- custom property object、array 深度最多 8，array 最多 256 项，key 稳定排序；
- 不支持的值被省略，不序列化 converter-specific object、异常或绝对路径。

### 6.3 单位与 shared origin

Converter boundary 规定输出已换算为米、right-handed、Y-up。Studio 不再重复缩放或手性
转换。

Studio 基于 source GLB 的 `f64` node transforms 和 accessor bounds 计算 world bounds，
得到：

```text
sourceOrigin = [minX, 0, minZ]
```

rebase 在映射到 `f32` `SourceScene` 之前完成：

- 优先平移唯一 source scene root；
- 多个 roots 时创建一个稳定的 `JT Root` 并统一平移；
- 不逐个修改 mesh local positions，以保留共享 mesh；
- asset extras 写入 converter contract version、source origin 和 coordinate system；
- 最终 committed output 不暴露 converter 临时路径或原模型绝对路径。

## 7. 隔离与失败关闭

JT adapter 与其他工业格式一样通过 `run_isolated_import` 在 Studio 自有 worker 中运行。
worker 再启动 converter，形成受控的两级进程树。

生产 CLI 的执行策略：

- worker 是独立 process group/session leader；
- converter 及其 descendants 属于同一可终止进程树；
- JT worker deadline 为 `ICE3_JT_TIMEOUT_MS` 加 5 秒发布宽限；
- 超时先向进程树发送 terminate，1 秒后仍未退出则 force terminate；
- Windows 使用等价 Job Object containment；不得只终止直接 child；
- converter stdout/stderr、worker stderr 和 worker response 都有独立上限；
- worker crash、协议损坏和输出超限均 fail closed；
- 暂存目录由 worker scope 清理，失败不产生公开 artifact。

保留 converter 语义的稳定 Diagnostic：

| 条件 | Studio code |
|---|---|
| converter 未配置或不可执行 | `ICE3_STUDIO_IMPORT_JT_CONVERTER_UNAVAILABLE` |
| converter deadline 到期 | `ICE3_STUDIO_IMPORT_JT_TIMEOUT` |
| converter 非零退出、启动失败或无合法输出 | `ICE3_STUDIO_IMPORT_JT_CONVERT_FAILED` |
| 输出不是合法 GLB | `ICE3_STUDIO_IMPORT_JT_EXPORT_INVALID` |
| Geometry、bounds 或 transform 无效 | `ICE3_STUDIO_IMPORT_JT_GEOMETRY_INVALID` |
| 无 triangle geometry | `ICE3_STUDIO_IMPORT_EMPTY_GEOMETRY` |
| output 超过 byte limit | `ICE3_STUDIO_IMPORT_LIMIT_EXCEEDED` |
| worker protocol 或 response 无效 | 沿用 Studio worker Diagnostic |

旧 `ICE3_TOOL_JT_*` code 到新 code 的映射写入迁移说明。Studio 新输出只发布
`ICE3_STUDIO_IMPORT_*` code；原始 converter stderr、客户路径和容器内部错误不进入
公开 Diagnostic。

## 8. Studio Sidecar 退场

### 8.1 Server 与 RPC

删除：

- `crates/studio-toolchain-client/`；
- `crates/studio-server/src/toolchain.rs`；
- Cargo workspace、`studio-server` 和 lockfile 中的 client dependency；
- `ServerConfig::toolchain_cli` 及所有 test-only injection；
- RPC session 中的 `ToolchainService`、start、shutdown、reconfigure 和 status；
- `system.capabilities.toolchain` 字段及其 generated schema/binding；
- Toolchain-specific RPC error conversion；
- `cache/toolchain/` 输出路径。

`export.runtimePreview` 和 candidate preview 直接使用 Studio Core 产生的 source GLB。
Preview content hash 为目标 GLB 的 SHA-256；asset ID 继续使用请求的 asset ID 或 candidate
digest。回退逻辑不再是“可选外部优化器”，因为不再存在第二条 preview pipeline。

新的派生目录为：

```text
cache/preview/<revision>.glb
cache/preview/candidates/<candidate-digest>.glb
cache/candidates/<candidate-digest>-source.glb
```

`GET /previews/...`、packet 和 candidate URL 保持现有公开路由形状。

### 8.2 Viewer

删除 `ToolchainStatus` API type、session state、`ToolchainPanel`、相关 CSS 和 unit/E2E
断言。工作台不得用固定文字伪装外部服务状态；Agent mission control、对象树、Scene
canvas、评审 dock 和人工接管能力保持不变。

现有“Toolchain incompatible 时仍可编辑”的 E2E 改为回归验证：

- Studio capabilities 不包含 Toolchain 字段；
- committed preview 和 candidate preview 正常生成；
- canvas、对象树和评审入口保持可用；
- 页面不出现 Toolchain 状态或不可用提示。

### 8.3 构建、测试与发布

删除 Studio repository 中的：

- Sidecar staging/verification/launch scripts 与 fixtures；
- `toolchain:stage`、`toolchain:verify` npm scripts；
- Toolchain bundle tests 和 independence 脚本中的 Sidecar 场景；
- Viewer `predev`、`pretest:e2e` 中的 Toolchain build；
- benchmark 中的 Toolchain build、`STUDIO_TOOLCHAIN_CLI` 和 `ICE3_TOOLCHAIN_CLI`；
- E2E server 的 Toolchain adapter 注入；
- Toolchain Sidecar reference guide、diagnostic、环境变量和架构描述。

“Toolchain independence”验证保留其有用含义并改为“Studio standalone”：在不存在相邻
Toolchain repository、Toolchain package 或 Sidecar 的环境中运行 CLI、preview、
candidate preview、export、pack/unpack 和 API 流程。

## 9. Fixture 与 parity 所有权

在删除 Toolchain adapter 之前，把 Studio 仍引用的 synthetic source corpus 和 JT
contract fixtures 复制到 Studio 自有 fixture root：

```text
fixtures/source-import/
├── gltf/
├── obj/
├── fbx/
├── usd/
├── step/
├── ifc/
└── jt/
```

要求：

- 只复制虚构、脱敏且可再分发的文件；
- 复制后校验 SHA-256，Rust 和 Node 测试全部改为 Studio 相对路径；
- baseline JSON 中旧的 source path 改为 Studio fixture 路径，但保留旧 Toolchain
  统计作为 `retiredBaseline` 证据；
- 新增 `scripts/compare-jt-imports.mjs`，从旧 Toolchain JT adapter 和 Studio `jt`
  adapter 双跑相同 corpus；
- parity 通过后才删除 Toolchain JT 生产 adapter；
- Studio 最终验证不要求 Toolchain repository 或其 `node_modules` 存在。

JT parity corpus 至少覆盖：

- synthetic contract；
- large origin；
- empty geometry；
- shared mesh instances；
- material names；
- object ID、assembly path 和 custom properties；
- converter unavailable；
- converter timeout；
- converter 非零退出；
- invalid GLB；
- converter 输出超过上限。

## 10. Toolchain repository 退役

Studio 不再引用 Toolchain 后，Toolchain repository 执行以下收口：

- 删除 JT adapter package、CLI import 分支、runtime tests 和 release check 中的 JT
  production path；
- 删除仅服务 Studio 的 Sidecar package、contract conformance、launch scripts 和
  frozen preview fixture；
- 删除 Studio 专用 npm scripts、release bundling 和依赖；
- 保留 LICENSE、NOTICE 和必要的历史迁移记录；
- 将历史 JT、Sidecar、Wastewater authoring 文档标为 Historical；
- README 和 architecture 明确 Toolchain 不再是 Studio 的生产依赖；
- `npm run verify` 不再要求构建或发布 Studio component。

上述清理完成并通过 Studio standalone 验收后，才可提出 repository archive。归档前必须
再次确认没有未合并分支、未提交 fixture、客户模型或仍需保留的 release artifact。

## 11. 实施阶段

### Phase 5A：Native JT bridge 与 baseline

- 增加 `jt` feature、`SourceFormat::Jt`、CLI adapter 和隔离 worker 路径。
- 实现受控 converter 执行、GLB 校验、JT metadata、Selection、shared origin 和
  Diagnostic。
- 复制 synthetic fixture，建立旧/新 parity baseline。
- 此阶段保留旧 Toolchain adapter 和 Sidecar，仅用于证据对照。

**退出条件：** Studio JT 正向、负向、确定性、资源上限和 parity 全部通过。

### Phase 5B：Studio 切换为唯一生产路径

- CLI、测试和 parity 改用 Studio JT adapter。
- 删除 Toolchain JT adapter、CLI import 分支和 release JT 验证。
- 移除 Studio Server 的 Sidecar client、配置、状态和 preview fallback。
- 移除 Viewer Toolchain surface、build hooks 和 benchmark 注入。
- 所有 source fixtures 改为 Studio-local。

**退出条件：** 不启动 Toolchain 的情况下 Studio 全功能验证通过。

### Phase 5C：最终退役与归档准备

- 执行 repository-wide dependency、script、docs 和客户模型审计。
- 删除剩余 Toolchain release/package 入口。
- 更新上级 Spec-016、文档索引、AGENTS、第三方声明、CLI 和 capability reference。
- 输出可复核的归档清单；等待单独授权后归档 repository。

**退出条件：** 上级 Phase 5 可标记 Implemented，Toolchain repository 可安全归档。

## 12. 验收门禁

### 12.1 Rust 与 API

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo tree -p studio-cli --no-default-features` 不包含 Toolchain client、Node
  runtime 或 JT parser dependency
- generated schema、TS bindings 和 API fixtures 不含 removed Toolchain fields
- committed/candidate preview URL、revision、digest 和 packet 测试通过

### 12.2 JT

- synthetic hierarchy、instances、materials、metadata 和 Selection parity 通过；
- large-origin sourceOrigin 和 shared root rebase 通过；
- 重复导入的 counts、semantics 和 output digest 稳定；
- converter shell 不执行，参数注入和请求控制路径被拒绝；
- unavailable、timeout、crash、invalid GLB、oversized output 和 empty geometry
  fail closed；
- timeout 后 worker、converter 和 descendants 均退出，没有遗留临时 output；
- default build 注册 `jt`，no-default build 返回
  `ICE3_STUDIO_IMPORT_ADAPTER_UNAVAILABLE`。

### 12.3 Standalone 与文档

- 临时移走/隐藏 `../ice3-toolchain` 后，Studio `npm run verify` 仍通过；
- `rg` 在 production source、scripts、package manifests、fixtures 和 active docs 中
  不发现 Toolchain package、Sidecar executable 或 `/cache/toolchain/`；
- historical documents 可以保留旧名称，但必须明确标注 Historical/Deprecated；
- CLI help、API、capabilities、diagnostics、architecture、README 和 AGENTS 与代码一致；
- dependency audit 为 0 个 high/critical vulnerability；
- Viewer E2E 使用用户安装的 Google Chrome，验证 Agent-first 主流程、对象选择、
  ChangeSet 审批、原生 preview 和响应式布局。

### 12.4 Repository 归档

- Studio `main` 不引用 Toolchain repository 或 npm package；
- Toolchain 没有客户模型、未提交 fixture、release 临时目录或未合并必要分支；
- LICENSE、NOTICE 和历史迁移记录完整；
- 在独立确认后才执行 archive/delete。

## 13. 风险与控制

| 风险 | 控制 |
|---|---|
| Converter 忽略 terminate 并产生孤儿进程 | 生产路径终止整个 process tree；1 秒后 force terminate |
| Large-origin GLB 在 `f32` 转换后抖动 | 在 source JSON/world transform 阶段用 `f64` 计算并先应用 shared origin |
| 删除 Sidecar 后 preview 性能回退 | 以 Studio native preview 建立新的 benchmark 基线，不用缺失外部进程掩盖结果 |
| Fixtures 留在旧 repo 形成假独立 | 复制、hash、重写所有 test path，并在隐藏旧 repo 后执行 full verify |
| Toolchain 与 Studio 同时保留 JT 语义实现 | Phase 5A 双跑只作为 parity oracle；Phase 5B 立即删除旧生产 adapter |
| 归档 repository 造成不可恢复损失 | Repository archive/delete 必须单独确认；先保留可恢复快照和归档清单 |

## 14. 关联文档

- `ICE3-STUDIO-SPEC-016` — [Studio 原生 Asset Pipeline 迁移](2026-11-01-native-asset-pipeline-migration-design.md)
- `ICE3-STUDIO-SPEC-017` — [Studio 原生 STEP 与 IFC Source Import](2026-11-02-native-step-ifc-import-design.md)
- `ICE3-STUDIO-SPEC-004` — [历史 Toolchain Sidecar Integration](2026-10-30-studio-toolchain-sidecar-design.md)
- `ICE3-STUDIO-ARC-001` — [Studio 架构](../architecture.md)
- `ICE3-STUDIO-POL-001` — [仓库规则](../../AGENTS.md)
