# ICE3 Studio 仓库规则

> Document ID: ICE3-STUDIO-POL-001
>
> Status: Proposed
>
> Owner: ICE3 Studio maintainers

## 范围

- 本仓库是 AI-native authoring workspace。
- `ICE3-STUDIO-SPEC-001` 是当前唯一批准的架构方向；在正式实施计划获得批准前，不得创建
  Rust crate、Viewer、API 或 fixture。
- 文档必须明确区分 Proposed 设计与 Implemented 能力。
- Studio Core 使用 Rust；Node.js 不得成为建模 Kernel 或 Runtime。
- 外部 AI 负责语言理解，Studio 不内置模型凭据或 Agent policy。
- 外部建模应用不得作为 Studio Core 的建模实现。

## 架构边界

- 公开 API 是产品边界；Web Viewer 只通过公开 API 展示和审查。
- Document 和 operation history 是真源，evaluated Geometry 是派生结果。
- Studio Core 不依赖 Toolchain、Engine、Viewer 或外部模型 SDK。
- 标准 GLB 是互操作输出；Runtime LOD 和分区继续由 Toolchain 负责。
- 本地 API 默认只监听 loopback，并限制项目路径和 Operation 能力。

## 交付

- 在功能分支开发，适用验证门禁通过后才可合入 `main`。
- 实施开始后必须提供文档检查、Rust format、typecheck、test、API integration 和 E2E
  verification command。
- 实施必须逐步遵循 `ICE3-STUDIO-SPEC-001` 与
  `ICE3-STUDIO-PLAN-001`，并以失败测试、通过验证和独立 commit 记录每项交付。

## 提交身份

- 本仓库所有 Git author 和 committer 名称必须为 `damoqiongqiu`。
- 提交、合并或推送前核对配置名称与提交记录。

## 命名与法律边界

- 文档、代码、注释、标识、链接、示例、Diagnostic 或 commit 中不得出现竞品、厂商或第三方产品名称。
- 技术比较使用中性类别描述。
- 不得链接会暴露禁止名称的外部调研资料。

## 关联文档

- `ICE3-STUDIO-RM-001` — [仓库 README](README.md)
