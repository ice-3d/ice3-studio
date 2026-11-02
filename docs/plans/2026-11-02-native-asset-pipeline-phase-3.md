# Studio 原生 Asset Pipeline Phase 3 实施计划

> Document ID: ICE3-STUDIO-PLAN-021
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

## Implementation Outcome

Phase 3 已实现。`studio-import` 提供原生 FBX ASCII/Binary 与 USD ASCII/Binary adapter，
CLI 通过 `--adapter auto|gltf|obj|fbx|usd` 接入。FBX/USD 在 Studio 自带 worker 中解析，
具备硬 timeout、崩溃隔离、协议/response/stderr 上限、scene budgets 和 USD composition
confinement。旧 Toolchain bridge、runtime dependency 与 conformance entry 已退役；
synthetic fixtures 保留为 parity corpus。

最终验证：Studio 18/18 gates 与 Chrome E2E 8/8 通过；Asset Pipeline、Source Import
和 FBX/USD parity 均为 `status=pass`；Toolchain 229/229 tests 通过，生产 package 与
conformance 不再引用已退役 bridge。

**目标：** 用 Studio 内部原生 FBX/USD importer 替换外部 DCC bridge，覆盖 ASCII/Binary、
层级、共享实例、颜色、metadata、单位换算、资源限制、超时和失败关闭。

**架构：** `studio-import` 增加 FBX 与 USD adapter；解析工作在 Studio 自带 worker 进程中运行，
parent 通过带版本号的临时二进制协议接收 `SourceScene`，负责硬超时、进程退出和稳定
Diagnostic。Studio Core 不链接格式 SDK，Worker 通过 feature-gated native dependencies
提供解析能力。

**技术栈：** Rust 1.98、`fbx-rs 0.1.3`、`openusd 0.8.0`、Serde、bincode 2
`serde` feature、Cargo test、
Node.js 24.22.1（仅 Viewer、E2E 和 parity）。

**Spec：** [Studio 原生 Asset Pipeline 迁移设计](../specs/2026-11-01-native-asset-pipeline-migration-design.md)

## Global Constraints

- Phase 3 只覆盖 FBX 与 USD；STEP、IFC、JT 和最终 Toolchain 退役留在后续阶段。
- 新 import 路径不得启动外部 DCC 应用、Node、Toolchain Sidecar 或格式转换 executable。
- Studio Core 不依赖 FBX/USD 原生库；解析隔离在 `studio-import` worker。
- adapter 只能产出 `SourceScene`；Prepared GLB 仍由 `studio-assets::canonical` 生成。
- 外部 references、sublayers、payloads 和资产路径必须限制在源文件目录内。
- 超时必须硬终止 worker；超时、崩溃、协议污染和越界资源都不得发布输出。
- 默认限制：1,000,000 nodes、20,000,000 triangles、512 MiB decoded memory、120 秒。
- 所有 warning、Diagnostic 和输出必须确定性排序。
- Git author/committer 为 `damoqiongqiu <damoqiongqiu@126.com>`。

## Review Focus

- Binary FBX property arrays、嵌套对象和共享 Geometry 不能在解析时越界或无限分配。
- FBX 单位、旋转顺序、坐标手性和 local origin 必须只应用一次。
- USD composition 的 sublayer/reference/payload 不得逃逸源目录或无限递归。
- USD instance/prototype 必须保持共享 Geometry 与稳定 Prim path identity。
- worker timeout 必须终止进程并清理临时文件；失败时 parent 保留上一份输出。

---

### Task 1: 扩展 import 契约、限制和 worker 协议

**文件：**
- Create: `crates/studio-import/src/limits.rs`
- Create: `crates/studio-import/src/worker.rs`
- Create: `crates/studio-import/src/bin/studio-import-worker.rs`
- Create: `crates/studio-import/tests/worker_protocol.rs`
- Modify: `crates/studio-import/Cargo.toml`
- Modify: `crates/studio-import/src/source.rs`
- Modify: `crates/studio-import/src/lib.rs`

**Interfaces:**
- Consumes: Phase 2 `SourceScene`、`SourceImporter`。
- Produces:
  - `NativeImportLimits`
  - `WorkerRequest`
  - `WorkerResponse`
  - `run_isolated_import(adapter, request, limits) -> Result<SourceScene, ImportError>`

- [x] **Step 1: 写 worker 错误与限制测试**

```rust
#[test]
fn rejects_a_scene_above_the_triangle_limit() {
    let scene = scene_with_triangles(11);
    let limits = NativeImportLimits {
        max_triangles: 10,
        ..NativeImportLimits::default()
    };
    let error = validate_scene_limits(&scene, &limits).unwrap_err();
    assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_LIMIT_EXCEEDED");
}

#[test]
fn rejects_a_worker_with_the_wrong_protocol_version() {
    let response = WorkerResponse::V2 { source_scene: fixture_scene() };
    let error = decode_worker_response(
        &bincode::serde::encode_to_vec(&response, bincode::config::standard()).unwrap(),
    )
    .unwrap_err();
    assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_WORKER_PROTOCOL_INVALID");
}
```

- [x] **Step 2: 运行 RED**

```bash
cargo test -p studio-import --test worker_protocol
```

预期：worker 和 limits API 尚不存在。

- [x] **Step 3: 定义限制**

```rust
#[derive(Debug, Clone, Copy)]
pub struct NativeImportLimits {
    pub max_nodes: usize,
    pub max_triangles: u64,
    pub max_memory_bytes: usize,
    pub timeout: std::time::Duration,
}

impl Default for NativeImportLimits {
    fn default() -> Self {
        Self {
            max_nodes: 1_000_000,
            max_triangles: 20_000_000,
            max_memory_bytes: 536_870_912,
            timeout: std::time::Duration::from_secs(120),
        }
    }
}
```

`validate_scene_limits` 在 worker 返回后再次统计 geometry，防止第三方库忽略预算。

- [x] **Step 4: 定义固定 worker 协议**

```rust
#[derive(Serialize, Deserialize)]
pub struct WorkerRequest {
    pub protocol: u32,
    pub adapter: String,
    pub path: PathBuf,
    pub asset_id: String,
    pub limits: WorkerLimits,
}

#[derive(Serialize, Deserialize)]
pub enum WorkerResponse {
    V1 {
        source_scene: SourceScene,
        warnings: Vec<ImportWarning>,
    },
    V2 {
        source_scene: SourceScene,
        warnings: Vec<ImportWarning>,
    },
}
```

协议版本固定为 `1`；`V2` 仅用于解码后明确拒绝。`SourceScene`、geometry 和 metadata
全部增加 Serde 支持。worker 把且只把一个 bincode response 文件写到 request 指定的
sibling 路径；日志写 stderr。parent 限制 response 为 512 MiB、stderr 为 1 MiB。

- [x] **Step 5: 实现硬超时**

`run_isolated_import` 使用 `current_exe() --internal-import-worker REQUEST_PATH` 启动子进程，
把 request 写入同目录临时文件。parent 轮询 `try_wait` 并以 `kill` 终止超时进程。成功时
从 response sibling 文件读取；任何 exit code、协议版本、长度或 bincode 错误都映射为
稳定错误。

- [x] **Step 6: 运行协议测试**

```bash
cargo test -p studio-import --test worker_protocol
cargo test -p studio-import
```

预期：限制、超时、崩溃和污染 stdout 测试全部通过。

- [x] **Step 7: 提交**

```bash
git add Cargo.toml Cargo.lock crates/studio-import
git commit -m "feat(studio): isolate native source import workers"
```

### Task 2: 实现原生 FBX importer

**文件：**
- Create: `crates/studio-import/src/fbx.rs`
- Create: `crates/studio-import/tests/fbx_import.rs`
- Modify: `crates/studio-import/Cargo.toml`
- Modify: `crates/studio-import/src/lib.rs`

**Interfaces:**
- Consumes: `fbx-rs` 的 owned Scene、Task 1 limits。
- Produces: `FbxImporter`，ID `fbx`，扩展名 `[".fbx"]`。

- [x] **Step 1: 写 ASCII、Binary、实例和单位测试**

```rust
#[test]
fn imports_ascii_and_binary_fbx_with_the_same_semantics() {
    let ascii = FbxImporter::new()
        .import_file(&toolchain_fixture("fbx/synthetic-ascii.fbx"), "fbx")
        .unwrap();
    let binary = FbxImporter::new()
        .import_file(&toolchain_fixture("fbx/synthetic-binary.fbx"), "fbx")
        .unwrap();
    assert_eq!(semantic_summary(&ascii), semantic_summary(&binary));
    assert_eq!(ascii.nodes[1].name, "Primary Pump");
    assert_eq!(ascii.materials[0].name, "primary-blue");
}

#[test]
fn preserves_fbx_shared_mesh_instances() {
    let scene = FbxImporter::new()
        .import_file(&toolchain_fixture("fbx/synthetic-binary.fbx"), "fbx")
        .unwrap();
    let mesh_counts = scene
        .nodes
        .iter()
        .filter_map(|node| node.mesh)
        .fold(BTreeMap::new(), |mut counts, mesh| {
            *counts.entry(mesh).or_insert(0_usize) += 1;
            counts
        });
    assert!(mesh_counts.values().any(|count| *count > 1));
}
```

- [x] **Step 2: 运行 RED**

```bash
cargo test -p studio-import --test fbx_import
```

预期：FBX adapter 不存在。

- [x] **Step 3: 实现 FBX 映射**

使用 `fbx_rs::load(bytes, &LoadOptions::default())`，并启用 native decoder 的
memory/mesh/node limits。映射规则：

- node hierarchy、local transforms、mesh ownership 和 shared instance 保持。
- mesh triangles 转为 `SourcePrimitive`；已有 normals/colors 保留，缺失 normals 由
  canonical writer 生成。
- material diffuse/base color 与 opacity 转为 `SourceMaterial`。
- FBX `UnitScaleFactor` 只在 importer 中应用一次；输出统一米制。
- `UserProperties` 只保留 string/bool/finite number 及最多 256 项的浅数组；深度上限 8。
- `selectionId = <assetId>/fbx/node/<ordinal>`，metadata 写入 `extras.ice3.fbx`。
- animation、skin、camera、light、NURBS 和 texture 进入稳定 warning。

- [x] **Step 4: 实现大坐标归零**

按世界 bounds 计算 `[minX,0,minZ]`，只平移 scene root，不修改每个 mesh 的 source
positions。asset extras 记录 `sourceOrigin` 和 `unitAssumption="meter"`。

- [x] **Step 5: 运行 FBX 测试**

```bash
cargo test -p studio-import --test fbx_import
cargo test -p studio-import
```

预期：ASCII/Binary、实例、颜色、metadata、单位、large-origin、empty、truncated 和
corrupt binary 测试全部通过。

- [x] **Step 6: 提交**

```bash
git add Cargo.toml Cargo.lock crates/studio-import
git commit -m "feat(studio): import FBX with a native decoder"
```

### Task 3: 实现原生 USD importer

**文件：**
- Create: `crates/studio-import/src/usd.rs`
- Create: `crates/studio-import/tests/usd_import.rs`
- Modify: `crates/studio-import/Cargo.toml`
- Modify: `crates/studio-import/src/lib.rs`

**Interfaces:**
- Consumes: `openusd` stage、source-directory resolver、Task 1 limits。
- Produces: `UsdImporter`，ID `usd`，扩展名 `[".usda", ".usdc"]`。

- [x] **Step 1: 写 ASCII、Binary、composition 和 instance 测试**

```rust
#[test]
fn imports_ascii_and_binary_usd_with_the_same_semantics() {
    let ascii = UsdImporter::new()
        .import_file(&toolchain_fixture("usd/synthetic-plant.usda"), "usd")
        .unwrap();
    let binary = UsdImporter::new()
        .import_file(&toolchain_fixture("usd/synthetic-plant.usdc"), "usd")
        .unwrap();
    assert_eq!(semantic_summary(&ascii), semantic_summary(&binary));
    assert_eq!(ascii.nodes[1].extras["ice3"]["usd"]["usdPrimPath"], "/World/PrimaryPump");
}

#[test]
fn preserves_usd_point_instancer_identity() {
    let scene = UsdImporter::new()
        .import_file(&fixture("instanced.usda"), "usd")
        .unwrap();
    assert_eq!(
        scene
            .nodes
            .iter()
            .filter(|node| node.mesh.is_some())
            .count(),
        4
    );
    assert_eq!(
        scene
            .nodes
            .iter()
            .filter_map(|node| node.mesh)
            .collect::<BTreeSet<_>>()
            .len(),
        1
    );
}
```

- [x] **Step 2: 运行 RED**

```bash
cargo test -p studio-import --test usd_import
```

预期：USD adapter 不存在。

- [x] **Step 3: 实现 USD stage 映射**

使用 `openusd::usd::Stage` 打开 ASCII/Binary stage。遍历 active prims：

- `Xform` -> node hierarchy 和 local transform。
- `Mesh` -> points、faceVertexIndices、faceVertexCounts、normals、colors、material binding。
- `Xformable` instance/reference -> 展开 stable instance nodes，但共享同一 source mesh。
- 保留 `usdPrimPath`、prim type、customData 白名单和 displayName。
- `metersPerUnit`、`upAxis` 和 handedness 只在 root 应用一次。
- `selectionId = <assetId>/usd/node/<ordinal>`。
- texture、skeleton、camera、light、volume 和 unsupported schema 产生 warning。

- [x] **Step 4: 限制 composition 资源**

为 Stage 配置 source-directory-confined resolver。扫描所有 resolved layer identifiers，
拒绝绝对路径、URI scheme、`..` 和 symlink escape。composition depth 上限 32，总 layer
数上限 1,024。缺失 layer/reference/payload 失败关闭。

- [x] **Step 5: 运行 USD 测试**

```bash
cargo test -p studio-import --test usd_import
cargo test -p studio-import
```

预期：USDA/USDC、composition、instance、color、metadata、unit、missing layer、
truncated、empty 和 cycle 测试全部通过。

- [x] **Step 6: 提交**

```bash
git add Cargo.toml Cargo.lock crates/studio-import
git commit -m "feat(studio): import USD with a native stage"
```

### Task 4: 接入 CLI registry 与 worker

**文件：**
- Create: `crates/studio-cli/tests/fbx_usd_import.rs`
- Modify: `crates/studio-cli/src/main.rs`
- Modify: `crates/studio-cli/Cargo.toml`

**Interfaces:**
- Consumes: `FbxImporter`、`UsdImporter`、`run_isolated_import`。
- Produces: `--adapter auto|gltf|obj|fbx|usd`。

- [x] **Step 1: 写 CLI integration 测试**

```rust
#[test]
fn imports_fbx_and_usd_through_the_internal_worker() {
    for (fixture, adapter, asset_id) in [
        ("fbx/synthetic-binary.fbx", "fbx", "fbx-fixture"),
        ("usd/synthetic-plant.usdc", "usd", "usd-fixture"),
    ] {
        let output = run_import(fixture, adapter, asset_id);
        assert!(output.status.success());
        let source = read_glb(&output.path).unwrap();
        assert_eq!(
            validate_prepared_profile(&source).unwrap().asset_id,
            asset_id
        );
        assert!(!output.stdout.contains("Bridge"));
    }
}

#[test]
fn timeout_does_not_replace_existing_output() {
    let output = install_keep_file();
    let result = run_worker_import_with_timeout(
        fixture("usd/slow.usda"),
        Duration::from_millis(1),
        &output,
    );
    assert_eq!(result.code(), "ICE3_STUDIO_IMPORT_TIMEOUT");
    assert_eq!(read_to_string(output), "keep");
}
```

- [x] **Step 2: 运行 RED**

```bash
cargo test -p studio-cli --test fbx_usd_import
```

预期：registry 未注册 FBX/USD，worker 入口不存在。

- [x] **Step 3: 注册 adapters**

registry 固定顺序为 `gltf`、`obj`、`fbx`、`usd`。FBX/USD 通过
`run_isolated_import` 执行；glTF/OBJ 保持当前 in-process 路径。

CLI 的隐藏子命令：

```text
ice3-studio --internal-import-worker REQUEST_PATH
```

该命令不打印普通用户输出，只写 response 文件和 stderr diagnostic。

- [x] **Step 4: 运行 CLI 测试**

```bash
cargo test -p studio-cli --test fbx_usd_import
cargo test -p studio-cli
```

预期：FBX/USD import、validate、force、timeout、worker crash 和 existing-output
测试全部通过。

- [x] **Step 5: 提交**

```bash
git add crates/studio-cli Cargo.lock
git commit -m "feat(studio-cli): import FBX and USD without external bridges"
```

### Task 5: parity 与失败关闭证据

**文件：**
- Create: `scripts/compare-fbx-usd-imports.mjs`
- Create: `fixtures/parity/fbx-baseline-v1.json`
- Create: `fixtures/parity/usd-baseline-v1.json`

**Interfaces:**
- Consumes: CLI `import` 输出和退役前 baseline。
- Produces: `output/fbx-usd-import-parity.json`。

- [x] **Step 1: 固化退役前 baseline**

使用当前 Toolchain 对 checked-in synthetic fixtures 生成 baseline，记录：

- source digest、triangle/vertex/node/mesh/material count。
- instance node count、selection IDs、bounds、material RGBA。
- warning code、ASCII/Binary semantic digest。

baseline 在迁移前生成并提交，之后不依赖已删除 bridge。

- [x] **Step 2: 实现 parity script**

脚本对比 native CLI 与 baseline。允许 output byte digest 和 vertex welding 差异，但必须
报告；node/mesh/triangle、instance、Selection、bounds、material 和 warning 语义必须一致。

- [x] **Step 3: 运行 parity 与负向 corpus**

```bash
node scripts/compare-fbx-usd-imports.mjs
```

预期：

- `status=pass`。
- empty、truncated、corrupt binary、unsafe USD reference、deep composition、worker
  timeout 和 worker crash 全部失败关闭且不发布 output。

- [x] **Step 4: 提交**

```bash
git add fixtures/parity scripts/compare-fbx-usd-imports.mjs
git commit -m "test(studio): add native FBX and USD parity"
```

### Task 6: 退役旧 FBX/USD bridge 与完整验证

**文件：**
- Modify outside Studio: `ice3-toolchain/packages/cli/src/commands.ts`
- Modify outside Studio: `ice3-toolchain/packages/cli/package.json`
- Modify outside Studio: `ice3-toolchain/package-lock.json`
- Modify outside Studio: `ice3-toolchain/tsconfig.json`
- Modify outside Studio: `ice3-toolchain/tools/conformance/corpus.ts`
- Delete outside Studio: `ice3-toolchain/packages/fbx-importer/`
- Delete outside Studio: `ice3-toolchain/packages/usd-importer/`
- Delete outside Studio: `ice3-toolchain/tests/fbx/`
- Delete outside Studio: `ice3-toolchain/tests/usd/`
- Delete outside Studio: `ice3-toolchain/tests/cli/fbxImportCommand.test.ts`
- Delete outside Studio: `ice3-toolchain/tests/cli/usdImportCommand.test.ts`
- Modify: `README.md`
- Modify: `docs/README.md`
- Modify: `docs/architecture.md`
- Modify: `docs/reference/capabilities.md`
- Modify: `docs/reference/diagnostics.md`
- Modify: `docs/guides/getting-started.md`
- Modify: `docs/specs/2026-11-01-native-asset-pipeline-migration-design.md`
- Modify: `docs/plans/2026-11-02-native-asset-pipeline-phase-3.md`

**Interfaces:**
- Consumes: Studio native FBX/USD importer 和三份 parity report。
- Produces: Toolchain bridge retirement、Phase 3 Implemented 状态和完整验证证据。

- [x] **Step 1: 删除 Toolchain adapters 与 runtime dependencies**

删除 FBX/USD npm adapters、Python bridge、bridge tests 和 lazy imports。移除
`assimpjs`、外部 DCC runtime 环境变量、CLI help 项及 TypeScript project references。
保留 fixtures 作为 parity corpus。

- [x] **Step 2: 更新 Toolchain conformance**

Toolchain conformance 缩减为 glTF/STEP/IFC/JT 四项。FBX/USD 改由 Studio native parity
覆盖，不再出现在 Toolchain manifest。

- [x] **Step 3: 更新 Studio 文档与阶段状态**

文档必须说明：

- Studio 原生支持 glTF/GLB、OBJ/MTL、FBX ASCII/Binary、USD ASCII/Binary。
- FBX/USD 解析在内部 worker，外部 DCC 不需要安装。
- 资源限制、硬超时、composition confinement 和失败关闭语义。
- Phase 3 milestone 标记 `Implemented`；整份 Spec-016 仍为 `Approved`。
- Toolchain 只保留 STEP、IFC、JT 迁移兼容能力。

- [x] **Step 4: 更新 AGENTS 边界**

Studio 架构规则改为 FBX/USD 由 `studio-import` worker 处理；不得恢复外部 DCC bridge。
Toolchain 规则同步删除 FBX/USD 生产职责。

- [x] **Step 5: 运行完整验证**

```bash
node ../ice3-doc/scripts/check-docs.mjs --root .
PLAYWRIGHT_CHANNEL=chrome npm run verify
node scripts/compare-asset-pipelines.mjs
node scripts/compare-source-imports.mjs
node scripts/compare-fbx-usd-imports.mjs
```

在 `ice3-toolchain`：

```bash
npm run verify
rg -n "fbx-importer|usd-importer|assimpjs|blender" packages tests tools package.json package-lock.json
```

预期：

- Studio 18 道 gate 通过。
- Toolchain 全部测试通过。
- 三份 parity report 通过。
- Toolchain 生产源码、package 和 conformance 不再引用 FBX/USD bridge 或外部 DCC。

- [x] **Step 6: 提交**

Studio：

```bash
git add README.md AGENTS.md docs
git commit -m "docs(studio): complete native FBX and USD import phase 3"
```

Toolchain：

```bash
git add -A
git commit -m "refactor(toolchain): retire FBX and USD source bridges"
```
