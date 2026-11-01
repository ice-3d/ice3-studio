# 污水模块 AI 验收夹具

> Document ID: ICE3-STUDIO-GUIDE-001
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

本夹具是**契约测试用途**，不是默认演示场景。默认开发场景是 `projects/` 中修改时间
最新的工程（见 [Getting Started](getting-started.md) 的 `npm run dev`）。`wastewater-mini` 只有
11 个节点，价值在于启动快、结果确定，被 `npm run verify`、Viewer E2E 和 MVP benchmark
用作固定输入；需要单独看它时用 `npm run dev:mini`。

`fixtures/wastewater-mini/ai-plan.json` 是确定性 AI 客户端提交的建模计划。它不包含
模型凭据，也不依赖 UI 操作。

计划依次完成：

1. 创建 6 个 Material 和地坪，形成 revision 1。
2. 用 `dryRun` 提交一条指向不存在设备的连接，预期收到结构化错误且 revision 不变。
3. 修正连接目标，创建两座曝气池、两座二沉池、两台泵、四条管线及四条 process
   relation，形成 revision 2。
4. 导出 source GLB，执行 undo/redo，并验证节点与关系身份保持稳定。

验证：

```bash
cargo test -p studio-server --test ai_fixture
node scripts/run-ai-fixture.mjs --fixture fixtures/wastewater-mini
```
