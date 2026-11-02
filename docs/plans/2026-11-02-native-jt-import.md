# Studio 原生 JT Source Import 实施计划

> Document ID: ICE3-STUDIO-PLAN-024
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers
>
> **For agentic workers:** REQUIRED SUB-SKILL: Use
> `superpowers:subagent-driven-development` (recommended) or
> `superpowers:executing-plans` to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

## Implementation Outcome

Phase 5A 已实现。JT converter bridge、GLB validation、`f64` shared origin、
hierarchy、共享实例、Selection、metadata、隔离 worker、资源限制、失败关闭和 legacy
parity 已迁入 Studio。Sidecar 和 Toolchain repository 退役仍由 Plan-025/026 执行。

**Goal：** 把 JT 的受控 converter bridge、GLB canonicalization、Selection、metadata、
shared origin、资源限制和失败关闭迁入 Studio `studio-import`。

**架构：** `studio-cli` 在 Studio 自有 isolated worker 中执行 feature-gated
`JtImporter`；worker 以固定参数启动组织批准的 converter，验证并读取标准 GLB，再转换
为 `SourceScene`。Prepared GLB 仍只由 `studio-assets::canonical` 生成。

**技术栈：** Rust 1.98、`command-group 5.0.1`、Serde、bincode 2、现有
`studio-assets` GLB reader、Cargo test、Node.js 24.22.1（仅合成 converter fixture 与
旧 baseline capture）。

**Spec：** [Studio 原生 JT Import 与 Toolchain 最终退役](../specs/2026-11-02-native-jt-toolchain-retirement-design.md)

## Global Constraints

- Adapter ID 固定为 `jt`，只接受 `.jt`。
- `jt` feature default-on；它不引入 JT parser 或第三方 converter crate。
- Converter 只来自 host-owned `ICE3_JT_CONVERTER`；请求、Agent、项目和 CLI 参数不得
  指定 executable 或额外参数。
- `ICE3_JT_TIMEOUT_MS` 必须是正整数，默认 `300000`；JT worker 截止时间为该值加 5 秒。
- Converter 固定参数为 `--input ABSOLUTE_SOURCE.jt --output ABSOLUTE_TEMP/raw.glb`，
  不得使用 shell。
- Converter stdout/stderr 分别最多保留 64 KiB，但必须持续 drain。
- Converter output 必须是 regular file、不得超过 512 MiB、必须是合法 GLB v2。
- 默认 import limits 保持 1,000,000 nodes、20,000,000 triangles、512 MiB estimated
  decoded memory。
- Selection ID 固定为 `<assetId>/jt/node/<ordinal>`。
- Source 必须为 meter、right-handed、Y-up；Studio 不重复缩放或变换手性。
- shared origin 固定为 `[minX,0,minZ]`，在映射到 `f32 SourceScene` 前用 `f64`
  world bounds 计算。
- 同一 mesh 的多个 node 必须共享一个 `SourceMesh`，不得复制 instances。
- metadata arrays 最多 256 项，nesting depth 最多 8；warnings、materials 和 properties
  必须稳定排序。
- 失败时不得发布部分 GLB，也不得泄漏 converter stderr、客户路径或临时目录。
- Git author/committer 固定为 `damoqiongqiu <damoqiongqiu@126.com>`。

## Review Focus

- `ICE3_JT_CONVERTER` 缺失、不可执行或来自请求数据时，必须在使用前拒绝，不能从
  `PATH` 猜测另一个 executable。
- Converter 忽略 terminate 并派生子进程时，timeout 后整棵进程树必须退出，不能留下
  orphan 或稍后写出文件。
- Converter 返回目录、symlink、超限文件、截断 GLB、错误 version/length 或空 Geometry
  时，必须失败关闭且目标 output 保持不变。
- Large-origin 装配必须在 `f32` narrowing 前应用 shared origin；多个 roots、empty
  assembly、shared mesh 和重复 occurrence 不能改变业务树。
- `jt` feature 关闭时，`--adapter jt` 必须返回 adapter unavailable，产物中不得注册
  converter handler。

---

### Task 1: 将 Source Import corpus 和 JT baseline 收归 Studio

**文件：**
- Create: `fixtures/source-import/gltf/minimal.glb`
- Create: `fixtures/source-import/obj/`
- Create: `fixtures/source-import/fbx/`
- Create: `fixtures/source-import/usd/`
- Create: `fixtures/source-import/step/`
- Create: `fixtures/source-import/ifc/`
- Create: `fixtures/source-import/jt/`
- Create: `fixtures/source-import/jt/converter.mjs`
- Create: `fixtures/parity/jt-baseline-v1.json`
- Create: `scripts/compare-jt-imports.mjs`
- Modify: Rust/Node fixture path helpers in the tasks that consume each format.

**接口：**
- Consumes: Toolchain synthetic fixtures and legacy JT adapter as a one-time
  parity oracle.
- Produces: Studio-owned immutable source corpus plus
  `fixtures/parity/jt-baseline-v1.json` and the `--capture` mode of
  `scripts/compare-jt-imports.mjs`.

- [x] **Step 1: 验证源 fixture 的许可证和客户数据边界**

运行：

```bash
find ../ice3-toolchain/tests/fixtures -maxdepth 2 -type f -print | sort
rg -n "/Users/|\\\\Users\\\\|customer|confidential|客户" \
  ../ice3-toolchain/tests/fixtures
```

Expected: 只有 repository 内合成 fixture；没有客户路径、客户名字或不可分发文件。

- [x] **Step 2: 复制 corpus 到 Studio**

```bash
mkdir -p fixtures/source-import/{gltf,obj,fbx,usd,step,ifc,jt}
cp ../ice3-toolchain/tests/fixtures/minimal.glb fixtures/source-import/gltf/
cp -R ../ice3-toolchain/tests/fixtures/obj/. fixtures/source-import/obj/
cp -R ../ice3-toolchain/tests/fixtures/fbx/. fixtures/source-import/fbx/
cp -R ../ice3-toolchain/tests/fixtures/usd/. fixtures/source-import/usd/
cp -R ../ice3-toolchain/tests/fixtures/step/. fixtures/source-import/step/
cp -R ../ice3-toolchain/tests/fixtures/ifc/. fixtures/source-import/ifc/
cp -R ../ice3-toolchain/tests/fixtures/jt/. fixtures/source-import/jt/
rm fixtures/source-import/jt/fake-converter.mjs
```

只删除刚复制进来的 `fake-converter.mjs`，保留 `.jt` contract marker、negative shell
converter 和其它合成 fixture。不得删除源 repository 文件。

随后把所有 test/script 的相对 fixture helper 改为
`fixtures/source-import/<format>`，涉及：

- `crates/studio-import/tests/{fbx,usd,step,ifc,obj,gltf}_import.rs`
- `crates/studio-import/tests/ifc_metadata.rs`
- `crates/studio-cli/tests/{import,fbx_usd_import,step_import,ifc_import}.rs`
- `crates/studio-import/src/ifc/adapter.rs` 的 tests
- `scripts/compare-{source,fbx-usd,step,ifc}-imports.mjs`
- `fixtures/parity/{obj,fbx,usd,step,step-instanced,ifc}-baseline-v1.json`

断言语料已完全归 Studio 所有：

```bash
rg -n "../ice3-toolchain/tests/fixtures|../../../ice3-toolchain/tests/fixtures" \
  crates scripts fixtures \
  --glob '!fixtures/parity/*.json'
```

Expected: 无输出。baseline JSON 中的旧 Toolchain 数据保存在
`retiredBaseline`，当前 fixture path 必须指向 Studio-local corpus。

- [x] **Step 3: 生成不依赖旧 Node package 的 raw GLB contract fixtures**

在 Toolchain dependency 仍存在时运行：

```bash
node ../ice3-toolchain/tests/fixtures/jt/fake-converter.mjs \
  --input fixtures/source-import/jt/synthetic-contract.jt \
  --output fixtures/source-import/jt/synthetic-contract.raw.glb
node ../ice3-toolchain/tests/fixtures/jt/fake-converter.mjs \
  --input fixtures/source-import/jt/large-origin.jt \
  --output fixtures/source-import/jt/large-origin.raw.glb
node ../ice3-toolchain/tests/fixtures/jt/fake-converter.mjs \
  --input fixtures/source-import/jt/empty.jt \
  --output fixtures/source-import/jt/empty.raw.glb
shasum -a 256 fixtures/source-import/jt/*.raw.glb
```

Expected: 三个文件都有 `glTF` magic；重复运行后 SHA-256 不变。

- [x] **Step 4: 写无外部 npm dependency 的合成 converter**

`fixtures/source-import/jt/converter.mjs`：

```javascript
#!/usr/bin/env node
import { copyFile, readFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = dirname(fileURLToPath(import.meta.url));
const inputIndex = process.argv.indexOf('--input');
const outputIndex = process.argv.indexOf('--output');
if (inputIndex < 0 || outputIndex < 0) process.exit(2);

const marker = await readFile(process.argv[inputIndex + 1], 'utf8');
if (!marker.includes('ICE3_JT_CONVERTER_CONTRACT_V1')) process.exit(3);
const name = marker.includes('EMPTY=1')
  ? 'empty.raw.glb'
  : marker.includes('ORIGIN=')
    ? 'large-origin.raw.glb'
    : 'synthetic-contract.raw.glb';
await copyFile(join(root, name), process.argv[outputIndex + 1]);
```

运行：

```bash
chmod +x fixtures/source-import/jt/converter.mjs
fixtures/source-import/jt/converter.mjs \
  --input fixtures/source-import/jt/synthetic-contract.jt \
  --output /tmp/studio-jt-fixture.glb
cmp fixtures/source-import/jt/synthetic-contract.raw.glb \
  /tmp/studio-jt-fixture.glb
```

Expected: exit 0，文件字节完全一致。

- [x] **Step 5: 写 baseline capture 的失败测试**

`scripts/tests/jt-baseline.test.mjs`：

```javascript
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';

test('JT baseline pins business semantics', async () => {
  const baseline = JSON.parse(await readFile(
    new URL('../../fixtures/parity/jt-baseline-v1.json', import.meta.url),
    'utf8',
  ));
  assert.equal(baseline.kind, 'ice3-jt-import-baseline');
  assert.equal(baseline.stats.nodeCount, 4);
  assert.equal(baseline.stats.meshCount, 2);
  assert.equal(baseline.stats.triangleCount, 36);
  assert.equal(baseline.stats.instanceNodeCount, 1);
  assert.deepEqual(baseline.nodeNames, [
    'TreatmentLine',
    'PrimaryPump',
    'PumpInstanceA',
    'BufferTank',
  ].sort());
  assert.ok(baseline.selectionIds.every((id) =>
    id.startsWith('parity-jt/jt/node/')));
  assert.ok(baseline.instances.some((instance) =>
    instance.objectId === 'JT-PUMP-102'));
  assert.deepEqual(baseline.materials, [
    { name: 'PumpBlue', baseColorFactor: [0.12, 0.38, 0.68, 1] },
    { name: 'TankTaupe', baseColorFactor: [0.62, 0.55, 0.4, 1] },
  ]);
});
```

运行：

```bash
node --test scripts/tests/jt-baseline.test.mjs
```

预期：因 baseline 文件尚不存在而 FAIL。

- [x] **Step 6: 实现 legacy baseline capture**

`scripts/compare-jt-imports.mjs` 的 capture path：

```javascript
const toolchainRoot = resolve(studioRoot, '../ice3-toolchain');
const converter = resolve(
  studioRoot,
  'fixtures/source-import/jt/converter.mjs',
);
const fixture = resolve(
  studioRoot,
  'fixtures/source-import/jt/synthetic-contract.jt',
);
const temporary = await mkdtemp(join(tmpdir(), 'ice3-jt-capture-'));
const output = join(temporary, 'legacy.glb');
const report = join(temporary, 'legacy.report.json');

run('npm', ['--prefix', toolchainRoot, 'run', 'build']);
run('node', [
  join(toolchainRoot, 'packages/cli/dist/main.js'),
  'import',
  fixture,
  '--output',
  output,
  '--report',
  report,
  '--asset-id',
  'parity-jt',
  '--adapter',
  'jt',
  '--json',
], toolchainRoot, { ICE3_JT_CONVERTER: converter });

const document = parseGlb(await readFile(output));
const baseline = summarizeLegacy(document);
await mkdir(dirname(baselinePath), { recursive: true });
await writeFile(baselinePath, `${JSON.stringify(baseline, null, 2)}\n`);
assertBaseline(baseline);
```

`summarizeLegacy` 从 GLB JSON 读取 nodes、meshes、materials、`extras.ice3.jt` 和
`asset.extras.ice3.jt.sourceOrigin`，不得硬编码 node 名称、metadata 或 Selection。

- [x] **Step 7: 运行 capture 和 baseline test**

```bash
node scripts/compare-jt-imports.mjs --capture
node --test scripts/tests/jt-baseline.test.mjs
```

Expected: baseline 写出；node=4、mesh=2、triangle=36、instance=1；测试 PASS。

- [x] **Step 8: 提交**

```bash
git add fixtures/source-import fixtures/parity/jt-baseline-v1.json \
  scripts/compare-jt-imports.mjs scripts/tests/jt-baseline.test.mjs
git commit -m "test(studio): capture native JT import baseline"
```

---

### Task 2: 注册 feature-gated JT adapter

**文件：**
- Modify: `crates/studio-import/Cargo.toml`
- Modify: `crates/studio-cli/Cargo.toml`
- Modify: `crates/studio-import/src/source.rs`
- Modify: `crates/studio-import/src/lib.rs`
- Modify: `crates/studio-import/src/registry.rs`
- Modify: `crates/studio-import/src/bin/studio-import-worker.rs`
- Modify: `crates/studio-cli/src/import.rs`
- Modify: `crates/studio-cli/src/main.rs`
- Create: `crates/studio-import/src/jt/mod.rs`
- Create: `crates/studio-import/tests/jt_registry.rs`

**接口：**
- Consumes: existing `SourceImporter`, `ImportRequest`, `SourceScene` and
  `SourceImporterRegistry`.
- Produces:
  - `SourceFormat::Jt`
  - `JtImporter`
  - registry order `gltf, obj, fbx, usd, step, ifc, jt`
  - CLI adapter value `jt`

- [x] **Step 1: 写 registry 失败测试**

`crates/studio-import/tests/jt_registry.rs`：

```rust
#![cfg(feature = "jt")]

use std::path::Path;
use studio_import::{JtImporter, SourceImporter, SourceImporterRegistry};

#[test]
fn resolves_jt_by_extension_and_explicit_id() {
    let registry = SourceImporterRegistry::try_new(vec![Box::new(JtImporter::new())])
        .unwrap();
    assert_eq!(
        registry.resolve(Path::new("assembly.JT"), None).unwrap().id(),
        "jt"
    );
    assert_eq!(
        registry
            .resolve(Path::new("assembly.jt"), Some("jt"))
            .unwrap()
            .id(),
        "jt"
    );
    assert_eq!(JtImporter::new().extensions(), [".jt"]);
}

#[test]
fn rejects_explicit_jt_for_other_extensions() {
    let registry = SourceImporterRegistry::try_new(vec![Box::new(JtImporter::new())])
        .unwrap();
    assert_eq!(
        registry
            .resolve(Path::new("assembly.glb"), Some("jt"))
            .unwrap_err()
            .code(),
        "ICE3_STUDIO_IMPORT_ADAPTER_UNAVAILABLE"
    );
}
```

- [x] **Step 2: 运行 RED**

```bash
cargo test -p studio-import --test jt_registry
```

Expected: compile failure，`JtImporter`、`jt` feature 和 `SourceFormat::Jt` 不存在。

- [x] **Step 3: 添加 feature、format 和空 adapter**

`crates/studio-import/Cargo.toml`：

```toml
[features]
default = ["fbx", "usd", "step", "ifc", "jt"]
jt = []
```

`crates/studio-cli/Cargo.toml`：

```toml
[features]
default = ["fbx", "usd", "step", "ifc", "jt"]
jt = ["studio-import/jt"]
```

`source.rs` 增加 `Jt`，`as_str()` 返回 `"jt"`。`jt/mod.rs` 先返回：

```rust
use crate::{
    ImportError, ImportRequest, SourceCapabilities, SourceFormat, SourceImporter, SourceScene,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct JtImporter;

impl JtImporter {
    pub const fn new() -> Self {
        Self
    }
}

impl SourceImporter for JtImporter {
    fn id(&self) -> &'static str { "jt" }
    fn extensions(&self) -> &'static [&'static str] { &[".jt"] }
    fn capabilities(&self) -> SourceCapabilities {
        SourceCapabilities {
            hierarchy: true,
            vertex_colors: false,
            material_colors: true,
            instances: true,
            semantic_metadata: true,
        }
    }
    fn import(&self, _request: &ImportRequest) -> Result<SourceScene, ImportError> {
        Err(ImportError::AdapterUnavailable(
            "JT converter implementation is not wired".into(),
        ))
    }
}

pub use adapter::JtImporter;
```

实际拆分为 `jt/mod.rs` 与 `jt/adapter.rs`，避免后续 converter、rebase 和 mapping
堆在一个文件。

- [x] **Step 4: 注册 CLI/worker**

`source_importer_registry` 和 `studio-import-worker` 都在 `ifc` 后加入：

```rust
#[cfg(feature = "jt")]
importers.push(Box::new(JtImporter::new()));
```

Clap value parser：

```rust
value_parser = ["auto", "gltf", "obj", "fbx", "usd", "step", "ifc", "jt"]
```

- [x] **Step 5: 运行 GREEN 和 feature-off gate**

```bash
cargo test -p studio-import --test jt_registry
cargo check -p studio-cli --no-default-features
cargo tree -p studio-import --no-default-features
```

Expected: registry tests PASS；no-default tree 没有 JT-specific dependency。

- [x] **Step 6: 提交**

```bash
git add Cargo.toml crates/studio-import crates/studio-cli
git commit -m "feat(studio-import): register feature-gated JT adapter"
```

---

### Task 3: 实现受控 converter 启动与输出验证

**文件：**
- Modify: `Cargo.toml`
- Modify: `crates/studio-import/Cargo.toml`
- Create: `crates/studio-import/src/jt/converter.rs`
- Create: `crates/studio-import/tests/jt_converter.rs`
- Create: `fixtures/source-import/jt/capture-args.sh`
- Create: `fixtures/source-import/jt/tree-converter.sh`
- Create: `fixtures/source-import/jt/noisy-converter.sh`

**接口：**
- Consumes: `ICE3_JT_CONVERTER`、`ICE3_JT_TIMEOUT_MS`、fixed input/output paths.
- Produces:
  - `JtConverterConfig::from_env() -> Result<Self, ImportError>`
  - `run_jt_converter(input: &Path, output: &Path, config: &JtConverterConfig) -> Result<(), ImportError>`
  - `validate_jt_converter_output(output: &Path) -> Result<(), ImportError>`

- [x] **Step 1: 写 converter 契约测试**

`fixtures/source-import/jt/capture-args.sh`：

```sh
#!/bin/sh
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
while [ "$#" -gt 0 ]; do
  if [ "$1" = "--input" ]; then input="$2"; shift 2
  elif [ "$1" = "--output" ]; then output="$2"; shift 2
  else exit 9
  fi
done
printf '%s\n' "$input" "$output" > "$output.args"
cp "$root/synthetic-contract.raw.glb" "$output"
```

`crates/studio-import/tests/jt_converter.rs`：

```rust
#![cfg(feature = "jt")]

use std::time::Duration;
use studio_import::jt::{JtConverterConfig, run_jt_converter};

#[test]
fn passes_only_fixed_absolute_paths() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("assembly.jt");
    let output = root.path().join("raw.glb");
    std::fs::write(&input, "ICE3_JT_CONVERTER_CONTRACT_V1\n").unwrap();
    let config = JtConverterConfig::new(
        fixture("capture-args.sh"),
        Duration::from_secs(2),
    );

    run_jt_converter(&input, &output, &config).unwrap();

    let args = std::fs::read_to_string(root.path().join("raw.glb.args")).unwrap();
    assert_eq!(args, format!("{}\n{}\n", input.display(), output.display()));
}

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/source-import/jt")
        .join(name)
}
```

- [x] **Step 2: 写 unavailable/failure/invalid output 测试**

追加：

```rust
#[test]
fn reports_unavailable_and_non_zero_converter() {
    let root = tempfile::tempdir().unwrap();
    let input = root.path().join("assembly.jt");
    let output = root.path().join("raw.glb");
    std::fs::write(&input, "").unwrap();

    let missing = JtConverterConfig::new(
        root.path().join("missing"),
        Duration::from_secs(1),
    );
    assert_eq!(
        run_jt_converter(&input, &output, &missing).unwrap_err().code(),
        "ICE3_STUDIO_IMPORT_JT_CONVERTER_UNAVAILABLE"
    );

    let failing = JtConverterConfig::new(
        fixture("failing-converter.sh"),
        Duration::from_secs(1),
    );
    assert_eq!(
        run_jt_converter(&input, &output, &failing).unwrap_err().code(),
        "ICE3_STUDIO_IMPORT_JT_CONVERT_FAILED"
    );
}

#[test]
fn validates_regular_glb_output() {
    let root = tempfile::tempdir().unwrap();
    let invalid = root.path().join("invalid.glb");
    std::fs::write(&invalid, b"not-a-glb").unwrap();
    assert_eq!(
        studio_import::jt::validate_jt_converter_output(&invalid)
            .unwrap_err()
            .code(),
        "ICE3_STUDIO_IMPORT_JT_EXPORT_INVALID"
    );

    std::fs::copy(
        fixture("synthetic-contract.raw.glb"),
        root.path().join("valid.glb"),
    )
    .unwrap();
    studio_import::jt::validate_jt_converter_output(&root.path().join("valid.glb"))
        .unwrap();
}

#[test]
fn rejects_oversized_and_symlink_outputs() {
    let root = tempfile::tempdir().unwrap();
    let oversized = root.path().join("oversized.glb");
    let file = std::fs::File::create(&oversized).unwrap();
    file.set_len(512 * 1024 * 1024 + 1).unwrap();
    assert_eq!(
        studio_import::jt::validate_jt_converter_output(&oversized)
            .unwrap_err()
            .code(),
        "ICE3_STUDIO_IMPORT_LIMIT_EXCEEDED"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let link = root.path().join("linked.glb");
        symlink(fixture("synthetic-contract.raw.glb"), &link).unwrap();
        assert_eq!(
            studio_import::jt::validate_jt_converter_output(&link)
                .unwrap_err()
                .code(),
            "ICE3_STUDIO_IMPORT_JT_EXPORT_INVALID"
        );
    }
}
```

- [x] **Step 3: 运行 RED**

```bash
cargo test -p studio-import --test jt_converter
```

Expected: compile failure，converter API 尚不存在。

- [x] **Step 4: 添加跨平台 process-group containment**

先赋予合成 scripts 执行权限：

```bash
chmod +x fixtures/source-import/jt/{capture-args,tree-converter,noisy-converter}.sh
```

Workspace dependency：

```toml
command-group = "5.0.1"
```

`studio-import` dependency 仅在 `jt` feature 下启用：

```toml
jt = ["dep:command-group"]
command-group = { workspace = true, optional = true }
```

核心执行：

```rust
let mut command = Command::new(&config.binary);
command
    .arg("--input")
    .arg(input)
    .arg("--output")
    .arg(output)
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
let mut child = command.group_spawn().map_err(|_| converter_failed())?;
```

`command-group` 在 Unix 创建 process group，在 Windows 创建 Job Object。stdout/stderr
各用一个 collector thread 持续读取，只保留最后 65,536 bytes。

超时处理：

```rust
#[cfg(unix)]
child.signal(command_group::Signal::SIGTERM)
    .map_err(|_| converter_failed())?;
#[cfg(windows)]
child.kill().map_err(|_| converter_failed())?;

let grace = Instant::now() + Duration::from_secs(1);
while Instant::now() < grace {
    if child.try_wait().map_err(|_| converter_failed())?.is_some() {
        return Err(jt_timeout());
    }
    std::thread::sleep(Duration::from_millis(10));
}
let _ = child.kill();
let _ = child.wait();
return Err(jt_timeout());
```

- [x] **Step 5: 实现 fixed config 和 output validation**

`from_env`：

- 读取并 trim `ICE3_JT_CONVERTER`；
- 空值、缺失或 metadata 不是 regular file 时返回
  `ICE3_STUDIO_IMPORT_JT_CONVERTER_UNAVAILABLE`；
- Unix 使用 `PermissionsExt::mode() & 0o111 != 0` 验证 executable；
- `ICE3_JT_TIMEOUT_MS` 缺省为 `Duration::from_secs(300)`；数值 0、负号、小数或溢出
  返回 `ICE3_STUDIO_IMPORT_JT_CONVERT_FAILED`；
- output cap 固定为 `512 * 1024 * 1024` bytes。

`validate_jt_converter_output` 使用 `symlink_metadata` 拒绝 symlink，检查 `is_file()`、
size 范围、12-byte header、magic `glTF`、version `2` 和 declared length。

- [x] **Step 6: 增加 timeout/process-tree 负向测试**

`fixtures/source-import/jt/tree-converter.sh`：

```sh
#!/bin/sh
trap '' TERM
(sleep 2; printf orphan > "$ICE3_JT_ORPHAN_MARKER") &
while :; do sleep 1; done
```

测试设置 `ICE3_JT_ORPHAN_MARKER` 和 50 ms timeout，调用 converter；收到
`ICE3_STUDIO_IMPORT_JT_TIMEOUT` 后等待 2.5 秒并断言 marker 不存在。

```rust
#[cfg(unix)]
#[test]
fn timeout_terminates_descendants() {
    let root = tempfile::tempdir().unwrap();
    let marker = root.path().join("orphan");
    let input = root.path().join("assembly.jt");
    let output = root.path().join("raw.glb");
    std::fs::write(&input, "").unwrap();
    unsafe { std::env::set_var("ICE3_JT_ORPHAN_MARKER", &marker) };
    let config = JtConverterConfig::new(
        fixture("tree-converter.sh"),
        Duration::from_millis(50),
    );
    assert_eq!(
        run_jt_converter(&input, &output, &config).unwrap_err().code(),
        "ICE3_STUDIO_IMPORT_JT_TIMEOUT"
    );
    std::thread::sleep(Duration::from_millis(2_500));
    assert!(!marker.exists());
}
```

该测试使用进程级 env，必须与其它设 env 的 tests 串行；统一放入
`crates/studio-import/tests/jt_converter.rs`，不跨 test target 并行修改环境。

`fixtures/source-import/jt/noisy-converter.sh`：

```sh
#!/bin/sh
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
output=
while [ "$#" -gt 0 ]; do
  if [ "$1" = "--input" ]; then shift 2
  elif [ "$1" = "--output" ]; then output="$2"; shift 2
  else exit 9
  fi
done
dd if=/dev/zero bs=65536 count=4 2>/dev/null
dd if=/dev/zero bs=65536 count=4 2>/dev/null >&2
cp "$root/synthetic-contract.raw.glb" "$output"
```

增加测试：converter 分别写入 256 KiB stdout/stderr 后仍成功；测试完成时间低于 2 秒，
说明两个 pipe 都被持续 drain，而不是等到 child exit 后一次性读取。

- [x] **Step 7: 运行 GREEN**

```bash
cargo test -p studio-import --test jt_converter
cargo check -p studio-cli --no-default-features
```

Expected: converter tests PASS；no-default build 不包含 `command-group`。

- [x] **Step 8: 提交**

```bash
git add Cargo.toml Cargo.lock crates/studio-import \
  fixtures/source-import/jt/capture-args.sh \
  fixtures/source-import/jt/tree-converter.sh
git commit -m "feat(studio-import): run JT converters in a contained process"
```

---

### Task 4: 将 raw GLB 转为确定性 SourceScene

**文件：**
- Modify: `crates/studio-import/src/gltf.rs`
- Modify: `crates/studio-import/src/jt/adapter.rs`
- Create: `crates/studio-import/src/jt/rebase.rs`
- Create: `crates/studio-import/src/jt/mapping.rs`
- Create: `crates/studio-import/tests/jt_import.rs`

**接口：**
- Consumes: validated converter GLB and private
  `GltfImporter::import_source_glb(&SourceGlb, &str)`.
- Produces:
  - `JtImporter::with_converter(binary: PathBuf, timeout: Duration) -> Self`
  - `JtImporter::import_file(&Path, &str) -> Result<SourceScene, ImportError>`
  - deterministic `SourceScene` with JT node metadata and shared origin.

- [x] **Step 1: 写业务语义失败测试**

`crates/studio-import/tests/jt_import.rs`：

```rust
#![cfg(feature = "jt")]

use std::path::PathBuf;
use std::time::Duration;
use studio_import::{JtImporter, SourceFormat};

#[test]
fn preserves_hierarchy_instances_materials_and_metadata() {
    let scene = importer()
        .import_file(&fixture("synthetic-contract.jt"), "synthetic-jt")
        .unwrap();
    assert_eq!(scene.source_format, SourceFormat::Jt);
    assert_eq!(scene.meshes.len(), 2);
    assert_eq!(scene.nodes.iter().filter(|node| node.mesh.is_some()).count(), 3);

    let pump = scene.nodes.iter().find(|node| node.name == "PrimaryPump").unwrap();
    let instance = scene.nodes.iter().find(|node| node.name == "PumpInstanceA").unwrap();
    assert_eq!(pump.mesh, instance.mesh);
    assert_eq!(pump.id, "synthetic-jt/jt/node/1");
    assert_eq!(instance.id, "synthetic-jt/jt/node/2");
    assert_eq!(pump.extras["ice3"]["jt"]["objectId"], "JT-PUMP-101");
    assert_eq!(
        pump.extras["ice3"]["jt"]["customProperties"]["EquipmentTag"],
        "P-101"
    );
    assert_eq!(
        instance.extras["ice3"]["jt"]["instanceGroup"],
        pump.extras["ice3"]["jt"]["instanceGroup"]
    );
    assert_eq!(
        pump.extras["ice3"]["selectionId"],
        "synthetic-jt/jt/node/1"
    );
}

#[test]
fn rebases_large_origin_once_before_f32_mapping() {
    let scene = importer()
        .import_file(&fixture("large-origin.jt"), "large-jt")
        .unwrap();
    let origin = scene.asset_extras["ice3"]["jt"]["sourceOrigin"]
        .as_array()
        .unwrap();
    assert!(origin[0].as_f64().unwrap() > 1_000_000.0);
    let root = scene.nodes.iter().find(|node| node.name == "TreatmentLine").unwrap();
    assert!(root.matrix[12].abs() < 20.0);
    assert_world_min_x_near_zero(&scene);
}

fn importer() -> JtImporter {
    JtImporter::with_converter(
        fixture("converter.mjs"),
        Duration::from_secs(2),
    )
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/source-import/jt")
        .join(name)
}
```

- [x] **Step 2: 运行 RED**

```bash
cargo test -p studio-import --test jt_import
```

Expected: adapter unavailable 或 `with_converter`/mapping 尚不存在。

- [x] **Step 3: 暴露可复用的 GLB-to-SourceScene 入口**

把 `gltf.rs` 当前私有 `import_source` 改名为：

```rust
pub(crate) fn import_source_glb(
    source: &studio_assets::SourceGlb,
    asset_id: &str,
) -> Result<SourceScene, ImportError>
```

现有 `GltfImporter::import` 调用该入口，行为不变；先运行：

```bash
cargo test -p studio-import --test gltf_import
```

Expected: PASS。

- [x] **Step 4: 实现 `f64` world bounds 和 root rebase**

`jt/rebase.rs` 提供：

```rust
pub(crate) struct JtOrigin(pub [f64; 3]);

pub(crate) fn rebase_json(
    json: &mut serde_json::Value,
    source: &studio_assets::SourceGlb,
) -> Result<JtOrigin, ImportError>;
```

实现规则：

- 以 node `matrix` 或 translation/rotation/scale 组装 `f64` local matrix；
- parent/children 关系只遍历一次，cycle、缺失 parent 或越界 index 返回
  `ICE3_STUDIO_IMPORT_JT_GEOMETRY_INVALID`；
- POSITION 优先使用 accessor `min`/`max`；缺失时通过
  `studio_assets::read_accessor` 计算 bounds；
- 8 个 local bounds corners 经 world matrix 转换为 world bounds；
- `origin = [minX, 0, minZ]`；
- 单一 default scene root 设置 `matrix = T(-origin) * existing`，删除 TRS；
- 多个 roots 时追加 stable `"JT Root"` node，设置 translation `-origin` 并把原 roots
  设为 children；
- 不修改 accessor 或 mesh POSITION。

新增 unit tests 覆盖单 root、多 roots、matrix root、TRS root、cycle 和超出
`f64` accuracy 的 large origin。

- [x] **Step 5: 实现 JT mapping**

`jt/mapping.rs` 提供：

```rust
pub(crate) fn apply_jt_semantics(
    scene: &mut SourceScene,
    source_json: &serde_json::Value,
    origin: [f64; 3],
) -> Result<(), ImportError>;
```

实现：

- `scene.source_format = SourceFormat::Jt`；
- 按 source node ordinal 设置 `<assetId>/jt/node/<ordinal>`；
- `objectType` 为 `MESH` 或 `EMPTY`；
- `sourceName` 为空时使用 `jt/node/<ordinal>`；
- material names 从 mesh primitives 去重排序；
- mesh use count >= 2 时写 `jt/instance/<index>`；
- 只复制 string `objectId`/`assemblyPath` 和 sanitized custom properties；
- `asset_extras` 写 coordinateSystem、converterContractVersion `1.0` 和 sourceOrigin；
- 所有 JSON object key 用 `BTreeMap` 或稳定排序。

- [x] **Step 6: 接通 adapter temp workflow**

`JtImporter::import`：

```rust
let temp = tempfile::Builder::new()
    .prefix("ice3-studio-jt-")
    .tempdir()?;
let raw = temp.path().join("raw.glb");
let config = self.config_or_env()?;
run_jt_converter(&request.path, &raw, &config)?;
let mut source = studio_assets::read_glb(&raw)
    .map_err(|_| jt_export_invalid())?;
let origin = rebase_json(&mut source.json, &source)?;
let mut scene = crate::gltf::import_source_glb(&source, request.asset_id.trim())
    .map_err(map_jt_import_error)?;
apply_jt_semantics(&mut scene, &source.json, origin.0)?;
Ok(scene)
```

`tempdir` 在成功和失败时都自动清理 raw GLB。

- [x] **Step 7: 运行 GREEN**

```bash
cargo test -p studio-import --test gltf_import
cargo test -p studio-import --test jt_import
```

Expected: PASS；instances 共用 mesh，root 的 X/Z 接近零，origin metadata 保留。

- [x] **Step 8: 提交**

```bash
git add crates/studio-import/src/gltf.rs crates/studio-import/src/jt \
  crates/studio-import/tests/jt_import.rs
git commit -m "feat(studio-import): canonicalize JT source semantics"
```

---

### Task 5: 接通 CLI、isolated worker 和失败关闭

**文件：**
- Modify: `crates/studio-import/src/worker.rs`
- Modify: `crates/studio-cli/src/import.rs`
- Modify: `crates/studio-cli/src/main.rs`
- Modify: `crates/studio-import/src/jt/converter.rs`
- Create: `crates/studio-cli/tests/jt_import.rs`
- Create: `crates/studio-cli/tests/jt_failures.rs`

**接口：**
- Consumes: `JtImporter`, `run_isolated_import`, `NativeImportLimits`.
- Produces:
  - `ice3-studio import ... --adapter jt`
  - JT worker timeout/process-tree containment
  - stable `ICE3_STUDIO_IMPORT_JT_*` and empty-geometry diagnostics.

- [x] **Step 1: 写 CLI 正向失败测试**

`crates/studio-cli/tests/jt_import.rs`：

```rust
use std::path::PathBuf;
use std::process::Command;

#[test]
fn imports_jt_through_the_studio_worker() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("jt.glb");
    let result = Command::new(env!("CARGO_BIN_EXE_studio-cli"))
        .args([
            "import",
            fixture("synthetic-contract.jt").to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--asset-id",
            "cli-jt",
            "--adapter",
            "jt",
        ])
        .env("ICE3_JT_CONVERTER", fixture("converter.mjs"))
        .env("ICE3_JT_TIMEOUT_MS", "2000")
        .output()
        .unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));

    let bytes = std::fs::read(&output).unwrap();
    assert_eq!(&bytes[..4], b"glTF");
    assert!(bytes.len() > 20);
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["adapter"], "jt");
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/source-import/jt")
        .join(name)
}
```

- [x] **Step 2: 写 Diagnostic 矩阵**

`jt_failures.rs` 使用 helper 断言 exit failure、stderr code 和 output 不存在：

| 条件 | code |
|---|---|
| env 缺失 | `ICE3_STUDIO_IMPORT_JT_CONVERTER_UNAVAILABLE` |
| failing converter | `ICE3_STUDIO_IMPORT_JT_CONVERT_FAILED` |
| invalid converter | `ICE3_STUDIO_IMPORT_JT_EXPORT_INVALID` |
| empty raw GLB | `ICE3_STUDIO_IMPORT_EMPTY_GEOMETRY` |
| `ICE3_STUDIO_IMPORT_WORKER_TIMEOUT_MS=1` | `ICE3_STUDIO_IMPORT_JT_TIMEOUT` |
| raw output > 512 MiB sparse file | `ICE3_STUDIO_IMPORT_LIMIT_EXCEEDED` |

测试输出前先写入 sentinel：

```rust
std::fs::write(&output, b"keep-existing").unwrap();
```

失败后断言仍为 `keep-existing`，证明没有部分发布。

- [x] **Step 3: 运行 RED**

```bash
cargo test -p studio-cli --test jt_import --test jt_failures
```

Expected: JT CLI 尚未走 isolated worker 或 Diagnostic 不匹配。

- [x] **Step 4: 用 process group 替换 worker 的直接 child kill**

`worker.rs` 使用 `command_group::CommandGroup`：

```rust
let mut child = command.group_spawn()?;
```

超时和 stderr limit 都调用同一个 helper：

```rust
fn terminate_group(child: &mut GroupChild) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use command_group::{Signal, UnixChildExt};
        let _ = child.signal(Signal::SIGTERM);
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            if child.try_wait()?.is_some() {
                return Ok(());
            }
            std::thread::sleep(WORKER_POLL_INTERVAL);
        }
    }
    child.kill()?;
    let _ = child.wait()?;
    Ok(())
}
```

Windows Job Object 由 `command-group` 的 GroupChild 提供 `kill()` containment。

- [x] **Step 5: 让 CLI 按 adapter 设置 JT deadline**

`import_source` 先使用 registry resolve selected adapter，再在 match 中选用：

```rust
#[cfg(feature = "jt")]
"jt" => {
    let mut limits = options.limits;
    limits.timeout = jt_timeout_from_environment()? + Duration::from_secs(5);
    run_isolated_import(&JtImporter::new(), &request, limits)?
}
```

Debug-only `ICE3_STUDIO_IMPORT_WORKER_TIMEOUT_MS` 若设置则保留调用者指定的测试 deadline，
用于进程树负向测试。

- [x] **Step 6: 映射稳定错误**

`converter.rs` 使用 `ImportError::SourceDiagnostic { code, message }`。空 Geometry 返回
`ImportError::EmptyGeometry`，外层保持 Studio generic code。外层的
`ImportError::WorkerTimeout` 在 selected adapter 为 `jt` 时映射为
`ICE3_STUDIO_IMPORT_JT_TIMEOUT`。

- [x] **Step 7: 添加 worker descendant 回归测试**

CLI 测试把 `ICE3_JT_CONVERTER` 指向 `tree-converter.sh`，设置
`ICE3_STUDIO_IMPORT_WORKER_TIMEOUT_MS=100` 和 `ICE3_JT_ORPHAN_MARKER`；命令失败后等待
2.5 秒，断言 marker 不存在。

运行：

```bash
cargo test -p studio-cli --test jt_import --test jt_failures
```

Expected: all PASS，没有 orphan marker。

- [x] **Step 8: 提交**

```bash
git add crates/studio-import/src/worker.rs crates/studio-cli/src/import.rs \
  crates/studio-cli/src/main.rs crates/studio-cli/tests/jt_import.rs \
  crates/studio-cli/tests/jt_failures.rs
git commit -m "feat(studio-cli): isolate native JT imports"
```

---

### Task 6: 建立 parity、确定性和 feature-off 门禁

**文件：**
- Modify: `scripts/compare-jt-imports.mjs`
- Create: `scripts/tests/jt-import-contract.test.mjs`
- Modify: `scripts/verify.mjs`
- Modify: `fixtures/parity/jt-baseline-v1.json`

**接口：**
- Consumes: native CLI output and checked-in legacy baseline.
- Produces: machine-readable parity evidence and repeatable deterministic digest.

- [x] **Step 1: 写 parity report test**

`scripts/tests/jt-import-contract.test.mjs`：

```javascript
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';

test('native JT parity report passes every business gate', async () => {
  const report = JSON.parse(await readFile(
    new URL('../../output/jt-import-parity.json', import.meta.url),
    'utf8',
  ));
  assert.equal(report.kind, 'ice3-jt-import-parity');
  assert.equal(report.status, 'pass');
  assert.deepEqual(report.failures, []);
  assert.equal(report.native.stats.nodeCount, report.baseline.stats.nodeCount);
  assert.equal(report.native.stats.meshCount, report.baseline.stats.meshCount);
  assert.equal(report.native.stats.triangleCount, report.baseline.stats.triangleCount);
  assert.deepEqual(report.native.selectionIds, report.baseline.selectionIds);
  assert.deepEqual(report.native.businessNodes, report.baseline.businessNodes);
  assert.deepEqual(report.native.materials, report.baseline.materials);
  assert.ok(report.native.bounds.every((value, axis) =>
    Math.abs(value - report.baseline.bounds[axis]) <= 1e-4));
});
```

- [x] **Step 2: 运行 RED**

```bash
node --test scripts/tests/jt-import-contract.test.mjs
```

Expected: FAIL because native parity report 尚不存在。

- [x] **Step 3: 扩展 compare script 的 native 模式**

脚本：

- 使用 Studio-owned `fixtures/source-import/jt`；
- 以 `ICE3_JT_CONVERTER=fixtures/source-import/jt/converter.mjs` 运行
  `cargo run -q -p studio-cli -- import ... --adapter jt`；
- 两次运行到两个临时 output，断言 byte digest 相等；
- 解析 GLB JSON，比较 nodes、selection IDs、mesh count、triangle count、instance
  group、JT metadata、material names/colors、bounds 和 sourceOrigin；
- 将统计写到 `output/jt-import-parity.json`；
- 任何未列入 normalization rule 的差异使 `status=fail` 和 exit 1。

固定 normalization rule：

```text
Converter output is copied byte-for-byte from the synthetic raw GLB; no geometric
difference is permitted. Only Prepared GLB structural ordering may differ from the
legacy Prepared GLB, while source semantic counts and JT extras remain exact.
```

- [x] **Step 4: 加入 `jt` 全 feature 测试**

`scripts/verify.mjs` 在 Rust tests 前加入：

```javascript
{
  label: 'JT source import contract',
  command: 'cargo',
  args: ['test', '-p', 'studio-import', '--test', 'jt_registry',
    '--test', 'jt_converter', '--test', 'jt_import'],
},
```

并在 CLI tests 中加入：

```javascript
{
  label: 'JT CLI isolation contract',
  command: 'cargo',
  args: ['test', '-p', 'studio-cli', '--test', 'jt_import',
    '--test', 'jt_failures'],
},
```

- [x] **Step 5: 运行完整 JT 门禁**

```bash
cargo test -p studio-import --test jt_registry --test jt_converter --test jt_import
cargo test -p studio-cli --test jt_import --test jt_failures
node scripts/compare-jt-imports.mjs
node --test scripts/tests/jt-import-contract.test.mjs
cargo check -p studio-cli --no-default-features
cargo tree -p studio-import --no-default-features
```

预期：

- parity `status=pass`，`failures=[]`；
- repeated import byte digest 相同；
- CLI 失败矩阵全部通过；
- no-default tree 不包含 `command-group` 或 JT module。

- [x] **Step 6: 更新文档状态**

更新：

- `README.md`
- `AGENTS.md`
- `docs/architecture.md`
- `docs/reference/api.md`
- `docs/reference/capabilities.md`
- `docs/reference/diagnostics.md`
- `docs/guides/getting-started.md`
- `docs/specs/2026-11-02-native-jt-toolchain-retirement-design.md`
- `docs/plans/2026-11-02-native-jt-import.md`

在计划获准执行时把 Spec-018 标为 Approved，并在本 Task 完成后标注 Phase 5A
Implemented；Sidecar 和 Toolchain retirement 尚未完成，因此 Spec-018 总体仍保持
Approved。

- [x] **Step 7: 运行文档检查**

```bash
node ../ice3-doc/scripts/check-docs.mjs --root . 2>&1 \
  | rg '2026-11-02-native-jt|docs/README.md'
```

Expected: 没有本次文档命中的 Diagnostic。

- [x] **Step 8: 提交**

```bash
git add README.md AGENTS.md docs scripts/verify.mjs \
  scripts/compare-jt-imports.mjs scripts/tests/jt-import-contract.test.mjs \
  fixtures/parity/jt-baseline-v1.json
git commit -m "test(studio): prove native JT parity and isolation"
```

## Plan Self-Review

- Spec coverage：fixture ownership、feature/registry、converter policy、process tree、
  output validation、JT metadata、Selection、instances、materials、shared origin、
  limits、Diagnostics、parity 和 deterministic output 均有对应 task。
- Placeholder scan：没有 TODO/TBD 或未定义后续项；所有测试均有具体命令和期望。
- Type consistency：`JtImporter`、`JtConverterConfig`、`run_jt_converter`、
  `validate_jt_converter_output`、`JtOrigin`、`rebase_json` 和
  `apply_jt_semantics` 的名称在调用 task 中一致。
- Review Focus：request-controlled executable、timeout orphan、invalid/oversize output、
  large-origin/instances 和 feature-off 都分别由 Task 3、4、5、6 的测试覆盖。
