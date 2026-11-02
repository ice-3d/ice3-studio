# Studio Sidecar 最终退役实施计划

> Document ID: ICE3-STUDIO-PLAN-025
>
> Status: Proposed
>
> Owner: ICE3 Studio maintainers

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> `superpowers:subagent-driven-development` (recommended) or
> `superpowers:executing-plans` to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal：** 删除 Studio 的 Toolchain Sidecar 客户端、Server 状态、preview fallback、
Viewer 状态面、构建注入和发布脚本，使 committed/candidate preview 只使用 Studio
原生产物，Studio 可以在没有相邻 Toolchain repository 的环境完整验证。

**架构：** `PreviewAdapter` 直接消费 `studio-core` 求值出的 GLB，并把 revision 或
candidate digest 映射到 `cache/preview/`。Server、RPC、bindings 和 Viewer 不再拥有
Toolchain transport、配置或状态；测试与 benchmark 删除 Sidecar fake。

**技术栈：** Rust 1.98、Axum、Tokio、Serde、ts-rs、schemars、React 19、Node.js
24.22.1、Chrome E2E。

**Spec：** [Studio 原生 JT Import 与 Toolchain 最终退役](../specs/2026-11-02-native-jt-toolchain-retirement-design.md)

## Global Constraints

- 本计划以 `ICE3-STUDIO-PLAN-024` 先完成 JT 迁移为前提。
- Studio Server 不得启动、探测、配置或报告 Toolchain/Sidecar。
- committed preview 和 candidate preview 只有一个实现：Studio Core export。
- `system.capabilities` 不再含 `toolchain`。
- committed preview 固定写 `cache/preview/<revision>.glb`。
- candidate source 固定写 `cache/candidates/<digest>-source.glb`。
- candidate preview 固定写 `cache/preview/candidates/<digest>.glb`。
- 公开 route 形状保持 `/previews/...` 与 `/preview-packets/...`。
- 不保留 Toolchain compatibility field、deprecated executable env 或隐藏 fallback。
- 所有 source fixture 与 benchmark 不得读取 `../ice3-toolchain`。
- Viewer E2E 必须使用用户安装的 Google Chrome，并复用已有窗口/tab。
- Git author/committer 固定为 `damoqiongqiu <damoqiongqiu@126.com>`。

## Review Focus

- committed preview 与 candidate preview 必须具有可重算 SHA-256，不能返回旧 Sidecar
  asset ID 或指向不存在文件。
- 删除 client 后 project.open、project.close、changeSet commit 和 server shutdown 不得
  留下 Sidecar handle、线程、child process 或 stale status。
- Capabilities/schema/Viewer 必须同步移除 `toolchain`，不能只删除 UI 而继续暴露旧字段。
- benchmark 与 E2E 不得通过 fake toolchain 绕过原生 preview，否则会隐藏真实性能或契约。
- 隐藏/移动 `../ice3-toolchain` 后 `npm run verify` 必须仍通过，测试不能偷偷读取旧 repo。

---

### Task 1: 把 committed 与 candidate preview 固化为原生路径

**Files：**
- Modify: `crates/studio-server/src/preview.rs`
- Modify: `crates/studio-server/src/changeset.rs`
- Modify: `crates/studio-server/src/rpc.rs`
- Replace: `crates/studio-server/tests/preview_fallback.rs`
  with `crates/studio-server/tests/native_preview.rs`
- Delete: `crates/studio-server/tests/preview_pipeline.rs`

**Interfaces：**
- Consumes:
  - `export_source_with_scene(&Document, &Path, Option<&dyn GeometryEvaluator>)`
  - `export_glb_from_scene(&Document, &EvaluatedScene)`
- Produces:
  - `PreviewAdapter::new() -> Self`
  - `PreviewAdapter::generate(...) -> Result<RuntimePreview, PreviewError>`
  - `PreviewAdapter::generate_candidate(...) -> Result<CandidatePreview, PreviewError>`

- [ ] **Step 1: 写原生 preview 失败测试**

`crates/studio-server/tests/native_preview.rs`：

```rust
mod common;

use sha2::{Digest, Sha256};
use studio_server::{ServerConfig, start_test_server};

#[tokio::test]
async fn committed_preview_is_written_by_studio_without_driver_state() {
    let server = start_test_server(ServerConfig::for_tests("native-preview")).await;
    server.apply_fixture_batch().await.unwrap();
    let response = common::rpc(
        &server,
        "export.runtimePreview",
        serde_json::json!({}),
    )
    .await;
    let preview = &response["result"];
    let path = std::path::PathBuf::from(preview["previewPath"].as_str().unwrap());
    let bytes = std::fs::read(&path).unwrap();

    assert_eq!(&bytes[..4], b"glTF");
    assert!(path.ends_with("cache/preview/00000001.glb"));
    assert_eq!(
        preview["contentHash"],
        format!("{:x}", Sha256::digest(&bytes))
    );
    assert!(!server
        .workspace_root()
        .join("native-preview/cache/toolchain")
        .exists());
}

#[tokio::test]
async fn candidate_preview_uses_the_native_candidate_path() {
    let server = start_test_server(ServerConfig::for_tests("native-candidate")).await;
    let candidate = common::create_and_evaluate_candidate(&server).await;
    let digest = candidate["digest"].as_str().unwrap();
    let response = common::rpc(
        &server,
        "export.runtimePreview",
        serde_json::json!({ "candidateDigest": digest }),
    )
    .await;

    assert_eq!(response["result"]["assetId"], digest);
    let preview = server
        .workspace_root()
        .join("native-candidate")
        .join("cache/preview/candidates")
        .join(format!("{digest}.glb"));
    assert!(preview.is_file());
    assert!(!server
        .workspace_root()
        .join("native-candidate/cache/toolchain")
        .exists());
}
```

在 `tests/common/mod.rs` 增加：

```rust
pub async fn rpc(
    server: &TestServer,
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    rpc_with_bearer(server, server.token(), method, params).await
}

pub async fn create_and_evaluate_candidate(server: &TestServer) -> serde_json::Value {
    let created = rpc(
        server,
        "changeset.create",
        serde_json::json!({ "baseRevision": 0, "title": "candidate preview" }),
    )
    .await;
    let change_set_id = created["result"]["id"].as_str().unwrap();
    rpc(
        server,
        "changeset.addOperations",
        serde_json::json!({
            "changeSetId": change_set_id,
            "expectedVersion": 0,
            "entries": [{
                "source": { "kind": "agent", "label": "native-preview" },
                "operation": {
                    "type": "node.create",
                    "node_id": "node-000001",
                    "parent_id": null,
                    "name": "Candidate tank",
                    "transform": {
                        "translation": { "x": 0, "y": 0, "z": 0 },
                        "rotation": [0, 0, 0, 1],
                        "scale": { "x": 1, "y": 1, "z": 1 }
                    },
                    "content": { "type": "group" },
                    "material_id": null,
                    "metadata": {}
                }
            }]
        }),
    )
    .await;
    let evaluated = rpc(
        server,
        "changeset.evaluate",
        serde_json::json!({
            "changeSetId": change_set_id,
            "expectedVersion": 1,
            "includeCandidate": false
        }),
    )
    .await;
    assert_eq!(evaluated["result"]["status"], "ready");
    evaluated["result"].clone()
}
```

- [ ] **Step 2: 运行 RED**

```bash
cargo test -p studio-server --test native_preview
```

Expected: FAIL，路径仍是 `cache/toolchain/...`，且 `PreviewAdapter::new` 仍要求
toolchain script。

- [ ] **Step 3: 删除 preview fallback**

`preview.rs` 删除：

```rust
use crate::toolchain::ToolchainService;
enum PreviewError { ... InvalidToolchain, Toolchain(...) }
fn toolchain_is_optional(...)
struct PreviewAdapter { toolchain: Arc<ToolchainService>, ... }
```

构造器变为：

```rust
impl PreviewAdapter {
    pub fn new() -> Self {
        Self {
            preview_packets_enabled: false,
            compute: None,
            scene_preview: None,
        }
    }
}

impl Default for PreviewAdapter {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 4: 统一 preview publication**

committed preview：

```rust
let preview_dir = project_dir.join("cache/preview");
std::fs::create_dir_all(&preview_dir).map_err(PreviewError::Io)?;
let preview_path = preview_dir.join(format!("{revision:08}.glb"));
std::fs::copy(&source_path, &preview_path).map_err(PreviewError::Io)?;
let content_hash = format!(
    "{:x}",
    Sha256::digest(std::fs::read(&preview_path).map_err(PreviewError::Io)?)
);
(asset_id.to_string(), content_hash)
```

candidate preview：

```rust
let preview_dir = project_dir.join("cache/preview/candidates");
let preview_path = preview_dir.join(format!("{digest}.glb"));
if valid_glb(&preview_path) {
    return cached_candidate_preview(digest, &preview_path);
}
// Existing Studio evaluation, packet and source export stay unchanged.
std::fs::copy(&source_path, &preview_path).map_err(PreviewError::Io)?;
let content_hash = source_digest.clone();
```

`cached_candidate_preview` 重新计算文件 SHA-256 和 asset ID，不信任缓存文件名。

- [ ] **Step 5: 更新所有构造点**

`rpc.rs` 和 `changeset.rs` 中：

```rust
PreviewAdapter::new()
```

保留 `.with_compute(...)`、`.with_preview_packets(...)` 和
`.with_scene_preview(...)` 组合。

- [ ] **Step 6: 删除旧 preview fake tests**

删除 `preview_pipeline.rs`。原 `preview_fallback.rs` 中“没有外部 toolchain”已不再是
可选 fallback，而是唯一 native path，由 `native_preview.rs` 取代。

- [ ] **Step 7: 运行 GREEN**

```bash
cargo test -p studio-server --test native_preview
cargo test -p studio-server --test changeset_rpc
cargo test -p studio-server --test preview_packet
```

Expected: PASS；所有 previewPath 都不含 `cache/toolchain`。

- [ ] **Step 8: 提交**

```bash
git add crates/studio-server/src/preview.rs \
  crates/studio-server/src/changeset.rs crates/studio-server/src/rpc.rs \
  crates/studio-server/tests
git commit -m "refactor(studio-server): make native preview the only path"
```

---

### Task 2: 删除 Server Toolchain client、配置和 capability

**Files：**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `crates/studio-server/Cargo.toml`
- Modify: `crates/studio-server/src/lib.rs`
- Modify: `crates/studio-server/src/rpc.rs`
- Modify: `crates/studio-cli/src/main.rs`
- Modify: `crates/studio-core/src/validation.rs`
- Delete: `crates/studio-server/src/toolchain.rs`
- Delete: `crates/studio-toolchain-client/`
- Delete: `crates/studio-server/bindings/ToolchainState.ts`
- Delete: `crates/studio-server/bindings/ToolchainStatus.ts`
- Modify: `crates/studio-server/bindings/Capabilities.ts`
- Modify: `crates/studio-server/tests/common/mod.rs`
- Modify: `crates/studio-server/tests/rpc_contract.rs`
- Delete: `crates/studio-server/tests/toolchain_sidecar.rs`

**Interfaces：**
- Consumes: native preview from Task 1 at every preview call site.
- Produces:
  - `ServerConfig` without `toolchain_cli`
  - `Session` without `ToolchainService`
  - API capabilities without `toolchain`
  - no `studio-toolchain-client` dependency.

- [ ] **Step 1: 写 capability 和 dependency 失败测试**

在 `rpc_contract.rs` 加入：

```rust
#[tokio::test]
async fn capabilities_do_not_expose_a_retired_asset_driver() {
    let server = start_test_server(ServerConfig::for_tests("native-capabilities")).await;
    let response = common::rpc(
        &server,
        "system.capabilities",
        serde_json::json!({}),
    )
    .await;

    assert!(response["result"].get("toolchain").is_none());
    assert!(response["result"]["methods"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!("export.runtimePreview")));
}
```

Shell contract：

```bash
! rg -n "studio-toolchain-client|ToolchainService|STUDIO_TOOLCHAIN_CLI" \
  Cargo.toml Cargo.lock crates/studio-server crates/studio-cli
```

- [ ] **Step 2: 运行 RED**

```bash
cargo test -p studio-server --test rpc_contract \
  capabilities_do_not_expose_a_retired_asset_driver
```

Expected: FAIL，capabilities 仍含 `toolchain`。

- [ ] **Step 3: 修改 ServerConfig、Session 和 RPC**

删除：

```rust
pub toolchain_cli: Option<PathBuf>,
pub(crate) toolchain_cli: Option<PathBuf>,
pub(crate) toolchain: Arc<ToolchainService>,
pub toolchain: ToolchainStatus,
pub async fn set_toolchain_cli(...)
```

并删除：

- `STUDIO_TOOLCHAIN_CLI` / `STUDIO_APP_ROOT` 读取；
- project.open/project.close 中 ToolchainService start/shutdown；
- changeSet commit 的 Toolchain crash 特判；
- preview diagnostic mapping 中的 `ToolchainUnavailable` 等分支。

`ServerConfig::for_tests` 删除 `toolchain_cli: None`。`studio-cli serve` 同步删除该字段。

- [ ] **Step 4: 删除 Diagnostic variants**

从 `studio-core/src/validation.rs` 删除：

```rust
ToolchainUnavailable
ToolchainIncompatible
ToolchainTimeout
ToolchainProtocolInvalid
ToolchainJobFailed
```

及其 `ICE3_STUDIO_TOOLCHAIN_*` code mapping。`ICE3_STUDIO_RUNTIME_PREVIEW_FAILED`
保留，并改为描述 Studio preview export failure。

- [ ] **Step 5: 删除 client crate**

```bash
git rm -r crates/studio-toolchain-client
git rm crates/studio-server/src/toolchain.rs
git rm crates/studio-server/tests/toolchain_sidecar.rs
git rm crates/studio-server/bindings/ToolchainState.ts
git rm crates/studio-server/bindings/ToolchainStatus.ts
```

从 workspace `members`、`studio-server/Cargo.toml` 和 `studio-server/src/lib.rs` 删除
引用。运行 `cargo check -p studio-server` 更新 lockfile。

- [ ] **Step 6: 删除 fake Sidecar test helpers**

从 `tests/common/mod.rs` 删除：

```rust
SUCCESS_TOOLCHAIN
FAILED_TOOLCHAIN
start_with_toolchain
replace_toolchain_body
```

把 `changeset_rpc.rs`、`preview_packet.rs` 改为普通
`start_test_server(ServerConfig::for_tests(...))`，不再安装 Node script。

- [ ] **Step 7: 重新生成 bindings 和 schema**

```bash
cargo run -q -p studio-cli -- schema --output schemas
```

Expected:

- `schemas/studio-types.ts` 不再 import/export ToolchainStatus；
- `schemas/studio-api.schema.json` 的 Capabilities required fields 不含 `toolchain`；
- `crates/studio-server/bindings/Capabilities.ts` 与生成 schema 一致。

- [ ] **Step 8: 运行 GREEN**

```bash
cargo test -p studio-server
cargo test -p studio-cli
cargo check -p studio-cli --no-default-features
cargo tree -p studio-server
```

Expected: PASS；`cargo tree` 没有 `studio-toolchain-client`。

- [ ] **Step 9: 提交**

```bash
git add -A Cargo.toml Cargo.lock crates schemas
git commit -m "refactor(studio-server): remove the toolchain sidecar contract"
```

---

### Task 3: 删除 Viewer Toolchain 状态面

**Files：**
- Modify: `apps/viewer/src/api/StudioClient.ts`
- Modify: `apps/viewer/src/session/SessionStore.ts`
- Modify: `apps/viewer/src/app/Workbench.tsx`
- Modify: `apps/viewer/src/app/studioTokens.css`
- Modify: `apps/viewer/e2e/viewer.spec.ts`
- Delete: `apps/viewer/src/toolchain/ToolchainPanel.tsx`
- Delete: `apps/viewer/tests/toolchain-status.test.ts`

**Interfaces：**
- Consumes: generated `Capabilities` without `toolchain`.
- Produces: Agent-first workbench with no asset-driver status UI.

- [ ] **Step 1: 写浏览器失败测试**

替换原 “Toolchain incompatible” E2E：

```typescript
test('uses the native asset pipeline without a driver status surface', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('tab', { name: 'Inspect' }).click();

  await expect(page.locator('canvas[data-layer="scene"]')).toBeVisible();
  await expect(page.getByRole('heading', { name: 'Objects' })).toBeVisible();
  await expect(page.getByText(/Toolchain/i)).toHaveCount(0);
  await expect(page.getByLabel('Toolchain status')).toHaveCount(0);
});
```

再使用 route 拦截 `system.capabilities`，返回不含 `toolchain` 的正常 payload，确认页面
仍加载 scene tree 和 Agent mission control。

- [ ] **Step 2: 运行 RED**

```bash
PLAYWRIGHT_CHANNEL=chrome npm --prefix apps/viewer run test:e2e -- \
  --grep "native asset pipeline"
```

Expected: FAIL，页面仍出现 Toolchain status。

- [ ] **Step 3: 删除 API type 和 store field**

`StudioClient.ts` 删除：

```typescript
export type ToolchainStatus = { ... };
toolchain: ToolchainStatus;
```

`SessionStore.ts` 删除：

```typescript
toolchain: ToolchainStatus | null;
toolchain: null,
toolchain: capabilities?.toolchain ?? this.snapshot.toolchain,
```

- [ ] **Step 4: 删除 panel 和样式**

删除 `ToolchainPanel.tsx`、其 unit test 和 `Workbench.tsx` import/render：

```tsx
<ToolchainPanel status={session.toolchain} />
```

只删除 `toolchain-panel` 专属 selector。`ComputePanel` 复用的通用 class 改为语义名称：

```css
.status-card__eyebrow
.status-card__reason
```

同步修改 `ComputePanel.tsx` 的两个 class，避免删除样式后显示退化。

- [ ] **Step 5: 更新 Viewer unit tests**

删除 `toolchain-status.test.ts`，增加 SessionStore 测试：

```typescript
test('loads capabilities without retired driver state', async () => {
  const store = new SessionStore({
    document: async () => ({
      projectId: 'native',
      revision: 0,
      scene: { nodes: [] },
      components: [],
    }),
    validation: async () => [],
    capabilities: async () => ({
      apiVersion: '1.0',
      schemaVersion: 2,
      methods: ['export.runtimePreview'],
      compute: {
        mode: 'cpu',
        selectedBackend: 'cpu',
        adapter: 'reference',
        tier: 'cpu',
        memoryBudgetBytes: 0,
        health: 'ready',
        fallbackReason: null,
        queueDepth: 0,
        maxConcurrentJobs: 1,
        gpuThresholdTriangles: 0,
        lastKernelMs: 0,
        lastUploadBytes: 0,
        lastReadbackBytes: 0,
        lastCacheHits: 0,
      },
    }),
  });
  await store.refresh();
  assert.equal('toolchain' in store.snapshot, false);
});
```

- [ ] **Step 6: 运行 Viewer GREEN**

```bash
PLAYWRIGHT_CHANNEL=chrome npm --prefix apps/viewer run test
PLAYWRIGHT_CHANNEL=chrome npm --prefix apps/viewer run typecheck
PLAYWRIGHT_CHANNEL=chrome npm --prefix apps/viewer run test:e2e
```

Expected: unit/typecheck/E2E PASS；页面不含 Toolchain 文案。

- [ ] **Step 7: 提交**

```bash
git add apps/viewer
git commit -m "refactor(viewer): remove toolchain status surface"
```

---

### Task 4: 删除 Sidecar build/staging/benchmark 注入

**Files：**
- Modify: `package.json`
- Modify: `apps/viewer/package.json`
- Modify: `scripts/run-viewer-e2e-server.mjs`
- Modify: `scripts/verify.mjs`
- Modify: `benchmarks/studio-mvp.mjs`
- Modify: `benchmarks/large-industrial-scene.mjs`
- Rename: `scripts/verify-toolchain-independence.mjs` to
  `scripts/verify-studio-standalone.mjs`
- Delete: `scripts/stage-toolchain.mjs`
- Delete: `scripts/verify-toolchain-bundle.mjs`
- Delete: `scripts/verify-toolchain-sidecar.mjs`
- Delete: `scripts/toolchain-bundle-lib.mjs`
- Delete: `scripts/toolchain-launch.mjs`
- Delete: `scripts/preview-example.mjs`
- Delete: `scripts/tests/toolchain-bundle.test.mjs`
- Delete: `toolchain/manifest.template.json`

**Interfaces：**
- Consumes: native preview and local fixtures from earlier tasks.
- Produces: Studio-only build, benchmark and standalone verification.

- [ ] **Step 1: 写 standalone 失败测试**

重命名后的 `verify-studio-standalone.mjs` 检查：

```javascript
const capabilities = await rpc('system.capabilities');
record(
  'retired asset driver is absent',
  capabilities.result && !('toolchain' in capabilities.result),
);
record(
  'preview path is Studio-owned',
  previewPath.endsWith('/cache/preview/00000001.glb'),
  previewPath,
);
record(
  'no toolchain cache was created',
  !(await stat(join(workspace, 'independence/cache/toolchain')).catch(() => null)),
);
```

脚本启动 server 前清理：

```javascript
for (const key of [
  'STUDIO_TOOLCHAIN_CLI',
  'ICE3_TOOLCHAIN_CLI',
  'STUDIO_APP_ROOT',
  'ICE3_JT_CONVERTER',
]) {
  delete env[key];
}
```

Run:

```bash
node scripts/verify-studio-standalone.mjs
```

Expected: FAIL，脚本仍期待 `toolchain.state === 'unavailable'`。

- [ ] **Step 2: 修改 standalone verifier**

保留原有 document、preview、source export、scene query、evaluation、ChangeSet、
pack/unpack 和 idempotency 验收。删除所有 `toolchain` state 断言，并增加：

- capability 不含 toolchain；
- committed/candidate preview path 都在 `cache/preview`；
- `cache/toolchain` 不存在；
- 不启动 Node asset subprocess。

- [ ] **Step 3: 删除 Viewer build hook**

`apps/viewer/package.json`：

```json
"predev": "npm --prefix ../../../ice3-engine run build",
"pretest:e2e": "npm --prefix ../../../ice3-engine run build && STUDIO_SESSION_TOKEN=studio-e2e npm run build"
```

不得再执行 `../ice3-toolchain run build`。

- [ ] **Step 4: 简化 E2E server**

删除 `toolchain`、`STUDIO_TOOLCHAIN_CLI`、`ICE3_TOOLCHAIN_CLI`、`preview-example.mjs`
复制和 `ICE3_TOOLCHAIN_CLI` fallback。server env 只保留：

```javascript
STUDIO_SESSION_TOKEN: 'studio-e2e',
STUDIO_VIEWER_DIR: join(repositoryRoot, 'apps/viewer/dist'),
```

- [ ] **Step 5: 删除 root scripts**

`package.json` 删除：

```json
"toolchain:stage"
"toolchain:verify"
```

`scripts/verify.mjs` 删除 Toolchain bundle contract；把 independence label 改为：

```javascript
{
  label: 'Studio standalone',
  command: 'node',
  args: ['scripts/verify-studio-standalone.mjs'],
},
```

- [ ] **Step 6: 移除 benchmark Sidecar build 和 state**

`benchmarks/studio-mvp.mjs` 删除：

```javascript
const toolchainSource = resolve('scripts/preview-example.mjs');
const toolchainCli = resolve('../ice3-toolchain/packages/cli/dist/main.js');
const toolchainAdapter = ...
const toolchainBuild = run('npm', ['--prefix', '../ice3-toolchain', 'run', 'build']);
STUDIO_TOOLCHAIN_CLI
ICE3_TOOLCHAIN_CLI
```

`minimal.glb` 使用 `fixtures/source-import/gltf/minimal.glb`。

`large-industrial-scene.mjs` 删除 `capabilities.toolchain` 条件与 report field，保持
`export.runtimePreview` 的 wall-clock 指标。

- [ ] **Step 7: 验证无旧脚本引用**

```bash
rg -n "STUDIO_TOOLCHAIN|ICE3_TOOLCHAIN|preview-example|toolchain:stage|toolchain:verify|cache/toolchain" \
  package.json apps/viewer scripts benchmarks
```

Expected: 无输出。

- [ ] **Step 8: 运行 GREEN**

```bash
node scripts/verify-studio-standalone.mjs
node --check benchmarks/studio-mvp.mjs
node --check benchmarks/large-industrial-scene.mjs
PLAYWRIGHT_CHANNEL=chrome npm --prefix apps/viewer run test:e2e
```

Expected: standalone 全部 PASS；benchmark script syntax PASS；Chrome E2E PASS。

- [ ] **Step 9: 提交**

```bash
git add package.json apps/viewer/package.json scripts benchmarks toolchain
git commit -m "build(studio): remove toolchain sidecar packaging"
```

---

### Task 5: 更新文档并执行 Studio standalone 全门禁

**Files：**
- Modify: `README.md`
- Modify: `AGENTS.md`
- Modify: `docs/README.md`
- Modify: `docs/architecture.md`
- Modify: `docs/reference/api.md`
- Modify: `docs/reference/capabilities.md`
- Modify: `docs/reference/diagnostics.md`
- Modify: `docs/guides/getting-started.md`
- Delete: `docs/reference/toolchain-sidecar.md`
- Modify: `docs/specs/2026-10-30-studio-toolchain-sidecar-design.md`
- Modify: `docs/plans/2026-10-30-studio-toolchain-sidecar.md`
- Modify: `docs/specs/2026-11-02-native-jt-toolchain-retirement-design.md`
- Modify: `docs/plans/2026-11-02-studio-sidecar-retirement.md`

**Interfaces：**
- Consumes: passing standalone code and tests.
- Produces: code-mirrored documentation and Phase 5B closeout evidence.

- [ ] **Step 1: 更新架构文档**

`AGENTS.md` 架构边界改为：

- Studio Core 不依赖 Toolchain、Engine、Viewer 或外部模型 SDK；
- Studio Server 不启动 Node Asset runtime 或 Sidecar；
- Source Import 除 JT 的受控 converter exception 外均由 `studio-import` 完成；
- Runtime preview、partition、LOD 和 Tileset 全部由 Studio 原生 crate 完成。

`docs/architecture.md` 删除 Sidecar client 节点、进程生命周期和状态说明，加入：

```text
Studio Server
└── PreviewAdapter
    └── Studio Core evaluated GLB
        └── cache/preview/<revision>.glb
```

- [ ] **Step 2: 更新公共 reference**

`docs/reference/api.md`：

- `system.capabilities` 行删除 Toolchain；
- preview 文档删除 Toolchain 优化器描述；
- route 和 cache path 改为 `cache/preview`。

`docs/reference/capabilities.md`：

- 删除 `STUDIO_TOOLCHAIN_CLI`、`ICE3_TOOLCHAIN_CLI`；
- 保留 `ICE3_JT_CONVERTER`、`ICE3_JT_TIMEOUT_MS`，说明它们由 Studio import 直接读取；
- 删除 Toolchain status UI 行。

`docs/reference/diagnostics.md`：

- 删除全部 `ICE3_STUDIO_TOOLCHAIN_*`；
- 保留/更新 Runtime preview failure 的描述；
- 加入 JT import codes。

- [ ] **Step 3: 标记历史文档**

`docs/reference/toolchain-sidecar.md` 删除；其内容由 git history 保留。

在 Sidecar Spec-004 和 Plan-005 顶部加入：

```markdown
> **历史说明：** Sidecar 已由 Studio 原生 preview 和 asset pipeline 取代。
> 本文仅记录退役前的设计，不得作为当前部署或接口依据。
```

把索引状态改为 `Historical`。

- [ ] **Step 4: 更新文档索引**

`docs/README.md`：

- 删除 REF-005 当前入口；
- 把 PLAN-025 链接加入；
- Spec-018 描述标注 Phase 5A/5B 状态；
- 保持历史 Spec-004/Plan-005 链接但标 Historical。

- [ ] **Step 5: 检查无副作用路径**

```bash
rg -n "studio-toolchain-client|ToolchainPanel|ToolchainStatus|cache/toolchain|STUDIO_TOOLCHAIN|ICE3_TOOLCHAIN" \
  Cargo.toml Cargo.lock crates apps scripts benchmarks package.json schemas \
  --glob '!**/target/**' --glob '!**/node_modules/**' --glob '!**/dist/**'
```

Expected: 无输出。

生产代码、测试、schema、scripts 和 package manifest 中不得留下 retired name。
历史文档可以说明已退役的能力，但不得表现为当前可用。

- [ ] **Step 6: 运行完整 Studio 门禁**

```bash
node ../ice3-doc/scripts/check-docs.mjs --root . 2>&1 \
  | rg '2026-11-02-studio-sidecar|2026-11-02-native-jt|docs/README.md'
PLAYWRIGHT_CHANNEL=chrome npm run verify
node scripts/compare-asset-pipelines.mjs
node scripts/compare-source-imports.mjs
node scripts/compare-fbx-usd-imports.mjs
node scripts/compare-step-imports.mjs
node scripts/compare-ifc-imports.mjs
node scripts/compare-jt-imports.mjs
```

Expected:

- 文档检查无本次相关 Diagnostic；
- Studio 全部验证门禁 PASS；
- Chrome E2E 全部 PASS；
- 五份旧格式 parity 与 JT parity 均 `status=pass`；
- `npm audit --audit-level high` 为 0 vulnerability。

- [ ] **Step 7: 验证不依赖相邻 Toolchain repository**

在测试用临时副本或通过 shell 临时隐藏该目录后运行：

```bash
npm run verify
```

本轮不得直接删除或移动用户真实 repository。可在独立临时 worktree 中执行 standalone
验收，或使用受控 shell scope 暂时隐藏后恢复，并先记录路径与 trap cleanup。

Expected: 全部验证 PASS，没有隐式读取 `../ice3-toolchain`。

- [ ] **Step 8: 提交**

```bash
git add README.md AGENTS.md docs package.json apps/viewer scripts benchmarks
git commit -m "docs(studio): complete native asset pipeline phase 5b"
```

## Plan Self-Review

- Spec coverage：原生 preview、cache path、Sidecar client/config/status/UI/build/
  benchmark/test/docs 删除与 standalone 验收均有 task。
- Placeholder scan：没有 TODO/TBD、泛化的“更新测试”或未定义 API。
- Type consistency：`PreviewAdapter::new`、`RuntimePreview`、`CandidatePreview`、
  `ServerConfig` 和 generated `Capabilities` 的字段变更在 task 间一致。
- Review Focus：digest/path、process lifecycle、schema/UI 同步、benchmark 真实性和
  neighbor-repository independence 都由 Task 1、2、3、4、5 的具体 gate 覆盖。
