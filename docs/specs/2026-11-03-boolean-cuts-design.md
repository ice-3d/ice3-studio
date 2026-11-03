# 布尔挖除（Boolean Cuts）设计

> Document ID: ICE3-STUDIO-SPEC-025
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

## 1. 背景与目标

`ComponentPart.subtract` 之前只支持轴对齐 `BoxCut`，且仅在部件内容是 `Box` 时才生效。
门窗洞、穿墙套管、设备开孔这些工业常见需求里，圆柱孔和球缺同样高频，拉伸板也无法
被挖除。本设计把挖除泛化为 **盒 / 圆柱 / 球**，并对**任意部件几何**（含 `extrude`、
`customMesh`、`sweep`）生效。

非目标：任意网格之间的布尔并/交（只做"减去凸体"）、带内孔的草图轮廓。

## 2. 数据模型

```rust
pub enum PartCut {
    Box(BoxCut),           // 原有结构，JSON 无 kind 字段，保持兼容
    Cylinder(CylinderCut), // radius, height, x, y, z, segments, axis
    Sphere(SphereCut),     // radius, x, y, z, segments, rings
}
```

- `PartCut` 使用 `untagged`，已有的盒体挖除 JSON（只有 `width/height/depth/x/y/z`）
  继续解析为 `Box`；圆柱带 `radius` 与 `height`，球只带 `radius`，靠字段集合区分。
- `CylinderCut.axis` 取 `x`/`y`/`z`，默认 `z`（穿过拉伸板/墙的竖直孔）。
- 所有尺寸都是 `ScalarBinding`，支持字面量、参数和表达式。

## 3. 算法

`geometry/csg.rs` 实现 BSP 布尔减法（csg.js 的经典算法）：

1. 三角形 → 带平面多边形，构建 BSP 树。
2. `a.invert(); a.clip_to(b); b.clip_to(a); b.invert(); b.clip_to(a); b.invert();
   a.build(b.all_polygons()); a.invert()`。
3. 结果多边形扇形三角化回 `TriangleMesh`，被切开的开口自动由裁剪面封口。

保护：底模 > 60,000 三角形或切体 > 20,000 三角形时返回 `CsgTooLarge`；未产生任何
三角形时返回 `CsgFailed`。切体在部件坐标系内构建（盒/圆柱/球 + 中心偏移），圆柱先
由原生的 Y 轴通过真旋转转到请求轴（绕序不变）。

## 4. 顺带修复：图元绕序

排查中发现两个既有缺陷：

- `Primitive::Cylinder` 侧面索引朝内、封口朝外，闭合网格的带符号体积只有真实值的
  1/3（r=0.5、h=4 时为 1.04 而非 3.12）。
- `Primitive::Sphere` 所有三角形朝内，带符号体积为负。

两者都会让 CSG 把切体当成"内外翻转"。已修正为一致朝外（圆柱侧面索引重排，球体整体
反转）。主渲染通道未开启背面剔除，视觉不受影响；阴影图按 `cullFace(FRONT)` 的语义
也因此更正确。

## 5. 求值与缓存

`expand_definition_parts` 中，非空 `subtract` 走
`load_or_evaluate_cut_part_mesh`：

1. 用"配方"（蓝图、部件 id、基础内容、挖除列表、参数）的 SHA-256 作为缓存键，
   与结果网格大小无关；
2. 未命中时求值基础内容 → 逐一切体做 CSG → 以 `CustomMesh` 校验并缓存；
3. 同一部件的多个实例共享同一份结果网格。

## 6. 代码落点

| 文件 | 变更 |
|---|---|
| `crates/studio-core/src/geometry/csg.rs` | 新增 BSP CSG |
| `crates/studio-core/src/geometry/mod.rs` | 导出 `csg_subtract`；新增 `CsgFailed` / `CsgTooLarge` |
| `crates/studio-core/src/geometry/primitives.rs` | 修正圆柱侧面与球体绕序 |
| `crates/studio-core/src/large_scene/component.rs` | `PartCut` / `CylinderCut` / `SphereCut`；`subtract: Vec<PartCut>` |
| `crates/studio-core/src/large_scene/evaluate.rs` | 通用挖除路径 + 切体构建；删除旧的解析式 `Block`/`carve` |
| `crates/studio-core/tests/csg.rs` | 体积与 JSON 兼容测试 |
| `crates/studio-core/tests/large_scene_evaluation.rs` | 圆柱挖除端到端 |

## 7. 验收

- 盒减盒（穿透）：体积 8 − 0.5 = 7.5。
- 盒减圆柱（r=0.5、32 段、穿透）：体积 ≈ 6.44。
- 盒减球（r=0.6、32×16）：体积 ≈ 7.10。
- 不相交切体：体积保持 1.0。
- `PartCut` JSON：无 `kind` 的旧盒体、圆柱、球分别解析为对应分支。
- 端到端：盒体部件挖一个半径 0.2 的圆柱孔，三角形数增加且体积小于实体盒。
- `cargo check --workspace`、`cargo test -p studio-core`、schema `--check` 通过。
