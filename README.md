# ICE3 Studio

> Document ID: ICE3-STUDIO-RM-001
>
> Status: Proposed
>
> Owner: ICE3 Studio maintainers

ICE3 Studio 是本地优先、API-first 的 AI-native 建模系统。Studio Core 使用 Rust 从零
构建场景、几何、材质、关系、校验、版本和导出能力；外部 AI 通过受控 API 创建和修改
模型，Web Viewer 负责展示、选择和人工审查。

当前状态是**架构设计待审阅**。Rust workspace、API、Web Viewer 和建模能力尚未实现，
不得把本 README 中的设计目标描述为已交付功能。

## 当前设计

- [AI-Native Modeling MVP 设计](docs/specs/2026-10-30-ai-native-studio-mvp-design.md)

## AI 辅助开发

本项目在设计和实现过程中大量使用了 AI 辅助开发。

## 许可证

本项目采用 [MIT License](LICENSE) 许可。
