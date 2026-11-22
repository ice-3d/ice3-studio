# Agent 材质内省设计

> Document ID: ICE3-STUDIO-SPEC-037
>
> Status: Approved
>
> Owner: ICE3 Studio maintainers

## 1. 摘要

Agent 已可通过 `scene.search` 获得 `materialId`，也可通过 `scene.bom groupBy=material`
聚合材质使用量，但缺少按 ID 读取单个材质定义的通道。要安全地提出 `material.patch`，
Agent 只能拉取完整 `document.get`，或者猜测当前 PBR、表面和描边字段。

本设计增加只读 JSON-RPC 方法 `material.get`。它按稳定 `materialId` 返回当前 committed
revision 的完整 Material 和解析后的 effective surface，不返回完整场景、节点引用或几何。

## 2. 目标

- Agent 拿到 `materialId` 后，一次调用即可获得：
  - id、名称、base color、opacity、edge color 与 shading；
  - 当前存储的 PBR surface；
  - 当 `surface` 缺省时，实际生效的电介质默认 surface。
- 返回结果可直接用于构造稀疏 `material.patch`，无需先调用 `document.get`。
- 只读 token 即可调用，不改变 revision、cache、preview、ChangeSet 或文件。
- 复用既有 `Material` 与 `MaterialSurface` wire 类型，不建立第二套材质模型。
- API、schema、TypeScript bindings 与 Agent manifest 使用同一份类型源。

## 3. 非目标

- 不返回材质引用节点、部件或实例清单；使用情况继续通过
  `scene.bom groupBy=material` 和 `scene.search` 获取。
- 不解析参数化 material role，不复制实例或组件的 binding 求值逻辑。
- 不回读贴图文件，不验证文件存在性，不返回绝对路径或资源字节。
- 不新增写操作；修改仍通过 ChangeSet 或既有 `material.patch` 工作流。
- 不增加 Viewer 材质编辑面板。
- 不改变 `document.get`、`scene.bom`、`scene.search` 或 `material.patch` 的响应形状。
- 不做 constraint 内省；其设计独立立项。

## 4. 方法契约

### 4.1 Method

`material.get` 是只读方法，read authority 即可调用。

### 4.2 Request

```text
MaterialGetParams {
  materialId: string
}
```

- `materialId` 必须非空、最多 256 个字符，并满足现有 Material ID 格式。
- 方法始终读取当前 committed Document。

### 4.3 Response

```text
MaterialGetResult {
  revision: number
  material: Material
  effectiveSurface: MaterialSurface
}
```

- `material` 与 Document 中的 `Material` schema 完全相同。
- `material.surface` 保持存储状态：显式 surface 存在时返回该对象，缺省时字段省略。
- `effectiveSurface` 始终有值。`material.surface` 存在时与其相同，否则返回
  `MaterialSurface::dielectric()` 的规范默认值。
- 纹理字段保持 Document 中的相对路径字符串；API 不打开文件。
- 单对象响应天然有界，不引入分页、cursor 或 truncation。

示例：

```json
{
  "revision": 1,
  "material": {
    "id": "material-000023",
    "name": "Pipe / return sludge",
    "baseColor": [0.52, 0.12, 0.1, 1.0],
    "opacity": 1.0,
    "edgeColor": [0.1, 0.08, 0.07, 1.0],
    "shading": "directional"
  },
  "effectiveSurface": {
    "metallic": 0.0,
    "roughness": 0.9,
    "emissive": null,
    "uvScale": [1.0, 1.0],
    "baseColorTexture": null,
    "normalTexture": null,
    "normalScale": null,
    "ormTexture": null
  }
}
```

### 4.4 Errors

- 空值、超过 256 字符或格式非法：`ICE3_STUDIO_REFERENCE_INVALID`，data category 为
  `invalid_id`，不可重试。
- 格式合法但 Material 不存在：`ICE3_STUDIO_REFERENCE_INVALID`，data category 为
  `not_found`，不可重试，并提示先调用 `scene.search` 查找 `materialId`。
- 错误消息不得包含 workspace 路径、token 或文件内容。

## 5. Server 与 Agent

- `material.get` 加入静态 schema、运行时 capabilities 和只读 dispatch。
- `system.agentManifest` 增加 read-only tool：
  `MaterialGetParams -> MaterialGetResult`。
- inspect workflow 为：
  `scene.context → scene.search → component.get | material.get → route.inspect →
  validation.run`。
- 修改材质的工作流为：
  `material.get → 本地修改 → changeset.addOperations(material.patch) →
  changeset.evaluate → 人工批准`。
- 方法不运行 validation，不触发 geometry evaluation，不刷新 scene snapshot。

API 版本从 `1.5` 升为 `1.6`；方法总数从 schema 42 / runtime 44 变为
schema 43 / runtime 45。

## 6. 兼容性与安全

- `Material` 已有的缺省 `surface` 语义保持不变。
- `effectiveSurface` 是新增读字段，不写回 Document。
- 相对纹理路径按现有 validation 规则返回；API 不解析路径，也不证明文件存在。
- `material.get` 受 read authority、loopback 和项目路径边界约束。
- 现有项目无需迁移；缺少 `surface` 的材质继续工作。

## 7. 测试

### 7.1 Core / Wire

- `MaterialGetParams` 使用 camelCase，拒绝未知字段。
- TypeScript binding 使用 `materialId`、`effectiveSurface` 和 `number`。
- 显式 surface 原样返回，且 `effectiveSurface` 与其相等。
- 缺少 `surface` 时返回规范电介质默认值，但不修改原 Document。
- 浮点值按现有 JSON 序列化规则回读。

### 7.2 Server

- review-required 模式使用 read token 可调用，不需要 approval cookie。
- unknown、empty、过长和格式非法 ID 返回稳定 `REFERENCE_INVALID`。
- capability method 计数、API version、manifest tool 与 schema 引用一致。
- 方法调用前后 revision、history、ChangeSet 和 cache 文件不变化。
- 实际污水厂材质可读，包括贴图材质和无显式 surface 材质。

### 7.3 Documentation

- API Reference 加入方法表与调用说明。
- Capabilities 方法分类和计数同步。
- Agent manifest workflow 与生成 bindings 同步。
- 根级 `npm run verify` 通过。

## 8. 验收标准

- Agent 不调用 `document.get` 即可读取任意现有材质的完整定义和 effective surface。
- 返回值足以构造不会清空未修改字段的 `material.patch`。
- unknown/invalid ID 错误稳定且可操作。
- `material.get` 无写副作用，不返回场景引用、几何、绝对路径或文件字节。
- API 1.6、schema 43 / runtime 45、manifest、bindings 和文档一致。
- Rust、schema、文档、Agent eval 和 Viewer 回归门禁全部通过。
