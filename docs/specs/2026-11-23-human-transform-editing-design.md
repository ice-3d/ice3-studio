# 人工 Transform 与基础编辑闭环设计

> Document ID: ICE3-STUDIO-SPEC-038
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

## 1. 摘要

Studio 的 Document、Operation、ChangeSet 和候选预览已经覆盖通用建模所需的数据能力，
但 Web Viewer 的 Inspect 目前只允许修改名称、显隐和组件实例参数。视口仍是只读的，
用户无法直接移动对象、修改图元尺寸、分配材质或撤销未提交的编辑。

本设计为现有对象补上第一版人工编辑闭环：

```text
Selection
  → Inspector / viewport gizmo
  → local EditorDraft
  → local preview
  → Stage draft
  → typed Operations
  → ChangeSet / candidate preview
  → human review
  → commit
```

成功标准是：用户不需要 Agent 或 CLI，就能修改已有对象的 Transform、图元尺寸和材质，
在视口中得到即时反馈，撤销/重做本次编辑，随后通过既有 ChangeSet 审阅和提交。

## 2. 范围

### 2.1 本阶段

- 编辑已有节点的 translation、rotation 和 scale；
- 在视口中提供移动、旋转和缩放手柄；
- 数值 Transform 编辑与局部/世界坐标切换；
- Box、Plane、Cylinder、Sphere 图元尺寸编辑；
- 组件实例参数编辑纳入同一草稿；
- 给节点分配工程内已有材质；
- 本地草稿撤销和重做；
- Stage 最小 Operation 集、候选预览、审阅和提交；
- 草稿过期、对象删除、Stage 失败和预览失败处理；
- Chrome E2E、Viewer 单测、typecheck、build 和根级验证。

### 2.2 非目标

- 创建、删除、复制或重新挂载对象；
- 多对象同时编辑或多选；
- 自定义枢轴；
- 需要非均匀 shear 的世界轴向缩放；Transform 仍只保存 TRS；
- 网格顶点编辑、雕刻和自由曲面；
- 约束求解、网格吸附或坐标网格；
- 跨刷新持久化本地草稿；
- 材质定义、贴图或 PBR 参数编辑；本阶段只分配已有材质；
- 修改 ICE3 Engine 的渲染、拾取或公开 API；
- 绕过 ChangeSet 直接写 revision。

## 3. 设计原则

1. **Document 仍是真源。** 草稿只是未提交的编辑状态，不能进入历史或导出。
2. **拖动不触发写 RPC。** 手势期间只更新渲染节点和本地草稿。
3. **最小操作集。** Stage 只提交真正变化的字段，不为未变化字段生成 Operation。
4. **ChangeSet 保持唯一写入口。** 所有持久化修改继续经过现有候选、digest 和人工审批。
5. **Viewer 不实现几何求值。** Transform 可本地预览；图元尺寸和材质最终值以候选预览为准。
6. **失败保留工作。** 网络、校验或预览失败不能悄悄丢失草稿。
7. **颜色不是唯一信号。** 手柄同时提供轴字母、数值和键盘入口。

## 4. 用户交互

### 4.1 编辑入口

编辑只在 `Inspect` tab 中启用。Agent tab 继续以只读上下文为主，Changes tab 继续负责
ChangeSet 审阅。

选中对象后，Inspect 显示以下分组：

| 分组 | 内容 |
|---|---|
| Transform | position、XYZ Euler rotation、scale、local/world 切换 |
| Geometry | 当节点内容是 primitive 时显示图元尺寸 |
| Parameters | 组件实例已有参数 |
| Material | 当前材质和工程内可选材质 |
| Draft | Stage draft、Discard draft、Undo、Redo、dirty/stale 状态 |

名称和显隐继续保留在 Inspect 中，并进入同一草稿。

### 4.2 视口手柄

手柄只在 Inspect tab 且有对象 Selection 时显示。手柄使用一个位于视口之上的 DOM/SVG
叠加层，不进入 ICE3 scene，也不修改 Engine overlay。

| 模式 | 交互 |
|---|---|
| Move | X/Y/Z 轴拖动；中心手柄按屏幕平面移动 |
| Rotate | X/Y/Z 圆环拖动；显示当前角度 |
| Scale | X/Y/Z 轴缩放；中心手柄等比缩放 |

- 枢轴为选中节点的世界原点。
- 默认使用局部坐标；可切换世界坐标。
- Move 与 Rotate 支持 local/world；Scale 始终沿节点局部轴，因为单节点 TRS 无法表示
  world-axis non-uniform shear。
- X/Y/Z 使用红/绿/蓝，同时显示字母标签。
- 投影过短或几乎垂直于屏幕的轴变暗并禁用。
- 按下手柄时不开始相机旋转；释放后恢复轨道控制。
- `Esc` 取消当前拖动并恢复拖动开始前状态。
- Shift 吸附：移动 0.1 m、旋转 15°、缩放 0.1。
- 每次手势从开始状态计算，不做增量累积。

### 4.3 数值编辑

- Position 和 Scale 显示三位有效小数，去掉浮点噪声。
- Rotation 对用户显示 XYZ Euler 角度，内部存储仍为四元数。
- 输入在 Enter 或失焦时提交；一次提交形成一个撤销命令。
- 非有限值、零或负 Scale 不写入草稿，并在字段旁显示错误。
- Undo/Redo 快捷键为 `Ctrl/Cmd+Z` 和 `Ctrl/Cmd+Shift+Z`。

## 5. EditorDraft

新增 `EditorDraftStore`，按 node ID 保存尚未 Stage 的对象草稿。允许用户在对象之间
切换而不丢失工作，但首版 UI 一次只编辑一个对象。

```text
NodeDraft {
  nodeId
  baseRevision
  baseNode
  values
  history
  future
  status: clean | dirty | staged | stale | failed
  error
}
```

`values` 包含：

```text
Transform + name + visible + primitive + component parameters + materialId
```

草稿创建规则：

- 首次编辑对象时，从当前 committed node 复制基线。
- 草稿不会从 candidate Document 创建，避免把未审批值当作新基线。
- 草稿命令栈最多保留 100 条；超出时丢弃最旧命令。
- Stage 成功后把对应草稿标记为 `staged` 并保留值，避免竞态预览期间表单回退到旧值。
- commit 成功后清除对应草稿。
- Stage 失败或候选预览失败时保留 dirty/failed 草稿。
- ChangeSet 被丢弃后把 staged 草稿恢复为 dirty，允许修正后重新 Stage。

## 6. Scene Preview

`SceneAdapter` 增加两项能力：

- 根据 node ID 和草稿 Transform 更新对应 rendered entity 的 local matrix；
- 清除本地 Transform override，恢复到当前 preview 或 candidate 值。

Transform override 使用现有 Selection-to-renderable 映射，保证组件实例、派生部件和
大资产 binding 的完整实体一起移动。

本地 override 只影响当前浏览器场景：

- 不调用后端；
- 不改 Document；
- 不写项目文件；
- 不改变 ChangeSet；
- 候选预览发布或 changeSet 状态变化时重新应用或清除 override。

图元尺寸、组件参数和材质分配不在 Viewer 中重建几何；这些值在 Stage 后由正式候选
预览展示。

## 7. Operation 编译

`Stage draft` 以 committed baseline 和草稿值生成最小 Operation 集。

### 7.1 Transform、名称、显隐

若任一字段变化，生成一条：

```json
{
  "type": "node.update",
  "nodeId": "node-000001",
  "name": null,
  "transform": { "translation": {}, "rotation": [], "scale": {} },
  "visible": null,
  "metadata": null
}
```

未变化字段为 null，已有编译器行为保持不变。

### 7.2 Primitive

当节点 content 是 `type: primitive` 且尺寸或分段变化时，生成
`primitive.update`。支持：

- `kind: box`：width、height、depth；
- `kind: plane`：width、depth；
- `kind: cylinder`：radius、height、segments；
- `kind: sphere`：radius、segments、rings。

非法尺寸和分段由现有 Core 校验失败关闭；Viewer 在 Stage 前提供同语义的基础输入检查。

### 7.3 Component Instance

组件参数继续使用现有 `instance.update` 编译器，并与 Transform 更新在同一个 Stage 批次
中提交。

### 7.4 Material

材质分配变化生成：

```json
{
  "type": "material.assign",
  "nodeId": "node-000001",
  "materialId": "material-000012"
}
```

只允许选择 `document.materials` 中现有材质或清空为 null。

### 7.5 批处理

Stage 按固定顺序生成 Operation：

1. `node.update`
2. `primitive.update`
3. `instance.update`
4. `material.assign`

只有存在变化的 Operation 进入 ChangeSet。操作通过现有 `ChangeSetStore.addOperations`
提交，source 为 `human / Inspector`。

## 8. 状态与错误

### 8.1 Stale

以下任一条件把草稿标记为 stale：

- Session revision 与 `baseRevision` 不同；
- 对应 node 已不存在；
- 对象内容类型在 revision 之间变化，无法安全合并；
- 活动 ChangeSet 已 stale 或 discarded。

stale 草稿禁止 Stage。用户可以选择：

- 丢弃草稿；
- 在显式确认后，从最新 committed node 建立新草稿。

首版不自动 rebase。

### 8.2 页面离开

存在 dirty 草稿时注册 `beforeunload`。浏览器关闭或刷新前显示标准离开确认。草稿不写磁盘。

### 8.3 Stage 与预览

- Stage 中显示 staging/evaluating 状态。
- ChangeSet 求值失败时保留草稿并展示诊断。
- 候选预览失败时保留草稿，仍允许审阅 ChangeSet 和诊断。
- Stage 成功后保留 staged 草稿，并继续用草稿值显示表单。
- commit 成功后清除已提交草稿。
- discard ChangeSet 后把草稿恢复为 dirty，用户可重新 Stage 或放弃。

## 9. 视觉与可访问性

- 手柄使用现有 dark workbench token，不另建主题。
- 手柄轴色对深色场景保持至少 3:1 图形对比，并提供字母标签。
- 所有按钮和数值字段有可访问名称和可见 focus ring。
- 手柄不可用时，数值输入仍是完整编辑路径。
- `Esc`、Undo/Redo 和 Stage 操作有键盘测试。
- 768px 以上保持三栏工作台；手柄不与 Measure、Section、Explode 工具重叠。

## 10. 预期代码落点

| 文件 | 变更 |
|---|---|
| `apps/viewer/src/editor/EditorDraftStore.ts` | 草稿、命令栈、stale 和 Stage 生命周期 |
| `apps/viewer/src/editor/EditorDraftProvider.tsx` | React external-store provider 与 hooks |
| `apps/viewer/src/editor/transformMath.ts` | Euler/四元数、Transform 比较和吸附纯函数 |
| `apps/viewer/src/editor/draftOperations.ts` | 最小 Operation 集编译 |
| `apps/viewer/src/editor/TransformEditor.tsx` | Transform、Geometry、Material 和 Draft 控件 |
| `apps/viewer/src/forms/FormRenderer.tsx` | 从直接 Stage 改为共享草稿编辑器 |
| `apps/viewer/src/viewport/TransformGizmo.ts` | SVG 手柄、拖动和键盘取消 |
| `apps/viewer/src/viewport/SceneAdapter.ts` | 本地 Transform override |
| `apps/viewer/src/viewport/Viewport.tsx` | 挂载/释放 gizmo，接收编辑状态 |
| `apps/viewer/src/app/Workbench.tsx` | 连接 Selection、草稿、preview 和 Stage |
| `apps/viewer/src/main.tsx` | 注入 EditorDraftStore |
| `apps/viewer/src/app/studioTokens.css` | 编辑器分组和手柄样式 |
| `apps/viewer/tests/*` | transform math、draft、operation 编译与 store 测试 |
| `apps/viewer/e2e/transform-editing.spec.ts` | Chrome 编辑闭环验收 |

## 11. 验收

### 11.1 功能

- 选择已有图元后可以用数值和手柄修改 Transform；
- 修改在视口中即时可见，拖动期间无写 RPC；
- 可以撤销/重做草稿手势；
- 可以修改图元尺寸和组件参数；
- 可以分配现有材质或清空材质；
- Stage 后产生最小 Operation 集并显示候选预览；
- 人工审阅后 commit 得到新 revision；
- 提交前没有新 revision；
- stale、失败和页面离开路径保留或明确处理草稿。

### 11.2 自动化

```bash
npm --prefix apps/viewer run typecheck
npm --prefix apps/viewer test
npm --prefix apps/viewer run build
npm --prefix apps/viewer run test:e2e
npm run docs:check
npm run verify
```

Chrome E2E 使用用户安装的 Google Chrome，并覆盖：

- 数值移动；
- 手柄拖动；
- 图元尺寸修改；
- 材质分配；
- Undo/Redo；
- Stage、candidate review 与 commit；
- 拖动期间零写请求；
- stale draft 禁止 Stage。

### 11.3 视觉

在 1440px、1024px、768px 使用 Chrome 检查：

- 手柄不与现有工具重叠；
- 选中状态、悬停状态和拖动状态清晰；
- 数值错误有文本诊断；
- 键盘焦点可见；
- 没有横向溢出或不可恢复的布局遮挡。

## 12. 关联文档

- `ICE3-STUDIO-POL-001` — [仓库规则](../../AGENTS.md)
- `ICE3-STUDIO-ARC-001` — [Studio 架构](../architecture.md)
- `ICE3-STUDIO-REF-003` — [ChangeSet 审阅](../reference/change-sets.md)
- `ICE3-STUDIO-SPEC-002` — [Studio Web Workbench](2026-10-30-studio-web-workbench-design.md)
- `ICE3-STUDIO-SPEC-014` — [Agent 主导工作台](2026-11-01-agent-led-workbench-design.md)
