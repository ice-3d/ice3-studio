# 0002：本地 API 是产品边界

> Document ID: ICE3-STUDIO-ADR-002
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers
>
> Date: 2026-10-30

## 背景

AI client、automation 和 Viewer 需要同一组建模能力。只提供 UI 路径会使 headless
operation 无法实现，并让 rendering concern 泄漏进 modeling kernel。在本地 contract
稳定前引入 remote service，则会过早增加账号和网络依赖。

## 决策

由 Rust Studio Server 暴露只监听 loopback 的 JSON-RPC API。外部 AI 负责自然语言理解，
并通过该 API 提交类型化 Operation。Web Viewer 是薄客户端，没有私有 mutation route。

写请求需要短生命周期 session token。项目路径由 host 控制；JT converter 是唯一
host-owned external executable exception。Studio 不持有 AI model credential 或 Agent
policy。

## 影响

- Headless workflow 和 Viewer 使用同一 contract。
- API schema、Operation、Diagnostics 和 event 必须版本化并维护文档。
- Loopback 和 workspace restriction 限制本地暴露面，但不提供多用户授权。
- 未来远程部署需要独立安全和租户设计，不能直接暴露本地边界。

## 关联文档

- `ICE3-STUDIO-IDX-002` — [架构决策索引](README.md)
- `ICE3-STUDIO-REF-002` — [本地 API](../reference/api.md)
- `ICE3-STUDIO-REF-005` — [Asset Pipeline](../reference/asset-pipeline.md)
