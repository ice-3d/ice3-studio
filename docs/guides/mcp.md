# MCP 适配器

> Document ID: ICE3-STUDIO-GUIDE-005
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

## 1. 它是什么

`studio mcp` 是一个 stdio 的 Model Context Protocol server。它不建模，只把运行中
Studio 的 `system.agentManifest` 转成 MCP tools，再把调用转发回 Studio 的 JSON-RPC。

工具清单、参数 schema、权限都来自运行中的 Studio，适配器不维护副本。Studio 增加或
调整工具时，MCP 客户端下次拉 `tools/list` 就能看到，不需要改适配器。

## 2. 启动

推荐让适配器自己拉起 Studio，配置一次就不会因为服务重启而失效：

```bash
studio mcp --url http://127.0.0.1:8120 --token <token> \
  --spawn --workspace ./workspace --project my-project
```

`--spawn` 只在该地址没有服务时生效：已在运行的实例永远优先，不会被顶掉。适配器退出时
会一并停掉自己启动的 Studio，不留孤儿进程。

也可以自己管理 Studio，先启动再连接：

```bash
studio serve --workspace ./workspace --project my-project --port 8120
studio mcp --url http://127.0.0.1:8120 --token <token>
```

给 `serve` 传固定的 `--token`（或 `STUDIO_TOKEN`）能让客户端配置跨重启有效；不传时
每次启动都会生成新 token，写死在客户端配置里的旧值会失效。

适配器对 loopback 请求禁用系统代理。环境里存在 `HTTP_PROXY` 时，代理会把本地请求
答成 502，看起来像 Studio 不可达。

## 3. 客户端配置

Claude Desktop 的 `claude_desktop_config.json`：

```json
{
  "mcpServers": {
    "ice3-studio": {
      "command": "/path/to/studio-cli",
      "args": [
        "mcp", "--url", "http://127.0.0.1:8120",
        "--spawn", "--workspace", "/path/to/workspace", "--project", "my-project"
      ],
      "env": { "STUDIO_TOKEN": "<token>" }
    }
  }
}
```

其他支持 MCP 的客户端（Cursor、Continue、Zed）使用同样的 `command` / `args` / `env`
形状。适配器只依赖 stdin/stdout，不监听端口。

## 4. 暴露什么

- `tools/list` 返回 manifest 中 `available: true` 的工具。不可用的工具不会出现：
  MCP 没有"权限不足"的展示位，暴露它只会产生必然失败的调用。
- `inputSchema` 与 `outputSchema` 从 `schemaUrl` 抓取后内联，因为 MCP 要求内联 schema
  而不能引用外部文件。schema 在进程内缓存一次。
- `annotations` 把 Studio 已有的声明翻译成 MCP 提示位：

| MCP annotation | 来源 |
|---|---|
| `readOnlyHint` | `sideEffect: read` |
| `destructiveHint` | `sideEffect: commit` 或 `direct_write` |
| `idempotentHint` | `idempotency: required` |
| `openWorldHint` | 恒为 `false`，Studio 只操作本地工程 |

`tools/call` 的返回值同时给出 `content[0].text`（格式化 JSON）和 `structuredContent`
（仅当结果是对象），兼容不同代的客户端。

## 5. 权限边界

适配器用你交给它的 token 调用 Studio，自身不放大权限。`review-required` 下的
proposal token 能读、创建变更集、追加/移除条目、求值，但**不能 commit 或 discard**。
所以 `changeset.commit` 不会出现在工具列表里，审批仍然由人在 Viewer 里完成。

不要在图省事时把 approval token 写进 MCP 配置——那等于让模型绕过人工审批。

## 6. 排错

| 现象 | 原因 |
|---|---|
| `studio is unreachable` | `studio serve` 未启动，或 `--url` 端口不匹配 |
| 工具列表缺少写操作 | token 只有 proposal 权限，或 `--write-mode` 不允许 |
| `schema ... is unreachable` | Studio 与 CLI 版本不匹配，`schemaUrl` 取不到 |
| 进程启动即退出 | 没有提供 token；用 `--token` 或 `STUDIO_TOKEN` |
| 工具调用返回 `isError: true` | 参数校验失败，文本里是 Studio 的诊断信息 |

## 7. 关联文档

- `ICE3-STUDIO-IDX-001` — [Studio 文档索引](../README.md)
- `ICE3-STUDIO-REF-002` — [本地 API](../reference/api.md)
- `ICE3-STUDIO-REF-001` — [能力清单](../reference/capabilities.md)
- `ICE3-STUDIO-GUIDE-004` — [本地入门](getting-started.md)
