# Studio 原生 IFC Source Import 实施计划

> Document ID: ICE3-STUDIO-PLAN-023
>
> Status: Historical
>
> Owner: ICE3 Studio maintainers

> **历史状态：** 本文保留已完成实施过程的审计记录，不代表当前生产依赖；当前能力以
> [Asset Pipeline](../reference/asset-pipeline.md) 和 [能力清单](../reference/capabilities.md)
> 为准。

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> `superpowers:subagent-driven-development` (recommended) or
> `superpowers:executing-plans` to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal：** 用 Studio 自带 worker 中的原生 Rust IFC parser/geometry pipeline 替换
Toolchain IFC adapter，同时保留 IFC4X3 空间层级、产品、Pset/Qto/System、实例、颜色、
单位、Selection 和失败关闭语义。

**架构：** `studio-import` 增加 feature-gated `ifc` adapter。adapter 对同一份 byte
buffer 提取 metadata 和 geometry，组装成 `SourceScene`；canonical writer 仍负责
Prepared GLB。CLI 通过现有 isolated worker 执行 IFC import，Toolchain IFC adapter
在 parity 通过后删除。

**技术栈：** Rust 1.98、`ifc-lite-core 27.1.0`、`ifc-lite-processing 27.1.0`、
Serde、bincode 2、Cargo test、Node.js 24.22.1（仅 parity 与 E2E）。

**Spec：** [Studio 原生 STEP 与 IFC Source Import](../specs/2026-11-02-native-step-ifc-import-design.md)

## 1. Global Constraints

- 只承诺 IFC4X3；扩展名固定为 `.ifc`。
- 新 import 路径不得启动外部 DCC、Node、Toolchain Sidecar 或转换 executable。
- Studio Core 不链接 IFC parser/geometry engine；解析只在 `studio-import` worker
  子进程内执行。
- adapter 只能产出 `SourceScene`；Prepared GLB 只能由 `studio-assets::canonical`
  生成。
- 默认限制固定为 1,000,000 nodes、20,000,000 triangles、512 MiB estimated decoded
  memory、120 秒硬 timeout。
- Selection ID 固定为 `<assetId>/ifc/<GlobalId>`；缺失或重复 GlobalId 使用稳定
  Express ID 后备。
- 输出统一为 right-handed、Y-up、meter，并只应用一次单位/朝向转换。
- source origin 固定为 `[minX,0,minZ]`，只平移 scene root。
- metadata 数组最多 256 项，depth 最多 8，warning 和输出顺序必须确定。
- `ifc` feature 关闭时不得注册 adapter，也不得链接 IFC parser/geometry engine。
- Git author/committer 固定为 `damoqiongqiu <damoqiongqiu@126.com>`。

## 2. Review Focus

- IFC metadata 与 geometry 两次读取必须按 Express ID 正确关联，不能因重复名称丢设备。
- spatial aggregate/containment 顺序必须稳定，未归属 geometry 产品不能消失。
- 缺失/冲突单位、非 IFC4X3 schema 和 malformed file 必须在发布前失败关闭。
- 同一 representation map 的多个 occurrence 必须共享 template mesh，同时保留独立
  Selection、transform 和 metadata。
- Pset/Qto/System、material、bounds、surface area 和 triangle ratio 必须满足
  `ICE3-STUDIO-SPEC-017` 的 parity 门槛。

---

### 2.1 Task 1: 固化 IFC baseline

**Files：**
- Create: `scripts/compare-ifc-imports.mjs`
- Create: `fixtures/parity/ifc-baseline-v1.json`

**Interfaces：**
- Consumes: Toolchain `packages/cli/src/main.ts import ... --adapter ifc`
- Produces: checked-in IFC baseline，由后续 native parity 使用

- [x] **Step 1: 写 capture 入口**

`scripts/compare-ifc-imports.mjs` 先实现 `--capture`。固定读取：

- `synthetic-plant.ifc`
- `duplicate-global-id.ifc`
- `missing-global-id.ifc`
- `orphan-product.ifc`
- `semantic-only-product.ifc`

baseline 结构：

```javascript
const baseline = {
  kind: 'ice3-ifc-import-baseline',
  schemaVersion: '1.0',
  assetId: 'parity-ifc',
  fixtures: {
    syntheticPlant: {
      sourceDigest,
      stats: {
        nodeCount,
        meshCount,
        triangleCount: 24,
        vertexCount,
        instanceNodeCount: 0,
        semanticMaterialCount: 2,
      },
      nodeNames,
      selectionIds,
      spatialPaths,
      propertySets,
      quantitySets,
      systems,
      bounds,
      surfaceArea,
      materials,
      warningCodes,
    },
    // negative metadata-only fixtures retain expected stable codes
  },
  normalizationRules: [
    'Tessellation triangles may differ while bounds, area and business identity remain equivalent.',
  ],
};
```

脚本必须解析 Toolchain `--report`、Prepared GLB JSON 和 node extras，不得硬编码
property/system 值。

- [x] **Step 2: 运行 capture**

```bash
node scripts/compare-ifc-imports.mjs --capture
```

Expected: 写出 baseline；主 fixture triangle count 为 `24`，mesh count 为 `2`，
包含 `Primary Pump`、`Buffer Tank`、`Pset_PumpTypeCommon`、`Qto_PumpBaseQuantities`
和 `Sludge Transfer`。

- [x] **Step 3: 增加 baseline invariants**

```javascript
function assertBaseline(value) {
  const main = value.fixtures.syntheticPlant;
  if (main.stats.meshCount !== 2) throw new Error('mesh baseline');
  if (main.stats.triangleCount !== 24) throw new Error('triangle baseline');
  if (!main.selectionIds.some((id) => id.startsWith('parity-ifc/ifc/'))) {
    throw new Error('selection baseline');
  }
  if (!main.propertySets.some((set) => set.name === 'Pset_PumpTypeCommon')) {
    throw new Error('pset baseline');
  }
  if (!main.quantitySets.some((set) => set.name === 'Qto_PumpBaseQuantities')) {
    throw new Error('qto baseline');
  }
  if (!main.systems.some((system) => system.name === 'Sludge Transfer')) {
    throw new Error('system baseline');
  }
}
```

- [x] **Step 4: 提交**

```bash
git add scripts/compare-ifc-imports.mjs fixtures/parity/ifc-baseline-v1.json
git commit -m "test(studio): capture native IFC import baseline"
```

---

### 2.2 Task 2: 添加 IFC feature 与 metadata extractor

**Files：**
- Modify: `Cargo.toml`
- Modify: `crates/studio-import/Cargo.toml`
- Modify: `crates/studio-import/src/source.rs`
- Modify: `crates/studio-import/src/lib.rs`
- Create: `crates/studio-import/src/ifc/mod.rs`
- Create: `crates/studio-import/src/ifc/metadata.rs`
- Create: `crates/studio-import/tests/ifc_metadata.rs`

**Interfaces：**
- Consumes: IFC source bytes and `NativeImportLimits`
- Produces:
  - `SourceFormat::Ifc`
  - `ImportError::SourceDiagnostic { code, message }`（若 Phase 4A 尚未引入则在本
    task 中一并添加）
  - `extract_ifc_metadata(bytes, asset_id, limits) -> Result<IfcMetadata, ImportError>`
  - `IfcMetadata`、`IfcNodeMetadata`、`IfcProductMetadata`

- [x] **Step 1: 写失败测试**

```rust
use std::path::PathBuf;

#[test]
fn extracts_ifc4x3_space_properties_quantities_and_systems() {
    let bytes = std::fs::read(fixture("ifc/synthetic-plant.ifc")).unwrap();
    let metadata = extract_ifc_metadata(
        &bytes,
        "ifc-fixture",
        NativeImportLimits::default(),
    )
    .unwrap();
    assert_eq!(metadata.schema, "IFC4X3");
    let pump = metadata.products.values()
        .find(|product| product.name == "Primary Pump")
        .unwrap();
    assert_eq!(pump.ifc_type, "IFCPUMP");
    assert_eq!(pump.selection_id, "ifc-fixture/ifc/1iwOd0pZv1BAvoVKqbjYQK");
    assert!(pump.property_sets.iter()
        .any(|set| set.name == "Pset_PumpTypeCommon"));
    assert!(pump.quantity_sets.iter()
        .any(|set| set.name == "Qto_PumpBaseQuantities"));
    assert!(pump.systems.iter()
        .any(|system| system.name == "Sludge Transfer"));
}

fn fixture(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../ice3-toolchain/tests/fixtures")
        .join(relative)
}
```

- [x] **Step 2: 运行 RED**

```bash
cargo test -p studio-import --test ifc_metadata
```

Expected: compile failure，IFC module 尚不存在。

- [x] **Step 3: 添加 feature**

`Cargo.toml`：

```toml
[workspace.dependencies]
ifc-lite-core = "=27.1.0"
ifc-lite-processing = "=27.1.0"
```

`crates/studio-import/Cargo.toml`：

```toml
[features]
default = ["fbx", "usd", "ifc"]
ifc = ["dep:ifc-lite-core", "dep:ifc-lite-processing"]

[dependencies]
ifc-lite-core = { workspace = true, optional = true }
ifc-lite-processing = { workspace = true, optional = true }
```

`SourceFormat` 增加 `Ifc`，字符串值为 `ifc`；`lib.rs` 增加
`#[cfg(feature = "ifc")] mod ifc;`。

- [x] **Step 4: 定义 metadata DTO**

```rust
#[derive(Debug)]
pub struct IfcMetadata {
    pub schema: String,
    pub project_id: u32,
    pub nodes: BTreeMap<u32, IfcNodeMetadata>,
    pub products: BTreeMap<u32, IfcProductMetadata>,
    pub systems: BTreeMap<u32, IfcSystemMetadata>,
    pub warnings: Vec<ImportWarning>,
}

#[derive(Debug)]
pub struct IfcNodeMetadata {
    pub express_id: u32,
    pub parent_id: Option<u32>,
    pub path: String,
    pub ifc_type: String,
    pub name: String,
    pub children: Vec<u32>,
    pub elements: Vec<u32>,
    pub elevation: Option<f64>,
}

#[derive(Debug)]
pub struct IfcProductMetadata {
    pub express_id: u32,
    pub ifc_type: String,
    pub global_id: Option<String>,
    pub selection_id: String,
    pub name: String,
    pub description: Option<String>,
    pub object_type: Option<String>,
    pub predefined_type: Option<String>,
    pub tag: Option<String>,
    pub property_sets: Vec<IfcPropertySetMetadata>,
    pub quantity_sets: Vec<IfcQuantitySetMetadata>,
    pub systems: Vec<IfcSystemReference>,
}
```

- [x] **Step 5: 实现 extraction**

```rust
pub fn extract_ifc_metadata(
    bytes: &[u8],
    asset_id: &str,
    limits: NativeImportLimits,
) -> Result<IfcMetadata, ImportError> {
    if bytes.len() > limits.max_memory_bytes {
        return Err(ImportError::LimitExceeded("IFC input byte budget".into()));
    }
    let schema = ifc_lite_processing::detect_schema_version(bytes);
    if !schema.eq_ignore_ascii_case("IFC4X3") && !schema.starts_with("IFC4X3_") {
        return Err(ImportError::SourceDiagnostic {
            code: "ICE3_STUDIO_IMPORT_IFC_SCHEMA_UNSUPPORTED".into(),
            message: format!("unsupported IFC schema: {schema}"),
        });
    }
    let data = ifc_lite_processing::data_model::extract_data_model(bytes);
    if data.entities.len() > limits.max_nodes {
        return Err(ImportError::LimitExceeded("IFC entity budget".into()));
    }
    build_metadata(data, schema, asset_id)
}
```

`build_metadata`：

- 从 `spatial_hierarchy` 生成 `IfcNodeMetadata`，按 `(level, path, entity_id)` 排序。
- `entities` 生成产品 lookup；geometry-bearing entity 即使不在空间树中也不能丢。
- `relationships` 的 `IfcRelDefinesByProperties` 关联 product -> pset/qset。
- `IfcRelAssignsToGroup` 关联 product -> system group。
- selection ID：

```rust
let selection_id = match global_id {
    Some(id) if first_use => format!("{asset_id}/ifc/{id}"),
    Some(id) => format!("{asset_id}/ifc/{id}/express/{express_id}"),
    None => format!("{asset_id}/ifc/express/{express_id}"),
};
```

- property/quantity/systems 都使用 `BTreeMap` 或按名称排序。
- metadata value 仅保留 string、finite number、bool 和 primitive array；嵌套深度
  超过 8 或数组超过 256 项时省略并产生
  `ICE3_STUDIO_IMPORT_IFC_METADATA_TRUNCATED` warning。

- [x] **Step 6: 运行 GREEN**

```bash
cargo test -p studio-import --test ifc_metadata
cargo test -p studio-import
```

Expected: all pass，metadata 输出固定。

- [x] **Step 7: 提交**

```bash
git add Cargo.toml Cargo.lock crates/studio-import/src/ifc \
  crates/studio-import/src/source.rs crates/studio-import/src/lib.rs \
  crates/studio-import/tests/ifc_metadata.rs
git commit -m "feat(studio): extract native IFC business metadata"
```

---

### 2.3 Task 3: 实现 IFC geometry、实例和 Importer

**Files：**
- Create: `crates/studio-import/src/ifc/geometry.rs`
- Create: `crates/studio-import/src/ifc/adapter.rs`
- Modify: `crates/studio-import/src/ifc/mod.rs`
- Create: `crates/studio-import/tests/ifc_import.rs`

**Interfaces：**
- Consumes: Task 2 `extract_ifc_metadata`
- Produces:
  - `UsdImporter` 同构的 `IfcImporter`
  - `IfcImporter::import_file(&Path, &str) -> Result<SourceScene, ImportError>`
  - shared geometry templates and occurrence nodes

- [x] **Step 1: 写失败测试**

```rust
use studio_import::{IfcImporter, SourceFormat};

#[test]
fn imports_ifc4x3_hierarchy_geometry_and_metadata() {
    let scene = IfcImporter::new()
        .import_file(&fixture("ifc/synthetic-plant.ifc"), "ifc-fixture")
        .unwrap();
    assert_eq!(scene.source_format, SourceFormat::Ifc);
    assert_eq!(scene.meshes.len(), 2);
    assert!(scene.nodes.iter().any(|node| {
        node.name == "Treatment Building"
            && node.extras["ice3"]["ifc"]["ifcType"] == "IFCBUILDING"
    }));
    let pump = scene.nodes.iter()
        .find(|node| node.name == "Primary Pump")
        .unwrap();
    assert_eq!(pump.id, "ifc-fixture/ifc/1iwOd0pZv1BAvoVKqbjYQK");
    assert_eq!(
        pump.extras["ice3"]["ifc"]["propertySets"][0]["name"],
        "Pset_PumpTypeCommon"
    );
    assert_eq!(
        pump.extras["ice3"]["ifc"]["quantitySets"][0]["name"],
        "Qto_PumpBaseQuantities"
    );
}
```

- [x] **Step 2: 运行 RED**

```bash
cargo test -p studio-import --test ifc_import
```

Expected: `IfcImporter` 尚未定义。

- [x] **Step 3: 调用原生 geometry pipeline**

```rust
let args = ifc_lite_processing::ProcessingRequest::full_pass()
    .retain_meshes(true)
    .instancing(true);
let result = ifc_lite_processing::process(bytes, &args, &mut ());
let unit_scale = result.metadata.length_unit_scale.ok_or_else(|| {
    ImportError::SourceDiagnostic {
        code: "ICE3_STUDIO_IMPORT_IFC_UNIT_UNSUPPORTED".into(),
        message: "missing IFC length unit".into(),
    }
})?;
if !unit_scale.is_finite() || unit_scale <= 0.0 {
    return Err(ImportError::SourceDiagnostic {
        code: "ICE3_STUDIO_IMPORT_IFC_UNIT_UNSUPPORTED".into(),
        message: format!("invalid IFC length unit scale: {unit_scale}"),
    });
}
```

若 `result.meshes` 无 triangle，返回 `EmptyGeometry`。triangle count 和 estimated
memory 必须在构造输出前检查。

- [x] **Step 4: 生成空间和产品节点**

先按 Task 2 metadata 创建所有 spatial nodes，再引用 `result.meshes` 与
`result.instances`。节点排序固定为：

1. metadata spatial depth-first；
2. geometry-only products by Express ID；
3. occurrence records by Express ID。

每个空间 node 的 matrix 为 identity；产品 geometry 使用 geometry pipeline 的米制
world frame 和共享 origin。`IfcMappedItem` template mesh 只创建一次；每个
`InstanceRecord` 生成 occurrence node，`mesh` 指向 template index，
`instanceGroup = ifc/instance/<rep_identity>`。

- [x] **Step 5: 转换 geometry 和 material**

```rust
for mesh in &result.meshes {
    let source_mesh = SourceMesh {
        id: format!("ifc/mesh/{}", mesh.express_id),
        primitives: vec![SourcePrimitive {
            positions: mesh.positions.chunks_exact(3)
                .map(|p| [p[0], p[2], -p[1]])
                .collect(),
            normals: (!mesh.normals.is_empty()).then(|| {
                mesh.normals.chunks_exact(3)
                    .map(|n| [n[0], n[2], -n[1]])
                    .collect()
            }),
            colors: None,
            indices: mesh.indices.clone(),
            material: Some(material_index),
        }],
        extras: json!({ "ice3": { "ifc": { "geometryItemId": mesh.geometry_item_id } } }),
    };
}
```

所有 positions 加上 `mesh.origin` 后在 `f64` 中计算 bounds 和 origin subtraction；
不得先转 f32 再减 origin。material 按 `[name, rounded rgba]` 去重并稳定排序。

- [x] **Step 6: 写 metadata extras**

产品 node extras 固定为 `extras.ice3.ifc`，包含 Task 2 的 `schema`、`expressId`、
`globalId`、`ifcType`、`name`、`description`、`objectType`、`predefinedType`、
`tag`、`spatialPath`、`propertySets`、`quantitySets` 和 `systems`。
geometry 阶段产生的 `instanceGroup` 合并到同一对象。asset extras 写入
`ice3.coordinateSystem`、`ice3.ifc.schema` 和 `ice3.ifc.sourceOrigin`。

- [x] **Step 7: 运行 GREEN**

```bash
cargo test -p studio-import --test ifc_import
cargo test -p studio-import
cargo clippy -p studio-import --all-targets -- -D warnings
```

Expected: all pass，Prepared GLB validation pass。

- [x] **Step 8: 新增实例回归 fixture 和测试**

创建 `crates/studio-import/tests/fixtures/ifc/instanced.ifc`，包含两个
`IFCMAPPEDITEM` product 引用同一 `IFCREPRESENTATIONMAP`。测试固定：

```rust
#[test]
fn preserves_ifc_representation_map_instances() {
    let scene = IfcImporter::new()
        .import_file(&fixture("ifc/instanced.ifc"), "ifc-instances")
        .unwrap();
    let mesh_nodes = scene.nodes.iter().filter(|node| node.mesh.is_some()).collect::<Vec<_>>();
    assert_eq!(mesh_nodes.len(), 2);
    assert_eq!(mesh_nodes[0].mesh, mesh_nodes[1].mesh);
    assert_ne!(mesh_nodes[0].id, mesh_nodes[1].id);
    assert!(mesh_nodes.iter().all(|node| {
        node.extras["ice3"]["ifc"]["instanceGroup"]
            .as_str()
            .unwrap()
            .starts_with("ifc/instance/")
    }));
}
```

运行 `cargo test -p studio-import --test ifc_import`，Expected: pass。

- [x] **Step 9: 增加 unsupported resource warning**

`MeshData.texture.is_some()` 时产生
`ICE3_STUDIO_IMPORT_IFC_UNSUPPORTED_RESOURCE`，message 固定包含
`surface texture <texture_id>`；camera/light/skeleton/volume 由 core entity scan 产生
同类 warning。warning 使用 `BTreeSet` 去重排序。

在 mapper unit test 中构造一个带 `texture` 的 `MeshData`，断言 warning code 和稳定
message；再断言无 texture 的 mesh 不产生该 warning。

- [x] **Step 10: 提交**

```bash
git add crates/studio-import/src/ifc crates/studio-import/tests/ifc_import.rs \
  crates/studio-import/tests/fixtures/ifc/instanced.ifc
git commit -m "feat(studio): import IFC with native geometry"
```

---

### 2.4 Task 4: 接入 CLI worker 与 IFC 失败关闭

**Files：**
- Modify: `crates/studio-cli/Cargo.toml`
- Modify: `crates/studio-cli/src/import.rs`
- Modify: `crates/studio-cli/src/main.rs`
- Modify: `crates/studio-import/src/bin/studio-import-worker.rs`
- Create: `crates/studio-cli/tests/ifc_import.rs`
- Modify: `scripts/compare-ifc-imports.mjs`

**Interfaces：**
- Consumes: `IfcImporter`、`run_isolated_import`
- Produces: `--adapter auto|...|ifc` 和 native IFC parity

- [x] **Step 1: 写 CLI tests**

```rust
#[test]
fn imports_ifc_through_the_internal_worker() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("ifc.glb");
    let result = Command::new(env!("CARGO_BIN_EXE_ice3-studio"))
        .args([
            "import",
            fixture("ifc/synthetic-plant.ifc").to_str().unwrap(),
            "--output", output.to_str().unwrap(),
            "--asset-id", "ifc-fixture",
            "--adapter", "ifc",
        ])
        .output()
        .unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let parsed = read_glb(&output).unwrap();
    assert_eq!(validate_prepared_profile(&parsed).unwrap().asset_id, "ifc-fixture");
}

#[test]
fn unsupported_ifc_schema_does_not_replace_output() {
    let root = tempfile::tempdir().unwrap();
    let output = root.path().join("ifc.glb");
    std::fs::write(&output, b"keep").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_ice3-studio"))
        .args([
            "import",
            fixture("ifc/ifc4-legacy.ifc").to_str().unwrap(),
            "--output", output.to_str().unwrap(),
            "--asset-id", "ifc-legacy",
        ])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr)
        .contains("ICE3_STUDIO_IMPORT_IFC_SCHEMA_UNSUPPORTED"));
    assert_eq!(std::fs::read(&output).unwrap(), b"keep");
}
```

- [x] **Step 2: 运行 RED**

```bash
cargo test -p studio-cli --test ifc_import
```

Expected: adapter unavailable。

- [x] **Step 3: 注册 feature、registry 和 worker**

```toml
[features]
default = ["fbx", "usd", "step", "ifc"]
ifc = ["studio-import/ifc"]
```

`studio-cli/src/import.rs`：

```rust
#[cfg(feature = "ifc")]
use studio_import::IfcImporter;

#[cfg(feature = "ifc")]
importers.push(Box::new(IfcImporter::new()));

let imported = match selected_adapter {
    #[cfg(feature = "ifc")]
    "ifc" => run_isolated_import(&IfcImporter::new(), &request, options.limits)?,
    // existing branches
};
```

Clap value parser 和 hidden worker registry 顺序固定为最后加入 `ifc`。

- [x] **Step 4: 扩展 parity**

`compare-ifc-imports.mjs` 比较：

- node/spatial path names、mesh count、instance count、Selection IDs；
- material identity/RGBA；
- bounds tolerance；
- surface area <= 2%；
- triangle ratio 在 `0.75..=1.33`；
- warnings 和 diagnostics；
- Pset/Qto/System exact semantic JSON。

triangle/vertex/digest 差值写入 report，不阻断。

- [x] **Step 5: 运行 GREEN**

```bash
cargo test -p studio-cli --test ifc_import
cargo test -p studio-cli
node scripts/compare-ifc-imports.mjs
```

Expected: tests pass，parity `status=pass`。

- [x] **Step 6: 增加负向 corpus**

对：

- `ifc4-legacy.ifc` -> `ICE3_STUDIO_IMPORT_IFC_SCHEMA_UNSUPPORTED`
- `unsupported-unit.ifc` -> `ICE3_STUDIO_IMPORT_IFC_UNIT_UNSUPPORTED`
- `conflicting-units.ifc` -> `ICE3_STUDIO_IMPORT_IFC_UNIT_UNSUPPORTED`
- `empty-model.ifc` -> `ICE3_STUDIO_IMPORT_EMPTY_GEOMETRY`
- `truncated.ifc` -> `ICE3_STUDIO_IMPORT_INVALID_SOURCE`
- `ICE3_STUDIO_IMPORT_MAX_TRIANGLES=1` -> `ICE3_STUDIO_IMPORT_LIMIT_EXCEEDED`
- `ICE3_STUDIO_IMPORT_WORKER_TIMEOUT_MS=1` -> `ICE3_STUDIO_IMPORT_TIMEOUT`

逐项断言不发布新输出，已有 sentinel 文件保持不变。

- [x] **Step 7: 提交**

```bash
git add crates/studio-cli crates/studio-import/src/bin/studio-import-worker.rs \
  scripts/compare-ifc-imports.mjs fixtures/parity/ifc-baseline-v1.json
git commit -m "feat(studio-cli): import IFC without external bridges"
```

---

### 2.5 Task 5: 退役 Toolchain IFC bridge 并完成验证

**Files：**
- Modify outside Studio: `ice3-toolchain/packages/cli/src/commands.ts`
- Modify outside Studio: `ice3-toolchain/packages/cli/package.json`
- Modify outside Studio: `ice3-toolchain/package-lock.json`
- Modify outside Studio: `ice3-toolchain/tsconfig.json`
- Modify outside Studio: `ice3-toolchain/tools/conformance/corpus.ts`
- Delete outside Studio: `ice3-toolchain/packages/ifc-importer/`
- Delete outside Studio: `ice3-toolchain/tests/ifc/`
- Delete outside Studio: `ice3-toolchain/tests/cli/ifcImportCommand.test.ts`
- Modify: `README.md`
- Modify: `AGENTS.md`
- Modify: `docs/README.md`
- Modify: `docs/architecture.md`
- Modify: `docs/reference/capabilities.md`
- Modify: `docs/reference/diagnostics.md`
- Modify: `docs/guides/getting-started.md`
- Modify: `docs/specs/2026-11-02-native-step-ifc-import-design.md`
- Modify: this plan

**Interfaces：**
- Consumes: passing native IFC parity
- Produces: Toolchain conformance 只剩 JT，Studio 默认加载 IFC

- [x] **Step 1: 删除 IFC production bridge**

从 Toolchain CLI lazy imports、help、package dependencies、tsconfig references 和
conformance manifest 删除 IFC。删除 `packages/ifc-importer`、`tests/ifc` 和
IFC-only command tests，但保留 `tests/fixtures/ifc/`。

- [x] **Step 2: 更新文档和状态**

文档写明：

- Studio 原生 IFC4X3 import、spatial hierarchy、Pset/Qto/System、Selection 和
  coordinate system。
- worker 限制、单位拒绝、几何失败关闭和 metadata 截断 warning。
- Toolchain conformance 只剩 JT。
- Phase 4B 和 Plan-023 标记 Implemented；Phase 4、Spec-017 一并标记 Implemented。

- [x] **Step 3: 更新 notices 和 feature isolation**

Studio `THIRD_PARTY_NOTICES.md` 添加 IFC parser/geometry MPL-2.0 完整 notice。
`third_party_notices.rs` 断言三个 package/version 和 MPL 文本存在。

```bash
cargo check -p studio-cli --no-default-features
cargo test -p studio-import --no-default-features --tests
cargo tree -p studio-import --no-default-features
```

Expected: 第四种依赖树不包含 `ifc-lite-core` 或 `ifc-lite-processing`。

- [x] **Step 4: 运行完整门禁**

Studio：

```bash
PLAYWRIGHT_CHANNEL=chrome npm run verify
node scripts/compare-asset-pipelines.mjs
node scripts/compare-source-imports.mjs
node scripts/compare-fbx-usd-imports.mjs
node scripts/compare-step-imports.mjs
node scripts/compare-ifc-imports.mjs
```

Toolchain：

```bash
npm run verify
rg -n "ifc-importer|web-ifc" packages tools package.json package-lock.json
test -f tests/fixtures/ifc/synthetic-plant.ifc
```

Expected: Studio 18/18 gates、Chrome E2E、五份 parity pass；Toolchain tests pass；
production search 无 IFC bridge；fixture 保留。

- [x] **Step 5: 提交**

Studio：

```bash
git add README.md AGENTS.md docs THIRD_PARTY_NOTICES.md \
  crates/studio-import/tests/third_party_notices.rs
git commit -m "docs(studio): complete native IFC import phase 4b"
```

Toolchain：

```bash
git add -A
git commit -m "refactor(toolchain): retire the IFC source bridge"
```

## 3. Plan Self-Review

- Spec coverage：IFC4X3 schema、spatial hierarchy、products、Pset/Qto/System、
  instances、material、worker、feature gate、parity 和 Toolchain 退役都有对应 task。
- Placeholder scan：未发现占位项；校验值、Diagnostic、命令和提交边界已固定。
- Type consistency：`IfcImporter`、`IfcMetadata`、`IfcProductMetadata`、
  `extract_ifc_metadata` 和 `SourceFormat::Ifc` 跨 task 一致。
- Review Focus：metadata/geometry 关联、空间顺序、单位/schema、共享实例和几何偏差
  都由 Task 2–4 的具体测试覆盖。
