# 污水模块 AI 验收夹具

> Document ID: ICE3-STUDIO-GUIDE-001
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

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
