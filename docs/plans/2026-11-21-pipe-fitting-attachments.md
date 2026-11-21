# 管线附件实体化实施计划

> Document ID: ICE3-STUDIO-PLAN-032
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers
>
> **For agentic workers:** REQUIRED SUB-SKILL: Use executing-plans. This repository
> defaults to Native execution. Do not spawn or delegate to sub-agents unless the
> user explicitly authorizes them. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 交付通用三通、异径管和穿墙套管契约、Core 对齐校验、参数化资产组件和污水厂迁移。

**Architecture:** Component Port 先支持参数绑定；资产层创建附件几何和真实端口；Core 在
`route_attachment` 中复用正式路由、宿主几何和附件预算，对附件引用和穿越事实失败关闭。
污水厂只通过资产层 metadata 和组件实例接入，不向 Core 增加业务枚举。

**Tech Stack:** Rust 2024、serde/schemars/ts-rs、glam、Node.js ESM、Studio Document、
ChangeSet API、Google Chrome。

**Spec:** [ICE3-STUDIO-SPEC-035](../specs/2026-11-21-pipe-fitting-attachments-design.md)

## Global Constraints

- Default execution is Native in the current session; no sub-agents without explicit user permission.
- Develop on `feat/pipe-fitting-attachments`, not directly on `main`.
- Browser development and visual checks use the installed Google Chrome only.
- Do not use a headless browser, Chrome for Testing, or another browser.
- Document remains the only source of truth; evaluated geometry remains derived.
- Core must not contain wastewater, road, basin, pump, or other business-specific attachment enums.
- Existing `connection.create`, support metadata, route policy and Diagnostic wire format stay compatible.
- Attachment position tolerance is `max(0.02 m, diameter * 0.05)`, capped at `0.10 m`.
- Axis tolerance is `2°`; sleeve surface target band is `0.15 m`.
- Attachment limits: 512 instances, 3 ports per attachment, 8,192 participant cache entries,
  4,096 penetration candidates, 250,000 triangle pairs and 8 diagnostics per attachment.
- Split hydraulic runs may use connection metadata `supportLineId` to bind support stations to
  the physical main while retaining a shared fitting `lineId`; absent metadata falls back to
  `lineId`.
- No hard performance limit may be widened silently.
- Git author and committer must be `damoqiongqiu <damoqiongqiu@126.com>`.
- No competitor, vendor, or third-party product names in docs, code, comments, identifiers or commits.

## Review Focus

- Malformed attachment metadata: unknown kind or missing `lineId` must fail with a stable code.
- Parameterized port templates: changing instance parameters must move the port, not only the mesh.
- Branch topology: a tee with three connections must use three distinct ports.
- Diameter transition: a reducer must join two different diameters without weakening connection checks.
- AABB-only wall contact: a sleeve must require a real triangle crossing, not only a bounding-box hit.
- Self geometry: an attachment must not collide with its own line but must remain an obstacle to other lines.
- Budget exhaustion: exceeding attachment or penetration limits must fail closed.

---

## File Structure

| File | Responsibility |
|---|---|
| `crates/studio-core/src/large_scene/component.rs` | Parameterized component port template types |
| `crates/studio-core/src/large_scene/route.rs` | Resolve port templates with instance parameters |
| `crates/studio-core/src/large_scene/route_attachment.rs` | Attachment parser, alignment, penetration and budget checks |
| `crates/studio-core/src/large_scene/route_safety.rs` | Expose attachment-aware self-line obstacle isolation |
| `crates/studio-core/src/validation.rs` | Stable attachment Diagnostic codes |
| `crates/studio-core/tests/route_attachments.rs` | Public Core attachment contract tests |
| `scripts/wastewater-plant/pipe-attachments.mjs` | Reusable fitting component and placement helpers |
| `scripts/wastewater-plant/site-plan.mjs` | Wastewater fitting definitions and migration |
| `scripts/wastewater-plant/validate.mjs` | Asset-level fitting and sleeve completeness checks |
| `scripts/tests/wastewater-plant-model.test.mjs` | Asset migration and validation tests |
| `scripts/wastewater-visual-review.mjs` | Fitting close-up Chrome inspection views |

---

### Task 1: Parameterized Component Ports

**Files:**
- Modify: `crates/studio-core/src/large_scene/component.rs:355-363`
- Modify: `crates/studio-core/src/large_scene/evaluate.rs:948-990`
- Modify: `crates/studio-core/src/large_scene/route.rs:29-101`
- Test: `crates/studio-core/tests/connections.rs`

**Interfaces:**
- Consumes: Existing `ScalarBinding`, `Vector3Binding`, `resolve_parameters`, component instance parameters.
- Produces: `ComponentPort::{position: Vector3Binding, direction: Vector3Binding, diameter: ScalarBinding}`;
  `resolve_port` resolves all three bindings before applying the instance world transform.

- [ ] **Step 1: Write the failing parameterized-port test**

Add to `crates/studio-core/tests/connections.rs`:

```rust
#[test]
fn parameterized_component_port_uses_instance_parameters() {
    let mut document = support::shared_pump_document();
    document.components[0].parameters.push(ComponentParameter {
        name: "outletDiameter".into(),
        value_type: ParameterType::Number {
            minimum: Some(0.1),
            maximum: Some(1.0),
        },
        default: ParameterValue::Number(0.4),
        unit: Some("m".into()),
    });
    document.components[0].ports.push(ComponentPort {
        name: "sized-outlet".into(),
        part_id: "body".into(),
        position: Vector3Binding::Components {
            x: ScalarBinding::Expression {
                expr: "outletDiameter / 2".into(),
            },
            y: ScalarBinding::Literal(0.0),
            z: ScalarBinding::Literal(0.0),
        },
        direction: Vector3Binding::Literal(Vec3 {
            x: 1.0,
            y: 0.0,
            z: 0.0,
        }),
        medium: "water".into(),
        diameter: ScalarBinding::Parameter {
            param: "outletDiameter".into(),
        },
    });
    let NodeContent::ComponentInstance(instance) =
        &mut document.scene.nodes_mut()[0].content
    else {
        panic!("expected component instance");
    };
    instance.parameters.insert(
        "outletDiameter".into(),
        ParameterValue::Number(0.4),
    );
    let definitions = document
        .components
        .iter()
        .map(|definition| (definition.id.clone(), definition))
        .collect::<BTreeMap<_, _>>();

    let port = studio_core::large_scene::route::resolve_port(
        &document,
        &definitions,
        &InstancePortRef::Instance {
            node_id: NodeId::from("node-000001"),
            port: "sized-outlet".into(),
        },
    )
    .unwrap();

    assert!((port.position.x - 2.2).abs() < 1e-6);
    assert!((port.diameter - 0.4).abs() < 1e-6);
}
```

- [ ] **Step 2: Run the test and verify it fails**

Run:

```bash
cargo test -p studio-core --test connections parameterized_component_port_uses_instance_parameters
```

Expected: compile failure because `ComponentPort` fields are numeric, not bindings.

- [ ] **Step 3: Change the port contract to bindings**

In `component.rs`, change `ComponentPort` to:

```rust
pub struct ComponentPort {
    pub name: String,
    pub part_id: String,
    pub position: Vector3Binding,
    pub direction: Vector3Binding,
    pub medium: String,
    pub diameter: ScalarBinding,
}
```

Existing JSON numeric literals remain compatible because both binding enums are `untagged`.

- [ ] **Step 4: Expose binding resolution**

Change the visibility of the existing helpers in `evaluate.rs`:

```rust
pub(crate) fn resolve_scalar(
    binding: &ScalarBinding,
    parameters: &BTreeMap<String, ParameterValue>,
) -> Result<f32, ParameterError> {
    // Existing body stays unchanged.
}

pub(crate) fn resolve_vector3(
    binding: &Vector3Binding,
    parameters: &BTreeMap<String, ParameterValue>,
) -> Result<Vec3, ParameterError> {
    // Existing body stays unchanged.
}
```

Do not duplicate expression evaluation.

- [ ] **Step 5: Resolve port bindings in `resolve_port`**

In `route.rs`, carry the instance parameter map alongside the definition ID:

```rust
let (definition_id, local_transform, parameters) = match (&node.content, ordinal) {
    (NodeContent::ComponentInstance(instance), None) => (
        &instance.definition_id,
        world_transform(document, node)?,
        &instance.parameters,
    ),
    (NodeContent::SetItem(set), Some(ordinal)) => {
        // Existing ordinal expansion stays unchanged.
        (&set.definition_id, transform, &set.parameters)
    }
    _ => return Err(ConnectionError::MissingEndpoint),
};
```

Then resolve the port:

```rust
let resolved_parameters = resolve_parameters(definition, parameters)
    .map_err(|_| ConnectionError::InvalidTransform)?;
let port_position = resolve_vector3(&port.position, &resolved_parameters)
    .map_err(|_| ConnectionError::InvalidTransform)?;
let port_direction = resolve_vector3(&port.direction, &resolved_parameters)
    .map_err(|_| ConnectionError::InvalidTransform)?;
let port_diameter = resolve_scalar(&port.diameter, &resolved_parameters)
    .map_err(|_| ConnectionError::InvalidTransform)?;
```

Use `port_position`, `port_direction` and `port_diameter` in the existing transform code.

- [ ] **Step 6: Mechanically update existing Rust test literals**

Wrap existing numeric literals:

```rust
position: Vector3Binding::Literal(Vec3 { x: 1.0, y: 0.0, z: 0.0 }),
direction: Vector3Binding::Literal(Vec3 { x: 1.0, y: 0.0, z: 0.0 }),
diameter: ScalarBinding::Literal(0.2),
```

Apply this to the six existing `ComponentPort` literals in `crates/studio-core`.

- [ ] **Step 7: Run connection and schema tests**

Run:

```bash
cargo test -p studio-core --test connections
cargo test -p studio-core --test agent_patches
cargo test -p studio-core --test large_scene_validation
cargo test -p studio-core --test schema_output
```

Expected: all tests pass and generated schemas represent binding fields.

- [ ] **Step 8: Format and commit**

```bash
cargo fmt --all
git add crates/studio-core/src/large_scene/component.rs \
  crates/studio-core/src/large_scene/evaluate.rs \
  crates/studio-core/src/large_scene/route.rs \
  crates/studio-core/tests
git commit -m "feat(core): parameterize component ports"
```

---

### Task 2: Attachment Diagnostics and Metadata Contract

**Files:**
- Modify: `crates/studio-core/src/validation.rs:13-205`
- Create: `crates/studio-core/src/large_scene/route_attachment.rs`
- Modify: `crates/studio-core/src/large_scene/mod.rs`
- Modify: `crates/studio-core/src/large_scene/validate.rs:104-115`
- Modify: `docs/reference/diagnostics.md`
- Test: `crates/studio-core/src/large_scene/route_attachment.rs` unit tests

**Interfaces:**
- Consumes: `Document`, `BTreeMap<ComponentId, &ComponentDefinition>`, node metadata.
- Produces: `validate_route_attachments(document, definitions) -> Vec<Diagnostic>`;
  internal `AttachmentKind::{Tee, Reducer, Sleeve}`; public diagnostic constants.

- [ ] **Step 1: Write the failing malformed-metadata test**

Add a unit test in `route_attachment.rs`:

```rust
#[test]
fn rejects_unknown_attachment_kind() {
    let mut document = Document::new("bad-attachment");
    document.scene.nodes_mut().push(SceneNode {
        id: NodeId::from("node-000001"),
        name: "Bad fitting".into(),
        parent_id: None,
        transform: Transform::IDENTITY,
        material_id: None,
        content: NodeContent::Primitive(Primitive::Box {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        }),
        metadata: BTreeMap::from([
            ("role".into(), "pipe-fitting".into()),
            ("fittingKind".into(), "elbow".into()),
            ("lineId".into(), "L-001".into()),
        ]),
        visible: true,
    });

    let diagnostics = validate_route_attachments(&document, &BTreeMap::new());

    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::RouteAttachmentInvalid
            && diagnostic.object_id.as_deref() == Some("node-000001")
            && diagnostic.pointer.ends_with("/metadata/fittingKind")
    }));
}
```

- [ ] **Step 2: Run the test and verify it fails**

Run:

```bash
cargo test -p studio-core route_attachment::tests::rejects_unknown_attachment_kind
```

Expected: compile failure because the module and Diagnostic variants do not exist.

- [ ] **Step 3: Add stable Diagnostic codes**

Add to `DiagnosticCode`:

```rust
#[serde(rename = "ICE3_STUDIO_ROUTE_ATTACHMENT_INVALID")]
#[ts(rename = "ICE3_STUDIO_ROUTE_ATTACHMENT_INVALID")]
RouteAttachmentInvalid,
#[serde(rename = "ICE3_STUDIO_ROUTE_ATTACHMENT_MISALIGNED")]
#[ts(rename = "ICE3_STUDIO_ROUTE_ATTACHMENT_MISALIGNED")]
RouteAttachmentMisaligned,
#[serde(rename = "ICE3_STUDIO_ROUTE_PENETRATION_INVALID")]
#[ts(rename = "ICE3_STUDIO_ROUTE_PENETRATION_INVALID")]
RoutePenetrationInvalid,
#[serde(rename = "ICE3_STUDIO_ROUTE_ATTACHMENT_BUDGET")]
#[ts(rename = "ICE3_STUDIO_ROUTE_ATTACHMENT_BUDGET")]
RouteAttachmentBudget,
```

Add the four strings to `DIAGNOSTIC_CODES` and `as_str`.

- [ ] **Step 4: Add the attachment parser**

Create `route_attachment.rs`:

```rust
use std::collections::{BTreeMap, BTreeSet};

use crate::document::{Document, NodeContent};
use crate::ids::ComponentId;
use crate::large_scene::component::ComponentDefinition;
use crate::validation::{Diagnostic, DiagnosticCode};

pub const MAX_ROUTE_ATTACHMENTS: usize = 512;
pub const MAX_ATTACHMENT_PORTS: usize = 3;
pub const MAX_PENETRATION_CANDIDATES: usize = 4_096;
pub const MAX_ATTACHMENT_TRIANGLE_PAIRS: usize = 250_000;
pub const MAX_ATTACHMENT_DIAGNOSTICS: usize = 8;
pub const ATTACHMENT_POSITION_TOLERANCE_FLOOR_M: f32 = 0.02;
pub const ATTACHMENT_POSITION_TOLERANCE_RATIO: f32 = 0.05;
pub const ATTACHMENT_POSITION_TOLERANCE_CAP_M: f32 = 0.10;
pub const ATTACHMENT_AXIS_TOLERANCE_RADIANS: f32 = 2.0_f32.to_radians();
pub const SLEEVE_SURFACE_TOLERANCE_M: f32 = 0.15;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AttachmentKind {
    Tee,
    Reducer,
    Sleeve,
}

impl AttachmentKind {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "tee" => Some(Self::Tee),
            "reducer" => Some(Self::Reducer),
            "sleeve" => Some(Self::Sleeve),
            _ => None,
        }
    }
}

pub(crate) fn validate_route_attachments(
    document: &Document,
    _definitions: &BTreeMap<ComponentId, &ComponentDefinition>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut count = 0_usize;
    for (index, node) in document.scene.nodes().iter().enumerate() {
        let role = node.metadata.get("role").map(String::as_str);
        if !matches!(role, Some("pipe-fitting" | "pipe-penetration")) {
            continue;
        }
        count += 1;
        if count > MAX_ROUTE_ATTACHMENTS {
            diagnostics.push(attachment_diagnostic(
                DiagnosticCode::RouteAttachmentBudget,
                index,
                node.id.as_str(),
                "route attachment budget exceeded",
            ));
            break;
        }
        let Some(kind) = node
            .metadata
            .get("fittingKind")
            .and_then(|value| AttachmentKind::parse(value))
        else {
            diagnostics.push(attachment_field_diagnostic(
                DiagnosticCode::RouteAttachmentInvalid,
                index,
                node.id.as_str(),
                "/scene/nodes/{index}/metadata/fittingKind",
                "fittingKind must be tee, reducer, or sleeve",
            ));
            continue;
        };
        if node
            .metadata
            .get("lineId")
            .is_none_or(|value| value.trim().is_empty())
        {
            diagnostics.push(attachment_field_diagnostic(
                DiagnosticCode::RouteAttachmentInvalid,
                index,
                node.id.as_str(),
                "/scene/nodes/{index}/metadata/lineId",
                "attachment lineId must not be empty",
            ));
            continue;
        }
        if matches!(kind, AttachmentKind::Sleeve)
            && node
                .metadata
                .get("hostNodeId")
                .is_none_or(|value| value.trim().is_empty())
        {
            diagnostics.push(attachment_field_diagnostic(
                DiagnosticCode::RouteAttachmentInvalid,
                index,
                node.id.as_str(),
                "/scene/nodes/{index}/metadata/hostNodeId",
                "sleeve hostNodeId must not be empty",
            ));
            continue;
        }
        if !matches!(node.content, NodeContent::ComponentInstance(_)) {
            diagnostics.push(attachment_field_diagnostic(
                DiagnosticCode::RouteAttachmentInvalid,
                index,
                node.id.as_str(),
                "/scene/nodes/{index}/content",
                "route attachments must be component instances",
            ));
        }
    }
    diagnostics
}

fn attachment_diagnostic(
    code: DiagnosticCode,
    node_index: usize,
    node_id: &str,
    message: impl Into<String>,
) -> Diagnostic {
    Diagnostic {
        code,
        operation_index: None,
        object_id: Some(node_id.into()),
        pointer: format!("/scene/nodes/{node_index}/metadata"),
        message: message.into(),
    }
}

fn attachment_field_diagnostic(
    code: DiagnosticCode,
    node_index: usize,
    node_id: &str,
    pointer: &str,
    message: impl Into<String>,
) -> Diagnostic {
    Diagnostic {
        code,
        operation_index: None,
        object_id: Some(node_id.into()),
        pointer: pointer.replace("{index}", &node_index.to_string()),
        message: message.into(),
    }
}
```

Remove the unused `BTreeSet` import when the later tasks have finished.

- [ ] **Step 5: Wire attachment validation into scene validation**

In `mod.rs`:

```rust
pub mod route_attachment;
```

At the end of `validate_large_scene`:

```rust
diagnostics.extend(
    crate::large_scene::route_attachment::validate_route_attachments(
        document,
        &definitions,
    ),
);
```

- [ ] **Step 6: Document all four codes**

Add exact rows to `docs/reference/diagnostics.md`:

```markdown
| `ICE3_STUDIO_ROUTE_ATTACHMENT_INVALID` | 附件 metadata、端口、引用、直径或宿主无效 |
| `ICE3_STUDIO_ROUTE_ATTACHMENT_MISALIGNED` | 附件中心线、方向或表面位置不满足容差 |
| `ICE3_STUDIO_ROUTE_PENETRATION_INVALID` | 套管未命中真实宿主、轴线错误或重复声明 |
| `ICE3_STUDIO_ROUTE_ATTACHMENT_BUDGET` | 附件或穿越检查预算耗尽 |
```

- [ ] **Step 7: Run tests**

```bash
cargo test -p studio-core route_attachment::tests::rejects_unknown_attachment_kind
cargo test -p studio-server --test verification_contract
cargo fmt --all -- --check
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/studio-core/src/validation.rs \
  crates/studio-core/src/large_scene/route_attachment.rs \
  crates/studio-core/src/large_scene/mod.rs \
  crates/studio-core/src/large_scene/validate.rs \
  docs/reference/diagnostics.md
git commit -m "feat(core): define route attachment contract"
```

---

### Task 3: Validate Tees

**Files:**
- Modify: `crates/studio-core/src/large_scene/route_attachment.rs`
- Create: `crates/studio-core/tests/route_attachments.rs`
- Modify: `crates/studio-core/tests/route_attachments.rs`
- Modify: `crates/studio-core/src/large_scene/route.rs` only if a shared route input helper is needed

**Interfaces:**
- Consumes: `AttachmentKind::Tee`, `resolve_port`, `route_connection`, connection `lineId`.
- Produces: tee validation requiring `run-in`, `run-out`, `branch` distinct ports, matching run diameters,
  collinear run axes and a branch angle in `30°..=150°`.

- [ ] **Step 1: Write the failing valid/invalid tee tests**

Create `route_attachments.rs` with a fixture helper and these assertions:

```rust
#[test]
fn accepts_a_tee_with_three_distinct_connected_ports() {
    let document = tee_document();
    let diagnostics = validate_document(&document);
    assert!(
        !diagnostics.iter().any(|diagnostic| matches!(
            diagnostic.code,
            DiagnosticCode::RouteAttachmentInvalid
                | DiagnosticCode::RouteAttachmentMisaligned
        )),
        "{diagnostics:?}",
    );
}

#[test]
fn rejects_a_reused_or_missing_tee_port() {
    let mut document = tee_document();
    let branch = connection_for_port(&mut document, "branch");
    branch.from = branch.to.clone();
    let diagnostics = validate_document(&document);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::RouteAttachmentInvalid
            && diagnostic.object_id.as_deref() == Some("node-000005")
    }));
}

#[test]
fn rejects_a_tee_branch_that_is_collinear_with_the_run() {
    let mut document = tee_document();
    let tee = document
        .components
        .iter_mut()
        .find(|definition| definition.id == ComponentId::from("component-000002"))
        .unwrap();
    let branch = tee.ports.iter_mut().find(|port| port.name == "branch").unwrap();
    branch.direction = Vector3Binding::Literal(Vec3 {
        x: 1.0,
        y: 0.0,
        z: 0.0,
    });
    let diagnostics = validate_document(&document);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::RouteAttachmentMisaligned
    }));
}
```

The helper `tee_document()` must create:

- one tee component with three ports and three box parts;
- three endpoint components with one water port each;
- one tee instance at `(0, 0, 0)`;
- three connections sharing `lineId="L-TEE"`;
- direct routes that terminate at `run-in`, `run-out` and `branch`.

- [ ] **Step 2: Run the test and verify it fails**

```bash
cargo test -p studio-core --test route_attachments accepts_a_tee_with_three_distinct_connected_ports
```

Expected: tee diagnostics are missing.

- [ ] **Step 3: Extend the parsed attachment into a resolved attachment**

Add internal types and parse component ports:

```rust
struct Attachment {
    node_index: usize,
    node_id: String,
    kind: AttachmentKind,
    line_id: String,
    host_node_id: Option<String>,
    ports: BTreeMap<String, ResolvedPort>,
}

fn resolved_attachment(
    document: &Document,
    definitions: &BTreeMap<ComponentId, &ComponentDefinition>,
    node: &SceneNode,
    index: usize,
    kind: AttachmentKind,
) -> Result<Attachment, Diagnostic> {
    let NodeContent::ComponentInstance(instance) = &node.content else {
        return Err(attachment_field_diagnostic(
            DiagnosticCode::RouteAttachmentInvalid,
            index,
            node.id.as_str(),
            "/scene/nodes/{index}/content",
            "route attachments must be component instances",
        ));
    };
    let definition = definitions
        .get(&instance.definition_id)
        .ok_or_else(|| attachment_field_diagnostic(
            DiagnosticCode::RouteAttachmentInvalid,
            index,
            node.id.as_str(),
            "/scene/nodes/{index}/content",
            "attachment component definition is missing",
        ))?;
    if definition.ports.len() > MAX_ATTACHMENT_PORTS {
        return Err(attachment_field_diagnostic(
            DiagnosticCode::RouteAttachmentInvalid,
            index,
            node.id.as_str(),
            "/scene/nodes/{index}/content",
            "attachment has too many ports",
        ));
    }
    let ports = definition
        .ports
        .iter()
        .map(|port| {
            let resolved = resolve_port(
                document,
                definitions,
                &InstancePortRef::Instance {
                    node_id: node.id.clone(),
                    port: port.name.clone(),
                },
            )
            .map_err(|_| attachment_field_diagnostic(
                DiagnosticCode::RouteAttachmentInvalid,
                index,
                node.id.as_str(),
                "/scene/nodes/{index}/content",
                format!("attachment port {} cannot be resolved", port.name),
            ))?;
            Ok((port.name.clone(), resolved))
        })
        .collect::<Result<BTreeMap<_, _>, Diagnostic>>()?;
    Ok(Attachment {
        node_index: index,
        node_id: node.id.to_string(),
        kind,
        line_id: node.metadata["lineId"].clone(),
        host_node_id: node.metadata.get("hostNodeId").cloned(),
        ports,
    })
}
```

- [ ] **Step 4: Implement the tee validator**

Add:

```rust
fn validate_tee(
    document: &Document,
    attachment: &Attachment,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let required = ["run-in", "run-out", "branch"];
    if required.iter().any(|name| !attachment.ports.contains_key(*name)) {
        diagnostics.push(attachment_diagnostic(
            DiagnosticCode::RouteAttachmentInvalid,
            attachment.node_index,
            &attachment.node_id,
            "tee requires run-in, run-out and branch ports",
        ));
        return;
    }
    let run_in = &attachment.ports["run-in"];
    let run_out = &attachment.ports["run-out"];
    let branch = &attachment.ports["branch"];
    if run_in.diameter != run_out.diameter
        || run_in.medium != run_out.medium
        || branch.medium != run_in.medium
    {
        diagnostics.push(attachment_diagnostic(
            DiagnosticCode::RouteAttachmentInvalid,
            attachment.node_index,
            &attachment.node_id,
            "tee run ports must share diameter and medium",
        ));
    }
    if dot(run_in.direction, run_out.direction) > -cos(ATTACHMENT_AXIS_TOLERANCE_RADIANS) {
        diagnostics.push(attachment_diagnostic(
            DiagnosticCode::RouteAttachmentMisaligned,
            attachment.node_index,
            &attachment.node_id,
            "tee run ports must face in opposite directions",
        ));
    }
    let branch_dot = dot(run_in.direction, branch.direction);
    if branch_dot.abs() > cos(30.0_f32.to_radians()) {
        diagnostics.push(attachment_diagnostic(
            DiagnosticCode::RouteAttachmentMisaligned,
            attachment.node_index,
            &attachment.node_id,
            "tee branch angle must be between 30 and 150 degrees",
        ));
    }
    let linked = connections_for_line_and_node(
        document,
        &attachment.line_id,
        &attachment.node_id,
    );
    let used_ports = linked
        .iter()
        .filter_map(|(_, port)| port.clone())
        .collect::<BTreeSet<_>>();
    if used_ports != BTreeSet::from(required.map(str::to_owned)) {
        diagnostics.push(attachment_diagnostic(
            DiagnosticCode::RouteAttachmentInvalid,
            attachment.node_index,
            &attachment.node_id,
            "tee requires one connection on each independent port",
        ));
    }
}

fn dot(left: Vec3, right: Vec3) -> f32 {
    left.x * right.x + left.y * right.y + left.z * right.z
}
```

Use the existing `Vec3` import and add `std::f32::consts::{cos}` only if imported directly;
plain `f32::cos()` calls are acceptable.

- [ ] **Step 5: Run focused tests**

```bash
cargo test -p studio-core --test route_attachments tee
cargo test -p studio-core --test connections
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/studio-core/src/large_scene/route_attachment.rs \
  crates/studio-core/tests/route_attachments.rs
git commit -m "feat(core): validate pipe tees"
```

---

### Task 4: Validate Reducers

**Files:**
- Modify: `crates/studio-core/src/large_scene/route_attachment.rs`
- Modify: `crates/studio-core/tests/route_attachments.rs`

**Interfaces:**
- Consumes: `AttachmentKind::Reducer`, resolved `large` and `small` ports.
- Produces: reducer validation requiring strict diameter decrease, equal medium, opposing directions,
  collinear centers and one connection per port.

- [ ] **Step 1: Write the failing reducer tests**

Add:

```rust
#[test]
fn accepts_a_reducer_with_two_different_diameters() {
    let diagnostics = validate_document(&reducer_document());
    assert!(!diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::RouteAttachmentInvalid
    }));
}

#[test]
fn rejects_equal_or_reversed_reducer_diameters() {
    for (large, small) in [(0.4, 0.4), (0.2, 0.4)] {
        let document = reducer_document_with_diameters(large, small);
        let diagnostics = validate_document(&document);
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == DiagnosticCode::RouteAttachmentInvalid
        }));
    }
}
```

The fixture uses ports `large` and `small`, direct connections on both sides,
and the same `lineId`.

- [ ] **Step 2: Run and verify failure**

```bash
cargo test -p studio-core --test route_attachments reducer
```

Expected: reducer cases are not validated yet.

- [ ] **Step 3: Implement reducer validation**

Add:

```rust
fn validate_reducer(
    document: &Document,
    attachment: &Attachment,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let (Some(large), Some(small)) = (
        attachment.ports.get("large"),
        attachment.ports.get("small"),
    ) else {
        diagnostics.push(attachment_diagnostic(
            DiagnosticCode::RouteAttachmentInvalid,
            attachment.node_index,
            &attachment.node_id,
            "reducer requires large and small ports",
        ));
        return;
    };
    if large.diameter <= small.diameter
        || large.medium != small.medium
        || dot(large.direction, small.direction) > -cos(ATTACHMENT_AXIS_TOLERANCE_RADIANS)
    {
        diagnostics.push(attachment_diagnostic(
            DiagnosticCode::RouteAttachmentInvalid,
            attachment.node_index,
            &attachment.node_id,
            "reducer requires decreasing diameters and opposite directions",
        ));
    }
    let linked = connections_for_line_and_node(
        document,
        &attachment.line_id,
        &attachment.node_id,
    );
    let used_ports = linked
        .iter()
        .filter_map(|(_, port)| port.clone())
        .collect::<BTreeSet<_>>();
    if used_ports != BTreeSet::from(["large".into(), "small".into()]) {
        diagnostics.push(attachment_diagnostic(
            DiagnosticCode::RouteAttachmentInvalid,
            attachment.node_index,
            &attachment.node_id,
            "reducer requires one connection on each port",
        ));
    }
}
```

Call `validate_reducer` from the main attachment loop.

- [ ] **Step 4: Verify no connection-diameter regression**

Run:

```bash
cargo test -p studio-core --test route_attachments reducer
cargo test -p studio-core --test connections
cargo test -p studio-core --test large_scene_validation
```

Expected: PASS. Existing direct connections still reject mismatched diameters.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-core/src/large_scene/route_attachment.rs \
  crates/studio-core/tests/route_attachments.rs
git commit -m "feat(core): validate pipe reducers"
```

---

### Task 5: Validate Wall Penetration Sleeves

**Files:**
- Modify: `crates/studio-core/src/large_scene/route_attachment.rs`
- Modify: `crates/studio-core/tests/route_attachments.rs`

- [ ] **Step 1: Write the failing sleeve tests**

Add:

```rust
#[test]
fn accepts_a_sleeve_that_crosses_a_real_wall() {
    let document = sleeve_document(true);
    let definitions = definitions(&document);
    let diagnostics = validate_route_attachments(&document, &definitions);
    assert!(!diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::RoutePenetrationInvalid
            || diagnostic.code == DiagnosticCode::RouteAttachmentMisaligned
    }));
}

#[test]
fn rejects_a_sleeve_that_only_touches_the_host_bounds() {
    let document = sleeve_document(false);
    let definitions = definitions(&document);
    let diagnostics = validate_route_attachments(&document, &definitions);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::RoutePenetrationInvalid
    }));
}

#[test]
fn rejects_duplicate_sleeves_for_the_same_line_and_host() {
    let mut document = sleeve_document(true);
    clone_sleeve(&mut document, "node-000006");
    let definitions = definitions(&document);
    let diagnostics = validate_route_attachments(&document, &definitions);
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::RoutePenetrationInvalid
    }));
}
```

`sleeve_document(true)` places a box wall centered at `(0, 0, 0)` and a direct route
through its center. `sleeve_document(false)` moves the route just outside the box triangles
while still touching the expanded AABB.

- [ ] **Step 2: Run and verify failure**

```bash
cargo test -p studio-core --test route_attachments sleeve
```

Expected: sleeve checks are absent.

- [ ] **Step 3: Add a selected-host triangle helper**

In `route_attachment.rs`:

```rust
fn host_triangles(
    document: &Document,
    host_id: &NodeId,
) -> Result<Vec<[[f32; 3]; 3]>, ()> {
    let mut store = crate::large_scene::cache::MemoryGeometryStore::default();
    let selected = BTreeSet::from([host_id.clone()]);
    let evaluated = crate::large_scene::evaluate::evaluate_large_scene_selected(
        document,
        &mut store,
        None,
        &selected,
    )
    .map_err(|_| ())?;
    let mut triangles = Vec::new();
    for node in evaluated.nodes.iter().filter(|node| node.owner_id == host_id.as_str()) {
        let Some(mesh_key) = &node.mesh_key else {
            continue;
        };
        let Some(mesh) = evaluated.meshes.get(mesh_key) else {
            continue;
        };
        let matrix = transform_matrix(node.transform);
        for triangle in mesh.indices.as_chunks::<3>().0 {
            let mut world = [[0.0_f32; 3]; 3];
            for (target, index) in world.iter_mut().zip(triangle) {
                let point = mesh.vertices[*index as usize].position;
                let point = matrix.transform_point3(GlamVec3::new(
                    point[0], point[1], point[2],
                ));
                *target = [point.x, point.y, point.z];
            }
            triangles.push(world);
        }
    }
    Ok(triangles)
}
```

This helper is called only after an AABB candidate exists. A document without a candidate
must not evaluate host geometry.

- [ ] **Step 4: Implement sleeve validation**

Add the wall-in/wall-out checks:

```rust
fn validate_sleeve(
    document: &Document,
    definitions: &BTreeMap<ComponentId, &ComponentDefinition>,
    attachment: &Attachment,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let host_id = attachment.host_node_id.as_ref().expect("validated metadata");
    let host_index = document
        .scene
        .nodes()
        .iter()
        .position(|node| node.id.as_str() == host_id);
    let Some(host_index) = host_index else {
        diagnostics.push(attachment_diagnostic(
            DiagnosticCode::RouteAttachmentInvalid,
            attachment.node_index,
            &attachment.node_id,
            format!("sleeve host {host_id} does not exist"),
        ));
        return;
    };
    let host = &document.scene.nodes()[host_index];
    if host.metadata.get("routeObstacle").map(String::as_str) == Some("ignored") {
        diagnostics.push(attachment_diagnostic(
            DiagnosticCode::RouteAttachmentInvalid,
            attachment.node_index,
            &attachment.node_id,
            "sleeve host must be a route barrier",
        ));
        return;
    }
    let Some(route) = first_route_for_line(document, definitions, &attachment.line_id) else {
        diagnostics.push(attachment_diagnostic(
            DiagnosticCode::RouteAttachmentInvalid,
            attachment.node_index,
            &attachment.node_id,
            format!("sleeve line {} has no route", attachment.line_id),
        ));
        return;
    };
    let Ok(triangles) = host_triangles(document, &host.id) else {
        diagnostics.push(attachment_diagnostic(
            DiagnosticCode::RoutePenetrationInvalid,
            attachment.node_index,
            &attachment.node_id,
            "sleeve host geometry cannot be evaluated",
        ));
        return;
    };
    let crossing = route
        .windows(2)
        .any(|segment| triangles.iter().any(|triangle| {
            crate::geometry::segment_triangle(*segment, *triangle)
        }));
    if !crossing {
        diagnostics.push(attachment_diagnostic(
            DiagnosticCode::RoutePenetrationInvalid,
            attachment.node_index,
            &attachment.node_id,
            format!("line {} does not cross host {}", attachment.line_id, host_id),
        ));
    }
    let (Some(wall_in), Some(wall_out)) = (
        attachment.ports.get("wall-in"),
        attachment.ports.get("wall-out"),
    ) else {
        diagnostics.push(attachment_diagnostic(
            DiagnosticCode::RouteAttachmentInvalid,
            attachment.node_index,
            &attachment.node_id,
            "sleeve requires wall-in and wall-out ports",
        ));
        return;
    };
    if wall_in.diameter <= route_pipe_diameter(document, &attachment.line_id)
        || wall_out.diameter != wall_in.diameter
        || dot(wall_in.direction, wall_out.direction) > -cos(ATTACHMENT_AXIS_TOLERANCE_RADIANS)
    {
        diagnostics.push(attachment_diagnostic(
            DiagnosticCode::RouteAttachmentMisaligned,
            attachment.node_index,
            &attachment.node_id,
            "sleeve diameter or axis is invalid",
        ));
    }
    for port in [wall_in, wall_out] {
        let distance = point_to_route_distance(port.position, &route);
        if distance > ATTACHMENT_POSITION_TOLERANCE_CAP_M {
            diagnostics.push(attachment_diagnostic(
                DiagnosticCode::RouteAttachmentMisaligned,
                attachment.node_index,
                &attachment.node_id,
                format!("sleeve port is {distance:.3} m off route"),
            ));
        }
    }
}
```

Implement `first_route_for_line`, `route_pipe_diameter` and
`point_to_route_distance` in this module; do not duplicate `route_connection`.

- [ ] **Step 5: Reject duplicate sleeve associations**

Before running sleeve validation, build:

```rust
let mut sleeve_keys = BTreeSet::new();
let key = format!("{}\0{}", attachment.line_id, host_id);
if !sleeve_keys.insert(key) {
    diagnostics.push(attachment_diagnostic(
        DiagnosticCode::RoutePenetrationInvalid,
        attachment.node_index,
        &attachment.node_id,
        "duplicate sleeve for route and host",
    ));
}
```

- [ ] **Step 6: Run tests and update diagnostics docs**

```bash
cargo test -p studio-core --test route_attachments sleeve
cargo test -p studio-server --test verification_contract
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/studio-core/src/large_scene/route_attachment.rs \
  crates/studio-core/tests/route_attachments.rs
git commit -m "feat(core): validate penetration sleeves"
```

---

### Task 6: Attachment Obstacle Isolation and Budgets

**Files:**
- Modify: `crates/studio-core/src/large_scene/route_safety.rs:92-470`
- Modify: `crates/studio-core/src/large_scene/route_attachment.rs`
- Modify: `crates/studio-core/tests/route_attachments.rs`

**Interfaces:**
- Consumes: `ObstacleOwner`, route `line_id`, attachment node metadata `role=pipe-fitting|pipe-penetration`.
- Produces: self-line attachment obstacle exemption without suppressing unrelated route collisions.

- [ ] **Step 1: Write failing self-line and cross-line tests**

Add:

```rust
#[test]
fn fitting_does_not_collide_with_its_own_line() {
    let diagnostics = route_safety_codes(&tee_document());
    assert!(!diagnostics.contains(&DiagnosticCode::RouteCollision));
}

#[test]
fn fitting_remains_an_obstacle_to_an_unrelated_line() {
    let document = crossing_line_through_tee_document();
    let diagnostics = route_safety_codes(&document);
    assert!(diagnostics.contains(&DiagnosticCode::RouteCollision));
}

#[test]
fn attachment_budget_exhaustion_fails_closed() {
    let document = tee_document();
    let definitions = definitions(&document);
    let diagnostics = validate_route_attachments_with_limits(
        &document,
        &definitions,
        RouteAttachmentLimits {
            max_attachments: 0,
            ..RouteAttachmentLimits::default()
        },
    );
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::RouteAttachmentBudget
    }));
}

#[test]
fn declared_penetration_host_collision_is_validated_by_the_attachment_gate() {
    let valid = validate_document(&sleeve_document(true));
    assert!(!valid
        .iter()
        .any(|diagnostic| diagnostic.code == DiagnosticCode::RouteCollision));

    let invalid = validate_document(&sleeve_document(false));
    assert!(invalid
        .iter()
        .any(|diagnostic| diagnostic.code == DiagnosticCode::RoutePenetrationInvalid));
}
```

- [ ] **Step 2: Run and verify self-line collision**

```bash
cargo test -p studio-core --test route_attachments fitting_does_not_collide_with_its_own_line
```

Expected: FAIL with `RouteCollision`.

- [ ] **Step 3: Carry attachment line identity into obstacles**

Extend `ObstacleOwner`:

```rust
struct ObstacleOwner {
    source_index: usize,
    source_id: NodeId,
    bounds: Bounds,
    class: ObstacleClass,
    narrow_phase: bool,
    attachment_line_id: Option<String>,
}
```

When creating the owner:

```rust
let attachment_line_id = matches!(
    node.metadata.get("role").map(String::as_str),
    Some("pipe-fitting" | "pipe-penetration")
)
.then(|| node.metadata.get("lineId").cloned())
.flatten();
```

Before the collision pass:

```rust
if obstacle.attachment_line_id.as_ref() == route.line_id.as_ref()
    && obstacle.attachment_line_id.is_some()
{
    continue;
}
```

Build penetration exemptions from attachment metadata before the collision pass:

```rust
let penetration_pairs = document
    .scene
    .nodes()
    .iter()
    .filter_map(|node| {
        (node.metadata.get("role").map(String::as_str) == Some("pipe-penetration"))
            .then(|| {
                Some((
                    node.metadata.get("lineId")?.clone(),
                    node.metadata.get("hostNodeId")?.clone(),
                ))
            })
            .flatten()
    })
    .collect::<BTreeSet<_>>();
```

Before broad-phase collision for a route:

```rust
if route
    .line_id
    .as_ref()
    .is_some_and(|line_id| {
        penetration_pairs.contains(&(line_id.clone(), obstacle.source_id.to_string()))
    })
{
    continue;
}
```

This exemption is line/host specific. The attachment gate validates the crossing; an invalid
or missing sleeve therefore still fails the complete `validate_document` result.

- [ ] **Step 4: Add attachment limits**

In `route_attachment.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteAttachmentLimits {
    pub max_attachments: usize,
    pub max_ports: usize,
    pub max_penetration_candidates: usize,
    pub max_triangle_pairs: usize,
    pub max_diagnostics_per_attachment: usize,
}

impl Default for RouteAttachmentLimits {
    fn default() -> Self {
        Self {
            max_attachments: MAX_ROUTE_ATTACHMENTS,
            max_ports: MAX_ATTACHMENT_PORTS,
            max_penetration_candidates: MAX_PENETRATION_CANDIDATES,
            max_triangle_pairs: MAX_ATTACHMENT_TRIANGLE_PAIRS,
            max_diagnostics_per_attachment: MAX_ATTACHMENT_DIAGNOSTICS,
        }
    }
}

pub(crate) fn validate_route_attachments_with_limits(
    document: &Document,
    definitions: &BTreeMap<ComponentId, &ComponentDefinition>,
    limits: RouteAttachmentLimits,
) -> Vec<Diagnostic> {
    let mut diagnostics = validate_route_attachments_with_budget(
        document,
        definitions,
        limits,
    );
    diagnostics.sort_by(|left, right| {
        left.object_id
            .cmp(&right.object_id)
            .then_with(|| left.pointer.cmp(&right.pointer))
            .then_with(|| left.code.as_str().cmp(right.code.as_str()))
    });
    diagnostics
}
```

The existing unlimited public entry uses `RouteAttachmentLimits::default()`.

- [ ] **Step 5: Run focused tests**

```bash
cargo test -p studio-core --test route_attachments fitting_
cargo test -p studio-core --test route_attachments attachment_budget
cargo test -p studio-core --test route_safety
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/studio-core/src/large_scene/route_safety.rs \
  crates/studio-core/src/large_scene/route_attachment.rs \
  crates/studio-core/tests/route_attachments.rs
git commit -m "fix(core): isolate fitting obstacle checks"
```

---

### Task 7: Add Asset Fitting Components

**Files:**
- Create: `scripts/wastewater-plant/pipe-attachments.mjs`
- Modify: `scripts/wastewater-plant/components.mjs`
- Modify: `scripts/wastewater-plant/site-plan.mjs:35-55, 680-710`
- Test: `scripts/tests/wastewater-plant-model.test.mjs`

**Interfaces:**
- Consumes: `createBuilder().define`, `b.part`, `b.port`, parameter expressions, component materials.
- Produces: `definePipeAttachmentComponents(b)`; component keys
  `pipe-tee-<medium>`, `pipe-reducer-<medium>`, `wall-sleeve-<medium>`.

- [ ] **Step 1: Write failing component-contract tests**

Add to `scripts/tests/wastewater-plant-model.test.mjs`:

```js
test('pipe attachment components expose independent fitting ports', () => {
  const plan = buildWastewaterPlan('wastewater-plant');
  const definitions = plan.operations
    .filter(({ type }) => type === 'component.define')
    .map(({ definition }) => definition);
  const tee = definitions.find(({ name }) => name.startsWith('Pipe tee ('));
  const reducer = definitions.find(({ name }) => name.startsWith('Pipe reducer ('));
  const sleeve = definitions.find(({ name }) => name.startsWith('Wall sleeve ('));

  assert.deepEqual(tee.ports.map(({ name }) => name).sort(),
    ['branch', 'run-in', 'run-out']);
  assert.deepEqual(reducer.ports.map(({ name }) => name).sort(),
    ['large', 'small']);
  assert.deepEqual(sleeve.ports.map(({ name }) => name).sort(),
    ['wall-in', 'wall-out']);
  assert.ok(tee.parts.length >= 4);
  assert.ok(reducer.parts.length >= 3);
  assert.ok(sleeve.parts.length >= 2);
});
```

- [ ] **Step 2: Run and verify missing definitions**

```bash
node --test scripts/tests/wastewater-plant-model.test.mjs
```

Expected: FAIL because attachment definitions are absent.

- [ ] **Step 3: Implement the reusable component geometry**

Create `pipe-attachments.mjs`:

```js
import { LIE_ALONG_X, box, cylinder } from './model-kit.mjs';

const number = (name, defaultValue) => ({
  name,
  valueType: { kind: 'number', minimum: 0.01, maximum: 100 },
  default: defaultValue,
  unit: 'm',
});

function teeParts(b, prefix) {
  return [
    b.part(`${prefix}-run`, b.cylinder(
      { expr: 'runDiameter / 2' },
      { expr: 'runLength' },
      24,
    ), 0, 0, 0, 'pipe-process', { rotation: LIE_ALONG_X }),
    b.part(`${prefix}-branch`, b.cylinder(
      { expr: 'branchDiameter / 2' },
      { expr: 'branchLength' },
      24,
    ), 0, { expr: 'branchLength / 2' }, 0, 'pipe-process'),
    b.part(`${prefix}-collar`, b.cylinder(
      { expr: 'branchDiameter * 0.72' },
      0.12,
      24,
    ), 0, { expr: 'branchLength * 0.82' }, 0, 'steel-dark'),
    b.part(`${prefix}-flange`, b.cylinder(
      { expr: 'runDiameter * 0.68' },
      0.08,
      24,
    ), { expr: '-runLength / 2' }, 0, 0, 'steel',
    { rotation: LIE_ALONG_X }),
  ];
}

function reducerParts(b, prefix) {
  return [
    b.part(`${prefix}-large-flange`, b.cylinder(
      { expr: 'largeDiameter * 0.62' }, 0.10, 24,
    ), { expr: '-length / 2' }, 0, 0, 'steel',
    { rotation: LIE_ALONG_X }),
    b.part(`${prefix}-shell`, b.cylinder(
      { expr: '(largeDiameter + smallDiameter) / 4' },
      { expr: 'length' },
      24,
    ), 0, 0, 0, 'pipe-process', { rotation: LIE_ALONG_X }),
    b.part(`${prefix}-small-flange`, b.cylinder(
      { expr: 'smallDiameter * 0.72' }, 0.10, 24,
    ), { expr: 'length / 2' }, 0, 0, 'steel',
    { rotation: LIE_ALONG_X }),
  ];
}

function sleeveParts(b, prefix) {
  const cut = {
    kind: 'cylinder',
    radius: { expr: 'pipeDiameter / 2' },
    height: { expr: 'wallThickness + extension * 2 + 0.02' },
    segments: 24,
    axis: 'y',
  };
  return [
    b.part(`${prefix}-wall-a`, b.cylinder(
      { expr: 'sleeveDiameter / 2' },
      { expr: 'wallThickness / 2 + extension' },
      24,
    ), { expr: '-(wallThickness + extension * 2) / 4' }, 0, 0,
    'steel-dark', { rotation: LIE_ALONG_X, subtract: [cut] }),
    b.part(`${prefix}-wall-b`, b.cylinder(
      { expr: 'sleeveDiameter / 2' },
      { expr: 'wallThickness / 2 + extension' },
      24,
    ), { expr: '(wallThickness + extension * 2) / 4' }, 0, 0,
    'steel-dark', { rotation: LIE_ALONG_X, subtract: [cut] }),
  ];
}

export function definePipeAttachmentComponents(b, media) {
  for (const [index, medium] of media.entries()) {
    const suffix = medium.replaceAll(/[^a-z0-9]+/g, '-');
    const pipeMaterial = medium === 'biogas' || medium === 'air'
      ? 'pipe-gas'
      : medium.includes('sludge') ? 'pipe-sludge' : 'pipe-process';
    const runDirection = { x: -1, y: 0, z: 0 };
    b.define(`pipe-tee-${suffix}`, `Pipe tee (${medium})`,
      teeParts(b, `pte-${index}`).map((part) => ({
        ...part,
        role: part.role === 'pipe-process' ? pipeMaterial : part.role,
      })), [
        b.port('run-in', `pte-${index}-run`,
          { expr: '-runLength / 2' }, 0, 0, runDirection, medium,
          { expr: 'runDiameter' }),
        b.port('run-out', `pte-${index}-run`,
          { expr: 'runLength / 2' }, 0, 0, { x: 1, y: 0, z: 0 }, medium,
          { expr: 'runDiameter' }),
        b.port('branch', `pte-${index}-branch`, 0,
          { expr: 'branchLength' }, 0, { x: 0, y: 1, z: 0 }, medium,
          { expr: 'branchDiameter' }),
      ], [
        number('runDiameter', 0.4),
        number('branchDiameter', 0.3),
        number('runLength', 2.0),
        number('branchLength', 1.2),
      ]);
    b.define(`pipe-reducer-${suffix}`, `Pipe reducer (${medium})`,
      reducerParts(b, `prd-${index}`).map((part) => ({
        ...part,
        role: part.role === 'pipe-process' ? pipeMaterial : part.role,
      })), [
        b.port('large', `prd-${index}-large-flange`,
          { expr: '-length / 2' }, 0, 0, runDirection, medium,
          { expr: 'largeDiameter' }),
        b.port('small', `prd-${index}-small-flange`,
          { expr: 'length / 2' }, 0, 0, { x: 1, y: 0, z: 0 }, medium,
          { expr: 'smallDiameter' }),
      ], [
        number('largeDiameter', 0.4),
        number('smallDiameter', 0.3),
        number('length', 1.2),
      ]);
    b.define(`wall-sleeve-${suffix}`, `Wall sleeve (${medium})`,
      sleeveParts(b, `wsl-${index}`), [
        b.port('wall-in', `wsl-${index}-wall-a`,
          { expr: '-(wallThickness + extension * 2) / 2' }, 0, 0,
          runDirection, medium, { expr: 'sleeveDiameter' }),
        b.port('wall-out', `wsl-${index}-wall-b`,
          { expr: '(wallThickness + extension * 2) / 2' }, 0, 0,
          { x: 1, y: 0, z: 0 }, medium, { expr: 'sleeveDiameter' }),
      ], [
        number('pipeDiameter', 0.3),
        number('sleeveDiameter', 0.5),
        number('wallThickness', 0.45),
        number('extension', 0.12),
      ]);
  }
}
```

The cylinder and its cut both use local Y before the `LIE_ALONG_X` part transform.
Do not add a solid inner cylinder.

- [ ] **Step 4: Register components in the wastewater plan**

In `site-plan.mjs`, import and call:

```js
import { definePipeAttachmentComponents } from './pipe-attachments.mjs';

definePipeAttachmentComponents(b, [
  'screened-water',
  'lifted-water',
  'settled-water',
  'mixed-liquor',
  'secondary-effluent',
  'return-sludge',
  'waste-sludge',
  'primary-sludge',
  'thickened-sludge',
  'digested-sludge',
  'biogas',
  'air',
  'chemical',
]);
```

- [ ] **Step 5: Run JavaScript tests**

```bash
node --test scripts/tests/wastewater-plant-model.test.mjs
node scripts/check-docs.mjs
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add scripts/wastewater-plant/pipe-attachments.mjs \
  scripts/wastewater-plant/site-plan.mjs \
  scripts/tests/wastewater-plant-model.test.mjs
git commit -m "feat(asset): add pipe attachment components"
```

---

### Task 8: Migrate Wastewater Plant and Validate Attachments

**Files:**
- Create: `scripts/wastewater-plant/pipe-attachments.mjs`
- Modify: `scripts/wastewater-plant/site-plan.mjs:830-1100`
- Modify: `scripts/wastewater-plant/validate.mjs:281-430, 670-770`
- Modify: `scripts/tests/wastewater-plant-model.test.mjs`

**Interfaces:**
- Consumes: Registered attachment component keys and `b.instance`, `b.connect`.
- Produces: `placeTee`, `placeReducer`, `placeSleeve`; asset validation
  `validatePipeFittingTopology(plan)` and migration counters.

- [ ] **Step 1: Write failing migration tests**

Add:

```js
test('wastewater plan contains executable fittings and real host sleeves', () => {
  const plan = buildWastewaterPlan('wastewater-plant');
  const fittings = plan.operations.filter(({ instance }) => {
    const metadata = instance?.metadata;
    return metadata?.role === 'pipe-fitting';
  });
  const sleeves = plan.operations.filter(({ instance }) => {
    const metadata = instance?.metadata;
    return metadata?.role === 'pipe-penetration';
  });
  const nodeIds = new Set(plan.operations
    .filter(({ type }) => type === 'node.create' || type === 'instance.create')
    .map(({ node_id }) => node_id));

  assert.ok(fittings.some(({ instance }) => instance.metadata.fittingKind === 'tee'));
  assert.ok(fittings.some(({ instance }) => instance.metadata.fittingKind === 'reducer'));
  assert.ok(sleeves.length > 0);
  for (const sleeve of sleeves) {
    assert.ok(nodeIds.has(sleeve.instance.metadata.hostNodeId));
  }
});

test('every tee has one connection on each functional port', () => {
  const plan = buildWastewaterPlan('wastewater-plant');
  const result = validatePipeFittingTopology(plan);
  assert.deepEqual(result, { ok: true, issues: [] });
});
```

Import `validatePipeFittingTopology` in the test file.

- [ ] **Step 2: Run and verify failure**

```bash
node --test scripts/tests/wastewater-plant-model.test.mjs
```

Expected: no fittings or sleeves exist yet.

- [ ] **Step 3: Add asset placement helpers**

Add to `pipe-attachments.mjs`:

```js
export function placeTee(b, {
  key,
  name,
  x,
  y,
  z,
  yaw = 0,
  medium,
  lineId,
  parameters,
  upstream,
  downstream,
  branch,
}) {
  const nodeId = b.instance(
    `pipe-tee-${medium}`,
    key,
    name,
    x,
    y,
    z,
    {
      role: 'pipe-fitting',
      fittingKind: 'tee',
      lineId,
      stage: upstream.metadata.stage,
      routeObstacle: 'solid',
    },
    parameters,
    yaw,
  );
  b.connect(upstream.key, upstream.port, key, 'run-in',
    `${name} 主管入口`, medium, parameters.runDiameter, { lineId });
  b.connect(key, 'run-out', downstream.key, downstream.port,
    `${name} 主管出口`, medium, parameters.runDiameter, { lineId });
  b.connect(key, 'branch', branch.key, branch.port,
    `${name} 支管`, medium, parameters.branchDiameter, { lineId });
  return nodeId;
}

export function placeReducer(b, {
  key,
  name,
  x,
  y,
  z,
  yaw = 0,
  medium,
  lineId,
  parameters,
  upstream,
  downstream,
}) {
  const nodeId = b.instance(
    `pipe-reducer-${medium}`,
    key,
    name,
    x,
    y,
    z,
    {
      role: 'pipe-fitting',
      fittingKind: 'reducer',
      lineId,
      stage: upstream.metadata.stage,
      routeObstacle: 'solid',
    },
    parameters,
    yaw,
  );
  b.connect(upstream.key, upstream.port, key, 'large',
    `${name} 大端入口`, medium, parameters.largeDiameter, { lineId });
  b.connect(key, 'small', downstream.key, downstream.port,
    `${name} 小端出口`, medium, parameters.smallDiameter, { lineId });
  return nodeId;
}

export function placeSleeve(b, {
  key,
  name,
  x,
  y,
  z,
  yaw = 0,
  medium,
  lineId,
  hostNodeId,
  parameters,
  stage,
}) {
  return b.instance(
    `wall-sleeve-${medium}`,
    key,
    name,
    x,
    y,
    z,
    {
      role: 'pipe-penetration',
      fittingKind: 'sleeve',
      lineId,
      hostNodeId,
      stage,
      routeObstacle: 'solid',
    },
    parameters,
    yaw,
  );
}
```

The actual asset names may use Chinese display text, but metadata values and component keys
must remain stable ASCII identifiers.

- [ ] **Step 4: Migrate a tee into the gas header**

Replace the two direct digester-to-collector connections with a real tee:

```js
const gasTeeLineId = 'L-GAS-HEADER';
placeTee(b, {
  key: 'tee-gas-1',
  name: 'GAS-TEE-101 沼气汇流三通',
  x: 340,
  y: 3.0,
  z: -125,
  medium: 'biogas',
  lineId: gasTeeLineId,
  parameters: {
    runDiameter: 0.35,
    branchDiameter: 0.35,
    runLength: 2.4,
    branchLength: 1.4,
  },
  upstream: { key: 'dg-1', port: 'gas-out', metadata: { stage: 'sludge' } },
  downstream: { key: 'gcl-1', port: 'inlet-a', metadata: { stage: 'sludge' } },
  branch: { key: 'dg-2', port: 'gas-out', metadata: { stage: 'sludge' } },
});
```

Keep the remaining collector-to-holder and holder-to-flare connections unchanged.

- [ ] **Step 5: Migrate a reducer into a single existing route**

Split one sludge route around a real diameter transition:

```js
placeReducer(b, {
  key: 'reducer-waste-a',
  name: 'RED-101 剩余污泥异径管',
  x: 404,
  y: 1.0,
  z: -140,
  medium: 'waste-sludge',
  lineId: 'L-WASTE-REDUCER-A',
  parameters: { largeDiameter: 0.35, smallDiameter: 0.3, length: 1.2 },
  upstream: { key: 'rws-1', port: 'waste-out-a', metadata: { stage: 'secondary' } },
  downstream: { key: 'th-1', port: 'inlet', metadata: { stage: 'sludge' } },
});
```

Remove the old direct `rws-1/waste-out-a -> th-1/inlet` connection.

- [ ] **Step 6: Add sleeves at real host crossings**

Add a real bulkhead definition before the connection block:

```js
b.define('pipe-bulkhead', 'Pipe penetration wall', [
  b.part('wall', b.box(0.45, 4.0, 3.0), 0, 2.0, 0, 'concrete'),
], []);
```

Give the first return-sludge header an explicit stable ID by changing its existing connection:

```js
const returnHeaderLineId = b.connect(
  'rws-1',
  'outlet-a',
  'ae-a2',
  'return-inlet',
  'RWS-101 → AE-102 回流污泥',
  'return-sludge',
  0.3,
  { lineId: 'L-RAS-HEADER-101' },
);
```

Place at least two sleeves at explicit bulkheads:

```js
b.instance('pipe-bulkhead', 'bulkhead-gas', 'GAS-WALL-101 沼气穿越墙',
  370, 0, -125, { stage: 'sludge', tag: 'GAS-WALL-101' });
placeSleeve(b, {
  key: 'sleeve-gas-house',
  name: 'SLV-101 沼气主管穿墙套管',
  x: 370,
  y: 3.0,
  z: -125,
  medium: 'biogas',
  lineId: gasHeaderLineId,
  hostNodeId: b.state.nodes.get('bulkhead-gas'),
  parameters: {
    pipeDiameter: 0.35,
    sleeveDiameter: 0.55,
    wallThickness: 0.45,
    extension: 0.12,
  },
  stage: 'sludge',
});

b.instance('pipe-bulkhead', 'bulkhead-return', 'RAS-WALL-101 回流污泥穿越墙',
  365, 0, 0, { stage: 'secondary', tag: 'RAS-WALL-101' }, {}, Math.PI / 2);
placeSleeve(b, {
  key: 'sleeve-return-basin',
  name: 'SLV-102 回流污泥穿池壁套管',
  x: 365,
  y: 1.0,
  z: 0,
  medium: 'return-sludge',
  lineId: returnHeaderLineId,
  hostNodeId: b.state.nodes.get('bulkhead-return'),
  parameters: {
    pipeDiameter: 0.3,
    sleeveDiameter: 0.5,
    wallThickness: 0.45,
    extension: 0.10,
  },
  stage: 'secondary',
});
```

Use the bulkhead instance node IDs returned by `b.state.nodes.get(...)` as
`hostNodeId`; those IDs are the real hosts checked by Core.

- [ ] **Step 7: Add asset-level topology validation**

In `validate.mjs`:

```js
export function validatePipeFittingTopology(plan) {
  const issues = [];
  const connections = plan.operations.filter(({ type }) => type === 'connection.create');
  const fittings = plan.operations.filter(({ type, instance }) => (
    type === 'instance.create'
    && instance?.metadata?.role === 'pipe-fitting'
  ));
  for (const fitting of fittings) {
    const lineId = fitting.instance.metadata.lineId;
    const linked = connections.filter(({ connection }) => (
      connection.metadata?.lineId === lineId
      && [connection.from, connection.to].some(({ nodeId }) => nodeId === fitting.node_id)
    ));
    const kind = fitting.instance.metadata.fittingKind;
    const expected = kind === 'tee'
      ? ['run-in', 'run-out', 'branch']
      : ['large', 'small'];
    const used = new Set(linked.flatMap(({ connection }) => (
      [connection.from, connection.to]
        .filter(({ nodeId }) => nodeId === fitting.node_id)
        .map(({ port }) => port)
    )));
    for (const port of expected) {
      if (!used.has(port)) {
        issues.push({
          code: 'PIPE_FITTING_PORT_MISSING',
          objectId: fitting.node_id,
          message: `${fitting.name} requires ${port}`,
        });
      }
    }
  }
  return issues.length === 0 ? { ok: true, issues } : { ok: false, issues };
}
```

Call it from `validateWastewaterPlan`.

- [ ] **Step 8: Run asset tests and route benchmark**

```bash
node --test scripts/tests/wastewater-plant-model.test.mjs
npm run asset:capabilities
npm run benchmark:route-safety
```

Expected: all pass; route-safety p95 limits remain `150 ms` and `250 ms`.

- [ ] **Step 9: Commit**

```bash
git add scripts/wastewater-plant/pipe-attachments.mjs \
  scripts/wastewater-plant/site-plan.mjs \
  scripts/wastewater-plant/validate.mjs \
  scripts/tests/wastewater-plant-model.test.mjs
git commit -m "feat(asset): migrate wastewater pipe fittings"
```

---

### Task 9: Chrome Review, Docs, and Final Gate

**Files:**
- Modify: `scripts/wastewater-visual-review.mjs`
- Modify: `scripts/tests/wastewater-plant-model.test.mjs`
- Modify: `docs/specs/2026-11-21-pipe-fitting-attachments-design.md`
- Modify: `docs/specs/2026-11-01-wastewater-process-piping-correction-design.md`
- Modify: `docs/README.md`
- Modify: `docs/reference/diagnostics.md`
- Modify: `docs/reference/capabilities.md`
- Modify: `projects/README.md`

- [ ] **Step 1: Add fitting close-up review views**

Add to `REVIEW_VIEWS`:

```js
Object.freeze({
  name: 'pipe-fittings',
  purpose: '泵组三通、异径管和法兰接口近景',
  target: Object.freeze([62, 2.0, 10]),
  orientation: Object.freeze([35, 18]),
  distance: 22,
}),
Object.freeze({
  name: 'wall-penetration',
  purpose: '管廊和池壁穿墙套管近景',
  target: Object.freeze([400, 2.2, -120]),
  orientation: Object.freeze([35, 20]),
  distance: 26,
}),
```

Add a test that both URLs preserve exact target, view and token.

- [ ] **Step 2: Rebuild the project through the public API**

Run:

```bash
cargo run --release -p studio-cli -- serve \
  --workspace . \
  --project wastewater-plant \
  --port 8150 \
  --write-mode autonomous
```

Expected: server reports the latest rebuilt revision and no validation errors.

- [ ] **Step 3: Open the fitting views in Google Chrome**

Run:

```bash
npm run asset:visual-review
```

Open the returned `pipe-fittings` and `wall-penetration` URLs in the installed Google
Chrome application. Do not use headless browser capture.

Expected visual checks:

- tee run and branch connections have no gaps;
- reducer transitions from larger to smaller diameter;
- sleeve surrounds the pipe without blocking it;
- no floating fittings, black faces, unexpected shadows or resource errors;
- no road clearance or collision diagnostics.

- [ ] **Step 4: Update documentation status**

Change `SPEC-035` and `PLAN-032` to `Implemented` only after all gates pass.
Update `SPEC-013` only when its remaining sleeve, tee and reducer items are complete.
Update the index rows, Diagnostics, capabilities and wastewater project guide.

- [ ] **Step 5: Run complete verification**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
npm run benchmark:route-safety
npm run verify
```

Expected:

- no format or lint errors;
- all Rust tests pass;
- route-safety p95 remains within `150 ms` / `250 ms`;
- all repository verification gates pass;
- Viewer E2E passes;
- dependency audit reports zero vulnerabilities.

- [ ] **Step 6: Commit final acceptance and documentation**

```bash
git add scripts/wastewater-visual-review.mjs \
  scripts/tests/wastewater-plant-model.test.mjs \
  docs projects/README.md
git commit -m "test(route): verify pipe fitting attachments"
```

- [ ] **Step 7: Final branch review and merge**

Review the full branch diff against `main`, verify author identity is
`damoqiongqiu <damoqiongqiu@126.com>`, then merge locally with `--no-ff`.
Do not push unless the user explicitly asks.

---

## Completion Checklist

- [ ] Component Port bindings resolve from instance parameters.
- [ ] Attachment metadata and four stable Diagnostic codes are documented.
- [ ] Tee requires three distinct connected ports and valid geometry.
- [ ] Reducer supports different diameters without weakening connection matching.
- [ ] Sleeve requires a real host triangle crossing and aligned surface ports.
- [ ] Self-line attachment collision is suppressed; unrelated-line collision remains.
- [ ] Attachment and penetration budgets fail closed.
- [ ] Reusable asset components are registered.
- [ ] Wastewater plant contains real tees, reducers and sleeves.
- [ ] Asset topology validation covers every fitting.
- [ ] Google Chrome close-up review passes.
- [ ] `npm run benchmark:route-safety` remains within limits.
- [ ] `npm run verify` passes.
- [ ] `SPEC-035`, `PLAN-032` and `SPEC-013` status match delivered behavior.
