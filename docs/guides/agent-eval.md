# Agent 评测基线

> Document ID: ICE3-STUDIO-GUIDE-006
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

## 1. 它测什么，不测什么

`npm run eval` 用 20 条任务驱动公开的 JSON-RPC 契约，每条任务跑 3 次，三次全过
才计为通过（`pass^3`）。

**它测的**：任务是否可解、契约是否自洽、诊断码是否稳定、幂等与权限边界是否守住。
每次任务都起一个全新服务、一份全新的 wastewater fixture，任务之间不共享状态。

**它不测的**：语言模型的能力。当前执行者 `scripts/eval/tasks.mjs` 里的参考实现
是"标准答案"——它按正确顺序调正确的方法。因此 20/20 只证明任务可解、grader 接线正确，
不构成模型评测结论。

接入模型时，实现同一个 client 表面（`call(method, params)`）替换参考实现即可，
grader 不动。

## 2. 运行

```bash
npm run eval                 # 3 次，写 target/eval-report.json
node scripts/eval/run.mjs --attempts 5
node scripts/eval/run.mjs --json /tmp/report.json
```

`npm run verify` 已经包含这一道门禁，所以契约回归会直接被拦住。

## 3. 接入模型

```bash
OPENAI_API_KEY=... node scripts/eval/run.mjs --client scripts/eval/clients/openai.mjs
```

可选变量：`OPENAI_BASE_URL`（默认官方端点，任意兼容实现均可）、`EVAL_MODEL`（默认
`gpt-4o-mini`）、`EVAL_MAX_TURNS`（默认 8）。

外部 solver 拿到的是 `goal` 和 `call(method, params)`，自己决定调什么。它与 grader 的
唯一约定是 `finalMethod`：runner 记录该方法的最后一次返回，再交给 `check` 断言。

没有 `finalMethod` 的任务是比较多次响应的复合断言（分页是否重不漏、重试是否重复追加、
越权是否被拒），外部 solver 无法把它们交回来，因此会被跳过并在报告的 `skipped` 里列出。
参考实现跑满 20 条，外部 solver 跑 15 条。

## 4. 覆盖范围

| 类别 | 任务 |
|---|---|
| 读取 | `context-summary`、`search-by-name`、`search-pagination`、`search-projection`、`search-kind-filter`、`measure-distance`、`bom-grouping`、`clash-audit`、`evaluation-query` |
| 提案 | `changeset-create`、`changeset-add-node`、`changeset-evaluate`、`changeset-impact`、`changeset-remove` |
| 约束 | `idempotent-add-replay`、`idempotency-key-reuse`、`version-conflict`、`invalid-operation`、`commit-denied`、`discard-proposal` |

断言写在 `check(result, { assert })` 里，全部是代码判断，不含自然语言评分。

## 5. 加一条任务

```js
{
  id: 'my-task',
  goal: 'What a caller wants to achieve.',
  async run({ client, approvalClient }) {
    return client.call('scene.search', { query: 'Tank' });
  },
  check(result, { assert }) {
    assert(result.items.length === 2, 'two tanks');
  },
}
```

`approvalClient` 是惰性的：只有需要人工权限的任务才触发 bootstrap，其余任务不会
为一次 303 往返付代价。

## 6. 判读报告

`target/eval-report.json`：

```json
{
  "summary": { "tasks": 20, "attemptsPerTask": 3, "stable": 20, "unstable": [] },
  "tasks": [{ "id": "search-by-name", "stable": true, "attempts": [{ "passed": true }] }]
}
```

`stable` 为 false 时，`attempts` 里能看到具体哪一次失败、错误是什么。反复不稳定的任务
通常说明契约里有非确定性行为，而不是 grader 写错了。

## 7. 关联文档

- `ICE3-STUDIO-IDX-001` — [Studio 文档索引](../README.md)
- `ICE3-STUDIO-REF-002` — [本地 API](../reference/api.md)
- `ICE3-STUDIO-REF-001` — [能力清单](../reference/capabilities.md)
- `ICE3-STUDIO-GUIDE-005` — [MCP 适配器](mcp.md)
