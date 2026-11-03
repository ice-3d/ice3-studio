# Agent 视觉自检截图通道设计

> Document ID: ICE3-STUDIO-SPEC-022
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

## 1. Summary

Agent 读回路（context → search → component.get → validation.run）覆盖了语义与空间信息，
但 Agent 看不到渲染结果：视觉质量判断完全依赖人工。本设计补上 Agent 的视觉自检通道
——`scripts/agent-screenshot.mjs`，一个通用场景截图工具。

形态是脚本而不是 RPC：Studio Server 不渲染几何（Core 不依赖 Engine/Viewer 的红线），
截图由工具层驱动用户已安装的 Google Chrome 加载 Web Viewer 完成，与 E2E 使用同一
Playwright + Chrome 通道，不引入新的浏览器二进制。

## 2. Goals

- 一条命令把运行中的 Studio 场景渲染为 PNG：默认机位或
  `?view=方位角,仰角,距离&target=x,y,z` 任意机位（复用 Viewer 既有的
  `applyViewOverride` URL 参数）。
- Agent 可编程调用（CLI 参数 + 退出码），输出路径可预测，失败可诊断（console error
  转发）。
- 可选自动拉起临时 server（`--project`），退出时清理，不碰已持锁的常驻服务。
- 对大 reference 资产等待 `data-reference-state="ready"`，避免截到空白画布。

## 3. Non-Goals

- 不新增 JSON-RPC 方法；Server 与 Core 不出现渲染代码。
- 不做 candidate（未提交 ChangeSet）截图；先覆盖 committed revision。
- 不做图像 diff / 相似度判定；Agent 拿到 PNG 自行判断。
- 不替代 `apps/viewer/shot.mjs`（保留其调试用途），但新脚本遵循仓库 Chrome 规则。

## 4. Interface

```text
node scripts/agent-screenshot.mjs [options]

--url <url>            完整 Viewer URL（含 token/view/target），与 --base-url 组互斥
--base-url <url>       Studio server 地址，默认 http://127.0.0.1:8120
--token <token>        写入 URL 的会话 token（只读预览可省略）
--view az,elev,dist    相机方位角/仰角/距离（度，米）
--target x,y,z         相机注视点（米）
--out <path>           输出 PNG，默认 output/agent-screenshot.png
--width / --height     视口，默认 1600x900
--wait <ms>            渲染 settle 时长，默认 3000
--project <id>         自动 spawn 临时 server（workspace 为 projects/，随机端口）
--cli <path>           studio-cli 路径，默认 target/debug/studio-cli
--channel <name>       Playwright 浏览器 channel，默认 chrome（遵循仓库 Chrome 规则）
```

URL 构造逻辑抽为纯函数 `buildScreenshotTarget` 供测试：
view/target 校验（分量数与数值）→ 拼接 query 参数。

## 5. Behavior

1. `--project` 给定时 spawn `studio-cli serve --workspace projects --project <id>
   --port 0`，从 stdout 读取实际地址与 token；进程退出时终止 server。
2. 打开页面后等待 `canvas[data-layer="scene"]` 可见；viewport 存在
   `data-reference-state` 时等待 `ready`（10 s 上限），再 settle `--wait`。
3. console error 原样转发到 stderr 便于诊断；截图写入 `--out`，打印绝对路径。
4. 失败（健康检查不过、超时、浏览器崩溃）以非零退出码结束并输出原因。

## 6. Testing

- `scripts/tests/agent-screenshot.test.mjs`：URL 构造（view/target 合法与非法输入、
  与 --url 的互斥、token 注入）纯函数契约，进入 `npm run verify` 门禁。
- 实拍验收：对默认污水厂工程截取总览与近景各一张，人工确认非空白。

## 7. Documentation

- `docs/guides/getting-started.md` 增加 Agent 视觉自检一节；
- `docs/reference/capabilities.md` 工具链一节登记脚本入口；
- 本设计记录形态决策（脚本而非 RPC）与 Chrome 规则依据。

## 8. 关联文档

- `ICE3-STUDIO-SPEC-020` — [Agent 组件定义内省](2026-11-03-agent-component-introspection-design.md)
- `ICE3-STUDIO-SPEC-021` — [场景空间重叠审计](2026-11-03-scene-overlap-audit-design.md)
