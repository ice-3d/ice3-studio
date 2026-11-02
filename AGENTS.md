# ICE3 Studio 仓库规则

> Document ID: ICE3-STUDIO-POL-001
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

## 范围

- 本仓库是 AI-native authoring workspace。
- `ICE3-STUDIO-SPEC-001` 和 `ICE3-STUDIO-PLAN-001` 描述已交付的 MVP；扩展能力必须通过
  新的设计或实施计划明确范围。
- 文档必须明确区分 Proposed 设计与 Implemented 能力。
- Studio Core 使用 Rust；Node.js 不得成为建模 Kernel 或 Runtime。
- 外部 AI 负责语言理解，Studio 不内置模型凭据或 Agent policy。
- 外部建模应用不得作为 Studio Core 的建模实现。

## 开发界面与模型

- Web 界面开发、调试、截图和视觉检查始终使用用户已安装的 Google Chrome，
  不使用无头浏览器、Chrome for Testing 或其它浏览器。
- 启动 Studio Viewer 时始终加载最新模型：从 `projects/*/project.json` 中选择
  `project.json` 修改时间最新的工程；若修改时间相同，选择 `currentRevision` 更高者，
  并从该工程加载最高 revision。仅当用户明确要求时，才改用 fixture 或参考项目。
- 污水处理厂场景的道路净空是硬约束：只允许管道从道路上方跨越；设备、池体、建筑、
  栈桥、支架、检查井、绿化、围栏、路灯及其它构件都必须完全避开道路面。
- 树木只能种在绿地内，不得占用道路、广场、停车场、设备装卸场地或其它硬化地坪。

## 架构边界

- 公开 API 是产品边界；Web Viewer 只通过公开 API 展示和审查。
- Document 和 operation history 是真源，evaluated Geometry 是派生结果。
- Studio Core 不依赖 Toolchain、Engine、Viewer 或外部模型 SDK。
- Studio Server 只能通过 `studio-toolchain-client` 和版本化本地 Sidecar 使用 Toolchain；
  不得把 Node.js 或 Toolchain 私有源码引入建模 Kernel。
- 标准 GLB 是互操作输出；Runtime GLB、LOD 和分区由 Studio 原生 crate 负责。
- glTF/GLB、OBJ/MTL、FBX、USD、STEP 与 IFC Source Import 由 `studio-import` 负责。
  FBX/USD/STEP/IFC 必须在
  自带隔离 worker 中解析，使用硬 timeout、进程崩溃隔离、response/stderr 上限、scene
  budgets、USD composition confinement 和 IFC resource/unit gates；不得恢复外部 DCC bridge，也不得启动
  Toolchain Sidecar、Node Asset runtime 或 Assimp。
- 本地 API 默认只监听 loopback，并限制项目路径和 Operation 能力。
- AI 提案和人工表单修改必须进入 ChangeSet；未批准候选不得修改当前 revision。

## 交付

- 在功能分支开发，适用验证门禁通过后才可合入 `main`。
- 实施开始后必须提供文档检查、Rust format、typecheck、test、API integration 和 E2E
  verification command。
- 根级 `npm run verify` 必须覆盖 Rust format、lint、test、schema、Viewer typecheck、
  unit、build、E2E 和依赖审计。
- 性能行为变化必须更新 `npm run benchmark` 的机器可读基线并核对设计上限。
- 文档变更必须保持 Proposed 与 Implemented 状态准确。

## 提交身份

- 本仓库所有 Git author 和 committer 名称必须为 `damoqiongqiu`。
- 提交、合并或推送前核对配置名称与提交记录。

## 命名与法律边界

- 文档、代码、注释、标识、链接、示例、Diagnostic 或 commit 中不得出现竞品、厂商或第三方产品名称。
- 技术比较使用中性类别描述。
- 不得链接会暴露禁止名称的外部调研资料。

## 关联文档

- `ICE3-STUDIO-RM-001` — [仓库 README](README.md)
