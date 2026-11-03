# CAD/BIM 回写（Export）设计

> Document ID: ICE3-STUDIO-SPEC-023
>
> Status: Proposed
>
> Owner: ICE3 Studio maintainers

> **阶段状态：** Phase 1–3b 已实现（STEP/IFC/STL/OBJ/DXF，均经往返校验或结构校验）。

## 1. 背景与目标

Studio 目前只能导入 STEP/IFC/JT 等源资产，产出只有 source GLB 与 Runtime Tileset，
**无法把建模结果回写到下游 CAD/BIM**，交付链路是断的。本设计补齐"导出"方向，
让 `Document → evaluate → 文件` 这条链路对 CAD/BIM 格式成立。

非目标（本设计不做）：B-rep 实体建模、STEP 参数化特征回写、图纸/标注生成。

## 2. Phase 1（已实现）：AP242 三角化 STEP

### 2.1 产物形态

- `ISO-10303-21` 文件，`FILE_SCHEMA` 为
  `AP242_MANAGED_MODEL_BASED_3D_ENGINEERING_MIM_LF`。
- 每个可见、带网格的节点 → 一个 `TRIANGULATED_SURFACE_SET`，共享
  `COORDINATES_LIST`，三角形索引 1-based。
- 节点 local transform 按父链合成为 world matrix 后**烘焙进位姿**（单坐标系），
  因此无需装配树即可被读取。
- 坐标单位：**毫米**（Studio 内部为米，导出时 ×1000）；不写单位上下文，
  读取方默认毫米（`oxideav-step` 的 `ContextUnits::default()` 即毫米）。
- 空网格 / 不可见节点跳过；非 ASCII 名称降级为 `_`。

### 2.2 入口

```
ice3-studio export step --project <dir> --output <file.step> [--force]
```

输出 JSON：`{ revision, output, bytes, triangles }`。已存在输出时无 `--force` 失败
（`ICE3_STUDIO_ASSET_OUTPUT_EXISTS`）。

### 2.3 代码落点

| 文件 | 变更 |
|---|---|
| `crates/studio-core/src/export/step.rs` | 新增：`encode_step`、`buckets_from_scene`、`export_step_from_scene`、`StepExportError` |
| `crates/studio-core/src/export/mod.rs` | 新增 `pub mod step;` |
| `crates/studio-core/Cargo.toml` | `[dev-dependencies] oxideav-step`（仅测试用，运行时零依赖） |
| `crates/studio-core/tests/step_export.rs` | 往返校验 + 变换烘焙 + 空场景 + 真实样本可选解析 |
| `crates/studio-cli/src/main.rs` | `ExportCommand::Step` 与 `export step` 处理 |

### 2.4 验收

- `cargo test -p studio-core --test step_export` 全绿。
- **往返校验**：cube 导出后由 `oxideav-step` 读回，`triangle_count() == 12`。
- **真实样本**：`projects/wastewater-plant` 导出 348,932 三角形 / 23.1 MB /
  4,950 个 face set，并可被 `oxideav-step` 重新解析。
- `cargo check -p studio-cli` 通过。

### 2.5 已知限制

- 名称非 ASCII 目前降级为 `_`（后续可改用 `\X2\…\X0\` 编码）。
- 非刚性（含缩放/剪切）节点仍会烘焙为独立 face set，不复用。

## 3. Phase 2（已实现）：材质、单位、装配

- **单位**：写 `GLOBAL_UNIT_ASSIGNED_CONTEXT`（`SI_UNIT(.MILLI.,.METRE.)`），
  `SHAPE_REPRESENTATION.context_of_items` 指向该上下文。
- **材质**：`STYLED_ITEM` → `PRESENTATION_STYLE_ASSIGNMENT` → `SURFACE_STYLE_USAGE`
  → `SURFACE_SIDE_STYLE` → `SURFACE_STYLE_FILL_AREA` → `FILL_AREA_STYLE` →
  `FILL_AREA_STYLE_COLOUR` → `COLOUR_RGB`；sRGB 转线性。
- **透明度**：`opacity < 1` 时在同一 `SURFACE_SIDE_STYLE` 追加
  `SURFACE_STYLE_RENDERING_WITH_PROPERTIES(.NORMAL_SHADING.,$,(SURFACE_STYLE_TRANSPARENT(1-opacity)))`。
- **实例复用**：按 `(mesh, material)` 生成唯一 `SHAPE_REPRESENTATION`，刚体节点用
  `REPRESENTATION_MAP`（单位原点）+ `MAPPED_ITEM`（节点世界位姿）放置；非刚性节点烘焙。
- **验收**：`exports_units_colours_and_instancing` 覆盖单位=毫米、共享网格计一次、
  两个实例、颜色与 alpha=0.5；真实工程导出由 23.1 MB 降到 **9.2 MB**（1,387 变体 /
  4,950 实例），并可被 `oxideav-step` 重新解析。

## 4. Phase 3a（已实现）：IFC4X3、STL、OBJ

- **IFC4X3**：`IFCPROJECT` + SI 毫米 `IFCUNITASSIGNMENT` + 几何上下文；每个网格一个
  共享 `IFCSHAPEREPRESENTATION`（`IFCTRIANGULATEDFACESET` +
  `IFCCARTESIANPOINTLIST3D`），每个节点一个 `IFCBUILDINGELEMENTPROXY`，用
  `IFCLOCALPLACEMENT` 设定世界位姿；非刚性节点烘焙。
- **STL（binary）**：80B 头 + u32 计数 + 50B/三角形，世界烘焙、毫米。
- **OBJ**：`o`/`v`/`f`，世界烘焙、毫米。
- **验收**：`ifc_export.rs` 用 `oxideav-ifc` 校验单位（0.001）、产品数与三角化几何；
  `mesh_export.rs` 校验 STL 头/计数与 OBJ 顶点/面；真实工程（348,932 三角形）：
  STL 17.4 MB、OBJ 26.0 MB、IFC 9.2 MB / 4,950 产品 / 1,387 共享几何，均可解析。

## 5. Phase 3b（已实现）：BIM 完整性与图纸

- **IFC 空间层级**：`IfcProject → IfcSite → IfcBuilding → IfcBuildingStorey`，
  经 `IfcRelAggregates` 聚合；所有产品经 `IfcRelContainedInSpatialStructure`
  归入 storey。
- **IFC 表面样式**：`IfcStyledItem` → `IfcSurfaceStyle` →
  `IfcSurfaceStyleShading` → `IfcColourRgb`，`opacity < 1` 写 transparency。
- **中文名**：STEP/IFC 字符串使用 `\X2\…\X0\`（UTF-16 code unit），
  `oxideav-step` / `oxideav-ifc` 可解码回 Unicode。
- **DXF**：ASCII DXF（`$ACADVER = AC1015` + `ENTITIES`），每个三角形一个
  `3DFACE`（四角面，三角形重复第三角）。
- **验收**：IFC 往返断言 project→site→building→storey→4,950 个产品、样式实体存在、
  中文名 `泵房` 解码；真实工程 IFC 9.4 MB / 4,950 产品 / 1,387 样式；DXF 73.2 MB /
  348,932 个 `3DFACE`。

## 6. 风险

- AP242 三角化在不同 CAD 的接受度不一（部分工具偏好 B-rep）。Phase 3 的 IFC 与
  STL/OBJ 作为互补通道。
- 大场景导出为单文件；Phase 2 已按变体复用，剩余体积来自网格本身，必要时按 part 分块。
- 真实 CAD 验收需在用户侧用具体软件打开确认；本仓库只保证 `oxideav-step`
  往返与结构自洽。
