# 0001：Document 与 Operation History 是唯一真源

> Document ID: ICE3-STUDIO-ADR-001
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers
>
> Date: 2026-10-30

## 背景

AI-native 建模系统需要确定性的变更审阅、重试、撤销和恢复。如果把求值后的可变 Scene
graph 当作唯一真源，语义修改将很难与临时 Geometry、renderer state 区分。

## 决策

持久化可序列化的 `Document` 及其 append-only transaction history。求值得到的 Mesh、
bounds、Diagnostics、source GLB 和 Runtime preview 都视为派生值。

每个接受的 batch 对应一个原子 revision。稳定 ID 独立于 display name 和 revision
number。Geometry 可以从 Document 内容重新计算。

## 影响

- AI Operation 可以检查、diff、重试、撤销和重放。
- Transaction 失败时，上一份 snapshot 保持不变。
- Cache invalidation 和增量求值具有明确的 Document dependency。
- 实现必须维护 schema migration 和确定性序列化。
- 大型派生 Asset 不能成为模型的唯一记录。

## 关联文档

- `ICE3-STUDIO-IDX-002` — [架构决策索引](README.md)
- `ICE3-STUDIO-ARC-001` — [Studio 架构](../architecture.md)
- `ICE3-STUDIO-REF-003` — [ChangeSet 审阅契约](../reference/change-sets.md)
