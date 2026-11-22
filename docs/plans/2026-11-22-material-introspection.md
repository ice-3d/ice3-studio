# Agent 材质内省实施计划

> Document ID: ICE3-STUDIO-PLAN-034
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers
>
> **For agentic workers:** REQUIRED SUB-SKILL: Use `executing-plans`. This repository
> defaults to Native execution in the current session. Do not spawn or delegate to
> sub-agents unless the user explicitly authorizes them. Steps use checkbox syntax
> for tracking.

**Goal:** 交付只读 `material.get`，让 Agent 按 ID 读取 committed Material 和有效 surface，
无需拉取完整 Document。

**Architecture:** 在 Core 的 Agent wire contract 中新增请求/响应类型；Server 以稳定 ID
查找当前 Document，返回存储 Material 和解析后的 effective surface；schema、bindings、
manifest、Agent eval 与文档同步更新。

**Tech Stack:** Rust 2024、serde/schemars/ts-rs、JSON-RPC 2.0、Node.js ESM。

**Spec:** [ICE3-STUDIO-SPEC-037](../specs/2026-11-22-material-introspection-design.md)

## Global Constraints

- Default execution is Native in the current session; no sub-agents without explicit user permission.
- Develop on a feature branch; do not work directly on `main`.
- `material.get` is read-only and must not create revisions, files, cache entries, previews,
  ChangeSets, or candidate state.
- `materialId` must be non-empty, at most 256 characters, and satisfy the existing Material ID format.
- The method always reads the committed Document and has no request revision field.
- `material` preserves stored fields; an absent `surface` is omitted.
- `effectiveSurface` is always present and equals `MaterialSurface::dielectric()` when stored surface is absent.
- Do not return node/part usage, geometry, file bytes, absolute paths, or token data.
- API version becomes `1.6`; schema method count becomes 43 and runtime method count becomes 45.
- Git author and committer must be `damoqiongqiu <damoqiongqiu@126.com>`.
- No competitor, vendor, or third-party product names in code, comments, docs, or commits.

## Review Focus

1. A material without `surface` still exposes usable dielectric defaults without mutating the Document.
2. Explicit PBR, texture-path, shading, color, and opacity values round-trip with camelCase names.
3. Empty, malformed, overlong, and absent IDs produce stable non-retryable reference errors.
4. Reading a material cannot alter revision, operation history, ChangeSet, preview, or cache state.
5. Generated TypeScript must use camelCase Material fields and optional omitted surface fields.

---

## File Structure

| File | Responsibility |
|---|---|
| `crates/studio-core/src/agent_contract.rs` | Public request/result types and generated contract declarations |
| `crates/studio-core/src/document.rs` | Correct camelCase and optional-field TypeScript declarations for Material types |
| `crates/studio-core/tests/agent_contract.rs` | Wire, TypeScript and effective-surface contract tests |
| `crates/studio-server/src/agent.rs` | `material.get` facade, manifest tool and workflow |
| `crates/studio-server/src/rpc.rs` | Read-only dispatch and runtime method advertisement |
| `crates/studio-server/tests/material_get.rs` | RPC behavior, permissions, errors and side-effect tests |
| `crates/studio-server/tests/rpc_contract.rs` | API 1.6 and capability contract |
| `crates/studio-cli/src/schema.rs` | Static schema method and type registration |
| `crates/studio-core/bindings/*` | Generated per-type TypeScript bindings |
| `schemas/studio-api.schema.json` | Generated static API schema |
| `schemas/studio-types.ts` | Generated aggregate TypeScript bindings |
| `scripts/eval/tasks.mjs` | Public material read evaluation task |
| `docs/reference/api.md` | API 1.6 and `material.get` usage |
| `docs/reference/capabilities.md` | Method catalog and counts |
| `docs/architecture.md` | Runtime method count |
| `docs/guides/agent-eval.md` | Task count and coverage |
| `docs/README.md` | SPEC-037 and PLAN-034 status |
| `docs/specs/2026-11-22-material-introspection-design.md` | Design status transition |
| `docs/plans/2026-11-22-material-introspection.md` | Plan status and checkbox completion |

---

### Task 1: Core Wire Contract and Material TypeScript Fidelity

**Files:**
- Modify: `crates/studio-core/src/agent_contract.rs`
- Modify: `crates/studio-core/src/document.rs`
- Modify: `crates/studio-core/tests/agent_contract.rs`
- Generated: `crates/studio-core/bindings/Material.ts`
- Generated: `crates/studio-core/bindings/MaterialShading.ts`
- Generated: `crates/studio-core/bindings/MaterialSurface.ts`
- Generated: `crates/studio-core/bindings/MaterialGetParams.ts`
- Generated: `crates/studio-core/bindings/MaterialGetResult.ts`

**Interfaces:**
- Consumes: existing `Material`, `MaterialSurface`, revision conventions and ts-rs generation.
- Produces: `MaterialGetParams { material_id: String }`,
  `MaterialGetResult { revision: u64, material: Material, effective_surface: MaterialSurface }`.

- [x] **Step 1: Add failing wire and TypeScript tests**

In `crates/studio-core/tests/agent_contract.rs`, extend imports with:

```rust
use studio_core::agent_contract::{
    MaterialGetParams, MaterialGetResult, MAX_MATERIAL_ID_CHARS,
};
use studio_core::document::{Material, MaterialSurface};
use studio_core::ids::MaterialId;
```

Append:

```rust
#[test]
fn material_get_serializes_camel_case_and_rejects_unknown_fields() {
    let request = MaterialGetParams {
        material_id: "material-000001".into(),
    };
    let value = serde_json::to_value(&request).unwrap();
    assert_eq!(value["materialId"], "material-000001");
    assert!(value.get("material_id").is_none());
    assert_eq!(MAX_MATERIAL_ID_CHARS, 256);

    let rejected = serde_json::from_value::<MaterialGetParams>(serde_json::json!({
        "materialId": "material-000001",
        "includeUsage": true
    }));
    assert!(rejected.is_err());
}

#[test]
fn material_get_result_preserves_stored_and_effective_surface_distinction() {
    let material = Material {
        id: MaterialId::from("material-000001"),
        name: "Default dielectric".into(),
        base_color: [0.4, 0.5, 0.6, 1.0],
        opacity: 1.0,
        edge_color: None,
        shading: None,
        surface: None,
    };
    let result = MaterialGetResult {
        revision: 1,
        effective_surface: material
            .surface
            .clone()
            .unwrap_or_else(MaterialSurface::dielectric),
        material,
    };
    let value = serde_json::to_value(&result).unwrap();
    assert_eq!(value["material"]["baseColor"][0], 0.4);
    assert!(value["material"].get("surface").is_none());
    assert_eq!(value["effectiveSurface"]["metallic"], 0.0);
    assert_eq!(value["effectiveSurface"]["roughness"], 0.9);
    assert_eq!(value["effectiveSurface"]["uvScale"], serde_json::json!([1.0, 1.0]));
}

#[test]
fn material_typescript_uses_wire_names_and_optional_surface() {
    let params = MaterialGetParams::decl();
    assert!(params.contains("materialId: string"));
    assert!(!params.contains("material_id"));

    let result = MaterialGetResult::decl();
    assert!(result.contains("revision: number"));
    assert!(result.contains("material: Material"));
    assert!(result.contains("effectiveSurface: MaterialSurface"));

    let material = Material::decl();
    assert!(material.contains("baseColor:"));
    assert!(material.contains("edgeColor:"));
    assert!(material.contains("surface?: MaterialSurface"));
    assert!(!material.contains("base_color"));

    let surface = MaterialSurface::decl();
    assert!(surface.contains("uvScale:"));
    assert!(surface.contains("baseColorTexture?:"));
    assert!(surface.contains("ormTexture?:"));

    let shading = studio_core::document::MaterialShading::decl();
    assert!(shading.contains("\"directional\""));
    assert!(shading.contains("\"water\""));
}
```

- [x] **Step 2: Run tests and verify RED**

```bash
CARGO_TARGET_DIR=$PWD/target \
  cargo test -p studio-core --test agent_contract
```

Expected: FAIL because the material contract types do not exist and Material TypeScript still uses
snake_case and non-optional surface fields.

- [x] **Step 3: Implement Core declarations and correct Material TS metadata**

In `crates/studio-core/src/agent_contract.rs`, import:

```rust
use crate::document::{Material, MaterialSurface};
```

Add near `MAX_PART_SUMMARIES`:

```rust
pub const MAX_MATERIAL_ID_CHARS: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, rename_all = "camelCase")]
pub struct MaterialGetParams {
    pub material_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct MaterialGetResult {
    #[ts(type = "number")]
    pub revision: u64,
    pub material: Material,
    pub effective_surface: MaterialSurface,
}
```

In `crates/studio-core/src/document.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub enum MaterialShading {
    Directional,
    Flat,
    Water,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct Material {
    // Existing fields stay unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub surface: Option<MaterialSurface>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, rename_all = "camelCase", optional_fields = nullable)]
pub struct MaterialSurface {
    // Existing fields stay unchanged.
}
```

Keep all existing derives, serde attributes, fields and defaults. Only add or adjust TS metadata.

- [x] **Step 4: Regenerate per-crate bindings and verify GREEN**

```bash
CARGO_TARGET_DIR=$PWD/target cargo test -p studio-core --lib
CARGO_TARGET_DIR=$PWD/target cargo test -p studio-core --test agent_contract
```

Expected: PASS. Generated bindings include camelCase Material fields, optional texture fields,
lowercase `MaterialShading` variants, `MaterialGetParams` and `MaterialGetResult`.

- [x] **Step 5: Commit**

```bash
git add crates/studio-core/src/agent_contract.rs \
  crates/studio-core/src/document.rs \
  crates/studio-core/tests/agent_contract.rs \
  crates/studio-core/bindings
git commit -m "feat(core): define material introspection contract"
```

---

### Task 2: Read-Only Server Method and Generated API Contract

**Files:**
- Modify: `crates/studio-server/src/agent.rs`
- Modify: `crates/studio-server/src/rpc.rs`
- Create: `crates/studio-server/tests/material_get.rs`
- Modify: `crates/studio-server/tests/rpc_contract.rs`
- Modify: `crates/studio-cli/src/schema.rs`
- Modify: `scripts/eval/tasks.mjs`
- Modify: `crates/studio-core/src/lib.rs`
- Modify: `crates/studio-core/tests/workspace_contract.rs`
- Modify: `crates/studio-cli/tests/schema_output.rs`
- Generated: `schemas/studio-api.schema.json`
- Generated: `schemas/studio-types.ts`

**Interfaces:**
- Consumes: `MaterialGetParams`, `MaterialGetResult`, `MAX_MATERIAL_ID_CHARS`,
  `MaterialSurface::dielectric()`.
- Produces: JSON-RPC `material.get`, API `1.6`, manifest tool
  `MaterialGetParams -> MaterialGetResult`, static schema count 43.

- [x] **Step 1: Write failing RPC tests**

Create `crates/studio-server/tests/material_get.rs`:

```rust
mod common;

use studio_server::{ServerConfig, TestServer, start_test_server};

use common::{rpc_with_approval_cookie, rpc_with_bearer};

async fn seed_materials(server: &TestServer) {
    let response = rpc_with_approval_cookie(
        server,
        "document.apply",
        serde_json::json!({
            "expectedRevision": 0,
            "dryRun": false,
            "idempotencyKey": "material-get-fixture",
            "operations": [
                {
                    "type": "material.upsert",
                    "materialId": "material-000001",
                    "name": "Brushed steel",
                    "baseColor": [0.31, 0.35, 0.37, 1.0],
                    "opacity": 0.9,
                    "edgeColor": [0.1, 0.1, 0.1, 1.0],
                    "shading": "flat",
                    "surface": {
                        "metallic": 0.8,
                        "roughness": 0.25,
                        "emissive": [0.01, 0.0, 0.0],
                        "uvScale": [2.0, 3.0],
                        "baseColorTexture": "textures/steel.png",
                        "normalTexture": "textures/steel-normal.png",
                        "normalScale": 1.2,
                        "ormTexture": "textures/steel-orm.png"
                    }
                },
                {
                    "type": "material.upsert",
                    "materialId": "material-000002",
                    "name": "Paint",
                    "baseColor": [0.2, 0.3, 0.4, 1.0],
                    "opacity": 1.0,
                    "edgeColor": null
                }
            ]
        }),
    )
    .await;
    assert_eq!(response["result"]["revision"], 1, "{response}");
}

async fn agent_rpc(
    server: &TestServer,
    method: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    rpc_with_bearer(server, server.agent_token(), method, params).await
}

#[tokio::test]
async fn material_get_returns_explicit_surface_and_effective_copy() {
    let server = start_test_server(ServerConfig::review_required("material-get-surface")).await;
    seed_materials(&server).await;
    let revision = agent_rpc(&server, "document.get", serde_json::json!({})).await
        ["revision"].clone();

    let response = agent_rpc(
        &server,
        "material.get",
        serde_json::json!({ "materialId": "material-000001" }),
    )
    .await;
    assert_eq!(response["result"]["revision"], revision, "{response}");
    assert_eq!(response["result"]["material"]["name"], "Brushed steel");
    assert_eq!(response["result"]["material"]["baseColor"][1], 0.35);
    assert_eq!(response["result"]["material"]["edgeColor"][0], 0.1);
    assert_eq!(response["result"]["material"]["shading"], "flat");
    assert_eq!(response["result"]["material"]["surface"]["metallic"], 0.8);
    assert_eq!(
        response["result"]["effectiveSurface"],
        response["result"]["material"]["surface"]
    );

    let after = agent_rpc(&server, "document.get", serde_json::json!({})).await;
    assert_eq!(after["revision"], revision);
}

#[tokio::test]
async fn material_get_defaults_effective_surface_without_adding_one() {
    let server = start_test_server(ServerConfig::review_required("material-get-default")).await;
    seed_materials(&server).await;
    let response = agent_rpc(
        &server,
        "material.get",
        serde_json::json!({ "materialId": "material-000002" }),
    )
    .await;
    assert!(response["result"]["material"].get("surface").is_none(), "{response}");
    assert_eq!(response["result"]["effectiveSurface"]["metallic"], 0.0);
    assert_eq!(response["result"]["effectiveSurface"]["roughness"], 0.9);
    assert_eq!(
        response["result"]["effectiveSurface"]["uvScale"],
        serde_json::json!([1.0, 1.0])
    );
}

#[tokio::test]
async fn material_get_errors_are_stable_and_non_retryable() {
    let server = start_test_server(ServerConfig::review_required("material-get-errors")).await;
    seed_materials(&server).await;

    let overlong = "x".repeat(257);
    for (material_id, category) in [
        ("material-999999", "not_found"),
        ("not-a-material-id", "invalid_id"),
        ("", "invalid_id"),
        (overlong.as_str(), "invalid_id"),
    ] {
        let response = agent_rpc(
            &server,
            "material.get",
            serde_json::json!({ "materialId": material_id }),
        )
        .await;
        assert_eq!(
            response["error"]["diagnosticCode"],
            "ICE3_STUDIO_REFERENCE_INVALID",
            "{response}"
        );
        assert_eq!(response["error"]["data"]["category"], category, "{response}");
        assert_eq!(response["error"]["data"]["retryable"], false, "{response}");
    }
}

#[tokio::test]
async fn material_get_is_available_to_read_only_callers_and_advertised() {
    let server = start_test_server(ServerConfig::review_required("material-get-read")).await;
    seed_materials(&server).await;
    let response = reqwest::Client::new()
        .post(server.rpc_url())
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "material.get",
            "params": { "materialId": "material-000001" }
        }))
        .send()
        .await
        .unwrap()
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert_eq!(response["result"]["material"]["id"], "material-000001", "{response}");

    let manifest = agent_rpc(&server, "system.agentManifest", serde_json::json!({})).await;
    let tool = manifest["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "material.get")
        .expect("manifest must advertise material.get");
    assert_eq!(tool["sideEffect"], "read");
    assert_eq!(tool["requiresApproval"], false);
    assert_eq!(tool["inputSchemaRef"], "#/types/MaterialGetParams");
    assert_eq!(tool["outputSchemaRef"], "#/types/MaterialGetResult");
}
```

- [x] **Step 2: Run tests and verify RED**

```bash
CARGO_TARGET_DIR=$PWD/target \
  cargo test -p studio-server --test material_get
```

Expected: FAIL because `material.get` is unknown and not advertised.

- [x] **Step 3: Implement facade, dispatch and manifest**

In `crates/studio-server/src/agent.rs`, import:

```rust
use studio_core::agent_contract::{
    MaterialGetParams, MaterialGetResult, MAX_MATERIAL_ID_CHARS,
};
use studio_core::document::{MaterialSurface, NodeContent};
use studio_core::ids::MaterialId;
```

Add manifest tool after `component.get`:

```rust
tool(
    "material.get",
    "Read one material definition and its effective surface.",
    AgentSideEffect::Read,
    false,
    AgentIdempotency::None,
    access.can_read,
    "read access is unavailable",
    "MaterialGetParams",
    Some("MaterialGetResult"),
),
```

Add `"material.get"` to the inspect workflow after
`"component.get|evaluation.query"`.

Add:

```rust
pub(crate) fn material_get(
    session: &mut Session,
    request: MaterialGetParams,
) -> Result<MaterialGetResult, RpcError> {
    if request.material_id.trim().is_empty()
        || request.material_id.chars().count() > MAX_MATERIAL_ID_CHARS
        || !MaterialId::from(request.material_id.clone()).is_valid()
    {
        return Err(material_reference_error(
            &request.material_id,
            "invalid_id",
            "material ID is invalid",
        ));
    }
    let material = session
        .document
        .materials
        .iter()
        .find(|material| material.id.as_str() == request.material_id)
        .cloned()
        .ok_or_else(|| {
            material_reference_error(
                &request.material_id,
                "not_found",
                "material does not exist",
            )
        })?;
    let effective_surface = material
        .surface
        .clone()
        .unwrap_or_else(MaterialSurface::dielectric);
    Ok(MaterialGetResult {
        revision: session.document.revision.as_u64(),
        material,
        effective_surface,
    })
}

fn material_reference_error(
    material_id: &str,
    category: &str,
    message: &str,
) -> RpcError {
    RpcError::with_diagnostic(
        -32010,
        DiagnosticCode::ReferenceInvalid.as_str(),
        format!("{}: {message}: {material_id}", DiagnosticCode::ReferenceInvalid.as_str()),
        Some(serde_json::json!({
            "category": category,
            "retryable": false,
            "objectId": material_id,
            "suggestions": ["run scene.search to list valid materialId values"]
        })),
    )
}
```

In `crates/studio-server/src/rpc.rs`, add `"material.get"` to runtime methods after
`"component.get"` and dispatch:

```rust
"material.get" => {
    let request: studio_core::agent_contract::MaterialGetParams =
        serde_json::from_value(request.params.clone()).map_err(invalid_params)?;
    serde_json::to_value(crate::agent::material_get(session, request)?)
        .map_err(serialize_error)
}
```

In `crates/studio-cli/src/schema.rs`, import
`MaterialGetParams`, `MaterialGetResult` and register:

```rust
"material.get",
```

after `"component.get"`, plus:

```rust
"MaterialGetParams": schema_for!(MaterialGetParams),
"MaterialGetResult": schema_for!(MaterialGetResult),
```

In `crates/studio-core/src/lib.rs`, change `API_VERSION` to `"1.6"`.

In `scripts/eval/tasks.mjs`, add after `bom-grouping`:

```js
{
  id: 'material-get',
  finalMethod: 'material.get',
  goal: 'Read one material without downloading the full document.',
  async run({ client }) {
    return client.call('material.get', { materialId: 'mat-000001' });
  },
  check(result, { assert }) {
    assert(result.revision > 0, 'committed revision');
    assert(result.material.id === 'mat-000001', 'material id');
    assert(result.material.name === 'Concrete', 'material name');
    assert(result.material.baseColor.length === 4, 'camelCase base color');
    assert(result.material.surface === undefined, 'stored surface remains absent');
    assert(result.effectiveSurface.metallic === 0, 'default metallic');
    assert(result.effectiveSurface.roughness === 0.9, 'default roughness');
  },
},
```

- [x] **Step 4: Update API contract tests**

In `crates/studio-server/tests/rpc_contract.rs`, change all API expectations from `1.5` to
`1.6`, update the route inspection capability/method assertions accordingly, add
`material.get` to the methods assertion, and in the manifest test assert:

```rust
assert!(manifest["result"]["tools"].as_array().unwrap().iter().any(|tool| {
    tool["name"] == "material.get"
        && tool["sideEffect"] == "read"
        && tool["requiresApproval"] == false
}));
```

In `crates/studio-core/tests/workspace_contract.rs`, expect `"1.6"`.

In `crates/studio-cli/tests/schema_output.rs`, assert schema and TypeScript include:

```rust
assert!(schema["types"].get("MaterialGetParams").is_some());
assert!(schema["types"].get("MaterialGetResult").is_some());
assert!(types.contains("export type MaterialGetResult"));
assert!(types.contains("materialId: string"));
assert!(types.contains("effectiveSurface: MaterialSurface"));
assert!(types.contains("baseColor: [number, number, number, number]"));
assert!(!types.contains("base_color:"));
```

- [x] **Step 5: Generate schema and verify GREEN**

```bash
CARGO_TARGET_DIR=$PWD/target cargo run -q -p studio-cli -- schema --output schemas
CARGO_TARGET_DIR=$PWD/target cargo test -p studio-server --test material_get
CARGO_TARGET_DIR=$PWD/target cargo test -p studio-server --test rpc_contract
CARGO_TARGET_DIR=$PWD/target cargo test -p studio-cli --test schema_output
CARGO_TARGET_DIR=$PWD/target cargo test -p studio-core --test workspace_contract
CARGO_TARGET_DIR=$PWD/target cargo build -p studio-cli
node scripts/eval/run.mjs --attempts 1
```

Expected: PASS. Static schema contains 43 methods; runtime advertises the same 43 plus two
experimental GPU methods. Agent eval runs 21 tasks and `material-get` passes.

- [x] **Step 6: Commit**

```bash
git add crates/studio-server/src/agent.rs \
  crates/studio-server/src/rpc.rs \
  crates/studio-server/tests/material_get.rs \
  crates/studio-server/tests/rpc_contract.rs \
  crates/studio-cli/src/schema.rs \
  crates/studio-cli/tests/schema_output.rs \
  scripts/eval/tasks.mjs \
  crates/studio-core/src/lib.rs \
  crates/studio-core/tests/workspace_contract.rs \
  crates/studio-core/bindings schemas
git commit -m "feat(api): expose material introspection"
```

---

### Task 3: Documentation and Final Gate

**Files:**
- Modify: `docs/reference/api.md`
- Modify: `docs/reference/capabilities.md`
- Modify: `docs/architecture.md`
- Modify: `docs/guides/agent-eval.md`
- Modify: `docs/README.md`
- Modify: `docs/specs/2026-11-22-material-introspection-design.md`
- Modify: `docs/plans/2026-11-22-material-introspection.md`

**Interfaces:**
- Consumes: public `material.get`.
- Produces: Implemented documentation and final verification evidence.

- [x] **Step 1: Update public documentation**

`docs/reference/api.md`:

- change API version `1.5` to `1.6`;
- add version 1.6 summary;
- method count to schema 43 / runtime 45;
- add method table row:

```markdown
| `material.get` | 否 | `MaterialGetParams` | 单材质定义、committed revision 与有效 surface |
```

- add usage text:

```markdown
`material.get` 接受 `{ materialId }`，返回 committed Document 中原始 `Material` 和始终有值的
`effectiveSurface`。无显式 surface 时，Material 保留缺省状态，effectiveSurface 使用
metallic 0、roughness 0.9、uvScale [1,1] 的规范电介质默认值。未知、非法或过长 ID 返回
`ICE3_STUDIO_REFERENCE_INVALID`。方法不返回引用节点、几何或贴图文件。
```

`docs/reference/capabilities.md`:

- API version `1.6`, counts `schema 43 / runtime 45`;
- add `### 2.6 material（1）`;
- document `material.get` response and Agent workflow;
- renumber the following sections.

`docs/architecture.md`:

```text
静态 Schema 固化 43 个 JSON-RPC 方法；运行中的 Server 另外公布
`evaluation.waitScene` 和 `evaluation.visibleInstances`，共 45 个方法。
```

`docs/guides/agent-eval.md`:

- update task counts to 21;
- update external-solver count to 16;
- add `material-get` to the read category;
- update report examples from 20 to 21.

`docs/README.md`:

- GUIDE-006 description from 20 to 21 tasks;
- SPEC-037 status to `Implemented`;
- add PLAN-034 row with `Implemented`.

`docs/specs/2026-11-22-material-introspection-design.md`: status `Implemented`.

- [x] **Step 2: Run complete gate**

```bash
cargo fmt --all -- --check
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=4 \
  CARGO_TARGET_DIR=$PWD/target \
  cargo clippy --workspace --all-targets --all-features -- -D warnings
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=4 \
  CARGO_TARGET_DIR=$PWD/target \
  cargo test --workspace --all-features
npm run benchmark:route-safety
node scripts/check-docs.mjs
node scripts/check-method-docs.mjs
PLAYWRIGHT_CHANNEL=chrome npm run verify
```

Expected:

- 21 Agent evaluation tasks pass three attempts each;
- schema method documentation is complete with 45 runtime methods;
- route-safety diagnostics remain zero;
- Viewer unit/typecheck/build/E2E and dependency audit pass;
- all 34 root verification gates pass.

- [x] **Step 3: Commit acceptance**

Mark all completed plan checkboxes, then:

```bash
git add scripts/eval/tasks.mjs docs README.md schemas crates apps
git commit -m "test(material): verify agent material introspection"
```

- [x] **Step 4: Final review and local merge**

Run the executing-plans final self-review because repository policy prohibits sub-agents unless
the user explicitly enables them. Fix Critical/Important findings with RED→GREEN tests, verify
the merged result, and merge locally with `--no-ff`. Do not push unless the user explicitly asks.

---

## Completion Checklist

- [x] `material.get` is read-only, revision-bound and does not mutate any state.
- [x] Explicit Material fields round-trip with camelCase wire names.
- [x] Missing surface returns dielectric `effectiveSurface` without adding a stored surface.
- [x] Empty, malformed, overlong and absent IDs return stable `REFERENCE_INVALID`.
- [x] Material TypeScript bindings use camelCase and optional omitted fields.
- [x] Server capabilities, manifest, static schema and aggregate bindings expose API 1.6.
- [x] Agent eval includes and passes `material-get`.
- [x] SPEC-037 and PLAN-034 are `Implemented`.
- [x] Root `npm run verify` and route-safety benchmark pass.
