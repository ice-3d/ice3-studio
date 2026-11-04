# 圆角与倒角（Fillet / Chamfer）设计

> Document ID: ICE3-STUDIO-SPEC-031
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

## 1. 背景与目标

到 SPEC-030 为止，Studio 的实体只有硬边：`Box` 的 8 个角、12 条棱都是尖的。真实设备
（机壳、水箱、混凝土构件）几乎都有圆角或倒角，尖边模型在视觉上像草模，在干涉检查里
也会给出过于保守的包围盒。

本设计给部件模型补一个 `RoundedBox` 内容类型：一次描述"边长 + 圆角半径"，用同一个
参数同时拿到倒角与真圆角。

非目标：任意 B-rep 圆角（对任意布尔结果的棱边做滚球）；`Cylinder` / `Extrude` 的顶部
倒角；圆角后自动重建 CSG 配方。

## 2. 参数

```json
{
  "kind": "rounded_box",
  "width": 2.0,
  "height": 2.0,
  "depth": 2.0,
  "radius": 0.25,
  "segments": 4
}
```

| 字段 | 含义 |
|---|---|
| `width` / `height` / `depth` | 原始盒的三边（圆角内切于这个盒子） |
| `radius` | 棱边圆角半径，0 < `radius` < `min(三边)/2` |
| `segments` | 圆弧分段，1..=32，缺省 4 |

`width` 等三个量与 `radius` 都是 `ScalarBinding`，可以直接绑定组件参数，所以同一份配方
在不同实例上可以是不同圆角半径。

**`segments = 1` 就是倒角。** 一段弦把圆角塌成 45° 斜面，等价于宽度为 `radius` 的
倒角（chamfer）；`segments ≥ 2` 才是分段的真圆角。这样不需要两个类型，也不会出现
"倒角宽"和"圆角半径"两套互相换算的参数。

## 3. 几何

圆角盒是 Minkowski 和 `内盒 ⊕ 半径 r 的球`，内盒为 `(w-2r, h-2r, d-2r)`。表面由三类
片构成：

| 片 | 数量 | 构造 |
|---|---|---|
| 平面 | 6 | 内盒的六个面，各占 `(w-2r) × (h-2r)` 的内缩矩形 |
| 边缘条带 | 12 | 四分之一圆柱，轴沿棱方向走 `±(内盒对应半边长)` |
| 角patch | 8 | 球面八分之一，扇心在极点，向外 `segments` 圈环 |

相邻片在边界上取同一组公式（平面边界 `H`、边缘条带端点 `I + r`、角patch 极点边界
`I + r`），因此三个方向的点数值相同，网格是闭合的。

每片独立发顶点，所以面与圆角之间保留硬折边（视觉上像真机加工件），片内部保持光滑。

### 3.1 绕序自校正

手推每片的三角形方向容易出错（SPEC-025 就因为圆柱侧面和球体绕序朝内导致体积为负）。
这里不做手推，而是逐片计算它对体积的贡献

```
V_patch = Σ det(a, b, c) / 6
```

凸体上外法向的片对总体积贡献为正，贡献为负就把该片所有三角形反转。判据与参考点无关，
只依赖原点在体内，不需要为每个片猜"朝外"是哪个方向。

### 3.2 体积验收

对 `a = w-2r` 的立方体，解析体积为

```
V = a³ + 6a²r + 3πar² + 4/3·πr³
```

`segments = 16` 时网格体积与解析值的相对误差 < 1%（实测 0.3%），这条断言直接钉住
"没有把材料切多或切少"，也间接验证了绕序。

## 4. 校验

`validate_component_parts` 增加分支：

- 四个标量绑定必须引用已声明参数；
- `segments` 必须在 `1..=MAX_ROUNDED_SEGMENTS`（32）内。

半径超出 `min(三边)/2` 时，求值阶段 `rounded_box` 返回 `InvalidDimensions`，与其它
图元的非法尺寸走同一条错误路径。

## 5. 代码落点

| 文件 | 变更 |
|---|---|
| `crates/studio-core/src/geometry/rounded.rs` | `rounded_box` + 分片构造与绕序校正 + 单测 |
| `crates/studio-core/src/geometry/mod.rs` | 导出 `rounded_box` / `MAX_ROUNDED_SEGMENTS` |
| `crates/studio-core/src/large_scene/component.rs` | `PartContent::RoundedBox` |
| `crates/studio-core/src/large_scene/evaluate.rs` | 求值到 `NodeContent::CustomMesh` |
| `crates/studio-core/src/large_scene/validate.rs` | 绑定与分段校验 |
| `crates/studio-core/src/agent_contract.rs` | `ComponentPartKind::RoundedBox` |
| `crates/studio-server/src/agent.rs` | 部件类型映射 |
| `crates/studio-core/tests/rounded_box.rs` | 体积收敛、倒角更省三角形、非法半径、参数绑定 |

## 6. 验收

- `2×2×2`、`r=0.25`、`segments=8`：体积与解析值误差 < 2%，且小于 `8`；
- `segments=1`：三角形更少、体积更小（倒角切掉的材料多于圆角），且大于内盒体积；
- `r = 半边`：求值报错；
- 半径绑定到组件参数 `fillet=0.3`：体积与解析值一致，误差 < 3%；
- 全部几何落在原盒范围内；
- schema `--check` 通过；方法清单不变（41），类型新增 `RoundedBox` 变体。
