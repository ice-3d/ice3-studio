# 剖切面（Section Plane）设计

> Document ID: ICE3-STUDIO-SPEC-027
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

## 1. 背景与目标

审查大型设备时，外壳、池体、屋顶会挡住内部结构。需要一条可随时开关的剖切面，
把场景沿水平面切开，看到内部管线与设备。

非目标：任意方向剖切、剖切面封口（切开后是空心）、剖切结果导出。

## 2. 语义

引擎新增裁剪面 `[nx, ny, nz, d]`：片元满足 `dot(worldPosition, n) + d < 0` 时
`discard`。零平面 `[0,0,0,0]` 恒为 0（保留全部），因此同时充当"关闭"。

- 引擎 API：`ICE3.setClipPlane(plane | null)`；`null` 或零平面关闭。
- Viewer 使用水平面 `[0,-1,0,cutY]`：保留 `y <= cutY`，丢弃上方，露出内部。
- `cutY` 由场景世界包围盒的 `minY/maxY` 与 UI 比例（0.05–0.95，默认 0.5）决定。

## 3. 引擎改动

| 文件 | 变更 |
|---|---|
| `packages/renderer/src/types.ts` | `ICERendererBackend.setClipPlane?` |
| `packages/renderer/src/WebGL2Program.ts` | `u_clipPlane` uniform、片元 `discard`、`setClipPlane` |
| `packages/renderer/src/WebGL2RendererBackend.ts` | 保存裁剪面并在每帧 `program.setClipPlane` |
| `packages/runtime/src/ICE3.ts` | `setClipPlane(plane \| null)`，落到 backend 并请求重绘 |

裁剪判断放在 `u_pickMode` 分支之前，因此拾取也尊重剖切（不会选中被裁掉的几何）。

## 4. Viewer 改动

| 文件 | 变更 |
|---|---|
| `apps/viewer/src/viewport/SceneAdapter.ts` | `setSectionRatio(ratio \| null)`；场景加载后应用；解析 `?section=` |
| `apps/viewer/src/viewport/SectionTool.ts` | 右上角 **Section** 开关 + 高度滑块 |
| `apps/viewer/src/viewport/Viewport.tsx` | 挂载/释放工具 |
| `apps/viewer/src/app/studioTokens.css` | 工具条与滑块样式 |

`?section=0.5` 可在加载时直接启用剖切，便于截图与分享。

## 5. 验收

- 引擎构建通过（`u_clipPlane` / `setClipPlane` 进入产物）。
- Viewer typecheck、单测（38 项）、构建通过。
- e2e `section.spec.ts`：同一机位截取 canvas，点击 **Section** 后截图字节不同，
  证明裁剪真实生效；全套 e2e 9/9 通过。
- `npm run verify` 31 项门禁通过。
