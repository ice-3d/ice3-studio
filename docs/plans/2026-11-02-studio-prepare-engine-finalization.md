# Studio Asset Prepare Engine 集成与最终验收实施计划

> Document ID: ICE3-STUDIO-PLAN-029
>
> Status: Draft
>
> Owner: ICE3 Studio maintainers

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> `superpowers:subagent-driven-development` (recommended) or
> `superpowers:executing-plans` to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal：** 完成 Phase 6C，用真实 Engine/Agent Console 消费验证 Studio edges 产物，
建立性能资源基线，文档化为“Studio 已完整替代 Toolchain 且不依赖 Blender”。

**架构：** 本计划不新增第二条准备实现。它验证 Plan-027/028 的单一 Prepare
Artifact 可被 Engine 读取，并验证 large fixture、CLI、Tileset、文档和全量门禁。

**技术栈：** Rust 1.98、Node.js 24.22.1、TypeScript、Playwright Chrome、现有
Studio/Engine/Agent Console 测试命令。

**Spec：** [Studio Asset Prepare 与 Toolchain 功能等价](../specs/2026-11-02-studio-asset-prepare-parity-design.md)

## Global Constraints

- 依赖 Plan-027 和 Plan-028 全部通过。
- Engine/Agent Console 只消费 Prepared GLB/Tileset，不链接 Prepare 实现。
- Edge line primitive 必须由 Engine reader 恢复为 edge indices/edge color。
- large-industrial fixture 记录 wall-clock、peak memory、bytes、triangles 和 edges。
- Studio p95 不得超过 legacy baseline 两倍；超限必须记录原因，不能静默通过。
- Chrome 测试使用用户安装的 Google Chrome。
- 不恢复 Toolchain、Blender、DCC、Node Asset Runtime 或任意动态 prepare executable。
- 最终文档必须准确表述 Toolchain 能力替代边界，不能重新写出当前不存在的依赖。
- Git author/committer 固定为 `damoqiongqiu <damoqiongqiu@126.com>`。

## Review Focus

- Studio profile 统计通过但 Engine 读取失败，必须视为发布阻断。
- 带 edges Accessor 的 byte alignment、accessor bounds、index type 和 material mapping
  必须由真实 Reader 验证。
- LOD/coarse 中 edge indices 必须对应其自身 positions，不能越界或引用 source mesh。
- 性能门禁必须使用同一 input/options 的 legacy baseline，不能只记录新数字。
- 文档不能把“Studio 是唯一生产入口”与“历史 Toolchain 记录”混写为当前依赖。

---

### Task 1: 建立 Engine 与 Agent Console 消费契约

**文件：**
- Create: `scripts/verify-prepare-engine.mjs`
- Create: `scripts/tests/prepare-engine-contract.test.mjs`
- Create: `fixtures/parity/prepare-engine-v1.json`
- Modify: `scripts/verify.mjs`

**接口：**
- 消费：
  - Studio `build` binary；
  - Engine built `packages/assets/dist/index.js`；
  - Engine exports `readPreparedGLB`, `verifyPreparedGLBIntegrity`；
  - Runtime Tileset export fixture。
- 产出：checked-in Engine contract baseline and verification report.

- [ ] **Step 1: 加 Engine 构建到 verify**

Studio root verify 已有 Viewer pretypecheck 会构建 Engine。为独立 contract 增加明确
步骤：

```javascript
{
  label: 'Engine build for prepare contract',
  command: 'npm',
  args: ['--prefix', '../../../ice3-engine', 'run', 'build'],
}
```

路径以当前 workspace 结构为准；若目录不存在则失败并给出明确 prerequisite。

- [ ] **Step 2: 写 Node contract 测试**

`scripts/tests/prepare-engine-contract.test.mjs`：

```javascript
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';

test('Engine consumes Studio edges and Preparation metadata', async () => {
  const report = JSON.parse(await readFile(
    new URL('../../output/prepare-engine-contract.json', import.meta.url),
    'utf8',
  ));
  assert.equal(report.status, 'pass');
  assert.deepEqual(report.failures, []);
  assert.equal(report.build.stats.edgeSegmentCount, 12);
  assert.equal(report.engine.edgeMaterialColor, '#e6f2ff');
  assert.equal(report.engine.edgeCount, 12);
  assert.equal(report.engine.selectionId, 'prepare-cube/node/0');
  assert.equal(report.runtimeTileset.loadedParts, report.runtimeTileset.partCount);
});
```

- [ ] **Step 3: 实现 Studio build + Engine read**

脚本流程：

1. 使用 `STUDIO_CLI_BIN` 或构建当前 Studio CLI。
2. `build fixtures/source-import/gltf/prepare-cube.gltf` 到临时目录。
3. 动态 import：

```javascript
const engine = await import(pathToFileURL(
  resolve(engineRoot, 'packages/assets/dist/index.js'),
).href);
const bytes = await readFile(output);
const integrity = await engine.verifyPreparedGLBIntegrity(bytes.buffer);
const snapshot = engine.readPreparedGLB(bytes.buffer);
```

4. 断言 line primitive 恢复、edge color、Selection、stats。
5. 写 `output/prepare-engine-contract.json`。

- [ ] **Step 4: 加 Tileset/Agent Console 消费**

对 `export tileset` fixture：

- 读取 `tileset.json`；
- 验证 relative URI；
- 通过 Engine Tileset loader 或 Agent Console model loader 加载 part；
- 检查至少一个 part 有 edge material/indices；
- 记录 loaded part count、triangles、edges。

若现有 loader 需要浏览器环境，则使用 Chrome E2E 加载 synthetic Tileset，并额外从
Node reader 验证 part GLB；必须在 report 中分别记录两段证据。

- [ ] **Step 5: 运行 RED/GREEN**

```bash
node scripts/verify-prepare-engine.mjs
node --test scripts/tests/prepare-engine-contract.test.mjs
```

Plan-027/028 尚未完成时预期因 CLI/edges 缺失而 FAIL；完成后预期 PASS，
`failures=[]`。

- [ ] **Step 6: 提交**

```bash
git add scripts/verify-prepare-engine.mjs scripts/tests/prepare-engine-contract.test.mjs \
  scripts/verify.mjs fixtures/parity/prepare-engine-v1.json
git commit -m "test(studio): verify engine consumption of prepared edges"
```

---

### Task 2: 建立 large-fixture 性能与资源基线

**文件：**
- Create: `scripts/benchmark-prepare.mjs`
- Create: `fixtures/parity/prepare-performance-v1.json`
- Create: `scripts/tests/prepare-performance.test.mjs`
- Modify: `package.json`
- Modify: `scripts/verify.mjs`

**接口：**
- 消费：Studio `build` and `partition` on large-industrial fixture.
- 产出：
  - machine-readable performance baseline；
  - edge-off/edge-on deltas；
  - peak memory and output bytes。

- [ ] **Step 1: 写性能门禁测试**

```javascript
test('prepare performance remains inside the accepted envelope', async () => {
  const baseline = JSON.parse(await readFile(
    new URL('../../fixtures/parity/prepare-performance-v1.json', import.meta.url),
    'utf8',
  ));
  for (const entry of baseline.cases) {
    assert.ok(entry.p95Ms > 0);
    assert.ok(entry.peakMemoryBytes <= 536_870_912);
    assert.ok(entry.outputBytes > 0);
    assert.ok(entry.edgeSegmentCount > 0);
    assert.ok(entry.legacyP95Ms > 0);
    assert.ok(entry.p95Ms <= entry.legacyP95Ms * 2);
  }
});
```

- [ ] **Step 2: 实现 benchmark**

用例：

- `build` 开启 edges；
- `build` 关闭 edges；
- `partition` 无 LOD、开启 edges；
- `partition` adaptive LOD + semantic proxy + edges；
- `partition` static batching + edges。

每个用例：

- 预热 1 次；
- 采样 5 次；
- 记录 p50/p95 wall-clock；
- 记录 process peak RSS；
- 记录 output bytes；
- 记录 triangle/edge/partition/batch/coarse counts；
- output digest 必须每次相同；
- 同参数 legacy baseline 从 `prepare-baseline-v1.json` / archived capture 取得。

- [ ] **Step 3: 添加 npm script**

```json
"benchmark:prepare": "node scripts/benchmark-prepare.mjs"
```

- [ ] **Step 4: 运行 baseline**

```bash
node scripts/benchmark-prepare.mjs --capture
node scripts/benchmark-prepare.mjs
node --test scripts/tests/prepare-performance.test.mjs
```

预期：baseline 写出；重复 run p95 在 baseline budget 内；无 output digest drift。

- [ ] **Step 5: 加入 verify**

verify 运行 contract 门禁，不重复运行完整 5× benchmark：

```javascript
{
  label: 'Prepare performance contract',
  command: 'node',
  args: ['--test', 'scripts/tests/prepare-performance.test.mjs'],
}
```

- [ ] **Step 6: 提交**

```bash
git add package.json scripts fixtures/parity/prepare-performance-v1.json
git commit -m "bench(studio): record asset prepare performance envelope"
```

---

### Task 3: 最终文档与能力表述

**文件：**
- Modify: `README.md`
- Modify: `AGENTS.md`
- Modify: `docs/README.md`
- Modify: `docs/architecture.md`
- Modify: `docs/reference/capabilities.md`
- Modify: `docs/reference/api.md`
- Modify: `docs/reference/diagnostics.md`
- Modify: `docs/guides/getting-started.md`
- Modify: `docs/guides/large-industrial-fixture.md`
- Modify: `docs/specs/2026-11-02-studio-asset-prepare-parity-design.md`
- Modify: `docs/plans/2026-11-02-studio-prepare-core.md`
- Modify: `docs/plans/2026-11-02-studio-partition-runtime-parity.md`
- Modify: `docs/plans/2026-11-02-studio-prepare-engine-finalization.md`

**接口：**
- 消费：Engine and performance evidence
- 产出：code-as-truth documentation and Phase 6 exit state

- [ ] **Step 1: 更新架构和能力清单**

明确：

- `studio-assets::prepare` 是 Prepared GLB 唯一 writer；
- `build/inspect/import/partition/export tileset` 的命令与默认值；
- metrics、budget、report；
- Edge line primitive/material；
- Engine/Agent Console 消费边界；
- 不存在 Toolchain/Blender/DCC 运行时依赖。

- [ ] **Step 2: 更新 Diagnostics**

列出全部 `ICE3_STUDIO_ASSET_*`，以及旧 baseline-only 的迁移映射。不得把
`ICE3_TOOL_*` 标为当前 output code。

- [ ] **Step 3: 更新 README 产品边界**

将“未实现”列表限定为布尔几何、自由曲线等后续建模能力，不再列 Toolchain
preparation/build/inspect/partition 为缺口。

新增一句话，必须有实际测试支撑：

```text
Studio 已完整接管通用 Source Import、Asset Prepare、partition、LOD、Tileset 和
Agent 交付链；生产路径不依赖 Toolchain、Node Asset Runtime 或 Blender。
```

JT converter exception 紧邻说明，防止读者误以为零外部 executable。

- [ ] **Step 4: 标记阶段**

- Spec-019: Implemented；
- Plan-027/028/029: Implemented；
- 所有 task checkbox 更新；
- 说明 Phase6A/6B/6C 已完成。

- [ ] **Step 5: 文档检查**

```bash
node ../ice3-doc/scripts/check-docs.mjs --root . 2>&1 \
  | rg '2026-11-02-studio-(prepare|partition)|docs/README.md|README.md|architecture'
```

预期：本次文档无 Diagnostic。

- [ ] **Step 6: 提交**

```bash
git add README.md AGENTS.md docs
git commit -m "docs(studio): complete asset prepare capability parity"
```

---

### Task 4: 最终全量验收与分支收口

**文件：**
- Modify only if verification exposes a defect; otherwise no source edits.
- Modify: phase plans/spec statuses and benchmark baseline for accepted final values.

**接口：**
- 消费：all Phase 6 deliverables
- 产出：review-ready feature branch and complete evidence

- [ ] **Step 1: 全量 Studio 验证**

```bash
PLAYWRIGHT_CHANNEL=chrome npm run verify
node scripts/compare-asset-pipelines.mjs
node scripts/compare-source-imports.mjs
node scripts/compare-fbx-usd-imports.mjs
node scripts/compare-step-imports.mjs
node scripts/compare-ifc-imports.mjs
node scripts/compare-jt-imports.mjs
node scripts/compare-prepare-imports.mjs
node scripts/compare-partition-imports.mjs
node scripts/verify-prepare-engine.mjs
node scripts/benchmark-prepare.mjs
```

预期：

- 所有 verify gates PASS；
- Chrome E2E PASS；
- 八份 parity `status=pass`；
- Engine contract `failures=[]`；
- performance p95/memory/digest gate PASS；
- dependency audit 0 vulnerability。

- [ ] **Step 2: standalone 验证**

临时隐藏相邻 Toolchain repository：

```bash
PLAYWRIGHT_CHANNEL=chrome npm run verify
```

预期：全部 PASS。不得通过 `node_modules` 或 test helper 读取 Toolchain/Blender。

- [ ] **Step 3: 依赖与命名审计**

```bash
rg -n -i "\bblender\b|\bbpy\b|BLENDER_BIN|@ice3/toolchain|ice3-toolchain" \
  crates apps scripts benchmarks package.json Cargo.toml schemas \
  --glob '!**/target/**' --glob '!**/node_modules/**' --glob '!**/dist/**'
```

预期：production/test/script scan 无输出；历史文档和明确负向断言可另行允许。

```bash
cargo tree -p studio-cli --no-default-features
npm audit --audit-level high
```

预期：no Toolchain/Blender dependency；0 vulnerability。

- [ ] **Step 4: 最终 self-review**

检查：

- 所有新增 public types 有文档；
- 所有新 CLI flags 有 help、reference 和 tests；
- 所有 output/report/file rename 失败路径测试；
- large fixture benchmark baseline 与实际代码一致；
- 没有 TODO、TBD、未使用旧实现或 dead code；
- worktree clean，只有预期 commits。

- [ ] **Step 5: final review**

使用 `requesting-code-review` 对 Phase6 全分支做 whole-branch review。任何
Critical/Important 修复后做一次 scoped re-review；不得跳过。

- [ ] **Step 6: 合并与归档状态**

只有用户明确选择本地 merge 后才合并 `main`。

当前阶段结束时：

- 不 push；
- 不 tag；
- 不 archive/delete 其它 repository；
- 汇报 branch、HEAD、commits、测试证据和 deferred minors。

## Plan Self-Review

- Spec coverage：Engine consumption、Agent Console/Tileset、performance、resource、
  docs、phase status、full verify 和 final review 均有 task。
- Placeholder scan：没有 TODO/TBD 或未定义实现。
- Type consistency：Plan-027 的 PrepareArtifact/AssetMetrics、Plan-028 的
  AssetProfile/PartitionStats 和本计划的 engine contract fields 一致。
- Review Focus：Engine reader、Accessor/index correctness、LOD edge identity、
  performance baseline 和文档声明都由具体测试覆盖。
