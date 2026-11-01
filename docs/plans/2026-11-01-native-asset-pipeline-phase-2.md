# Studio 原生 Asset Pipeline Phase 2 实施计划

> Document ID: ICE3-STUDIO-PLAN-020
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

## Implementation Outcome

Phase 2 已实现。Studio 新增 `studio-import`、Prepared GLB canonical writer、原生
glTF/GLB 与 OBJ/MTL importer，以及 CLI `import`/`validate`。OBJ parity fixture 的
node/mesh/triangle、Selection、bounds、material 和 warning code 与退役前 Toolchain
结果一致；Toolchain OBJ npm adapter 已删除。

**目标：** 把 glTF/GLB 与 OBJ/MTL 的 Source Import 正式迁入 Studio，使 CLI 可直接生成
Studio Prepared GLB，并移除 OBJ 生产路径对 Toolchain、Node、Assimp 和 Sidecar 的依赖。

**架构：** 新增 `studio-import`；adapter 只把格式解析成统一 `SourceScene`，由
`studio-assets` 的 canonical writer 生成 Prepared GLB。glTF/GLB adapter 复用 Phase 1
reader；OBJ/MTL 使用 Rust parser，资源解析限制在源文件目录内。

**技术栈：** Rust 1.98、`gltf` 1.4.1、Serde、SHA-256、Cargo test、Node.js
24.22.1（仅 Viewer 构建、E2E 与 parity）。

**Spec：** [Studio 原生 Asset Pipeline 迁移设计](../specs/2026-11-01-native-asset-pipeline-migration-design.md)

## Global Constraints

- Phase 2 只覆盖 glTF/GLB 和 OBJ/MTL；FBX、USD、STEP、IFC、JT 留在后续阶段。
- `studio-import` 不读取 Project、HTTP、ChangeSet 或审批状态。
- adapter 不直接发布 Tileset；统一 canonical writer 负责 Prepared GLB。
- 新 import 路径不启动 Node、Assimp、Blender 或 Toolchain Sidecar。
- OBJ 的 `mtllib`、纹理和任何外部资源只能读取源文件目录内文件。
- OBJ 假定单位为米，并发出稳定 warning；顶点色与纹理不被静默保留。
- 失败不得留下 partial GLB；输出使用同目录 temporary file 后原子 rename。
- Prepared Profile 固定为 `profile=ice3-prepared`、`profileVersion=1.0`。
- Git author/committer 为 `damoqiongqiu <damoqiongqiu@126.com>`。

## Review Focus

- OBJ 的绝对路径、URI scheme、`..`、symlink escape 和缺失 MTL 必须失败关闭或产生稳定
  warning，不能越出源目录。
- OBJ face 支持 `v`、`v/vt`、`v//vn`、`v/vt/vn`、负数索引和多边形 fan triangulation。
- glTF 共享 Mesh、节点层级、Selection ID、material color、opacity 和 extras 必须保留。
- OBJ 大世界坐标必须把 geometry origin 归一到 `minX/minZ`，同时保留 provenance。
- CLI output 已存在时默认拒绝，`--force` 只原子替换成功生成的 GLB。

---

### Task 1: 建立 `studio-import` 契约与 registry

**文件：**
- Create: `crates/studio-import/Cargo.toml`
- Create: `crates/studio-import/src/lib.rs`
- Create: `crates/studio-import/src/registry.rs`
- Create: `crates/studio-import/src/source.rs`
- Create: `crates/studio-import/tests/fixtures/obj/escaping-material.obj`
- Create: `crates/studio-import/tests/fixtures/obj/faces.obj`
- Create: `crates/studio-import/tests/fixtures/obj/material.mtl`
- Create: `crates/studio-import/tests/fixtures/gltf/shared-mesh.gltf`
- Create: `crates/studio-import/tests/registry.rs`
- Modify: `Cargo.toml`

**Interfaces:**
- Consumes: `studio_assets::SourceGlb`。
- Produces:
  - `SourceImporter` trait
  - `SourceImporterRegistry`
  - `SourceScene`
  - `ImportRequest`
  - `ImportError`

- [x] **Step 1: 写 registry 失败测试**

```rust
#[test]
fn resolves_an_importer_by_extension() {
    let registry = SourceImporterRegistry::new(vec![Box::new(FakeImporter {
        id: "obj",
        extensions: &[".obj"],
    })]);
    assert_eq!(registry.resolve(Path::new("plant.OBJ"), None).id(), "obj");
}

#[test]
fn rejects_duplicate_importer_ids() {
    let first = Box::new(FakeImporter { id: "obj", extensions: &[".obj"] });
    let second = Box::new(FakeImporter { id: "obj", extensions: &[".objx"] });
    let error = SourceImporterRegistry::try_new(vec![first, second]).unwrap_err();
    assert_eq!(error.code(), "ICE3_STUDIO_IMPORT_REGISTRY_INVALID");
}
```

- [x] **Step 2: 运行 RED**

```bash
cargo test -p studio-import --test registry
```

预期：`studio-import` crate 和 registry API 尚不存在，测试编译失败。

- [x] **Step 3: 建立契约类型**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceFormat {
    Gltf,
    Obj,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SourceMaterial {
    pub id: String,
    pub name: String,
    pub base_color: [f32; 3],
    pub opacity: f32,
    pub extras: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct SourceMesh {
    pub id: String,
    pub primitives: Vec<SourcePrimitive>,
    pub extras: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct SourcePrimitive {
    pub positions: Vec<[f32; 3]>,
    pub normals: Option<Vec<[f32; 3]>>,
    pub colors: Option<Vec<[f32; 3]>>,
    pub indices: Vec<u32>,
    pub material: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct SourceNode {
    pub id: String,
    pub name: String,
    pub parent: Option<String>,
    pub matrix: [f32; 16],
    pub mesh: Option<usize>,
    pub material: Option<usize>,
    pub extras: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct SourceScene {
    pub asset_id: String,
    pub source_format: SourceFormat,
    pub nodes: Vec<SourceNode>,
    pub meshes: Vec<SourceMesh>,
    pub materials: Vec<SourceMaterial>,
    pub warnings: Vec<ImportWarning>,
    pub asset_extras: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceCapabilities {
    pub hierarchy: bool,
    pub vertex_colors: bool,
    pub material_colors: bool,
    pub instances: bool,
    pub semantic_metadata: bool,
}

#[derive(Debug, Clone)]
pub struct ImportRequest {
    pub path: std::path::PathBuf,
    pub asset_id: String,
    pub requested_adapter: Option<String>,
}

pub trait SourceImporter: Send + Sync {
    fn id(&self) -> &'static str;
    fn extensions(&self) -> &'static [&'static str];
    fn capabilities(&self) -> SourceCapabilities;
    fn import(&self, request: &ImportRequest) -> Result<SourceScene, ImportError>;
}
```

`SourceMaterial` 的 `base_color` 是 sRGB RGB，`opacity` 独立存储；canonical writer 再转成
glTF 所需线性 RGB 与 alpha。

错误码固定为：

```rust
pub enum ImportError {
    Unsupported(String),              // ICE3_STUDIO_IMPORT_UNSUPPORTED
    AdapterUnavailable(String),       // ICE3_STUDIO_IMPORT_ADAPTER_UNAVAILABLE
    RegistryInvalid(String),          // ICE3_STUDIO_IMPORT_REGISTRY_INVALID
    InvalidSource(String),            // ICE3_STUDIO_IMPORT_INVALID_SOURCE
    EmptyGeometry,                    // ICE3_STUDIO_IMPORT_EMPTY_GEOMETRY
    ResourceIgnored(String),          // ICE3_STUDIO_IMPORT_RESOURCE_IGNORED
}
```

`ResourceIgnored` 默认是 warning 数据来源；只有调用方启用 `warnings-as-errors` 时才终止。

- [x] **Step 4: 实现 registry 与稳定错误**

`resolve` 必须先按显式 adapter ID 和扩展名双重匹配，再按小写扩展名选择；未知扩展名返回
`ICE3_STUDIO_IMPORT_UNSUPPORTED`。`ImportWarning` 必须包含稳定 `code`、`message` 和
可选 `path`，并按 `(code, path, message)` 排序去重。

- [x] **Step 5: 运行 crate 测试**

```bash
cargo test -p studio-import
```

预期：registry 测试全部通过。

- [x] **Step 6: 提交**

```bash
git add Cargo.toml Cargo.lock crates/studio-import
git commit -m "feat(studio): define native source importer registry"
```

### Task 2: 实现通用 SourceScene → Prepared GLB writer

**文件：**
- Create: `crates/studio-assets/src/canonical.rs`
- Create: `crates/studio-assets/tests/canonical_writer.rs`
- Modify: `crates/studio-assets/src/lib.rs`

**Interfaces:**
- Consumes: Task 1 的 `SourceScene` 数据形状；为避免依赖方向反转，canonical writer 使用
  `CanonicalScene` 镜像类型。
- Produces:
  - `CanonicalScene`
  - `PreparedGlbOptions`
  - `write_prepared_glb(&CanonicalScene, &PreparedGlbOptions) -> Result<Vec<u8>, AssetError>`

`CanonicalScene` 与 Task 1 的 `SourceScene` 字段一一对应，但不含 warning。转换函数属于
`studio-import`，因此 `studio-assets` 不反向依赖 import crate。

```rust
pub struct PreparedGlbOptions {
    pub asset_id: String,
    pub source_format: String,
    pub source_digest: String,
}
```

Task 6 调用 `studio_import::to_canonical(&source_scene)`，该函数逐项复制 geometry、
material、node 和 `asset_extras`，不读取文件系统。

- [x] **Step 1: 写 hierarchy 与 material 测试**

```rust
#[test]
fn writes_hierarchy_selection_ids_and_materials() {
    let scene = fixture_scene();
    let bytes = write_prepared_glb(&scene, &options("writer-fixture")).unwrap();
    let source = read_glb_bytes(&bytes, &std::env::temp_dir()).unwrap();
    let profile = validate_prepared_profile(&source).unwrap();
    assert_eq!(profile.asset_id, "writer-fixture");
    assert_eq!(source.json["nodes"].as_array().unwrap().len(), 2);
    assert_eq!(
        source.json["nodes"][1]["extras"]["ice3"]["selectionId"],
        "plant/equipment/AE-101"
    );
    assert_eq!(source.json["materials"][0]["alphaMode"], "BLEND");
}
```

- [x] **Step 2: 运行 RED**

```bash
cargo test -p studio-assets --test canonical_writer
```

预期：canonical writer API 不存在。

- [x] **Step 3: 实现 canonical binary writer**

writer 必须：

1. 为每个 accessor 写入 4-byte 对齐的 binary segment。
2. 保留 node parent/children、matrix、mesh、material 和 extras。
3. 对缺失 Selection ID 的节点生成 `<assetId>/node/<ordinal>`。
4. 写入 `ice3-prepared` profile、bounds、stats、content digest、coordinate system 和
   `runtimeConversionRequired=false`。
5. 拒绝空 mesh、越界 index、NaN/Infinity 和不存在的 parent。
6. 输出确定性 JSON key 顺序与 GLB chunk padding。

- [x] **Step 4: 运行 writer 和 reader 测试**

```bash
cargo test -p studio-assets --test canonical_writer
cargo test -p studio-assets
```

预期：writer 输出可被同一个 `read_glb` 和 `validate_prepared_profile` 接受。

- [x] **Step 5: 提交**

```bash
git add crates/studio-assets
git commit -m "feat(studio): write canonical prepared GLBs"
```

### Task 3: 实现原生 glTF/GLB importer

**文件：**
- Create: `crates/studio-import/src/gltf.rs`
- Create: `crates/studio-import/tests/gltf_import.rs`
- Modify: `crates/studio-import/src/lib.rs`

**Interfaces:**
- Consumes: `studio_assets::{read_glb, read_accessor}`、Task 1 `SourceScene`。
- Produces: `GltfImporter`，ID `gltf`，扩展名 `[".gltf", ".glb"]`。

- [x] **Step 1: 写 GLB profile 与共享 Mesh 测试**

```rust
#[test]
fn imports_gltf_geometry_hierarchy_and_extras() {
    let scene = GltfImporter::new()
        .import(&fixture("minimal.glb"), "gltf-fixture")
        .unwrap();
    assert_eq!(scene.source_format, SourceFormat::Gltf);
    assert_eq!(scene.nodes.len(), 1);
    assert_eq!(scene.meshes.len(), 1);
    assert_eq!(scene.meshes[0].primitives[0].indices, [0, 1, 2]);
    assert!(scene.nodes[0].extras["ice3"]["selectionId"].is_string());
}

#[test]
fn preserves_a_mesh_shared_by_two_nodes() {
    let scene = GltfImporter::new()
        .import(&fixture("shared-mesh.gltf"), "shared")
        .unwrap();
    assert_eq!(scene.meshes.len(), 1);
    assert_eq!(scene.nodes.iter().filter(|node| node.mesh == Some(0)).count(), 2);
}
```

- [x] **Step 2: 运行 RED**

```bash
cargo test -p studio-import --test gltf_import
```

预期：`GltfImporter` 不存在。

- [x] **Step 3: 实现 import**

read accessor 后执行：

- 只支持 `TRIANGLES`；line/point primitive 产生 `ICE3_STUDIO_IMPORT_PRIMITIVE_UNSUPPORTED`。
- 保留 base color、opacity、material extras、node matrix/TRS、extras、层级和共享 Mesh。
- 去除 animation、skin、camera、texture 和 image，并记录 warning。
- 使用 node 的 `extras.ice3.selectionId`；缺失时生成 `<assetId>/node/<ordinal>`。
- 外部资源继续使用 Preview 的 bounded resolver。

- [x] **Step 4: 运行聚焦与全仓测试**

```bash
cargo test -p studio-import --test gltf_import
cargo test --workspace
```

预期：全部通过。

- [x] **Step 5: 提交**

```bash
git add crates/studio-import
git commit -m "feat(studio): import glTF through the native registry"
```

### Task 4: 实现 OBJ 资源与 MTL parser

**文件：**
- Create: `crates/studio-import/src/obj/mod.rs`
- Create: `crates/studio-import/src/obj/resources.rs`
- Create: `crates/studio-import/src/obj/mtl.rs`
- Create: `crates/studio-import/tests/obj_resources.rs`

**Interfaces:**
- Consumes: bounded source-directory resolver。
- Produces:
  - `read_obj_resources(path) -> Result<ObjResources, ImportError>`
  - `parse_mtl(bytes) -> Result<BTreeMap<String, SourceMaterial>, ImportError>`

- [x] **Step 1: 写 path escape 与 material 测试**

```rust
#[test]
fn ignores_material_paths_outside_the_obj_directory() {
    let resources = read_obj_resources(&fixture("escaping-material.obj")).unwrap();
    assert!(resources.materials.is_empty());
    assert!(resources.warnings.iter().any(|warning| {
        warning.code == "ICE3_STUDIO_IMPORT_OBJ_RESOURCE_IGNORED"
    }));
}

#[test]
fn parses_mtl_color_and_opacity() {
    let mtl = parse_mtl(br#"
newmtl steel
Kd 0.2 0.5 0.9
d 0.75
"#).unwrap();
    assert_eq!(mtl["steel"].base_color, [0.2, 0.5, 0.9]);
    assert_eq!(mtl["steel"].opacity, 0.75);
}
```

- [x] **Step 2: 运行 RED**

```bash
cargo test -p studio-import --test obj_resources
```

预期：OBJ resource/MTL API 不存在。

- [x] **Step 3: 实现资源解析**

读取每一行 `mtllib`，拒绝空值、绝对路径、URI scheme、`..` 和 symlink escape。缺失 MTL
产生 `ICE3_STUDIO_IMPORT_OBJ_MATERIAL_UNAVAILABLE` warning。MTL 支持 `newmtl`、`Kd`、
`d`、`Tr` 和 `map_Kd`；纹理引用只产生
`ICE3_STUDIO_IMPORT_OBJ_TEXTURE_UNSUPPORTED`，不读取图像。

- [x] **Step 4: 运行测试**

```bash
cargo test -p studio-import --test obj_resources
```

预期：path escape、缺失 MTL、重复 material、opacity 和 material warning 全部通过。

- [x] **Step 5: 提交**

```bash
git add crates/studio-import
git commit -m "feat(studio): parse bounded OBJ material resources"
```

### Task 5: 实现 OBJ geometry、层级与 canonical metadata

**文件：**
- Create: `crates/studio-import/src/obj/parser.rs`
- Create: `crates/studio-import/tests/obj_import.rs`
- Modify: `crates/studio-import/src/obj/mod.rs`
- Modify: `crates/studio-import/src/lib.rs`

**Interfaces:**
- Consumes: Task 4 `ObjResources`、Task 1 `SourceScene`。
- Produces: `ObjImporter`，ID `obj`，扩展名 `[".obj"]`。

- [x] **Step 1: 写 face 与 negative index 测试**

```rust
#[test]
fn imports_all_obj_face_forms_and_triangulates_polygons() {
    let scene = ObjImporter::new()
        .import(&fixture("faces.obj"), "faces")
        .unwrap();
    let primitive = &scene.meshes[0].primitives[0];
    assert_eq!(primitive.indices.len(), 9);
    assert_eq!(primitive.positions.len(), 4);
    assert_eq!(primitive.normals.as_ref().unwrap().len(), 4);
}

#[test]
fn shifts_large_world_coordinates_to_the_source_origin() {
    let scene = ObjImporter::new()
        .import(&toolchain_fixture("obj/large-origin.obj"), "large")
        .unwrap();
    let b = bounds(&scene);
    assert!(b.min[0].abs() < 1e-5);
    assert!(b.min[2].abs() < 1e-5);
    assert_eq!(
        scene.nodes[0].extras["ice3"]["obj"]["sourceOrigin"],
        serde_json::json!([1_000_000.0, 0.0, -1_000_000.0])
    );
}
```

- [x] **Step 2: 运行 RED**

```bash
cargo test -p studio-import --test obj_import
```

预期：OBJ importer 不存在。

- [x] **Step 3: 实现 parser**

parser 必须处理：

- `v`、`vn`、`vt`、`o`、`g`、`usemtl`、`mtllib`、`f` 和注释。
- 正负 vertex/normal/texture index。
- 对大于三边 face 使用围绕首点的 deterministic fan triangulation。
- 相同 `(v, vt, vn, material)` 复用 output vertex；无 normal 时标记 writer 重新计算。
- `o` 建 top-level node；`g` 建 child node；无 object/group 时使用 `OBJ Root`。
- 顶点色线格式发出 `ICE3_STUDIO_IMPORT_OBJ_VERTEX_COLORS_UNSUPPORTED`。
- 空几何、越界 index、退化 face 和未终止数字报稳定错误。

- [x] **Step 4: 生成 metadata**

OBJ node extras 固定为：

```json
{
  "ice3": {
    "selectionId": "<assetId>/obj/node/<ordinal>",
    "displayName": "<source object name>",
    "obj": {
      "objectType": "MESH",
      "sourceName": "<source object name>",
      "materialNames": ["steel"]
    }
  }
}
```

asset extras 记录 `coordinateSystem(right,y,meter)`、`unitAssumption`、
`sourceOrigin` 和 `converter="native-rust"`。

- [x] **Step 5: 运行测试**

```bash
cargo test -p studio-import --test obj_import
cargo test -p studio-import
```

预期：OBJ hierarchy、negative index、polygon、normal、large-origin、empty 和 truncated
测试全部通过。

- [x] **Step 6: 提交**

```bash
git add crates/studio-import
git commit -m "feat(studio): import OBJ geometry in Rust"
```

### Task 6: CLI `import` 与 `validate`

**文件：**
- Create: `crates/studio-cli/tests/import.rs`
- Modify: `crates/studio-cli/src/main.rs`
- Modify: `crates/studio-cli/Cargo.toml`
- Modify: `Cargo.toml`

**Interfaces:**
- Consumes: `SourceImporterRegistry`、`write_prepared_glb`。
- Produces:
  - `ice3-studio import SOURCE --output FILE --asset-id ID [--adapter auto|gltf|obj]`
  - `ice3-studio validate FILE`

- [x] **Step 1: 写 parser 与 output 测试**

```rust
#[test]
fn parses_import_and_validate_commands() {
    let import = Cli::try_parse_from([
        "ice3-studio", "import", "plant.obj",
        "--output", "plant.glb", "--asset-id", "plant",
        "--adapter", "obj", "--warnings-as-errors", "--force",
    ]).unwrap();
    assert!(matches!(import.command, Command::Import { .. }));

    let validate = Cli::try_parse_from(["ice3-studio", "validate", "plant.glb"]).unwrap();
    assert!(matches!(validate.command, Command::Validate { .. }));
}

#[test]
fn imports_obj_to_a_readable_prepared_glb() {
    let result = run_import(fixture("synthetic-plant.obj"), "plant");
    assert!(result.status.success());
    let source = read_glb(&result.output).unwrap();
    assert_eq!(
        validate_prepared_profile(&source).unwrap().asset_id,
        "plant"
    );
}
```

- [x] **Step 2: 运行 RED**

```bash
cargo test -p studio-cli parses_import_and_validate_commands
cargo test -p studio-cli --test import
```

预期：命令尚未定义。

- [x] **Step 3: 实现 import orchestration**

流程固定为：

1. registry 按 `--adapter`/扩展名解析。
2. importer 返回 `SourceScene`。
3. `studio-import` 把 `SourceScene` 转成 canonical mirror。
4. `studio_assets::write_prepared_glb` 在 memory 中生成 bytes。
5. `warnings-as-errors` 在写文件前拒绝。
6. 输出必须通过 `read_glb` 与 `validate_prepared_profile`。
7. 写 sibling temporary file，`sync_all` 后 rename；默认拒绝已有 output，`--force` 才替换。

- [x] **Step 4: 实现 validate**

`validate` 对 GLB 调用 `read_glb` + `validate_prepared_profile`；对 `.gltf` 调用 source
reader。成功输出机器可读 JSON，失败使用稳定 `AssetError.code()`。

- [x] **Step 5: 运行 CLI 与全仓测试**

```bash
cargo test -p studio-cli
cargo test --workspace
```

预期：全部通过。

- [x] **Step 6: 提交**

```bash
git add Cargo.toml Cargo.lock crates/studio-cli crates/studio-import crates/studio-assets
git commit -m "feat(studio-cli): import and validate native sources"
```

### Task 7: parity、Toolchain OBJ 退役与完整验证

**文件：**
- Create: `scripts/compare-source-imports.mjs`
- Create: `fixtures/parity/obj-baseline-v1.json`
- Modify: `README.md`
- Modify: `docs/README.md`
- Modify: `docs/architecture.md`
- Modify: `docs/reference/capabilities.md`
- Modify: `docs/guides/getting-started.md`
- Modify: `docs/specs/2026-11-01-native-asset-pipeline-migration-design.md`
- Modify: `docs/plans/2026-11-01-native-asset-pipeline-phase-2.md`
- Modify outside Studio: `ice3-toolchain/packages/cli/src/commands.ts`
- Modify outside Studio: `ice3-toolchain/package.json`
- Modify outside Studio: `ice3-toolchain/package-lock.json`
- Delete outside Studio: `ice3-toolchain/packages/obj-importer/`

- [x] **Step 1: 固化退役前 parity baseline**

先用当前 Toolchain 对以下 checked-in fixture 运行：

```bash
node ../ice3-toolchain/packages/cli/dist/main.js import \
  ../ice3-toolchain/tests/fixtures/obj/synthetic-plant.obj \
  --output output/toolchain-obj-baseline.glb \
  --asset-id parity-obj \
  --adapter obj
```

`fixtures/parity/obj-baseline-v1.json` 记录 source digest、GLB digest、triangle/vertex/
node/mesh/material counts、selection IDs、warnings、bounds 和 material colors。

- [x] **Step 2: 写 parity script**

`compare-source-imports.mjs`：

1. 对同一个 OBJ 运行 `cargo run -q -p studio-cli -- import`。
2. 读取新 GLB 与 baseline。
3. 比较 triangle/vertex/node/mesh/material count、selection IDs、bounds、material RGBA
   和 warning code。
4. 允许 GLB byte digest 不同，但必须同时记录 baseline/new digest 和原因。
5. 输出 `output/source-import-parity.json`，无法解释的差异 exit 1。

- [x] **Step 3: 运行 parity**

```bash
node scripts/compare-source-imports.mjs
```

预期：`status=pass`，OBJ semantic counts、Selection、bounds、material 和 warning code
一致；byte digest 差异被报告。

- [x] **Step 4: 退役 Toolchain OBJ 生产路径**

在 `ice3-toolchain`：

1. 删除 `packages/obj-importer/`。
2. 从 `packages/cli/src/commands.ts` 移除 OBJ lazy import 和 `obj` help 项。
3. 从 root/CLI package dependencies 移除 `@ice3/obj-importer`。
4. 保留 `tests/fixtures/obj/` 作为 parity corpus。
5. 更新 Toolchain 文档，明确 OBJ import 由 Studio 提供。

在 Toolchain 仓库运行：

```bash
npm run verify
rg -n "@ice3/obj-importer|createObjImporter|assimpjs" .
```

预期：verify 通过；生产源码和 lockfile 不再引用 OBJ adapter 或 Assimp。

- [x] **Step 5: 更新文档与阶段状态**

文档必须说明：

- `ice3-studio import` 支持 glTF/GLB 与 OBJ/MTL。
- OBJ 假定米制，移位 provenance、unsupported texture/vertex-color warning 和被忽略的
  unsafe resource。
- 新 import 路径不启动 Node、Assimp、Blender 或 Toolchain Sidecar。
- Phase 2 milestone 标记 `Implemented`；整份 Spec-016 仍为 `Approved`。
- Toolchain 只保留后续 FBX、USD、STEP、IFC、JT 迁移所需的兼容层。

- [x] **Step 6: 运行完整验证**

```bash
node ../ice3-doc/scripts/check-docs.mjs --root .
PLAYWRIGHT_CHANNEL=chrome npm run verify
node scripts/compare-asset-pipelines.mjs
node scripts/compare-source-imports.mjs
```

预期：

- Documentation checks 通过。
- 18 道 Studio gate 通过。
- Phase 1 parity 继续通过。
- Phase 2 source-import parity 通过。

- [x] **Step 7: 提交**

```bash
git add scripts/compare-source-imports.mjs fixtures/parity \
  README.md docs
git commit -m "docs(studio): complete native source import phase 2"
```
