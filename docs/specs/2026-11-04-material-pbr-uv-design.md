# 材质表面、UV 与 PBR 导出设计

> Document ID: ICE3-STUDIO-SPEC-032
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

## 1. 背景与目标

到 SPEC-031 为止，材质只有"基础色 + 不透明度 + 描边色 + 着色模式"，glTF 导出里
`metallicFactor` 硬编码 0、`roughnessFactor` 硬编码 0.9，网格也完全没有 UV。结果
是：导出的资产进 Blender / three.js 后只能当彩色塑料块，贴不上图。

本设计补三件事：

1. **PBR 表面参数**：`MaterialSurface`（metallic / roughness / emissive / uvScale /
   baseColorTexture），作为材质的可选子结构；
2. **UV 生成**：不改网格顶点结构，按主导轴盒投影为任意网格（含导入模型、布尔结果）
   现算 UV，单位是米；
3. **glTF 导出保真**：`TEXCOORD_0` + 真实 PBR 因子 + `images`/`samplers`/`textures`
   + `KHR_texture_transform`。

非目标：视口/引擎里的贴图采样（本轮只把数据与导出契约做对）、法线贴图与 PBR 通道打包
（ORM）、真实图像存储与上传（贴图以相对路径引用，随 glTF 一起分发）。

## 2. 表面参数

```json
{
  "id": "material-000001",
  "name": "Brushed steel",
  "baseColor": [0.6, 0.6, 0.62, 1.0],
  "opacity": 1.0,
  "surface": {
    "metallic": 1.0,
    "roughness": 0.25,
    "emissive": [0, 0.4, 0.8],
    "uvScale": [2, 3],
    "baseColorTexture": "textures/steel.png"
  }
}
```

| 字段 | 缺省 | 语义 |
|---|---|---|
| `metallic` | 0 | 0 = 电介质，1 = 金属 |
| `roughness` | 0.9 | 0 = 镜面，1 = 全漫反射 |
| `emissive` | 无 | 线性 RGB 自发光 |
| `uvScale` | `[1, 1]` | 贴图平铺倍率 |
| `baseColorTexture` | 无 | 相对 glTF 的图片路径 |

**`surface` 整体可选**，且 `skip_serializing_if = "Option::is_none"`：从没碰过 PBR 的
项目序列化后不多一个字节，历史文档 byte-identical。缺省语义集中在
`MaterialSurface::dielectric()`，导出侧调用它，所以"没有 surface"和"surface 全缺省"
在 glTF 里完全等价。

字段对齐 glTF 的 metallic-roughness 模型，不做 Studio 自创命名，导出时不需要翻译表。

## 3. UV 生成

`geometry::uv::box_projected_uvs(mesh)` 按**顶点法线的主导轴**做盒投影：

| 主导轴 | UV | 说明 |
|---|---|---|
| ±X | `(∓z, y)` | 负向翻转 U，避免从外侧看镜面 |
| ±Y | `(x, ±z)` | |
| ±Z | `(x, y)` | |
| 法线退化 | `(x, y)` | 全零法线按 XY 平面兜底 |

三个设计取舍：

- **不改 `Vertex`**。给顶点加 `uv` 字段会牵动 27 处构造、GPU 内核与所有导入器，而
  盒投影对任意网格都成立，包括布尔结果与 STEP/IFC 导入的模型。对参数化图元来说，
  builder 输出的每个面本来就有独立顶点与面法线，主导轴稳定，面内无缝。
- **单位是米**。一个 UV 单位 = 一米表面，所以不同尺寸的零件纹素密度一致，"贴图被拉伸"
  只可能来自材质平铺设置，不会来自几何。
- **平铺属于材质**。`uvScale` 通过 glTF 的 `KHR_texture_transform.scale` 作用在贴图
  引用上，几何与材质因此解耦：同一份网格换材质不需要重新导几何。

## 4. glTF 导出

- 每个 primitive 增加 `TEXCOORD_0`（`VEC2`/`5126`），访问器数量与顶点数一致；
- `pbrMetallicRoughness` 写入真实 `metallicFactor` / `roughnessFactor`；
- `emissiveFactor` 仅在设置了 `emissive` 时出现；
- 有 `baseColorTexture` 时生成 `images`（按路径去重）、`samplers`（REPEAT 环绕）、
  `textures`（sampler 0 + source i），并在根上声明 `extensionsUsed: ["KHR_texture_transform"]`；
- 贴图路径必须在 `uvScale` 之外保持相对：校验拒绝绝对路径、`..` 与 `://`，否则资产
  换机器即碎。

## 5. 变更入口

- `MaterialSurface` 既是文档字段也是 `material.upsert` 的字段，可直接建 PBR 材质；
- `material.patch` 新增 `surface`（整体替换）与 `clearSurface`（回到电介质缺省），
  两者同时出现按既有风格报 `invalid_patch`；
- 表面参数校验并入文档校验：metallic/roughness ∈ [0,1]、emissive 非负有限、
  uvScale 正有限、贴图路径相对。

## 5.1 资产链路（同一轮补齐）

- Prepared GLB profile 放开 `textures`/`images`/`samplers`，但只接受**相对路径**的外部
  图片；`bufferView` 内嵌图片、绝对路径、`..`、`://` 一律拒绝（profile 只有一个 buffer，
  内嵌图片渲染器无法寻址）。
- `CanonicalMaterial.texture` → canonical writer 写 `images`/`samplers`/`textures` 与
  `baseColorTexture`（`KHR_texture_transform.scale` 携带 `uvScale`）。
- 分区打包把 `images`/`samplers`/`textures`/`extensionsUsed` 原样复制到每个 tile：
  贴图表是全局且索引稳定的，复制不会让 `baseColorTexture.index` 失配。
- 引擎读取器把 `baseColorTexture → textures[].source → images[].uri` 解析成材质的
  `map`，`KHR_texture_transform.scale` 作为平铺倍率带出。

## 6. 代码落点

| 文件 | 变更 |
|---|---|
| `crates/studio-core/src/geometry/uv.rs` | `box_projected_uvs` + 单测 |
| `crates/studio-core/src/document.rs` | `MaterialSurface` + `Material.surface` |
| `crates/studio-core/src/operation.rs` | `material.upsert` 增加 `surface` |
| `crates/studio-core/src/patch.rs` | `surface` / `clearSurface` |
| `crates/studio-core/src/apply.rs` | 落库与冲突校验 |
| `crates/studio-core/src/validation.rs` | 表面参数与贴图路径校验 |
| `crates/studio-core/src/export/glb.rs` | `TEXCOORD_0`、PBR 因子、贴图数组 |
| `crates/studio-core/tests/material_surface.rs` | 导出与补丁契约 |

## 7. 验收

- `metallic=1`、`roughness=0.25` 原样出现在 `pbrMetallicRoughness`；
- 无 `surface` 的材质导出 `metallicFactor=0`、`roughnessFactor=0.9`、无 `emissiveFactor`、
  无 `textures` 数组；
- 带贴图时 `baseColorTexture.extensions.KHR_texture_transform.scale == uvScale`，
  `images[0].uri` 等于路径，`samplers[0].wrapS == 10497`；
- 每个 primitive 都有 `TEXCOORD_0`，访问器 `count` 与 `POSITION` 相同；
- `metallic=1.5`、贴图绝对路径、`uvScale=0` 三种情况校验报错，正常参数校验通过；
- 补丁能设置与清除 `surface`，`surface + clearSurface` 同时出现被拒绝且文档不变；
- 无 `surface` 的材质序列化后不含 `surface` 键，反序列化仍是 `None`；
- `MaterialSurface` 对 `{}` 与部分字段都能补全缺省；
- schema `--check` 通过；方法清单不变（41）。
