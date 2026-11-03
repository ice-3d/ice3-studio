# 草图拉伸（Extrude）设计

> Document ID: ICE3-STUDIO-SPEC-024
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

## 1. 背景与目标

Studio 的图元只有 Box/Plane/Cylinder/Sphere/CustomMesh/Sweep，缺少"由 2D 轮廓生成
棱柱"的能力。工业场景里的墙板、风管、槽体、支架截面都依赖这一操作。本设计新增
`PartContent::Extrude`，让 AI 可以用一组二维点描述截面、再给定高度生成实体。

非目标：带孔轮廓（内环）、旋转体（revolve）、放样（loft）、一般布尔。

## 2. 数据模型

`PartContent` 新增：

```json
{
  "kind": "extrude",
  "profile": [[0, 0], [1, 0], [1, 1], [0, 1]],
  "height": 2.0
}
```

- `profile`：闭合多边形，单位与场景一致（米）；首尾点自动闭合，连续重复点会被丢弃。
- `height`：`ScalarBinding`，可为字面量、参数或表达式；必须为正。

`ComponentPartKind` 新增 `Extrude`，供 Agent 摘要识别。

## 3. 求值

`resolve_part_content` 将 `Extrude` 直接求值为 `NodeContent::CustomMesh`，不新增
渲染图元：

1. 校验点数 ≥ 3、坐标有限、面积非零、height > 0。
2. 按有符号面积判定朝向，顺时针时反转，统一为逆时针。
3. 耳切法（ear clipping）三角化顶面；底面复用索引并反向。
4. 每条边生成两个侧壁三角形，法线朝外。
5. 每个三角形使用独立顶点（非索引共享），`validate_mesh` 因此得到平面法线，
   棱边清晰。

自相交轮廓在裁剪停滞时退化为扇形三角化（尽力而为，不报错）。

## 4. 校验

`validate.rs` 对 `Extrude` 检查：`height` 绑定已声明、`profile` 至少三个有限点。
违反时产生 `ICE3_STUDIO_COMPONENT_INVALID` 诊断。

## 5. 代码落点

| 文件 | 变更 |
|---|---|
| `crates/studio-core/src/geometry/extrude.rs` | 新增：`extrude_profile`、耳切三角化 |
| `crates/studio-core/src/geometry/mod.rs` | `mod extrude`、导出、`GeometryError::InvalidProfile` |
| `crates/studio-core/src/large_scene/component.rs` | `PartContent::Extrude` |
| `crates/studio-core/src/large_scene/evaluate.rs` | `resolve_part_content` 分支 |
| `crates/studio-core/src/large_scene/validate.rs` | 校验分支 |
| `crates/studio-core/src/agent_contract.rs` | `ComponentPartKind::Extrude` |
| `crates/studio-server/src/agent.rs` | `part_kind` 映射 |
| `crates/studio-core/tests/extrude.rs` | 几何测试（体积/三角形/退化输入） |
| `crates/studio-core/tests/large_scene_evaluation.rs` | 端到端求值测试 |

## 6. 验收

- 几何：1×1 方形 + height 2 → 12 三角形、体积 2；L 形（面积 3）→ 20 三角形、体积 3；
  顺时针输入被校正；闭合重复点被容忍；点数不足/共线/零高度/NaN 全部拒绝。
- 端到端：组件中的 Extrude 部件求值为 12 三角形棱柱，两个实例共享一个网格、
  有效三角形 24。
- `cargo test -p studio-core`、`cargo test -p studio-server`、schema `--check` 通过。
