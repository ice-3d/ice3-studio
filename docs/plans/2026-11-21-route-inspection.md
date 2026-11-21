# 连接与管线附件检视实施计划

> Document ID: ICE3-STUDIO-PLAN-033
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers
>
> **For agentic workers:** REQUIRED SUB-SKILL: Use executing-plans. This repository
> defaults to Native execution. Do not spawn or delegate to sub-agents unless the
> user explicitly authorizes them. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** 交付只读 `route.inspect` 与 Viewer Route Inspector，让 Agent 和用户能按
`lineId` 查看同源的分段路由、解析端口、附件、宿主和支撑事实。

**Architecture:** Core 新增 bounded route inspection，复用正式
`resolve_port`/`route_connection`；Server 暴露 revision-bound read method；Viewer 通过
独立的 RouteInspectionStore 消费公开 API，不解析组件表达式或重建路径。

**Tech Stack:** Rust 2024、serde/schemars/ts-rs、JSON-RPC 2.0、React 19、TypeScript、
Node.js ESM、Google Chrome。

**Spec:** [ICE3-STUDIO-SPEC-036](../specs/2026-11-21-route-inspection-design.md)

## Global Constraints

- Default execution is Native in the current session; no sub-agents without explicit user permission.
- Develop on `feat/route-inspection`, not directly on `main`.
- Browser verification uses the installed Google Chrome only; never use a headless browser,
  Chrome for Testing, or another browser.
- `route.inspect` is read-only and never creates revisions, cache entries, files, previews,
  ChangeSets, or candidate state.
- Viewer consumes only the public API and must not resolve component expressions or duplicate route math.
- `lineId` must be non-empty and at most 256 characters.
- Optional request `revision` must equal the committed Document revision.
- Response limits are 128 segments, 4,096 total route points, 128 attachments and 512 supports.
- Result ordering is stable by node ID; truncation must be explicit for every bounded collection.
- API version becomes `1.5`; schema method count becomes 42 and runtime method count becomes 44.
- Existing `document.get`, `scene.search`, `component.get`, route safety and attachment behavior
  remain compatible.
- Git author and committer must be `damoqiongqiu <damoqiongqiu@126.com>`.
- No competitor, vendor, or third-party product names in code, comments, docs or commits.

## Review Focus

1. A response for an old selection arrives after a new selection; stale data must not replace the
   current Route Inspector.
2. A selected support node has no direct `lineId`; its `supportLineId` must resolve through the
   current Document's connection metadata.
3. A split hydraulic main has more points than the global budget; the response must show a partial
   route with `truncated.points=true`.
4. A route has no segments or a stale expected revision; the method must return stable errors while
   the ordinary Inspect form remains usable.
5. A line has unsorted attachments/supports and unrelated same-role nodes; only matching line facts
   must be returned in stable node-ID order.

---

## File Structure

| File | Responsibility |
|---|---|
| `crates/studio-core/src/large_scene/route_inspection.rs` | Public inspection types, route facts, aggregation and budgets |
| `crates/studio-core/src/large_scene/mod.rs` | Export route inspection module |
| `crates/studio-core/tests/route_inspection.rs` | Core resolution, aggregation, revision and truncation tests |
| `crates/studio-core/src/lib.rs` | API version `1.5` |
| `crates/studio-server/src/rpc.rs` | `route.inspect` dispatch and capability advertisement |
| `crates/studio-server/src/agent.rs` | Agent tool metadata and inspect workflow |
| `crates/studio-server/tests/rpc_contract.rs` | Public RPC, version and schema assertions |
| `crates/studio-cli/src/schema.rs` | Method and type schema registration |
| `apps/viewer/src/api/StudioClient.ts` | Route inspection wire types and method |
| `apps/viewer/src/route/routeInspectionModel.ts` | Selection-to-line identity and request-key helpers |
| `apps/viewer/src/route/RouteInspectionStore.ts` | Bounded async state and stale-response rejection |
| `apps/viewer/src/route/RouteInspectionProvider.tsx` | React context for the store |
| `apps/viewer/src/route/RouteInspector.tsx` | Read-only route/attachment/host/support panel |
| `apps/viewer/src/main.tsx` | Store construction and provider wiring |
| `apps/viewer/src/app/Workbench.tsx` | Selection/revision wiring and related-node selection |
| `apps/viewer/src/app/studioTokens.css` | Route Inspector layout and truncation/error styles |
| `apps/viewer/tests/route-inspection.test.ts` | Line identity, stale response and state tests |
| `apps/viewer/e2e/route-inspection.spec.ts` | Chrome-compatible Route Inspector E2E |
| `docs/reference/api.md` | Method contract |
| `docs/reference/capabilities.md` | API version, method count and route inspection behavior |
| `docs/README.md` | Spec and plan status/index |
| `projects/README.md` | Attachment inspection workflow |

---

### Task 1: Core Route Inspection Happy Path

**Files:**
- Create: `crates/studio-core/src/large_scene/route_inspection.rs`
- Modify: `crates/studio-core/src/large_scene/mod.rs`
- Create: `crates/studio-core/tests/route_inspection.rs`

**Interfaces:**
- Consumes: `Document`, `ComponentDefinition`, `resolve_port`, `route_connection`.
- Produces: `RouteInspectionParams`, `RouteInspection`, `RouteInspectionSegment`,
  `RouteInspectionPort`, `RouteInspectionLimits`, `RouteInspectionError` and
  `inspect_route(document, definitions, line_id, expected_revision, limits)`.

- [x] **Step 1: Write the failing happy-path Core test**

Create `crates/studio-core/tests/route_inspection.rs`:

```rust
mod support;

use std::collections::BTreeMap;

use studio_core::large_scene::route_inspection::{
    RouteInspectionLimits, inspect_route,
};
use studio_core::ids::Revision;

fn definitions(
    document: &studio_core::document::Document,
) -> BTreeMap<studio_core::ids::ComponentId, &studio_core::large_scene::component::ComponentDefinition> {
    document
        .components
        .iter()
        .map(|definition| (definition.id.clone(), definition))
        .collect()
}

#[test]
fn inspects_the_formal_route_and_resolved_endpoints() {
    let mut document = support::route_collision_document();
    document.revision = Revision::new(1);
    let result = inspect_route(
        &document,
        &definitions(&document),
        "L-001",
        Some(1),
        RouteInspectionLimits::default(),
    )
    .unwrap();

    assert_eq!(result.revision, 1);
    assert_eq!(result.line_id, "L-001");
    assert_eq!(result.segments.len(), 1);
    let segment = &result.segments[0];
    assert_eq!(segment.node_id, "node-000003");
    assert_eq!(segment.from.port, "outlet");
    assert_eq!(segment.to.port, "inlet");
    assert_eq!(segment.from.position.x, 5.0);
    assert_eq!(segment.to.position.x, 7.0);
    assert_eq!(segment.points.first(), Some(&segment.from.position_array()));
    assert_eq!(segment.points.last(), Some(&segment.to.position_array()));
    assert!(!result.truncated.segments);
    assert!(!result.truncated.points);
}
```

- [x] **Step 2: Run the test and verify RED**

```bash
CARGO_TARGET_DIR=/Users/felix/Windows-E-workspace/felix/ice3/ice3-studio/target \
  cargo test -p studio-core --test route_inspection
```

Expected: FAIL because `large_scene::route_inspection` does not exist.

- [x] **Step 3: Implement the public types and happy path**

Create `route_inspection.rs` with:

```rust
use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::Serialize;
use ts_rs::TS;

use crate::document::{Document, NodeContent};
use crate::ids::{ComponentId, NodeId, Revision};
use crate::large_scene::component::ComponentDefinition;
use crate::large_scene::connection::{InstancePortRef, RoutePolicy};
use crate::large_scene::route::{ConnectionError, resolve_port, route_connection};
use crate::math::Vec3;

pub const MAX_INSPECTION_SEGMENTS: usize = 128;
pub const MAX_INSPECTION_POINTS: usize = 4_096;
pub const MAX_INSPECTION_ATTACHMENTS: usize = 128;
pub const MAX_INSPECTION_SUPPORTS: usize = 512;
pub const MAX_LINE_ID_CHARS: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct RouteInspectionParams {
    pub line_id: String,
    pub revision: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteInspectionLimits {
    pub max_segments: usize,
    pub max_points: usize,
    pub max_attachments: usize,
    pub max_supports: usize,
}

impl Default for RouteInspectionLimits {
    fn default() -> Self {
        Self {
            max_segments: MAX_INSPECTION_SEGMENTS,
            max_points: MAX_INSPECTION_POINTS,
            max_attachments: MAX_INSPECTION_ATTACHMENTS,
            max_supports: MAX_INSPECTION_SUPPORTS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RouteInspectionPort {
    pub node_id: String,
    pub port: String,
    pub medium: String,
    pub diameter: f32,
    pub position: Vec3,
    pub direction: Vec3,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RouteInspectionSegment {
    pub node_id: String,
    pub name: String,
    pub medium: String,
    pub diameter: f32,
    pub route_policy: RoutePolicy,
    pub from: RouteInspectionPort,
    pub to: RouteInspectionPort,
    pub points: Vec<[f32; 3]>,
}

impl RouteInspectionSegment {
    pub fn from_position_array(&self) -> [f32; 3] {
        [self.from.position.x, self.from.position.y, self.from.position.z]
    }

    pub fn to_position_array(&self) -> [f32; 3] {
        [self.to.position.x, self.to.position.y, self.to.position.z]
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RouteInspectionTruncation {
    pub segments: bool,
    pub points: bool,
    pub attachments: bool,
    pub supports: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RouteInspection {
    pub revision: u64,
    pub line_id: String,
    pub support_line_ids: Vec<String>,
    pub segments: Vec<RouteInspectionSegment>,
    pub attachments: Vec<RouteInspectionAttachment>,
    pub supports: Vec<RouteInspectionSupport>,
    pub truncated: RouteInspectionTruncation,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RouteInspectionAttachment {
    pub node_id: String,
    pub name: String,
    pub role: String,
    pub fitting_kind: String,
    pub line_id: String,
    pub support_line_id: Option<String>,
    pub host_node_id: Option<String>,
    pub host_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RouteInspectionSupport {
    pub node_id: String,
    pub name: String,
    pub support_line_id: String,
    pub support_group: Option<String>,
    pub position: Vec3,
}

#[derive(Debug, thiserror::Error)]
pub enum RouteInspectionError {
    #[error("route inspection revision conflict: expected {expected}, actual {actual}")]
    RevisionConflict { expected: u64, actual: u64 },
    #[error("route line {line_id} was not found")]
    LineNotFound { line_id: String },
    #[error("route line id is invalid")]
    InvalidLineId,
    #[error("route {node_id} cannot be resolved")]
    Route {
        node_id: NodeId,
        #[source]
        source: ConnectionError,
    },
}

pub fn inspect_route(
    document: &Document,
    definitions: &BTreeMap<ComponentId, &ComponentDefinition>,
    line_id: &str,
    expected_revision: Option<u64>,
    limits: RouteInspectionLimits,
) -> Result<RouteInspection, RouteInspectionError> {
    if line_id.trim().is_empty() || line_id.chars().count() > MAX_LINE_ID_CHARS {
        return Err(RouteInspectionError::InvalidLineId);
    }
    let actual = document.revision.as_u64();
    if let Some(expected) = expected_revision
        && expected != actual
    {
        return Err(RouteInspectionError::RevisionConflict { expected, actual });
    }
    let mut connections = document
        .scene
        .nodes()
        .iter()
        .filter(|node| {
            matches!(&node.content, NodeContent::Connection(connection)
                if connection.metadata.get("lineId").map(String::as_str) == Some(line_id))
        })
        .collect::<Vec<_>>();
    if connections.is_empty() {
        return Err(RouteInspectionError::LineNotFound {
            line_id: line_id.to_string(),
        });
    }
    connections.sort_by(|left, right| left.id.as_str().cmp(right.id.as_str()));
    let segments_truncated = connections.len() > limits.max_segments;
    let mut points_left = limits.max_points;
    let mut points_truncated = false;
    let mut segments = Vec::new();
    for node in connections.into_iter().take(limits.max_segments) {
        let NodeContent::Connection(connection) = &node.content else {
            unreachable!()
        };
        let from = resolve_port(document, definitions, &connection.from)
            .map_err(|source| RouteInspectionError::Route {
                node_id: node.id.clone(),
                source,
            })?;
        let to = resolve_port(document, definitions, &connection.to)
            .map_err(|source| RouteInspectionError::Route {
                node_id: node.id.clone(),
                source,
            })?;
        let mut points = route_connection(document, definitions, connection)
            .map_err(|source| RouteInspectionError::Route {
                node_id: node.id.clone(),
                source,
            })?;
        if points.len() > points_left {
            points.truncate(points_left);
            points_truncated = true;
        }
        points_left = points_left.saturating_sub(points.len());
        segments.push(RouteInspectionSegment {
            node_id: node.id.to_string(),
            name: node.name.clone(),
            medium: connection.medium.clone(),
            diameter: connection.diameter,
            route_policy: connection.route_policy.clone(),
            from: RouteInspectionPort {
                node_id: connection_owner(&connection.from),
                port: endpoint_port(&connection.from),
                medium: from.medium,
                diameter: from.diameter,
                position: from.position,
                direction: from.direction,
            },
            to: RouteInspectionPort {
                node_id: connection_owner(&connection.to),
                port: endpoint_port(&connection.to),
                medium: to.medium,
                diameter: to.diameter,
                position: to.position,
                direction: to.direction,
            },
            points,
        });
    }
    Ok(RouteInspection {
        revision: actual,
        line_id: line_id.to_string(),
        support_line_ids: Vec::new(),
        segments,
        attachments: Vec::new(),
        supports: Vec::new(),
        truncated: RouteInspectionTruncation {
            segments: segments_truncated,
            points: points_truncated,
            attachments: false,
            supports: false,
        },
    })
}

fn connection_owner(reference: &InstancePortRef) -> String {
    match reference {
        InstancePortRef::Instance { node_id, .. } => node_id.to_string(),
        InstancePortRef::SetItem { set_node_id, .. } => set_node_id.to_string(),
    }
}

fn endpoint_port(reference: &InstancePortRef) -> String {
    match reference {
        InstancePortRef::Instance { port, .. } | InstancePortRef::SetItem { port, .. } => {
            port.clone()
        }
    }
}
```

Add to `large_scene/mod.rs`:

```rust
pub mod route_inspection;
```

- [x] **Step 4: Run the test and verify GREEN**

```bash
CARGO_TARGET_DIR=/Users/felix/Windows-E-workspace/felix/ice3/ice3-studio/target \
  cargo test -p studio-core --test route_inspection
```

Expected: PASS.

- [x] **Step 5: Commit**

```bash
git add crates/studio-core/src/large_scene/mod.rs \
  crates/studio-core/src/large_scene/route_inspection.rs \
  crates/studio-core/tests/route_inspection.rs
git commit -m "feat(core): inspect connection route facts"
```

---

### Task 2: Core Attachment, Host, Support and Budget Aggregation

**Files:**
- Modify: `crates/studio-core/src/large_scene/route_inspection.rs`
- Modify: `crates/studio-core/tests/route_inspection.rs`

**Interfaces:**
- Consumes: Task 1 `inspect_route` output and stable node metadata.
- Produces: populated `support_line_ids`, `attachments`, `supports` and all truncation flags.

- [x] **Step 1: Write failing aggregation and failure tests**

Append helpers and tests to `route_inspection.rs` test:

```rust
use studio_core::document::{NodeContent, SceneNode};
use studio_core::math::Transform;

fn add_inspection_nodes(document: &mut studio_core::document::Document) {
    let fitting = SceneNode {
        id: studio_core::ids::NodeId::from("node-000009"),
        name: "Reducer".into(),
        parent_id: None,
        transform: Transform::IDENTITY,
        material_id: None,
        content: NodeContent::Group,
        metadata: BTreeMap::from([
            ("role".into(), "pipe-fitting".into()),
            ("fittingKind".into(), "reducer".into()),
            ("lineId".into(), "L-001".into()),
        ]),
        visible: true,
    };
    let host = SceneNode {
        id: studio_core::ids::NodeId::from("node-000008"),
        name: "Wall".into(),
        parent_id: None,
        transform: Transform::IDENTITY,
        material_id: None,
        content: NodeContent::Group,
        metadata: BTreeMap::new(),
        visible: true,
    };
    let mut sleeve = fitting.clone();
    sleeve.id = studio_core::ids::NodeId::from("node-000010");
    sleeve.name = "Sleeve".into();
    sleeve.metadata = BTreeMap::from([
        ("role".into(), "pipe-penetration".into()),
        ("fittingKind".into(), "sleeve".into()),
        ("lineId".into(), "L-001".into()),
        ("hostNodeId".into(), "node-000008".into()),
    ]);
    let support_a = support_node("node-000012", "S-A", "L-001", 12.0);
    let support_b = support_node("node-000011", "S-B", "L-001", 6.0);
    let unrelated = support_node("node-000013", "S-X", "L-OTHER", 8.0);
    document.scene.nodes_mut().extend([
        host, fitting, sleeve, support_a, support_b, unrelated,
    ]);
    if let Some(connection) = document.scene.nodes_mut().iter_mut().find_map(|node| {
        let NodeContent::Connection(connection) = &mut node.content else {
            return None;
        };
        Some(connection)
    }) {
        connection
            .metadata
            .insert("supportLineId".into(), "L-SUPPORT".into());
    }
}

fn support_node(id: &str, name: &str, line: &str, x: f32) -> SceneNode {
    SceneNode {
        id: studio_core::ids::NodeId::from(id),
        name: name.into(),
        parent_id: None,
        transform: Transform {
            translation: studio_core::math::Vec3 { x, y: 0.0, z: 0.0 },
            ..Transform::IDENTITY
        },
        material_id: None,
        content: NodeContent::Group,
        metadata: BTreeMap::from([
            ("role".into(), "pipe-support".into()),
            ("supportLineId".into(), line.into()),
            ("supportGroup".into(), format!("{line}-group")),
        ]),
        visible: true,
    }
}

#[test]
fn aggregates_only_matching_line_facts_in_stable_order() {
    let mut document = support::route_collision_document();
    add_inspection_nodes(&mut document);
    let result = inspect_route(
        &document,
        &definitions(&document),
        "L-001",
        Some(1),
        RouteInspectionLimits::default(),
    )
    .unwrap();

    assert_eq!(result.support_line_ids, vec!["L-SUPPORT"]);
    assert_eq!(result.attachments.iter().map(|item| item.node_id.as_str())
        .collect::<Vec<_>>(), vec!["node-000009", "node-000010"]);
    assert_eq!(result.attachments[1].host_name.as_deref(), Some("Wall"));
    assert_eq!(result.supports.iter().map(|item| item.node_id.as_str())
        .collect::<Vec<_>>(), vec!["node-000011", "node-000012"]);
}

#[test]
fn reports_revision_and_missing_line_errors() {
    let document = support::route_collision_document();
    assert!(matches!(
        inspect_route(
            &document,
            &definitions(&document),
            "L-001",
            Some(99),
            RouteInspectionLimits::default(),
        ),
        Err(studio_core::large_scene::route_inspection::RouteInspectionError::RevisionConflict { .. })
    ));
    assert!(matches!(
        inspect_route(
            &document,
            &definitions(&document),
            "L-MISSING",
            None,
            RouteInspectionLimits::default(),
        ),
        Err(studio_core::large_scene::route_inspection::RouteInspectionError::LineNotFound { .. })
    ));
}

#[test]
fn marks_every_budget_when_results_are_truncated() {
    let mut document = support::route_collision_document();
    add_inspection_nodes(&mut document);
    let result = inspect_route(
        &document,
        &definitions(&document),
        "L-001",
        None,
        RouteInspectionLimits {
            max_segments: 1,
            max_points: 1,
            max_attachments: 1,
            max_supports: 1,
        },
    )
    .unwrap();
    assert!(result.truncated.points);
    assert!(result.truncated.attachments);
    assert!(result.truncated.supports);
    assert_eq!(result.attachments.len(), 1);
    assert_eq!(result.supports.len(), 1);
}
```

- [x] **Step 2: Run tests and verify RED**

```bash
CARGO_TARGET_DIR=/Users/felix/Windows-E-workspace/felix/ice3/ice3-studio/target \
  cargo test -p studio-core --test route_inspection
```

Expected: FAIL because support/attachment collections are always empty and truncation flags are false.

- [x] **Step 3: Implement aggregation**

After segment collection:

```rust
let support_line_ids = segments
    .iter()
    .filter_map(|segment| {
        let node = document.scene.nodes().iter().find(|node| node.id.as_str() == segment.node_id)?;
        let NodeContent::Connection(connection) = &node.content else {
            return None;
        };
        connection
            .metadata
            .get("supportLineId")
            .filter(|value| !value.is_empty())
            .cloned()
            .or_else(|| connection.metadata.get("lineId").cloned())
    })
    .collect::<std::collections::BTreeSet<_>>()
    .into_iter()
    .collect::<Vec<_>>();

let mut attachments = document
    .scene
    .nodes()
    .iter()
    .filter(|node| {
        node.metadata.get("lineId").map(String::as_str) == Some(line_id)
            && matches!(
                node.metadata.get("role").map(String::as_str),
                Some("pipe-fitting" | "pipe-penetration")
            )
            && node.metadata.contains_key("fittingKind")
    })
    .map(|node| {
        let host_node_id = node.metadata.get("hostNodeId").cloned();
        let host_name = host_node_id.as_ref().and_then(|host_id| {
            document
                .scene
                .nodes()
                .iter()
                .find(|host| host.id.as_str() == host_id)
                .map(|host| host.name.clone())
        });
        RouteInspectionAttachment {
            node_id: node.id.to_string(),
            name: node.name.clone(),
            role: node.metadata["role"].clone(),
            fitting_kind: node.metadata["fittingKind"].clone(),
            line_id: line_id.to_string(),
            support_line_id: node
                .metadata
                .get("supportLineId")
                .filter(|value| !value.is_empty())
                .cloned(),
            host_node_id,
            host_name,
        }
    })
    .collect::<Vec<_>>();
attachments.sort_by(|left, right| left.node_id.cmp(&right.node_id));
let attachments_truncated = attachments.len() > limits.max_attachments;
attachments.truncate(limits.max_attachments);

let supported_lines = support_line_ids
    .iter()
    .map(String::as_str)
    .collect::<std::collections::BTreeSet<_>>();
let mut supports = document
    .scene
    .nodes()
    .iter()
    .filter_map(|node| {
        let support_line_id = node
            .metadata
            .get("supportLineId")
            .filter(|value| !value.is_empty())
            .or_else(|| node.metadata.get("lineId").filter(|value| !value.is_empty()))?;
        if node.metadata.get("role").map(String::as_str) != Some("pipe-support")
            || !supported_lines.contains(support_line_id.as_str())
        {
            return None;
        }
        Some(RouteInspectionSupport {
            node_id: node.id.to_string(),
            name: node.name.clone(),
            support_line_id: support_line_id.clone(),
            support_group: node.metadata.get("supportGroup").cloned(),
            position: node.transform.translation,
        })
    })
    .collect::<Vec<_>>();
supports.sort_by(|left, right| left.node_id.cmp(&right.node_id));
let supports_truncated = supports.len() > limits.max_supports;
supports.truncate(limits.max_supports);
```

Populate the response with these collections and set `attachments`/`supports` truncation flags.
Skip nodes missing `fittingKind`, matching the public inspection contract.

- [x] **Step 4: Run Core tests**

```bash
CARGO_TARGET_DIR=/Users/felix/Windows-E-workspace/felix/ice3/ice3-studio/target \
  cargo test -p studio-core --test route_inspection
```

Expected: PASS.

- [x] **Step 5: Commit**

```bash
git add crates/studio-core/src/large_scene/route_inspection.rs \
  crates/studio-core/tests/route_inspection.rs
git commit -m "feat(core): aggregate route inspection facts"
```

---

### Task 3: Public RPC, Agent Manifest and API 1.5

**Files:**
- Modify: `crates/studio-core/src/lib.rs`
- Modify: `crates/studio-core/tests/workspace_contract.rs`
- Modify: `crates/studio-server/src/rpc.rs`
- Modify: `crates/studio-server/src/agent.rs`
- Modify: `crates/studio-server/tests/rpc_contract.rs`
- Modify: `crates/studio-cli/src/schema.rs`

**Interfaces:**
- Consumes: Task 1/2 `RouteInspectionParams` and `inspect_route`.
- Produces: JSON-RPC `route.inspect`, API `1.5`, schema/binding/manifest registration.

- [x] **Step 1: Write failing public-contract tests**

Add to `rpc_contract.rs`:

```rust
#[tokio::test]
async fn route_inspection_is_a_read_only_public_method_with_stable_errors() {
    let server = start_test_server(ServerConfig::for_tests("route-inspection-rpc")).await;
    let capabilities = common::rpc(&server, "system.capabilities", serde_json::json!({})).await;
    assert_eq!(capabilities["result"]["apiVersion"], "1.5");
    assert!(capabilities["result"]["methods"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!("route.inspect")));

    let manifest = common::rpc(&server, "system.agentManifest", serde_json::json!({})).await;
    assert!(manifest["result"]["tools"].as_array().unwrap().iter().any(|tool| {
        tool["name"] == "route.inspect"
            && tool["sideEffect"] == "read"
            && tool["requiresApproval"] == false
    }));

    let missing = common::rpc(
        &server,
        "route.inspect",
        serde_json::json!({ "lineId": "L-MISSING" }),
    )
    .await;
    assert_eq!(
        missing["error"]["diagnosticCode"],
        "ICE3_STUDIO_REFERENCE_INVALID"
    );

    let stale = common::rpc(
        &server,
        "route.inspect",
        serde_json::json!({ "lineId": "L-MISSING", "revision": 99 }),
    )
    .await;
    assert_eq!(
        stale["error"]["diagnosticCode"],
        "ICE3_STUDIO_REVISION_CONFLICT"
    );
}
```

Change existing version assertions from `1.4` to `1.5` in `rpc_contract.rs` and
`workspace_contract.rs`.

- [x] **Step 2: Run tests and verify RED**

```bash
CARGO_TARGET_DIR=/Users/felix/Windows-E-workspace/felix/ice3/ice3-studio/target \
  cargo test -p studio-server --test rpc_contract route_inspection_is_a_read_only_public_method_with_stable_errors
```

Expected: FAIL because `route.inspect` is unknown and API is still `1.4`.

- [x] **Step 3: Implement dispatch and manifest**

Set:

```rust
pub const API_VERSION: &str = "1.5";
```

In `rpc.rs` capability methods add `"route.inspect"` after `"component.get"`.

Add dispatch branch:

```rust
"route.inspect" => {
    let params: studio_core::large_scene::route_inspection::RouteInspectionParams =
        serde_json::from_value(request.params.clone()).map_err(invalid_params)?;
    let definitions = session
        .document
        .components
        .iter()
        .map(|definition| (definition.id.clone(), definition))
        .collect::<BTreeMap<_, _>>();
    let result = studio_core::large_scene::route_inspection::inspect_route(
        &session.document,
        &definitions,
        &params.line_id,
        params.revision,
        Default::default(),
    )
    .map_err(route_inspection_error)?;
    serde_json::to_value(result).map_err(serialize_error)
}
```

Add:

```rust
fn route_inspection_error(
    error: studio_core::large_scene::route_inspection::RouteInspectionError,
) -> RpcError {
    use studio_core::large_scene::route_inspection::RouteInspectionError;
    let code = match &error {
        RouteInspectionError::RevisionConflict { .. } => DiagnosticCode::RevisionConflict,
        RouteInspectionError::LineNotFound { .. } | RouteInspectionError::InvalidLineId => {
            DiagnosticCode::ReferenceInvalid
        }
        RouteInspectionError::Route { .. } => DiagnosticCode::ConnectionInvalid,
    };
    RpcError::with_diagnostic(
        -32010,
        code.as_str(),
        format!("{}: {error}", code.as_str()),
        None,
    )
}
```

In `agent.rs` add:

```rust
tool(
    "route.inspect",
    "Inspect resolved route segments, ports, fittings, hosts, and supports for one line.",
    AgentSideEffect::Read,
    false,
    AgentIdempotency::None,
    access.can_read,
    "read access is unavailable",
    "RouteInspectionParams",
    Some("RouteInspection"),
),
```

Add `"route.inspect"` to the `inspect` workflow after `"component.get|evaluation.query"`.

In `schema.rs`:

- add `"route.inspect"` to methods after `"component.get"`;
- import all route inspection public types;
- add `RouteInspectionParams`, `RouteInspection`, `RouteInspectionSegment`,
  `RouteInspectionPort`, `RouteInspectionAttachment`, `RouteInspectionSupport` and
  `RouteInspectionTruncation` to `types`.

- [x] **Step 4: Generate bindings and verify GREEN**

```bash
cargo test -p studio-core --lib export_bindings_route
cargo run -p studio-cli -- schema --output schemas
cargo run -p studio-cli -- schema --output schemas --check
CARGO_TARGET_DIR=/Users/felix/Windows-E-workspace/felix/ice3/ice3-studio/target \
  cargo test -p studio-server --test rpc_contract
```

Expected: PASS; generated bindings and schemas contain `route.inspect`, its request/response types,
and API version `1.5`.

- [x] **Step 5: Commit**

```bash
git add crates/studio-core/src/lib.rs crates/studio-server/src/rpc.rs \
  crates/studio-server/src/agent.rs crates/studio-server/tests/rpc_contract.rs \
  crates/studio-core/tests/workspace_contract.rs \
  crates/studio-cli/src/schema.rs crates/*/bindings schemas
git commit -m "feat(api): expose route inspection"
```

---

### Task 4: Viewer Route State and Selection Identity

**Files:**
- Modify: `apps/viewer/src/api/StudioClient.ts`
- Create: `apps/viewer/src/route/routeInspectionModel.ts`
- Create: `apps/viewer/src/route/RouteInspectionStore.ts`
- Create: `apps/viewer/src/route/RouteInspectionProvider.tsx`
- Modify: `apps/viewer/src/main.tsx`
- Create: `apps/viewer/tests/route-inspection.test.ts`

**Interfaces:**
- Consumes: Task 3 wire types and `route.inspect`.
- Produces: `StudioRouteInspection`, `StudioClient.routeInspection`,
  `routeLineIdForNode`, `RouteInspectionStore`, `useRouteInspection`,
  `useRouteInspectionStore`.

- [x] **Step 1: Write failing Viewer state tests**

Create `apps/viewer/tests/route-inspection.test.ts`:

```ts
import assert from 'node:assert/strict';
import test from 'node:test';
import type { StudioDocument, StudioNode, StudioRouteInspection } from '../src/api/StudioClient';
import { routeLineIdForNode } from '../src/route/routeInspectionModel';
import { RouteInspectionStore } from '../src/route/RouteInspectionStore';

function inspection(lineId: string): StudioRouteInspection {
  return {
    revision: 2,
    lineId,
    supportLineIds: [],
    segments: [],
    attachments: [],
    supports: [],
    truncated: { segments: false, points: false, attachments: false, supports: false },
  };
}

test('selection resolves direct, connection, and support line identities', () => {
  const fitting = {
    id: 'node-000001',
    name: 'Fitting',
    parentId: null,
    metadata: { lineId: 'L-FIT' },
    content: {},
  } satisfies StudioNode;
  const connection = {
    id: 'node-000002',
    name: 'Segment',
    parentId: null,
    metadata: {},
    content: { type: 'connection', metadata: { lineId: 'L-CONN', supportLineId: 'L-SEG' } },
  } satisfies StudioNode;
  const support = {
    id: 'node-000003',
    name: 'Support',
    parentId: null,
    metadata: { supportLineId: 'L-SEG' },
    content: {},
  } satisfies StudioNode;
  const document = {
    projectId: 'route',
    revision: 2,
    scene: { nodes: [fitting, connection, support] },
    components: [],
  } satisfies StudioDocument;

  assert.equal(routeLineIdForNode(fitting, document), 'L-FIT');
  assert.equal(routeLineIdForNode(connection, document), 'L-CONN');
  assert.equal(routeLineIdForNode(support, document), 'L-CONN');
});

test('a stale route response cannot replace the current selection', async () => {
  const pending = new Map<string, (value: StudioRouteInspection) => void>();
  const store = new RouteInspectionStore({
    routeInspection(lineId) {
      return new Promise((resolve) => pending.set(lineId, resolve));
    },
  });
  const first = store.load('L-A', 2);
  const second = store.load('L-B', 2);
  pending.get('L-B')!(inspection('L-B'));
  await second;
  pending.get('L-A')!(inspection('L-A'));
  await first;
  assert.equal(store.snapshot.lineId, 'L-B');
  assert.equal(store.snapshot.inspection?.lineId, 'L-B');
});

test('route failures leave the store in a retryable error state', async () => {
  const store = new RouteInspectionStore({
    async routeInspection() {
      throw new Error('route unavailable');
    },
  });
  await store.load('L-A', 2);
  assert.equal(store.snapshot.status, 'error');
  assert.match(store.snapshot.error ?? '', /route unavailable/);
});
```

- [x] **Step 2: Run tests and verify RED**

```bash
npm --prefix apps/viewer test
```

Expected: FAIL because the route model and store do not exist.

- [x] **Step 3: Implement wire types, model and store**

Add to `StudioClient.ts`:

```ts
export type StudioRoutePolicy =
  | { kind: 'direct' }
  | { kind: 'orthogonal' }
  | { kind: 'buried'; depth: number }
  | { kind: 'rack'; elevation: number };

export type StudioRouteInspectionPort = {
  nodeId: string;
  port: string;
  medium: string;
  diameter: number;
  position: StudioVec3;
  direction: StudioVec3;
};

export type StudioRouteInspectionSegment = {
  nodeId: string;
  name: string;
  medium: string;
  diameter: number;
  routePolicy: StudioRoutePolicy;
  from: StudioRouteInspectionPort;
  to: StudioRouteInspectionPort;
  points: Array<[number, number, number]>;
};

export type StudioRouteInspectionAttachment = {
  nodeId: string;
  name: string;
  role: string;
  fittingKind: string;
  lineId: string;
  supportLineId: string | null;
  hostNodeId: string | null;
  hostName: string | null;
};

export type StudioRouteInspectionSupport = {
  nodeId: string;
  name: string;
  supportLineId: string;
  supportGroup: string | null;
  position: StudioVec3;
};

export type StudioRouteInspection = {
  revision: number;
  lineId: string;
  supportLineIds: string[];
  segments: StudioRouteInspectionSegment[];
  attachments: StudioRouteInspectionAttachment[];
  supports: StudioRouteInspectionSupport[];
  truncated: {
    segments: boolean;
    points: boolean;
    attachments: boolean;
    supports: boolean;
  };
};

routeInspection(lineId: string, revision: number): Promise<StudioRouteInspection> {
  return this.rpc('route.inspect', { lineId, revision });
}
```

Create `routeInspectionModel.ts`:

```ts
import type { StudioDocument, StudioNode } from '../api/StudioClient';

function metadataOf(node: StudioNode): Record<string, string> {
  const contentMetadata = node.content?.metadata;
  return {
    ...(typeof contentMetadata === 'object' && contentMetadata ? contentMetadata : {}),
    ...node.metadata,
  } as Record<string, string>;
}

export function routeLineIdForNode(
  node: StudioNode | undefined,
  document: StudioDocument | null,
): string | null {
  if (!node) return null;
  const metadata = metadataOf(node);
  if (typeof metadata.lineId === 'string' && metadata.lineId) return metadata.lineId;
  if (typeof metadata.supportLineId === 'string' && metadata.supportLineId && document) {
    const connection = document.scene.nodes.find((candidate) => {
      const candidateMetadata = metadataOf(candidate);
      return candidate.content?.type === 'connection'
        && (
          candidateMetadata.supportLineId === metadata.supportLineId
          || candidateMetadata.lineId === metadata.supportLineId
        );
    });
    const lineId = connection && metadataOf(connection).lineId;
    return typeof lineId === 'string' && lineId ? lineId : null;
  }
  return null;
}
```

Create `RouteInspectionStore.ts` with an `ExternalStore` snapshot:

```ts
import type { StudioRouteInspection } from '../api/StudioClient';
import { ExternalStore } from '../shared/ExternalStore';

type Source = {
  routeInspection(lineId: string, revision: number): Promise<StudioRouteInspection>;
};

export type RouteInspectionSnapshot = {
  key: string | null;
  lineId: string | null;
  revision: number;
  status: 'idle' | 'loading' | 'ready' | 'error';
  inspection: StudioRouteInspection | null;
  error: string | null;
};

export class RouteInspectionStore extends ExternalStore<RouteInspectionSnapshot> {
  private generation = 0;

  constructor(private readonly source: Source) {
    super({
      key: null,
      lineId: null,
      revision: 0,
      status: 'idle',
      inspection: null,
      error: null,
    });
  }

  async load(lineId: string, revision: number): Promise<void> {
    const key = `${revision}\0${lineId}`;
    const generation = ++this.generation;
    const sameSelection = this.snapshot.key === key;
    this.update({
      key,
      lineId,
      revision,
      status: sameSelection && this.snapshot.status === 'ready' ? 'ready' : 'loading',
      inspection: sameSelection ? this.snapshot.inspection : null,
      error: null,
    });
    try {
      const inspection = await this.source.routeInspection(lineId, revision);
      if (generation !== this.generation) return;
      this.update({ key, lineId, revision, status: 'ready', inspection, error: null });
    } catch (cause) {
      if (generation !== this.generation) return;
      this.update({
        key,
        lineId,
        revision,
        status: 'error',
        inspection: null,
        error: cause instanceof Error ? cause.message : String(cause),
      });
    }
  }

  clear(): void {
    this.generation += 1;
    this.update({
      key: null,
      lineId: null,
      revision: 0,
      status: 'idle',
      inspection: null,
      error: null,
    });
  }
}
```

Create `RouteInspectionProvider.tsx`:

```tsx
import { createContext, useContext, useSyncExternalStore, type ReactNode } from 'react';
import type {
  RouteInspectionSnapshot,
  RouteInspectionStore,
} from './RouteInspectionStore';

const RouteInspectionContext = createContext<RouteInspectionStore | null>(null);

export function RouteInspectionProvider({
  store,
  children,
}: {
  store: RouteInspectionStore;
  children: ReactNode;
}) {
  return (
    <RouteInspectionContext.Provider value={store}>
      {children}
    </RouteInspectionContext.Provider>
  );
}

export function useRouteInspectionStore(): RouteInspectionStore {
  const store = useContext(RouteInspectionContext);
  if (!store) throw new Error('RouteInspectionProvider is missing');
  return store;
}

export function useRouteInspection(): RouteInspectionSnapshot {
  const store = useRouteInspectionStore();
  return useSyncExternalStore(store.subscribe, () => store.snapshot);
}
```

- [x] **Step 4: Wire the provider and verify GREEN**

In `main.tsx`:

```tsx
const routeInspection = new RouteInspectionStore(client);

<SessionProvider store={session}>
  <RouteInspectionProvider store={routeInspection}>
    <ChangeSetProvider store={changeSet}>
      <Workbench />
    </ChangeSetProvider>
  </RouteInspectionProvider>
</SessionProvider>
```

Run:

```bash
npm --prefix apps/viewer test
npm --prefix apps/viewer run typecheck
```

Expected: PASS.

- [x] **Step 5: Commit**

```bash
git add apps/viewer/src/api/StudioClient.ts apps/viewer/src/route \
  apps/viewer/src/main.tsx apps/viewer/tests/route-inspection.test.ts
git commit -m "feat(viewer): load route inspection state"
```

---

### Task 5: Viewer Route Inspector UI

**Files:**
- Create: `apps/viewer/src/route/RouteInspector.tsx`
- Modify: `apps/viewer/src/app/Workbench.tsx`
- Modify: `apps/viewer/src/app/studioTokens.css`

**Interfaces:**
- Consumes: Task 4 store, model and wire types.
- Produces: accessible Route region with partial/error states and related-node selection.

- [x] **Step 1: Add a failing component-level E2E expectation**

In `apps/viewer/e2e/route-inspection.spec.ts`, add:

```ts
import { expect, test } from '@playwright/test';

test('inspects a selected fitting route without leaving Inspect', async ({ page }) => {
  await page.route('**/rpc', async (route) => {
    const request = route.request().postDataJSON() as { method?: string; params?: unknown };
    if (request.method === 'document.get') {
      const response = await route.fetch();
      const body = await response.json();
      body.result.scene.nodes.push({
        id: 'node-fitting',
        name: 'Route reducer',
        parentId: null,
        metadata: {
          role: 'pipe-fitting',
          fittingKind: 'reducer',
          lineId: 'L-E2E',
        },
        content: { type: 'group' },
        transform: {
          translation: { x: 0, y: 0, z: 0 },
          rotation: [0, 0, 0, 1],
          scale: { x: 1, y: 1, z: 1 },
        },
        visible: true,
      });
      await route.fulfill({ response, json: body });
      return;
    }
    if (request.method === 'route.inspect') {
      await route.fulfill({
        json: {
          result: {
            revision: 2,
            lineId: 'L-E2E',
            supportLineIds: ['L-E2E'],
            segments: [{
              nodeId: 'node-segment',
              name: 'Main line',
              medium: 'water',
              diameter: 0.2,
              routePolicy: { kind: 'direct' },
              from: {
                nodeId: 'node-fitting',
                port: 'small',
                medium: 'water',
                diameter: 0.2,
                position: { x: 0, y: 1, z: 0 },
                direction: { x: 1, y: 0, z: 0 },
              },
              to: {
                nodeId: 'node-target',
                port: 'inlet',
                medium: 'water',
                diameter: 0.2,
                position: { x: 10, y: 1, z: 0 },
                direction: { x: -1, y: 0, z: 0 },
              },
              points: [[0, 1, 0], [10, 1, 0]],
            }],
            attachments: [{
              nodeId: 'node-fitting',
              name: 'Route reducer',
              role: 'pipe-fitting',
              fittingKind: 'reducer',
              lineId: 'L-E2E',
              supportLineId: null,
              hostNodeId: null,
              hostName: null,
            }],
            supports: [],
            truncated: {
              segments: false,
              points: false,
              attachments: false,
              supports: false,
            },
          },
        },
      });
      return;
    }
    await route.continue();
  });

  await page.goto('/');
  await page.getByPlaceholder('Search objects').fill('Route reducer');
  await page.getByRole('button', { name: 'Route reducer' }).click();
  await page.getByRole('button', { name: 'Inspect selected object' }).click();
  const inspector = page.getByRole('region', { name: 'Route inspection' });
  await expect(inspector).toContainText('L-E2E');
  await expect(inspector).toContainText('small');
  await expect(inspector).toContainText('0.200 m');
});
```

- [x] **Step 2: Run E2E and verify RED**

```bash
npm --prefix apps/viewer run test:e2e -- route-inspection.spec.ts
```

Expected: FAIL because the Route inspection region does not exist.

- [x] **Step 3: Implement `RouteInspector` and Workbench wiring**

Create `RouteInspector.tsx`:

```tsx
import { useEffect } from 'react';
import type { StudioDocument, StudioNode } from '../api/StudioClient';
import { useRouteInspection, useRouteInspectionStore } from './RouteInspectionProvider';
import { routeLineIdForNode } from './routeInspectionModel';

type Props = {
  node: StudioNode | undefined;
  document: StudioDocument | null;
  revision: number;
  onSelect(nodeId: string): void;
};

export function RouteInspector({ node, document, revision, onSelect }: Props) {
  const store = useRouteInspectionStore();
  const state = useRouteInspection();
  const lineId = routeLineIdForNode(node, document);

  useEffect(() => {
    if (!lineId || !document) {
      store.clear();
      return;
    }
    void store.load(lineId, revision);
  }, [document, lineId, revision, store]);

  if (!lineId) return null;
  return (
    <section aria-label="Route inspection" className="route-inspector">
      <div className="route-inspector__heading">
        <div>
          <span className="panel-kicker">Route</span>
          <h3>{lineId}</h3>
        </div>
        <span className="route-inspector__revision">Revision {revision}</span>
      </div>
      {state.status === 'loading' && <p role="status">Loading route facts…</p>}
      {state.status === 'error' && (
        <div className="route-inspector__error" role="alert">
          <span>{state.error}</span>
          <button
            type="button"
            className="button button--secondary"
            onClick={() => void store.load(lineId, revision)}
          >
            Retry
          </button>
        </div>
      )}
      {state.status === 'ready' && state.inspection && (
        <>
          {(state.inspection.truncated.segments
            || state.inspection.truncated.points
            || state.inspection.truncated.attachments
            || state.inspection.truncated.supports) && (
            <p className="route-inspector__warning">
              Partial route: response budget reached.
            </p>
          )}
          <section aria-labelledby="route-segments-heading">
            <h4 id="route-segments-heading">
              Segments · {state.inspection.segments.length}
            </h4>
            {state.inspection.segments.map((segment) => (
              <article className="route-inspector__segment" key={segment.nodeId}>
                <button
                  type="button"
                  className="route-inspector__link"
                  onClick={() => onSelect(segment.nodeId)}
                >
                  {segment.name}
                </button>
                <span>{segment.medium} · {segment.diameter.toFixed(3)} m</span>
                <span>{segment.routePolicy.kind} · {segment.points.length} points</span>
                <PortFact label="From" port={segment.from} onSelect={onSelect} />
                <PortFact label="To" port={segment.to} onSelect={onSelect} />
              </article>
            ))}
          </section>
          <RelatedFacts title="Attachments" items={state.inspection.attachments}
            onSelect={onSelect} />
          <RelatedFacts title="Supports" items={state.inspection.supports}
            onSelect={onSelect} />
        </>
      )}
    </section>
  );
}
```

Add the internal renderers:

```tsx
function PortFact({
  label,
  port,
  onSelect,
}: {
  label: string;
  port: StudioRouteInspectionPort;
  onSelect(nodeId: string): void;
}) {
  return (
    <div className="route-inspector__fact">
      <span>{label}</span>
      <button
        type="button"
        className="route-inspector__link"
        onClick={() => onSelect(port.nodeId)}
      >
        {port.port}
      </button>
      <span>{port.diameter.toFixed(3)} m</span>
      <code>
        {port.position.x.toFixed(2)}, {port.position.y.toFixed(2)}, {port.position.z.toFixed(2)}
      </code>
      <code>
        {port.direction.x.toFixed(2)}, {port.direction.y.toFixed(2)}, {port.direction.z.toFixed(2)}
      </code>
    </div>
  );
}

function RelatedFacts({
  title,
  items,
  onSelect,
}: {
  title: string;
  items: Array<{
    nodeId: string;
    name: string;
    fittingKind?: string;
    hostName?: string | null;
    supportGroup?: string | null;
  }>;
  onSelect(nodeId: string): void;
}) {
  return (
    <section>
      <h4>{title} · {items.length}</h4>
      {items.map((item) => (
        <button
          type="button"
          className="route-inspector__related"
          key={item.nodeId}
          onClick={() => onSelect(item.nodeId)}
        >
          <span>{item.name}</span>
          <small>
            {[item.fittingKind, item.hostName, item.supportGroup, item.nodeId]
              .filter(Boolean)
              .join(' · ')}
          </small>
        </button>
      ))}
    </section>
  );
}
```

Import `StudioRouteInspectionPort` from `StudioClient`.

In `Workbench.tsx`, render:

```tsx
<RouteInspector
  node={selected}
  document={session.document}
  revision={session.revision}
  onSelect={(nodeId) => sessionStore.select([nodeId])}
/>
```

Place the Route Inspector immediately after `ComputePanel` and before `FormRenderer`.

- [x] **Step 4: Add styles and verify GREEN**

Add `.route-inspector`, `.route-inspector__heading`, `.route-inspector__segment`,
`.route-inspector__link`, `.route-inspector__warning` and `.route-inspector__error` styles to
`studioTokens.css`. Use existing tokens, 1px borders, `--panel-alt`, wrapped text and no fixed
height so long relation lists scroll inside the Review dock.

Run:

```bash
npm --prefix apps/viewer test
npm --prefix apps/viewer run typecheck
npm --prefix apps/viewer run build
npm --prefix apps/viewer run test:e2e -- route-inspection.spec.ts
```

Expected: PASS.

- [x] **Step 5: Commit**

```bash
git add apps/viewer/src/route/RouteInspector.tsx \
  apps/viewer/src/app/Workbench.tsx apps/viewer/src/app/studioTokens.css \
  apps/viewer/e2e/route-inspection.spec.ts
git commit -m "feat(viewer): inspect route attachments"
```

---

### Task 6: Public API and Agent Acceptance Coverage

**Files:**
- Modify: `crates/studio-server/tests/rpc_contract.rs`
- Modify: `docs/reference/api.md`
- Modify: `docs/reference/capabilities.md`

**Interfaces:**
- Consumes: Task 3 `route.inspect` and Task 5 Viewer E2E.
- Produces: API reference, Agent evaluation task, capability counts and public examples.

- [x] **Step 1: Add a public RPC fixture test**

Append to `rpc_contract.rs` a helper that applies a two-instance component route through the
public API:

```rust
async fn apply_route_fixture(server: &studio_server::TestServer) {
    let response = common::rpc_with_approval_cookie(
        server,
        "document.apply",
        serde_json::json!({
            "expectedRevision": 0,
            "dryRun": false,
            "idempotencyKey": "route-inspection-fixture",
            "operations": [
                    {
                        "type": "component.define",
                        "definition": {
                            "id": "component-route",
                            "name": "Route endpoint",
                            "parameters": [],
                            "parts": [{
                                "id": "body",
                                "parentId": null,
                                "transform": {},
                                "content": {
                                    "kind": "box",
                                    "width": 1,
                                    "height": 1,
                                    "depth": 1
                                },
                                "subtract": [],
                                "repeat": null,
                                "materialRole": null,
                                "metadata": {},
                                "visible": true
                            }],
                            "ports": [
                                {
                                    "name": "out",
                                    "partId": "body",
                                    "position": { "x": 0.5, "y": 0, "z": 0 },
                                    "direction": { "x": 1, "y": 0, "z": 0 },
                                    "medium": "water",
                                    "diameter": 0.2
                                },
                                {
                                    "name": "in",
                                    "partId": "body",
                                    "position": { "x": -0.5, "y": 0, "z": 0 },
                                    "direction": { "x": -1, "y": 0, "z": 0 },
                                    "medium": "water",
                                    "diameter": 0.2
                                }
                            ],
                            "metadata": {}
                        }
                    },
                    {
                        "type": "instance.create",
                        "node_id": "node-route-a",
                        "parent_id": null,
                        "name": "Route A",
                        "transform": {
                            "translation": { "x": 0, "y": 0, "z": 0 },
                            "rotation": [0, 0, 0, 1],
                            "scale": { "x": 1, "y": 1, "z": 1 }
                        },
                        "instance": {
                            "definitionId": "component-route",
                            "parameters": {},
                            "overrides": {},
                            "metadata": {}
                        },
                        "material_id": null,
                        "metadata": {}
                    },
                    {
                        "type": "instance.create",
                        "node_id": "node-route-b",
                        "parent_id": null,
                        "name": "Route B",
                        "transform": {
                            "translation": { "x": 5, "y": 0, "z": 0 },
                            "rotation": [0, 0, 0, 1],
                            "scale": { "x": 1, "y": 1, "z": 1 }
                        },
                        "instance": {
                            "definitionId": "component-route",
                            "parameters": {},
                            "overrides": {},
                            "metadata": {}
                        },
                        "material_id": null,
                        "metadata": {}
                    },
                    {
                        "type": "relation.create",
                        "relation": {
                            "id": "relation-route",
                            "relationType": "process",
                            "fromNodeId": "node-route-a",
                            "toNodeId": "node-route-b",
                            "metadata": {}
                        }
                    },
                    {
                        "type": "connection.create",
                        "node_id": "node-route-segment",
                        "parent_id": null,
                        "name": "Route segment",
                        "transform": {
                            "translation": { "x": 0, "y": 0, "z": 0 },
                            "rotation": [0, 0, 0, 1],
                            "scale": { "x": 1, "y": 1, "z": 1 }
                        },
                        "connection": {
                            "from": { "kind": "instance", "nodeId": "node-route-a", "port": "out" },
                            "to": { "kind": "instance", "nodeId": "node-route-b", "port": "in" },
                            "medium": "water",
                            "diameter": 0.2,
                            "routePolicy": { "kind": "direct" },
                            "relationId": "relation-route",
                            "metadata": {
                                "lineId": "L-RPC",
                                "supportLineId": "L-RPC"
                            }
                        },
                        "material_id": null
                    }
            ]
        }),
    )
    .await;
    assert_eq!(response["result"]["revision"], 1, "{response}");
}

#[tokio::test]
async fn route_inspection_returns_public_route_facts() {
    let server = start_test_server(ServerConfig::for_tests("route-inspection-rpc")).await;
    apply_route_fixture(&server).await;
    let body = common::rpc(
        &server,
        "route.inspect",
        serde_json::json!({ "lineId": "L-RPC", "revision": 1 }),
    )
    .await;
    assert_eq!(body["result"]["segments"][0]["nodeId"], "node-route-segment");
    assert_eq!(body["result"]["segments"][0]["from"]["nodeId"], "node-route-a");
    assert_eq!(body["result"]["segments"][0]["from"]["position"]["x"], 0.5);
}
```

- [x] **Step 2: Run API tests and verify RED/GREEN boundary**

Before updating docs and eval, run:

```bash
CARGO_TARGET_DIR=/Users/felix/Windows-E-workspace/felix/ice3/ice3-studio/target \
  cargo test -p studio-server --test rpc_contract route_inspection_returns_public_route_facts
```

Expected: PASS. This task adds public acceptance and documentation; the route behavior itself is
already implemented by Tasks 1-3.

- [x] **Step 3: Add Agent evaluation and docs**

Update `docs/reference/api.md` with:

- method table row for `route.inspect`;
- request/response example from SPEC-036;
- stable error mapping;
- limits and truncation semantics;
- API version `1.5`.

Update `docs/reference/capabilities.md` with:

- schema method count 42 and runtime method count 44;
- API 1.5 description;
- Route Inspection subsection describing Core reuse, Viewer behavior and limits.

- [x] **Step 4: Run docs/API verification**

```bash
node scripts/check-docs.mjs
node scripts/check-method-docs.mjs
CARGO_TARGET_DIR=/Users/felix/Windows-E-workspace/felix/ice3/ice3-studio/target \
  cargo test -p studio-server --test rpc_contract
```

Expected: PASS; method counts match.

- [x] **Step 5: Commit**

```bash
git add crates/studio-server/tests/rpc_contract.rs \
  docs/reference/api.md docs/reference/capabilities.md
git commit -m "test(api): accept route inspection"
```

---

### Task 7: Chrome Review, Documentation and Final Gate

**Files:**
- Modify: `docs/specs/2026-11-21-route-inspection-design.md`
- Modify: `docs/plans/2026-11-21-route-inspection.md`
- Modify: `docs/README.md`
- Modify: `projects/README.md`
- Modify: `README.md` if the current capability list mentions route inspection as absent.

**Interfaces:**
- Consumes: all prior tasks.
- Produces: Implemented status, current usage guidance and complete acceptance evidence.

- [x] **Step 1: Run the latest project through the public API**

Build the Viewer and rebuild the latest project:

```bash
npm --prefix apps/viewer run build
node scripts/build-wastewater-model.mjs --workspace projects
```

Start:

```bash
STUDIO_VIEWER_DIR="$PWD/apps/viewer/dist" \
STUDIO_SESSION_TOKEN=wastewater-review \
  target/release/studio-cli serve \
  --workspace projects --project wastewater-plant --port 8150 \
  --write-mode autonomous
```

Generate the preview once:

```bash
curl -fsS -X POST http://127.0.0.1:8150/rpc \
  -H 'authorization: Bearer wastewater-review' \
  -H 'content-type: application/json' \
  --data '{"jsonrpc":"2.0","id":"preview","method":"export.runtimePreview","params":{"revision":1}}'
```

- [x] **Step 2: Verify in installed Google Chrome**

Reuse the existing Google Chrome window/tab. Open the latest Viewer, select `RED-101 回流污泥异径管`
or `SLV-101 沼气主管穿墙套管`, open Inspect, and confirm:

- Route section displays `L-RAS-HEADER-101` or `L-GAS-HOLDER`;
- split segments and resolved port diameters appear;
- attachments, host and supports are shown;
- related-node buttons change Selection;
- Diagnostics remains empty;
- no blank canvas, black face, unexpected shadow or resource error.

If the selected object has no direct line identity, select its segment or support and verify the
Document lookup path.

- [x] **Step 3: Update documentation status**

Change SPEC-036 and PLAN-033 to `Implemented`. Update:

- `docs/README.md` index rows and statuses;
- `projects/README.md` with the Route Inspector workflow and current stats;
- README/capability references affected by new public method counts.

Resolve any stale counts by reading `npm run asset:capabilities` and current route-safety output.

- [x] **Step 4: Run the complete gate**

```bash
cargo fmt --all -- --check
CARGO_TARGET_DIR=/Users/felix/Windows-E-workspace/felix/ice3/ice3-studio/target \
  cargo clippy --workspace --all-targets --all-features -- -D warnings
CARGO_TARGET_DIR=/Users/felix/Windows-E-workspace/felix/ice3/ice3-studio/target \
  cargo test --workspace --all-features
npm run benchmark:route-safety
npm run verify
```

Expected:

- all Rust and Node tests pass;
- schemas and bindings are current;
- route-safety diagnostics remain zero and p95 remains below `150 ms`;
- Viewer typecheck, unit, build and E2E pass;
- dependency audit reports zero vulnerabilities.

- [x] **Step 5: Commit acceptance**

```bash
git add docs README.md projects scripts apps crates schemas
git commit -m "test(route): verify route inspection"
```

- [ ] **Step 6: Final review and local merge**

Review the complete branch against `main`, confirm every commit author and committer is
`damoqiongqiu <damoqiongqiu@126.com>`, then merge locally with `--no-ff`.
Do not push unless the user explicitly asks.

---

## Completion Checklist

- [x] `route.inspect` is read-only, bounded and revision-safe.
- [x] Split lines sharing one `lineId` return all segments.
- [x] Resolved ports use the formal Core route implementation.
- [x] Attachments, hosts, supportLineIds and supports are stable and correctly filtered.
- [x] Server capabilities, manifest, schema and bindings expose API 1.5.
- [x] Viewer resolves line identity without parsing expressions.
- [x] Stale route responses cannot overwrite a new selection.
- [x] Route Inspector shows loading, error, partial, segment, port, attachment, host and support facts.
- [x] Related-node controls use existing Selection only.
- [x] Google Chrome inspection passes on the latest project.
- [x] `SPEC-036` and `PLAN-033` status match delivered behavior.
- [x] `npm run benchmark:route-safety` and `npm run verify` pass.
