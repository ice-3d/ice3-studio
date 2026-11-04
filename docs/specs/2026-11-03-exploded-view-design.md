# 爆炸视图（Exploded View）设计

> Document ID: ICE3-STUDIO-SPEC-028
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

## 1. 背景与目标

装配体内部件互相遮挡，剖切只能看一个方向。爆炸视图把各部件沿远离场景中心的方向
推开，露出装配关系。纯 Viewer 实现，不改引擎。

非目标：按设备分组爆炸、动画过渡、爆炸标注/引线。

## 2. 语义

- 目标节点：场景的直接子节点；若场景只有一个根，则用该根的直接子节点（避免整棵
  场景一起平移而没有分离效果）。
- 方向：节点世界位置减去场景包围盒中心后归一化；节点正好在中心时退化为按黄金角
  分布的固定方向，保证仍能分开。
- 位移：`factor × 场景对角线 × 0.6`，`factor ∈ [0,1]`。
- 每次应用都从**原始局部矩阵**重算，不做累积；`factor = 0` 时写回原始矩阵即还原。
- 世界位置在首次捕获时记录（此时尚未移动），后续拖动滑块不受已偏移影响。

## 3. 实现

| 文件 | 变更 |
|---|---|
| `apps/viewer/src/viewport/SceneAdapter.ts` | `setExplodeFactor`、`bindScene`、`applyExplode`、`explodeTargets`、`localMatrixOf`；解析 `?explode=` |
| `apps/viewer/src/viewport/ExplodeTool.ts` | 右上角 **Explode** 开关 + 强度滑块（默认 0.35） |
| `apps/viewer/src/viewport/Viewport.tsx` | 挂载/释放工具 |
| `apps/viewer/src/app/tokens.css` | 工具条样式（位于 Section 下方） |

局部矩阵取 `node.toJSON().matrix`；无显式矩阵时用 `eulerTransformMatrix` 从
position/rotation/scale 合成，再在列主序的 `[12..14]` 上加平移分量。

## 4. 验收

- typecheck、单测（38 项）、构建通过。
- e2e `explode.spec.ts`：同机位截取 canvas，点击 **Explode** 后截图字节不同，
  证明节点确实被推开；全套 e2e 10/10 通过。
- `npm run verify` 31 项门禁通过。
