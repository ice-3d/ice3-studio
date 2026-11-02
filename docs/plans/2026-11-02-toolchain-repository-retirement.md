# Toolchain Repository 最终退役实施计划

> Document ID: ICE3-STUDIO-PLAN-026
>
> Status: Proposed
>
> Owner: ICE3 Studio maintainers

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> `superpowers:subagent-driven-development` (recommended) or
> `superpowers:executing-plans` to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal：** 在 Studio 已独立支持全部 Source Import 和 preview 后，删除 Toolchain
repository 中残留的 JT adapter、Studio Sidecar、Studio 发布入口和活动文档状态，使
repository 可安全归档。

**架构：** Toolchain 不再是 Studio 的生产依赖。Studio-owned fixture 已在 Plan-024
落地；本计划只清理旧 repository 的 executable surface、tests、release scripts、
contracts 和文档。物理归档或删除 repository 不在本计划自动执行范围内。

**技术栈：** Node.js 24.22.1、TypeScript 7、现有 Node test runner、Rust 1.98
Studio standalone gate、Git 审计。

**Spec：** [Studio 原生 JT Import 与 Toolchain 最终退役](../specs/2026-11-02-native-jt-toolchain-retirement-design.md)

## Global Constraints

- 必须先完成 Plan-024 和 Plan-025，并保存两者通过记录。
- Toolchain 不得再含 JT production adapter、Sidecar executable 或 Studio package
  export。
- Toolchain 不得恢复 STEP/IFC/FBX/USD/OBJ adapter 或外部 DCC bridge。
- 只删除当前 repository 内的明确文件；不删除用户 home、workspace root 或相邻
  repository。
- Studio-owned fixtures 不得依赖 Toolchain `node_modules`。
- 不得提交客户模型、派生 GLB、绝对路径、日志或截图。
- Historical docs 可以保留旧名称，但必须明确“已退役”，且不能作为当前接口。
- Repository archive/delete 必须在本计划全部门禁后单独获得用户确认。
- Git author/committer 固定为 `damoqiongqiu <damoqiongqiu@126.com>`。

## Review Focus

- 删除 npm package 后 `package-lock.json`、workspace、tsconfig references 和 release
  scripts 必须同步，不能留下可被 `npm ci` 安装的 stale dependency。
- Toolchain CLI 不能再接受 `.jt`、调用 converter 或向 Studio 暴露 Sidecar method。
- `package.json` exports/release tarball 不能继续发布 `./sidecar` 或 JT adapter。
- Tests 不能因为删除了 test 文件而“通过”；architecture test 必须主动断言路径和
  dependency 已不存在。
- 归档清单必须确认没有未提交改动、客户 fixture、release artifact、未合并分支或仍在
  Studio 使用的文件。

---

### Task 1: 删除 Toolchain JT production adapter

**Files：**
- Modify: `tests/architecture/layout.test.ts`
- Modify: `packages/cli/src/commands.ts`
- Modify: `packages/cli/src/main.ts`
- Modify: `packages/cli/package.json`
- Modify: `packages/cli/README.md`
- Modify: `package-lock.json`
- Modify: `tsconfig.json`
- Modify: `packages/cli/tsconfig.json`
- Modify: `tools/conformance/corpus.ts`
- Modify: `tools/conformance/manifest.json`
- Modify: `tools/conformance/build.mjs`
- Modify: `scripts/check-release-package.mjs`
- Delete: `packages/jt-importer/`
- Delete: `tests/jt/`
- Delete: `tests/cli/jtImportCommand.test.ts`
- Delete: `tests/fixtures/jt/`

**Interfaces：**
- Consumes: passing Studio JT parity from Plan-024.
- Produces: Toolchain source tree with no `.jt` adapter, converter invocation,
  JT conformance case or release check.

- [ ] **Step 1: 先修改 architecture test 让现状失败**

`tests/architecture/layout.test.ts`：

```typescript
test('JT import is owned by Studio', () => {
  expect(existsSync('packages/jt-importer')).toBe(false);
  expect(existsSync('tests/jt')).toBe(false);
  expect(existsSync('tests/fixtures/jt')).toBe(false);

  const cli = JSON.parse(
    readFileSync('packages/cli/package.json', 'utf8'),
  ) as {
    dependencies?: Record<string, string>;
    exports?: Record<string, unknown>;
  };
  expect(cli.dependencies?.['@ice3/jt-importer']).toBeUndefined();

  const conformance = readFileSync('tools/conformance/corpus.ts', 'utf8');
  expect(conformance).not.toContain("adapter: 'jt'");
  expect(conformance).not.toContain("id: 'jt'");
});
```

同时把旧 layout test 中的 `expect(existsSync('packages/jt-importer/package.json')).toBe(true)`
改为 `false`。

- [ ] **Step 2: 运行 RED**

```bash
npm run test:architecture
```

Expected: FAIL；JT package、tests 和 corpus entry 仍存在。

- [ ] **Step 3: 删除 JT CLI path**

从 `commands.ts` 删除：

- `importCommand` 和 `loadSourceImporters`；
- `runCli` 的 `args.command === 'import'` 分支；
- help 中的 `import`；
- 非 glTF source 的 partition adapter fallback。

`readPartitionSource` 对非 `.gltf/.glb` 直接返回：

```typescript
throw new Error(
  `ICE3_TOOL_SOURCE_UNSUPPORTED: ${extname(sourcePath)}; use ice3-studio import`,
);
```

`main.ts` 只保留 `runCli` 和 sidecar removal 由 Task 2 处理的入口。

- [ ] **Step 4: 删除 JT package、tests 和 fixtures**

```bash
git rm -r packages/jt-importer tests/jt tests/fixtures/jt
git rm tests/cli/jtImportCommand.test.ts
```

这些 fixture 已由 Plan-024 复制并校验到
`ice3-studio/fixtures/source-import/jt/`。删除前再次运行：

```bash
shasum -a 256 tests/fixtures/jt/*.jt
shasum -a 256 ../ice3-studio/fixtures/source-import/jt/*.jt
```

Expected: 同名 `.jt` contract marker 与 negative scripts 的 hash 有对应副本。

- [ ] **Step 5: 清理 package、lock 和 tsconfig**

从 `packages/cli/package.json` 删除 dependency：

```json
"@ice3/jt-importer": "0.0.0"
```

从 root `tsconfig.json` 和 `packages/cli/tsconfig.json` 删除
`packages/jt-importer` reference。更新 lockfile：

```bash
npm install --package-lock-only
```

- [ ] **Step 6: 清理 conformance 和 release check**

从 `tools/conformance/corpus.ts` 删除 JT case。`manifest.json` 可以只保留 glTF case。
`tools/conformance/build.mjs` 删除 `ICE3_JT_CONVERTER` 注入。

`scripts/check-release-package.mjs` 删除 JT converter fixture、JT import command、
`ICE3_RELEASE_JT_REPORT_INVALID` 和 JT tarball assertions。发布包测试应只验证 Toolchain
仍保留的通用 glTF/prepare/partition surface。

- [ ] **Step 7: 运行 Toolchain JT retirement gate**

```bash
npm run typecheck
npm test
! rg -n "jt-importer|ICE3_JT_CONVERTER|ICE3_TOOL_JT_" \
  packages tools tests scripts package.json package-lock.json \
  --glob '!tests/fixtures/**'
```

Expected: typecheck/tests PASS；production search 无输出。

- [ ] **Step 8: 提交**

```bash
git add -A package.json package-lock.json tsconfig.json packages tools tests scripts
git commit -m "refactor(toolchain): retire the JT source adapter"
```

---

### Task 2: 删除 Toolchain Sidecar executable 和 contract

**Files：**
- Modify: `tests/architecture/layout.test.ts`
- Modify: `packages/cli/src/main.ts`
- Modify: `packages/cli/src/commands.ts`
- Modify: `packages/cli/package.json`
- Modify: `packages/cli/README.md`
- Modify: `scripts/build-release-package.mjs`
- Modify: `scripts/check-release-package.mjs`
- Modify: `tests/release/package.test.ts`
- Modify: `tools/conformance/README.md`
- Delete: `packages/cli/src/sidecar/`
- Delete: `tests/sidecar/`
- Delete: `tests/conformance/studio-sidecar.test.ts`

**Interfaces：**
- Consumes: no Studio references from Plan-025.
- Produces: CLI package without `sidecar` executable, `./sidecar` export,
  contract fixtures, cancellation queue or Studio package manifest.

- [ ] **Step 1: 写 Sidecar removal 失败测试**

在 `tests/architecture/layout.test.ts`：

```typescript
test('Studio no longer ships a Toolchain sidecar', () => {
  expect(existsSync('packages/cli/src/sidecar')).toBe(false);
  expect(existsSync('tests/sidecar')).toBe(false);
  expect(existsSync('tests/conformance/studio-sidecar.test.ts')).toBe(false);

  const cli = JSON.parse(
    readFileSync('packages/cli/package.json', 'utf8'),
  ) as { exports?: Record<string, unknown> };
  expect(cli.exports?.['./sidecar']).toBeUndefined();
});
```

在 `tests/release/package.test.ts` 反向断言：

```typescript
expect(manifest.exports?.['./sidecar']).toBeUndefined();
```

Run:

```bash
npm run test:architecture
```

Expected: FAIL。

- [ ] **Step 2: 删除 CLI sidecar entry**

`main.ts` 删除：

```typescript
import { serveSidecar } from './sidecar/server.js';
export async function runSidecarCommand(...)
process.argv[2] === 'sidecar' ? ...
```

入口只保留：

```typescript
if (
  process.argv[1]
  && import.meta.url === pathToFileURL(realpathSync(process.argv[1])).href
) {
  process.exitCode = await runCli(process.argv.slice(2));
}
```

`commands.ts` help 中删除 `sidecar`。

- [ ] **Step 3: 删除 Sidecar source、tests 和 export**

```bash
git rm -r packages/cli/src/sidecar tests/sidecar
git rm tests/conformance/studio-sidecar.test.ts
```

从 `packages/cli/package.json` 删除：

```json
"./sidecar": {
  "types": "./dist/sidecar/server.d.ts",
  "import": "./dist/sidecar/server.js"
}
```

- [ ] **Step 4: 清理 release bundling**

从 `scripts/build-release-package.mjs` 删除：

- `sidecar` export entry；
- `dist/sidecar` mkdir；
- sidecar `.d.ts` copy list；
- esbuild `sidecar/server.ts` entry point；
- `toolchain-capabilities-v1` 或 `preview-request-v1` Studio contract check。

从 `scripts/check-release-package.mjs` 删除 Node sidecar launch、NDJSON request、
cancellation 和 Studio preview 验证。

`tests/release/package.test.ts` 只检查剩余 CLI exports。

- [ ] **Step 5: 删除 Studio conformance wording**

`tools/conformance/README.md` 删除侧车契约和 Studio preview 说明，只记录 Toolchain
自身保留下来的 glTF prepare/partition tests。

- [ ] **Step 6: 运行 GREEN**

```bash
npm run typecheck
npm test
npm run build:conformance
! rg -n "sidecar|Sidecar|toolchain\\.capabilities|cache/toolchain" \
  packages tools tests scripts package.json \
  --glob '!docs/**' --glob '!tests/fixtures/**'
```

Expected: PASS；production/test/script search 无输出。

- [ ] **Step 7: 提交**

```bash
git add -A packages tests scripts package.json package-lock.json tools
git commit -m "refactor(toolchain): remove the Studio sidecar server"
```

---

### Task 3: 停止 Toolchain Studio release 入口

**Files：**
- Modify: `package.json`
- Modify: `tests/architecture/layout.test.ts`
- Modify: `README.md`
- Modify: `THIRD_PARTY_NOTICES.md`
- Delete: `scripts/build-release-package.mjs`
- Delete: `scripts/check-release-package.mjs`
- Delete: `tests/release/package.test.ts`

**Interfaces：**
- Consumes: no JT or Sidecar package surface.
- Produces: private historical workspace with no Studio release script.

- [ ] **Step 1: 写 release retirement 失败测试**

更新 `workspace root remains a private orchestration manifest`：

```typescript
test('workspace no longer publishes a Studio dependency', () => {
  const root = JSON.parse(readFileSync('package.json', 'utf8')) as {
    scripts?: Record<string, string>;
    dependencies?: Record<string, string>;
  };
  expect(root.scripts?.['release:package']).toBeUndefined();
  expect(root.scripts?.['release:check']).toBeUndefined();
  expect(root.dependencies ?? {}).toEqual({});
  expect(existsSync('scripts/build-release-package.mjs')).toBe(false);
  expect(existsSync('scripts/check-release-package.mjs')).toBe(false);
  expect(existsSync('tests/release/package.test.ts')).toBe(false);
});
```

- [ ] **Step 2: 运行 RED**

```bash
npm run test:architecture
```

Expected: FAIL，release scripts 仍存在。

- [ ] **Step 3: 删除 publish/release surface**

从 `package.json` 删除：

```json
"release:package"
"release:check"
```

删除：

```bash
git rm scripts/build-release-package.mjs
git rm scripts/check-release-package.mjs
git rm tests/release/package.test.ts
```

保留 `build`、`typecheck`、`test`、`verify`，使 repository archive 前仍能审计历史代码。

- [ ] **Step 4: 更新 README 的定位**

README 顶部改为：

```markdown
> **历史状态：** 本 repository 的 JT Source Import、Studio Sidecar 和 Studio
> asset publication 已迁入 ICE3 Studio。此 repository 仅保留历史实现和通用 glTF
> preprocessing 记录，不再发布 Studio production package。
```

删除安装、`npx ice3-tool` Studio workflow、Sidecar launch、JT converter 和 release
命令作为当前能力。保留历史章节时加明确迁移链接。

- [ ] **Step 5: 更新 notices**

移除只由 JT 或 Sidecar release bundle 使用、且已不在 dependency graph 中的声明。运行：

```bash
npm ls --all
```

Expected: 只保留 toolchain-core/CLI 仍实际链接的包。不得为了“清空”删除仍被依赖的
license notice。

- [ ] **Step 6: 运行 GREEN**

```bash
npm run verify
npm run build:conformance
node tools/conformance/build.mjs
```

Expected: 全部 PASS，且不再发布 Studio package。

- [ ] **Step 7: 提交**

```bash
git add -A package.json README.md THIRD_PARTY_NOTICES.md \
  tests/architecture scripts tests
git commit -m "build(toolchain): stop Studio release publication"
```

---

### Task 4: 文档、AGENTS 和 dependency graph 收口

**Files：**
- Modify: `AGENTS.md`
- Modify: `README.md`
- Modify: `docs/README.md`
- Modify: `docs/architecture.md`
- Modify: `docs/reference/cli.md`
- Modify: `docs/guides/capabilities.md`
- Modify: `docs/specs/2026-10-29-jt-importer.md`
- Modify: `docs/plans/2026-10-29-jt-importer.md`
- Modify: `docs/specs/2026-10-29-source-import-adapters.md`
- Modify: `docs/plans/2026-10-29-source-import-adapters.md`
- Modify: `docs/specs/2026-11-01-studio-owned-wastewater-authoring-design.md`
- Modify: `docs/plans/2026-11-01-studio-owned-wastewater-authoring.md`
- Create: `docs/retirement-checklist.md`

**Interfaces：**
- Consumes: clean JT/Sidecar/release code.
- Produces: accurate historical documentation and archive checklist.

- [ ] **Step 1: 更新 AGENTS 架构边界**

把当前 JT 和 Studio Sidecar 当前职责改成历史说明：

```markdown
- JT Source Import、Runtime preview 和 asset publication 已由 ICE3 Studio 负责。
- 本 repository 不提供 Studio Sidecar、JT converter adapter 或 production import
  bridge。
- 不得恢复 STEP、IFC、FBX、USD、OBJ、JT 或外部 DCC production adapter。
- 保留的 glTF preprocessing CLI 仅用于历史代码审计，不是 Studio 产品入口。
```

- [ ] **Step 2: 标记已迁移规格和计划**

JT Spec/Plan、多格式 Spec/Plan 和 Studio-owned authoring Spec/Plan 顶部统一加入：

```markdown
> **历史说明：** JT、Sidecar 和 Studio asset publication 已迁入 ICE3 Studio。
> 本文只保留退役前设计与实施记录，不代表当前产品入口。
```

`docs/README.md` 状态改为 `Historical`。不得删除历史文档来伪造“从未存在”。

- [ ] **Step 3: 更新 CLI 和 capability guide**

`docs/reference/cli.md`：

- 删除 `import`、`sidecar`、JT converter env 和 JT Diagnostic；
- 只记录当前 glTF build/partition/validate/inspect 中实际保留的命令；
- 指向 Studio CLI 作为 source import 和 asset publication 的唯一入口。

`docs/guides/capabilities.md`：

- 删除 JT/Sidecar 当前能力；
- 保留“已迁移至 ICE3 Studio”的链接；
- 删除会误导用户配置 `ICE3_JT_CONVERTER` 的 Toolchain 指令。

- [ ] **Step 4: 创建归档清单**

`docs/retirement-checklist.md`：

```markdown
# Toolchain Retirement Checklist

> Document ID: ICE3-TOOL-RET-001
>
> Status: Proposed

- [ ] Studio Plan-024, Plan-025 and Plan-026 pass on the same local date.
- [ ] `git status --short` is clean.
- [ ] No customer models, derived GLBs, reports, screenshots, logs or absolute paths
      are tracked.
- [ ] No JT adapter, Sidecar, Studio package export or release script remains.
- [ ] `npm run verify` passes.
- [ ] Studio `npm run verify` passes while this repository is not discoverable by
      Studio tests.
- [ ] All remote branches are merged or explicitly listed as intentionally discarded.
- [ ] Repository snapshot/tag is created separately.
- [ ] User explicitly confirms archive/delete.
```

- [ ] **Step 5: 运行文档检查**

```bash
npm run docs:check
rg -n "当前.*(?:JT|Sidecar)|必然启动.*Sidecar|install.*sidecar" README.md docs AGENTS.md
```

Expected: docs check PASS；没有把已退役能力描述为当前能力。

- [ ] **Step 6: 提交**

```bash
git add AGENTS.md README.md docs
git commit -m "docs(toolchain): record final Studio migration"
```

---

### Task 5: 跨仓库最终审计和归档候选

**Files：**
- Modify: `docs/retirement-checklist.md`
- Modify outside Toolchain: `ice3-studio/README.md`
- Modify outside Toolchain: `ice3-studio/AGENTS.md`
- Modify outside Toolchain: `ice3-studio/docs/README.md`
- Modify outside Toolchain:
  `ice3-studio/docs/specs/2026-11-01-native-asset-pipeline-migration-design.md`
- Modify outside Toolchain:
  `ice3-studio/docs/specs/2026-11-02-native-jt-toolchain-retirement-design.md`
- Modify outside Toolchain:
  `ice3-studio/docs/plans/2026-11-02-{native-jt-import,studio-sidecar-retirement,toolchain-repository-retirement}.md`

**Interfaces：**
- Consumes: all previous tasks.
- Produces: Phase 5 Implemented evidence and a repository archive candidate.

- [ ] **Step 1: Studio standalone audit**

在 Studio 中运行：

```bash
PLAYWRIGHT_CHANNEL=chrome npm run verify
node scripts/compare-asset-pipelines.mjs
node scripts/compare-source-imports.mjs
node scripts/compare-fbx-usd-imports.mjs
node scripts/compare-step-imports.mjs
node scripts/compare-ifc-imports.mjs
node scripts/compare-jt-imports.mjs
cargo tree -p studio-cli --no-default-features
```

Expected:

- 所有 gates PASS；
- six parity reports `status=pass`；
- no-default Studio dependency tree 不含 Toolchain、Node runtime 或 JT parser；
- Chrome E2E 使用用户安装的 Chrome 并全部通过。

- [ ] **Step 2: Toolchain audit**

```bash
npm run verify
npm run build:conformance
! rg -n "jt-importer|ICE3_JT_CONVERTER|sidecar|toolchain\\.capabilities|cache/toolchain" \
  packages tools tests scripts package.json package-lock.json \
  --glob '!tests/fixtures/**'
```

Expected: tests/build PASS；production surface search 无输出。

- [ ] **Step 3: 跨仓库引用审计**

```bash
rg -n "ice3-toolchain|@ice3/toolchain|studio-toolchain-client|ice3-tool sidecar" \
  ice3-studio/Cargo.toml ice3-studio/Cargo.lock ice3-studio/crates \
  ice3-studio/apps ice3-studio/scripts ice3-studio/benchmarks \
  ice3-studio/package.json ice3-studio/apps/viewer/package.json
```

Expected: 无输出。Historical documents 和明确归档清单不在此 production scan。

- [ ] **Step 4: 客户数据和 Git 审计**

在两个 repository 中运行：

```bash
git status --short
git diff --check
git ls-files | rg "\\.(jt|glb|gltf|fbx|usd|step|stp|ifc)$"
git branch --no-merged main
```

Expected:

- worktree clean；
- 只有明确合成 `.jt/.glb/...` fixtures；
- 没有客户路径或新生成的 output；
- 未合并分支列表为空，或在 checklist 中逐项记录。

- [ ] **Step 5: 标记 Phase 5 Implemented**

Studio 更新：

- Spec-018: `Implemented`，注明 Phase 5A/5B/5C；
- Plan-024/025/026: `Implemented`；
- Spec-016: Phase 5 `Implemented`，整份迁移设计 `Implemented`；
- `README.md`、`AGENTS.md`、`docs/README.md` 改为 Studio 唯一入口；
- `docs/retirement-checklist.md` 除 archive confirmation 外全部勾选。

Toolchain 只保留 Historical/Retired 状态，不宣称生产可用。

- [ ] **Step 6: 执行最终文档与提交**

```bash
node ../ice3-doc/scripts/check-docs.mjs --root . 2>&1 \
  | rg '2026-11-02-|native-asset-pipeline|docs/README.md'
git diff --check
```

Expected: 本次文档无 Diagnostic，diff clean。

Studio：

```bash
git add README.md AGENTS.md docs
git commit -m "docs(studio): complete native asset pipeline phase 5"
```

Toolchain：

```bash
git add docs/retirement-checklist.md
git commit -m "docs(toolchain): mark the Studio migration complete"
```

- [ ] **Step 7: 停止并请求归档授权**

不得在这一步自动 archive/delete repository。输出：

- 两个 repository 的当前 branch、HEAD、clean status；
- 全部验证命令和结果；
- 未合并 branch 清单；
- 可恢复 tag/snapshot 建议；
- 明确询问是否执行 repository archive/delete。

## Plan Self-Review

- Spec coverage：JT adapter、Sidecar、package export、release scripts、fixtures、
  docs、AGENTS、cross-repo audit、Phase 5 status 和 archive confirmation 均有 task。
- Placeholder scan：没有 TODO/TBD；无泛化的“清理相关代码”步骤。
- Type consistency：Plan-024 的 `jt`、Plan-025 的 native preview 和本计划的
  no-production-surface 条件一致。
- Review Focus：lockfile/stale dependency、CLI source acceptance、package exports、
  active architecture assertions 和 archive data audit 均由具体 gate 覆盖。
