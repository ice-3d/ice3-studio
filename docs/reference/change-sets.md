# ChangeSet 审阅契约

> Document ID: ICE3-STUDIO-REF-003
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

ChangeSet 是从不可变项目 revision 派生、可持久化并可供审阅的候选提案。在显式批准并
提交之前，它不会修改 committed Document。

## 1. 生命周期

| Status | 含义 |
|---|---|
| `draft` | 接收 Operation；候选尚未求值，或已过期 |
| `evaluating` | 正在求值候选 |
| `ready` | 候选通过验证并具有 digest |
| `blocked` | 候选验证或 preview 失败 |
| `stale` | 项目 revision 已超过候选的 base revision |
| `committed` | Operation 已作为一个新 revision 提交 |
| `discarded` | 已放弃提案，项目未修改 |

增加或删除 entry 必须携带当前 `expectedVersion`，递增 `version`，并使候选 digest 与
Diagnostics 失效。

## 2. API

以下公开方法实现审阅生命周期：

- `changeset.create`
- `changeset.list`
- `changeset.get`
- `changeset.addOperations`
- `changeset.removeOperations`
- `changeset.evaluate`
- `changeset.impact`
- `changeset.getCandidate`
- `changeset.commit`
- `changeset.discard`

`changeset.evaluate` 返回有序 diff、Diagnostics 和 SHA-256 candidate digest。
`includeCandidate` 决定响应是否包含派生 Document。

`changeset.impact` 求值当前 ChangeSet，并返回有界规划证据：source、derived、relation、
component 和 material 数量；`local`、`targeted`、`broad`、`full_rebuild` 或
`unavailable` scope；新生成和复用 Geometry；cache hit/miss；以及去重后的 reason code。
它返回的 candidate digest 与 `changeset.commit` 校验的 digest 相同。

该方法可以持久化 candidate evaluation state，但不得修改 committed Document，也不得发布
Scene revision。Impact analysis 复用现有增量依赖 planner，并且可以预热内容寻址
Geometry cache；它不会把候选合并到 active scene snapshot。提交后，
`evaluation.stats` 报告 committed revision 的执行证据。

`changeset.commit` 要求：

- `expectedVersion` 匹配；
- 当前 base revision 匹配；
- `approvedDigest` 等于一次新求值得到的 digest；
- 调用方拥有 approval capability；
- 提供唯一 idempotency key。

### 写操作的幂等

`changeset.create`、`changeset.addOperations`、`changeset.removeOperations` 和
`changeset.discard` 都要求 `idempotencyKey`，语义与 `document.apply` 一致：

- 同一个键配同一份请求（`baseRevision` + `title`、`entries`、`entryIds` 或 `changeSetId`）
  只执行一次；重试返回该变更集的当前状态，不产生第二次副作用。
- 同一个键配不同请求会被拒绝，返回 `ICE3_STUDIO_OPERATION_INVALID`。
- 幂等命中先于 `expectedVersion` 校验。重试通常带着已经过期的版本，
  先比版本会把合法重试误判成冲突。
- 键由调用方负责唯一；不要跨变更集复用同一个键。

## 3. 持久化

```text
project/
├── changesets/<change-set-id>.json
├── changesets/idempotency/keys.json
├── cache/candidates/<digest>-source.glb
└── cache/preview/<digest>.glb
```

ChangeSet 文件使用 canonical JSON，通过 sibling temporary file 和 atomic rename 写入。
Candidate preview 使用内容寻址。ChangeSet 在进程重启后仍然存在；单个畸形 ChangeSet
被隔离并报告，不得阻止当前 revision 加载。

## 4. 权限

`review-required` 是默认模式。Agent proposal token 可以读取、创建和编辑 ChangeSet，
求值或分析候选，并请求 candidate preview；它不能 commit、approve、discard，也不能调用
`document.apply`。

人工审批能力由一次性 loopback bootstrap endpoint 发放，并保存在 `HttpOnly`、
`SameSite=Strict` cookie 中。该 capability 可以提交或放弃提案，不会返回给页面脚本或
model prompt。

`autonomous` 模式必须由 host configuration 显式启用。它授予受信任 writer token 直接
执行 transaction 和 ChangeSet commit 的权限。

## 5. 事件

通过认证的 WebSocket 发布：

- `changeset.created`
- `changeset.updated`
- `changeset.evaluated`
- `changeset.committed`
- `changeset.discarded`
- `revision.committed`

提交 ChangeSet 时，`changeset.committed` 先于 `revision.committed` 发布。

## 6. 恢复

- Evaluation 失败时，committed revision 保持不变。
- 被 blocked 或 stale 的 impact request 返回 `scope: "unavailable"`，不预测部分派生
  Geometry。
- Candidate preview 失败时保留上一份有效 candidate 文件。
- Base revision 过期时，ChangeSet 标记为 stale，并以
  `ICE3_STUDIO_REVISION_CONFLICT` 拒绝提交。
- Review 后被修改的 candidate 无法通过 digest 校验，并报告
  `ICE3_STUDIO_OPERATION_INVALID`。
- 使用相同 idempotency key 的重复提交返回原始结果。

## 7. 关联文档

- `ICE3-STUDIO-IDX-001` — [文档索引](../README.md)
- `ICE3-STUDIO-REF-002` — [API Reference](api.md)
- `ICE3-STUDIO-REF-004` — [Diagnostics](diagnostics.md)
