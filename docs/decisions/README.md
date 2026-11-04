# ICE3 Studio 架构决策索引

> Document ID: ICE3-STUDIO-IDX-002
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

| ID | 标题 | Status | 决策 |
|---|---|---|---|
| `ICE3-STUDIO-ADR-001` | [Document 与 Operation History 是唯一真源](0001-document-as-source-of-truth.md) | Implemented | Geometry、bounds、GLB 和 preview 都从 Document 重算 |
| `ICE3-STUDIO-ADR-002` | [本地 API 是产品边界](0002-local-api-boundary.md) | Implemented | Agent、automation 和 Viewer 共享 loopback JSON-RPC |
| `ICE3-STUDIO-ADR-003` | [Operation wire format 统一为 camelCase](0003-wire-format-camel-case.md) | Implemented | 读双接受、写单输出，别名保留到 0.2.0 之后的下一个 minor |
| `ICE3-STUDIO-ADR-004` | [NodeContent 字段统一为 camelCase](0004-node-content-camel-case.md) | Implemented | 与 PartContent 对齐，历史 `radial_segments` 别名双读 |

## 1. 关联文档

- `ICE3-STUDIO-IDX-001` — [Studio 文档索引](../README.md)
- `ICE3-STUDIO-ARC-001` — [Studio 架构](../architecture.md)
