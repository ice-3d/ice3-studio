# Large-Scene Authoring Foundation Implementation Plan

> Document ID: ICE3-STUDIO-PLAN-006
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

> **For agentic workers:** REQUIRED SUB-SKILL: Use `executing-plans` to implement this plan
> task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add generic parameterized components, component instances, instance sets, routed
connections, shared CPU evaluation, and revision-safe schema migration to ICE3 Studio.

**Architecture:** Extend the existing Rust Document with declarative large-scene source data.
Compile definitions into deterministic blueprints, expand instances and sets into derived scene
nodes, share geometry by content-derived cache keys, and keep the existing ChangeSet and
`document.apply` transaction boundary. This plan implements M1 from
`ICE3-STUDIO-SPEC-005`; GPU execution and Runtime streaming remain separate follow-up plans.

**Tech Stack:** Rust 1.98, serde, schemars, ts-rs, glam, SHA-256; TypeScript 7 with `node:test`
and esbuild; JSON-RPC; Node.js 24.22.1.

**Spec:** [`docs/specs/2026-10-30-large-scene-authoring-design.md`](../specs/2026-10-30-large-scene-authoring-design.md)

## Global Constraints

- Studio Core must remain independent of Server, Viewer, Toolchain, Engine, and native GPU code.
- Modeling vocabularies must stay generic; no wastewater-specific Operation names or Core types.
- All source mutations must pass through `document.apply`, dry-run, idempotency, validation, and
  ChangeSet approval.
- Component definitions and connections are declarative data; no scripts, arbitrary expressions,
  file paths, URLs, or executable shaders are accepted.
- Existing `wastewater-mini` behavior and stable IDs must remain compatible.
- Use Node.js `24.22.1` for JavaScript and TypeScript commands.
- Use Rust `1.98`; do not raise workspace versions without a separate decision.
- Do not use Vite or Vitest. Viewer tests continue to use esbuild plus `node:test`.
- Do not stage customer assets, generated GLBs, benchmark outputs, or project caches.
- Git author and committer must be `damoqiongqiu`.
- Every behavior change follows RED-GREEN-REFACTOR and ends with a focused commit.
- Root acceptance is `npm run verify`.

## Review Focus

- A missing definition, missing part, or cyclic component reference must be rejected before
  evaluation; it must never recurse indefinitely or produce partial geometry.
- Wrong parameter types, non-finite values, out-of-range values, and an oversized parameter map
  must return object-specific diagnostics without changing the revision.
- Zero, negative, overflowing, or memory-unsafe instance counts must be rejected before expansion;
  valid ordinal identities must survive save, reopen, and unchanged reevaluation.
- Missing ports, incompatible media, zero-length routes, or unresolved instance-set ordinals must
  fail transaction validation and leave every connection unchanged.
- Cache keys must include every output-affecting input, and two instances must never mutate the
  same shared geometry buffer; transform-only edits must reuse mesh data.

---

### Task 1: Extend the Document schema and migrate version 1 projects

**Files:**
- Modify: `crates/studio-core/src/ids.rs`
- Modify: `crates/studio-core/src/lib.rs`
- Modify: `crates/studio-core/src/document.rs`
- Create: `crates/studio-core/src/large_scene/mod.rs`
- Create: `crates/studio-core/src/large_scene/component.rs`
- Create: `crates/studio-core/src/large_scene/connection.rs`
- Modify: `crates/studio-project/src/store.rs`
- Test: `crates/studio-core/tests/large_scene_document.rs`
- Test: `crates/studio-project/tests/project_store.rs`

**Interfaces:**
- Produces: `ComponentId`
- Produces: `Document.components: Vec<ComponentDefinition>`
- Produces: `NodeContent::{ComponentInstance, InstanceSet, Connection}`
- Produces: `Document::migrate(self) -> Result<Document, DocumentSchemaError>`
- Produces: `PROJECT_SCHEMA_VERSION = 2`
- Later tasks consume these exact type and field names.

- [ ] **Step 1: Write failing schema and migration tests**

Add `crates/studio-core/tests/large_scene_document.rs`:

```rust
use std::collections::BTreeMap;

use studio_core::document::{Document, NodeContent, SceneNode};
use studio_core::ids::{ComponentId, NodeId};
use studio_core::large_scene::component::ComponentInstance;
use studio_core::math::Transform;
use studio_core::PROJECT_SCHEMA_VERSION;

#[test]
fn creates_a_version_two_document_with_empty_large_scene_collections() {
    let document = Document::new("large");
    assert_eq!(document.schema_version, PROJECT_SCHEMA_VERSION);
    assert_eq!(document.schema_version, 2);
    assert!(document.components.is_empty());
}

#[test]
fn migrates_version_one_without_changing_existing_nodes() {
    let mut document = Document::new("legacy");
    document.scene.nodes_mut().push(SceneNode {
        id: NodeId::from("node-000001"),
        name: "Legacy".into(),
        parent_id: None,
        transform: Transform::IDENTITY,
        material_id: None,
        content: NodeContent::Group,
        metadata: BTreeMap::new(),
        visible: true,
    });
    document.schema_version = 1;

    let migrated = document.clone().migrate().unwrap();
    assert_eq!(migrated.schema_version, 2);
    assert_eq!(migrated.scene, document.scene);
    assert!(migrated.components.is_empty());
}

#[test]
fn serializes_a_component_instance_node() {
    let node = SceneNode {
        id: NodeId::from("node-000002"),
        name: "Equipment".into(),
        parent_id: None,
        transform: Transform::IDENTITY,
        material_id: None,
        content: NodeContent::ComponentInstance(ComponentInstance {
            definition_id: ComponentId::from("component-000001"),
            parameters: BTreeMap::new(),
            overrides: BTreeMap::new(),
            metadata: BTreeMap::new(),
        }),
        metadata: BTreeMap::new(),
        visible: true,
    };
    let json = serde_json::to_value(node).unwrap();
    assert_eq!(json["content"]["type"], "component_instance");
    assert_eq!(json["content"]["definitionId"], "component-000001");
}
```

Append to `crates/studio-project/tests/project_store.rs`:

```rust
#[test]
fn opens_a_version_one_project_and_writes_the_next_revision_as_version_two() {
    let root = tempfile::tempdir().unwrap();
    let path = WorkspacePath::new(root.path(), "legacy-schema").unwrap();
    let store = ProjectStore::create(path.clone()).unwrap();
    let mut legacy = Document::new("legacy-schema");
    legacy.schema_version = 1;
    let bytes = serde_json::to_vec(&legacy).unwrap();
    let digest = format!("{:x}", sha2::Sha256::digest(&bytes));
    std::fs::create_dir_all(root.path().join("legacy-schema/revisions")).unwrap();
    std::fs::create_dir_all(root.path().join("legacy-schema/history")).unwrap();
    std::fs::write(
        root.path().join("legacy-schema/revisions/00000000.json"),
        &bytes,
    )
    .unwrap();
    std::fs::write(
        root.path().join("legacy-schema/history/operations.jsonl"),
        format!(
            "{{\"revision\":0,\"digest\":\"{digest}\",\"event\":{{\"kind\":\"snapshot\"}}}}\n"
        ),
    )
    .unwrap();
    std::fs::write(
        root.path().join("legacy-schema/project.json"),
        format!(
            "{{\"schemaVersion\":1,\"projectId\":\"legacy-schema\",\"currentRevision\":0,\"snapshotDigest\":\"{digest}\",\"undoStack\":[],\"redoStack\":[]}}"
        ),
    )
    .unwrap();
    drop(store);

    let reopened = ProjectStore::open(path).unwrap();
    let mut loaded = reopened.load_current().unwrap();
    assert_eq!(loaded.schema_version, 2);
    loaded.revision = loaded.revision.next();
    reopened.write_snapshot(&loaded).unwrap();
    assert_eq!(reopened.load_current().unwrap().schema_version, 2);
}
```

Add `sha2.workspace = true` to `crates/studio-project/Cargo.toml` if it is not already present.

- [ ] **Step 2: Run the tests to verify they fail**

Run:

```bash
cargo test -p studio-core --test large_scene_document
cargo test -p studio-project --test project_store opens_a_version_one_project
```

Expected: compilation fails because `ComponentId`, `large_scene`, `components`, `migrate`, and new
`NodeContent` variants do not exist.

- [ ] **Step 3: Add the core types and migration**

Add `ComponentId` to the existing `define_id!` list in `crates/studio-core/src/ids.rs`.

Create `crates/studio-core/src/large_scene/mod.rs`:

```rust
pub mod component;
pub mod connection;
```

Create `crates/studio-core/src/large_scene/component.rs` with the schema foundation:

```rust
use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::geometry::TriangleMesh;
use crate::ids::{ComponentId, MaterialId};
use crate::math::{Transform, Vec3};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ComponentDefinition {
    pub id: ComponentId,
    pub name: String,
    pub parameters: Vec<ComponentParameter>,
    pub parts: Vec<ComponentPart>,
    pub ports: Vec<ComponentPort>,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ComponentParameter {
    pub name: String,
    pub value_type: ParameterType,
    pub default: ParameterValue,
    pub unit: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum ParameterType {
    Number { minimum: Option<f32>, maximum: Option<f32> },
    Vector3,
    Color,
    Enumeration { values: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
#[ts(export)]
pub enum ParameterValue {
    Number(f32),
    Vector3(Vec3),
    Color([f32; 4]),
    Enumeration(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ComponentPart {
    pub id: String,
    pub parent_id: Option<String>,
    pub transform: PartTransform,
    pub content: PartContent,
    pub material_role: Option<MaterialBinding>,
    pub metadata: BTreeMap<String, String>,
    pub visible: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct PartTransform {
    pub translation: Vector3Binding,
    pub rotation: [f32; 4],
    pub scale: Vector3Binding,
}

impl Default for PartTransform {
    fn default() -> Self {
        Self {
            translation: Vector3Binding::Literal(Vec3::default()),
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: Vector3Binding::Literal(Vec3 { x: 1.0, y: 1.0, z: 1.0 }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum PartContent {
    Group,
    Box {
        width: ScalarBinding,
        height: ScalarBinding,
        depth: ScalarBinding,
    },
    Plane {
        width: ScalarBinding,
        depth: ScalarBinding,
    },
    Cylinder {
        radius: ScalarBinding,
        height: ScalarBinding,
        segments: u32,
    },
    Sphere {
        radius: ScalarBinding,
        segments: u32,
        rings: u32,
    },
    CustomMesh { mesh: TriangleMesh },
    Sweep {
        points: Vec<[f32; 3]>,
        radius: ScalarBinding,
        radial_segments: u32,
    },
    ComponentReference {
        definition_id: ComponentId,
        parameters: BTreeMap<String, ParameterValue>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
#[ts(export)]
pub enum ScalarBinding {
    Literal(f32),
    Parameter(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
#[ts(export)]
pub enum Vector3Binding {
    Literal(Vec3),
    Parameter(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
#[ts(export)]
pub enum MaterialBinding {
    Literal(String),
    Parameter(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ComponentPort {
    pub name: String,
    pub part_id: String,
    pub position: Vec3,
    pub direction: Vec3,
    pub medium: String,
    pub diameter: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ComponentInstance {
    pub definition_id: ComponentId,
    pub parameters: BTreeMap<String, ParameterValue>,
    pub overrides: BTreeMap<String, PartOverride>,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct PartOverride {
    pub material_id: Option<MaterialId>,
    pub visible: Option<bool>,
    pub metadata: Option<BTreeMap<String, String>>,
    pub parameters: BTreeMap<String, ParameterValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum InstanceLayout {
    Linear {
        origin: Vec3,
        direction: Vec3,
        spacing: f32,
        count: u32,
    },
    Grid {
        origin: Vec3,
        x_direction: Vec3,
        z_direction: Vec3,
        rows: u32,
        columns: u32,
        row_spacing: f32,
        column_spacing: f32,
    },
    Radial {
        centre: Vec3,
        radius: f32,
        start_angle_radians: f32,
        count: u32,
    },
    Path {
        placements: Vec<Transform>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct InstanceSet {
    pub definition_id: ComponentId,
    pub layout: InstanceLayout,
    pub shared_parameters: BTreeMap<String, ParameterValue>,
    pub item_overrides: BTreeMap<u32, BTreeMap<String, ParameterValue>>,
    pub metadata: BTreeMap<String, String>,
}
```

Create `crates/studio-core/src/large_scene/connection.rs`:

```rust
use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::{NodeId, RelationId};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum InstancePortRef {
    Instance { node_id: NodeId, port: String },
    SetItem { set_node_id: NodeId, ordinal: u32, port: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum RoutePolicy {
    Direct,
    Orthogonal,
    Buried { depth: f32 },
    Rack { elevation: f32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct ConnectionIntent {
    pub from: InstancePortRef,
    pub to: InstancePortRef,
    pub medium: String,
    pub diameter: f32,
    pub route_policy: RoutePolicy,
    pub relation_id: RelationId,
    pub metadata: BTreeMap<String, String>,
}
```

Modify `crates/studio-core/src/lib.rs`:

```rust
pub mod large_scene;
pub const PROJECT_SCHEMA_VERSION: u32 = 2;
```

Modify `crates/studio-core/src/document.rs`:

```rust
use crate::large_scene::component::{ComponentDefinition, ComponentInstance, InstanceSet};
use crate::large_scene::connection::ConnectionIntent;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Document {
    pub schema_version: u32,
    pub revision: Revision,
    pub project_id: String,
    pub scene: Scene,
    pub materials: Vec<Material>,
    pub relations: Vec<Relation>,
    pub constraints: Vec<Constraint>,
    #[serde(default)]
    pub components: Vec<ComponentDefinition>,
    pub metadata: BTreeMap<String, String>,
    pub idempotency_keys: Vec<crate::apply::IdempotencyRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum NodeContent {
    #[default]
    Group,
    Primitive(Primitive),
    CustomMesh(TriangleMesh),
    Sweep {
        points: Vec<[f32; 3]>,
        radius: f32,
        radial_segments: u32,
    },
    ComponentInstance(ComponentInstance),
    InstanceSet(InstanceSet),
    Connection(ConnectionIntent),
}
```

Update `Document::new` to initialize `components: Vec::new()`.

Add the migration error and method in `document.rs`:

```rust
#[derive(Debug, thiserror::Error)]
pub enum DocumentSchemaError {
    #[error("unsupported document schema version {0}")]
    Unsupported(u32),
}

impl Document {
    pub fn migrate(mut self) -> Result<Self, DocumentSchemaError> {
        match self.schema_version {
            1 | 2 => {
                self.schema_version = PROJECT_SCHEMA_VERSION;
                Ok(self)
            }
            version => Err(DocumentSchemaError::Unsupported(version)),
        }
    }
}
```

In `ProjectStore::read_revision_with_digest`, deserialize and then call `.migrate()`. Change
`read_metadata` to accept schema versions `1 | PROJECT_SCHEMA_VERSION`. Keep digest verification
against the original bytes.

- [ ] **Step 4: Run the focused tests**

Run:

```bash
cargo test -p studio-core --test large_scene_document
cargo test -p studio-project --test project_store
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-core/Cargo.toml crates/studio-core/src crates/studio-core/tests/large_scene_document.rs \
  crates/studio-project/Cargo.toml crates/studio-project/src/store.rs crates/studio-project/tests/project_store.rs
git commit -m "feat(studio-core): add large-scene document schema"
```

---

### Task 2: Validate component definitions and bounded parameters

**Files:**
- Create: `crates/studio-core/src/large_scene/validate.rs`
- Modify: `crates/studio-core/src/large_scene/mod.rs`
- Modify: `crates/studio-core/src/validation.rs`
- Test: `crates/studio-core/tests/large_scene_validation.rs`

**Interfaces:**
- Consumes: `Document.components` and component types from Task 1.
- Produces: `validate_large_scene(document: &Document) -> Vec<Diagnostic>`
- Produces: `resolve_parameters(definition: &ComponentDefinition, supplied: &BTreeMap<String,
  ParameterValue>) -> Result<BTreeMap<String, ParameterValue>, ParameterError>`
- Later evaluators call `resolve_parameters` and must not duplicate validation rules.

- [ ] **Step 1: Write failing validation tests**

Create `crates/studio-core/tests/large_scene_validation.rs`:

```rust
use std::collections::BTreeMap;

use studio_core::document::Document;
use studio_core::ids::ComponentId;
use studio_core::large_scene::component::*;
use studio_core::large_scene::validate::{resolve_parameters, validate_large_scene};
use studio_core::math::Vec3;

fn definition(id: &str) -> ComponentDefinition {
    ComponentDefinition {
        id: ComponentId::from(id),
        name: "Equipment".into(),
        parameters: vec![ComponentParameter {
            name: "width".into(),
            value_type: ParameterType::Number {
                minimum: Some(0.1),
                maximum: Some(10.0),
            },
            default: ParameterValue::Number(1.0),
            unit: Some("m".into()),
        }],
        parts: vec![ComponentPart {
            id: "body".into(),
            parent_id: None,
            transform: PartTransform::default(),
            content: PartContent::Box {
                width: ScalarBinding::Parameter("width".into()),
                height: ScalarBinding::Literal(1.0),
                depth: ScalarBinding::Literal(1.0),
            },
            material_role: None,
            metadata: BTreeMap::new(),
            visible: true,
        }],
        ports: vec![ComponentPort {
            name: "inlet".into(),
            part_id: "body".into(),
            position: Vec3 { x: -0.5, y: 0.0, z: 0.0 },
            direction: Vec3 { x: -1.0, y: 0.0, z: 0.0 },
            medium: "water".into(),
            diameter: 0.2,
        }],
        metadata: BTreeMap::new(),
    }
}

#[test]
fn accepts_a_valid_component_definition() {
    let mut document = Document::new("valid");
    document.components.push(definition("component-000001"));
    assert!(validate_large_scene(&document).is_empty());
}

#[test]
fn rejects_parameter_out_of_range_and_wrong_type() {
    let component = definition("component-000001");
    let mut values = BTreeMap::new();
    values.insert("width".into(), ParameterValue::Number(20.0));
    assert!(resolve_parameters(&component, &values).is_err());

    values.insert("width".into(), ParameterValue::Enumeration("wide".into()));
    assert!(resolve_parameters(&component, &values).is_err());
}

#[test]
fn rejects_unknown_and_cyclic_component_references() {
    let mut first = definition("component-000001");
    first.parts[0].content = PartContent::ComponentReference {
        definition_id: ComponentId::from("component-000002"),
        parameters: BTreeMap::new(),
    };
    let mut second = definition("component-000002");
    second.parts[0].content = PartContent::ComponentReference {
        definition_id: ComponentId::from("component-000001"),
        parameters: BTreeMap::new(),
    };
    let mut document = Document::new("cycle");
    document.components.extend([first, second]);
    let messages = validate_large_scene(&document)
        .into_iter()
        .map(|diagnostic| diagnostic.message)
        .collect::<Vec<_>>();
    assert!(messages.iter().any(|message| message.contains("cycle")));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run:

```bash
cargo test -p studio-core --test large_scene_validation
```

Expected: compilation fails because `large_scene::validate` does not exist.

- [ ] **Step 3: Implement validation and parameter resolution**

Create `large_scene/validate.rs` and implement these exact public entry points:

```rust
pub const MAX_COMPONENT_DEPTH: usize = 16;
pub const MAX_COMPONENT_PARTS: usize = 4096;

#[derive(Debug, thiserror::Error)]
pub enum ParameterError {
    #[error("unknown parameter: {0}")]
    Unknown(String),
    #[error("parameter {0} has the wrong type")]
    WrongType(String),
    #[error("parameter {0} is outside its allowed range")]
    OutOfRange(String),
    #[error("parameter {0} is not finite")]
    NonFinite(String),
}

pub fn validate_large_scene(document: &Document) -> Vec<Diagnostic> {
    let definitions = document
        .components
        .iter()
        .map(|definition| (definition.id.clone(), definition))
        .collect::<BTreeMap<_, _>>();
    let mut diagnostics = Vec::new();
    for definition in &document.components {
        validate_definition(definition, &definitions, &mut diagnostics);
    }
    for (index, node) in document.scene.nodes().iter().enumerate() {
        match &node.content {
            NodeContent::ComponentInstance(instance) => {
                validate_instance(instance, &definitions, index, node.id.as_str(), &mut diagnostics);
            }
            NodeContent::InstanceSet(set) => {
                validate_instance_set(set, &definitions, index, node.id.as_str(), &mut diagnostics);
            }
            NodeContent::Connection(connection) => {
                validate_connection_references(document, connection, index, node.id.as_str(), &mut diagnostics);
            }
            _ => {}
        }
    }
    diagnostics
}

pub fn resolve_parameters(
    definition: &ComponentDefinition,
    supplied: &BTreeMap<String, ParameterValue>,
) -> Result<BTreeMap<String, ParameterValue>, ParameterError> {
    let mut resolved = definition
        .parameters
        .iter()
        .map(|parameter| (parameter.name.clone(), parameter.default.clone()))
        .collect::<BTreeMap<_, _>>();
    for (name, value) in supplied {
        let declaration = definition
            .parameters
            .iter()
            .find(|parameter| parameter.name == *name)
            .ok_or_else(|| ParameterError::Unknown(name.clone()))?;
        validate_value(declaration, value)?;
        resolved.insert(name.clone(), value.clone());
    }
    Ok(resolved)
}
```

Extend `DiagnosticCode` with:

```rust
#[serde(rename = "ICE3_STUDIO_COMPONENT_INVALID")]
ComponentInvalid,
#[serde(rename = "ICE3_STUDIO_PARAMETER_INVALID")]
ParameterInvalid,
#[serde(rename = "ICE3_STUDIO_INSTANCE_INVALID")]
InstanceInvalid,
#[serde(rename = "ICE3_STUDIO_INSTANCE_SET_INVALID")]
InstanceSetInvalid,
#[serde(rename = "ICE3_STUDIO_CONNECTION_INVALID")]
ConnectionInvalid,
#[serde(rename = "ICE3_STUDIO_GEOMETRY_CACHE_FAILED")]
GeometryCacheFailed,
```

Use the corresponding `#[ts(rename = "...")]` attributes and extend `DIAGNOSTIC_CODES` with the
same six values. Change its declaration to a slice so future additions do not require an array
length update: `pub const DIAGNOSTIC_CODES: &[&str] = &[...]`. Keep every existing string in its
current order, then append the six new strings in the order shown above.

Implement `validate_value` with these exact checks:

- `Number`: finite and within inclusive declared bounds.
- `Vector3`: all three components finite.
- `Color`: four finite components in `0.0..=1.0`.
- `Enumeration`: present in `values`.
- A type mismatch returns `ParameterError::WrongType`.

Implement `validate_definition`, `validate_instance`, `validate_instance_set`, and
`validate_connection_references`. The definition walk must use a recursion-stack set and report a
diagnostic containing `cycle` before exceeding `MAX_COMPONENT_DEPTH`. Duplicate definition IDs,
part IDs, parameter names, port names, missing ports' `part_id`, non-finite port values,
non-positive port diameter, invalid Scalar/Vector3/Material binding parameter names, unknown part
IDs in overrides, invalid override parameter values, and unresolved nested definition IDs are
errors.

Call `validate_large_scene` at the end of `validate_document`:

```rust
diagnostics.extend(crate::large_scene::validate::validate_large_scene(document));
diagnostics
```

- [ ] **Step 4: Run the focused tests**

Run:

```bash
cargo test -p studio-core --test large_scene_validation
cargo test -p studio-core --test document_model
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-core/src/large_scene crates/studio-core/src/validation.rs \
  crates/studio-core/tests/large_scene_validation.rs
git commit -m "feat(studio-core): validate parameterized components"
```

---

### Task 3: Expand explicit component instances with stable derived identities

**Files:**
- Create: `crates/studio-core/src/large_scene/evaluate.rs`
- Modify: `crates/studio-core/src/large_scene/mod.rs`
- Modify: `crates/studio-core/src/geometry/evaluate.rs`
- Create: `crates/studio-core/tests/support/mod.rs`
- Test: `crates/studio-core/tests/large_scene_evaluation.rs`

**Interfaces:**
- Consumes: `resolve_parameters` from Task 2.
- Produces: `evaluate_large_scene(document: &Document) -> Result<EvaluatedScene, LargeSceneError>`
- Produces: `resolved_part_geometry_key(blueprint_digest, part_id, content, parameters) ->
  Result<String, LargeSceneError>`
- Produces: `EvaluatedScene { nodes, meshes, stats }`
- `EvaluatedSceneNode.id` is a `String` so it can represent instance-part identities.

- [ ] **Step 1: Write a failing shared-geometry test**

Create `crates/studio-core/tests/support/mod.rs`:

```rust
use std::collections::BTreeMap;

use studio_core::document::{Document, NodeContent, SceneNode};
use studio_core::ids::{ComponentId, NodeId};
use studio_core::large_scene::component::*;
use studio_core::math::{Transform, Vec3};

pub fn shared_pump_document() -> Document {
    let component = ComponentDefinition {
        id: ComponentId::from("component-000001"),
        name: "Pump".into(),
        parameters: Vec::new(),
        parts: vec![ComponentPart {
            id: "body".into(),
            parent_id: None,
            transform: PartTransform::default(),
            content: PartContent::Box {
                width: ScalarBinding::Literal(2.0),
                height: ScalarBinding::Literal(1.0),
                depth: ScalarBinding::Literal(1.0),
            },
            material_role: None,
            metadata: BTreeMap::new(),
            visible: true,
        }],
        ports: Vec::new(),
        metadata: BTreeMap::new(),
    };
    let mut document = Document::new("shared");
    document.components.push(component);
    for ordinal in 1..=2 {
        let mut transform = Transform::IDENTITY;
        transform.translation = Vec3 {
            x: ordinal as f32 * 4.0,
            y: 0.0,
            z: 0.0,
        };
        document.scene.nodes_mut().push(SceneNode {
            id: NodeId::new("node", ordinal).unwrap(),
            name: format!("Pump {ordinal}"),
            parent_id: None,
            transform,
            material_id: None,
            content: NodeContent::ComponentInstance(ComponentInstance {
                definition_id: ComponentId::from("component-000001"),
                parameters: BTreeMap::new(),
                overrides: BTreeMap::new(),
                metadata: BTreeMap::new(),
            }),
            metadata: BTreeMap::new(),
            visible: true,
        });
    }
    document
}
```

Create `crates/studio-core/tests/large_scene_evaluation.rs`:

```rust
mod support;

use studio_core::large_scene::evaluate::evaluate_large_scene;

use support::shared_pump_document;

#[test]
fn identical_instances_share_one_mesh_with_stable_part_identity() {
    let document = shared_pump_document();
    let evaluated = evaluate_large_scene(&document).unwrap();
    assert_eq!(evaluated.nodes.len(), 2);
    assert_eq!(evaluated.meshes.len(), 1);
    assert_eq!(evaluated.nodes[0].id, "node-000001/body");
    assert_eq!(evaluated.nodes[0].owner_id, "node-000001");
    assert_eq!(evaluated.nodes[1].id, "node-000002/body");
    assert_eq!(evaluated.stats.effective_triangles, 24);
    assert_eq!(evaluated.stats.unique_triangles, 12);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
cargo test -p studio-core --test large_scene_evaluation
```

Expected: compilation fails because the evaluator result types do not exist.

- [ ] **Step 3: Implement explicit instance expansion**

Create `large_scene/evaluate.rs`:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct EvaluatedSceneNode {
    pub id: String,
    pub owner_id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub transform: Transform,
    pub material_id: Option<MaterialId>,
    pub mesh_key: Option<String>,
    pub metadata: BTreeMap<String, String>,
    pub visible: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LargeSceneStats {
    pub source_nodes: usize,
    pub expanded_nodes: usize,
    pub unique_meshes: usize,
    pub effective_triangles: usize,
    pub unique_triangles: usize,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EvaluatedScene {
    pub nodes: Vec<EvaluatedSceneNode>,
    pub meshes: BTreeMap<String, EvaluatedMesh>,
    pub stats: LargeSceneStats,
}

#[derive(Debug, thiserror::Error)]
pub enum LargeSceneError {
    #[error(transparent)]
    Parameter(#[from] ParameterError),
    #[error(transparent)]
    Geometry(#[from] GeometryError),
    #[error("component reference is missing or cyclic")]
    InvalidComponentReference,
}

pub fn resolved_part_geometry_key(
    blueprint_digest: &str,
    part_id: &str,
    content: &PartContent,
    parameters: &BTreeMap<String, ParameterValue>,
) -> Result<String, LargeSceneError> {
    let bytes = serde_json::to_vec(&(
        "cpu",
        "geometry-kernel-v1",
        blueprint_digest,
        part_id,
        content,
        parameters,
    ))
        .map_err(|_| LargeSceneError::InvalidComponentReference)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
```

Expose the cached evaluator with this exact generic signature:

```rust
pub fn evaluate_large_scene_with_cache<S: GeometryStore>(
    document: &Document,
    store: &mut S,
) -> Result<EvaluatedScene, LargeSceneError>
```

The implementation must:

1. Use `resolve_parameters` for the instance.
2. Compute a canonical `blueprint_digest` from the complete ComponentDefinition and every
   transitive nested definition; include nested definition order and content.
3. Recursively expand definition parts in declared order.
4. Apply `PartOverride.parameters` only to that part's resolution map.
5. Resolve `PartTransform.translation` and `PartTransform.scale`, then compose parent-part
   transforms and the owning instance transform.
6. Resolve each `ScalarBinding` and `MaterialBinding`.
7. Evaluate `PartContent::Group`, Primitive variants, CustomMesh, and Sweep through existing
   geometry functions.
8. Generate IDs exactly as `"{instance_id}/{part_id}"`.
9. Store one mesh per geometry key in `meshes`.
10. Apply the part's material, visibility, and metadata overrides without affecting sibling
    parts.
11. Count each instance use in `effective_triangles` and each cache entry in
   `unique_triangles`.
12. Keep legacy Nodes unchanged using their existing values, without adding them to
   `large_scene.stats`.

Add a legacy conversion branch so callers can use one evaluator for every Node content.
For the new large-scene content variants, have `evaluate_node_content` return
`EvaluatedMesh::empty()`; document-aware evaluation in `large_scene/evaluate.rs` owns their
geometry.

- [ ] **Step 4: Run the focused tests**

Run:

```bash
cargo test -p studio-core --test large_scene_evaluation
cargo test -p studio-core --test geometry_evaluation
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-core/src/large_scene crates/studio-core/src/geometry/evaluate.rs \
  crates/studio-core/tests/support/mod.rs crates/studio-core/tests/large_scene_evaluation.rs
git commit -m "feat(studio-core): expand shared component instances"
```

---

### Task 4: Expand deterministic instance-set layouts

**Files:**
- Modify: `crates/studio-core/src/large_scene/evaluate.rs`
- Modify: `crates/studio-core/src/large_scene/validate.rs`
- Test: `crates/studio-core/tests/instance_sets.rs`

**Interfaces:**
- Consumes: `evaluate_large_scene` and instance-set schema from Tasks 1 and 3.
- Produces: `expand_instance_layout(layout: &InstanceLayout) -> Result<Vec<Transform>, LayoutError>`
- Produces derived IDs in the form `"{set_node_id}/{ordinal:06}/{part_id}"`.

- [ ] **Step 1: Write failing layout tests**

Create `crates/studio-core/tests/instance_sets.rs`:

```rust
use studio_core::large_scene::component::InstanceLayout;
use studio_core::large_scene::evaluate::expand_instance_layout;
use studio_core::math::Vec3;

#[test]
fn expands_linear_and_grid_layouts_in_stable_order() {
    let linear = expand_instance_layout(&InstanceLayout::Linear {
        origin: Vec3 { x: 0.0, y: 0.0, z: 0.0 },
        direction: Vec3 { x: 1.0, y: 0.0, z: 0.0 },
        spacing: 2.0,
        count: 3,
    })
    .unwrap();
    assert_eq!(linear.len(), 3);
    assert_eq!(linear[2].translation.x, 4.0);

    let grid = expand_instance_layout(&InstanceLayout::Grid {
        origin: Vec3 { x: 0.0, y: 0.0, z: 0.0 },
        x_direction: Vec3 { x: 1.0, y: 0.0, z: 0.0 },
        z_direction: Vec3 { x: 0.0, y: 0.0, z: 1.0 },
        rows: 2,
        columns: 3,
        row_spacing: 4.0,
        column_spacing: 2.0,
    })
    .unwrap();
    assert_eq!(grid.len(), 6);
    assert_eq!(grid[3].translation.x, 0.0);
    assert_eq!(grid[3].translation.z, 4.0);
}

#[test]
fn rejects_zero_counts_and_non_finite_spacing() {
    assert!(expand_instance_layout(&InstanceLayout::Linear {
        origin: Vec3::default(),
        direction: Vec3 { x: 1.0, y: 0.0, z: 0.0 },
        spacing: f32::NAN,
        count: 2,
    })
    .is_err());
    assert!(expand_instance_layout(&InstanceLayout::Radial {
        centre: Vec3::default(),
        radius: 2.0,
        start_angle_radians: 0.0,
        count: 0,
    })
    .is_err());
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run:

```bash
cargo test -p studio-core --test instance_sets
```

Expected: FAIL because `expand_instance_layout` does not exist.

- [ ] **Step 3: Implement deterministic layout expansion**

Add `MAX_INSTANCE_SET_ITEMS: u32 = 100_000`.

Implement:

```rust
#[derive(Debug, thiserror::Error)]
pub enum LayoutError {
    #[error("instance layout requires at least one item")]
    Empty,
    #[error("instance layout exceeds the configured item limit")]
    TooManyItems,
    #[error("instance layout contains non-finite or invalid values")]
    Invalid,
}

pub fn expand_instance_layout(
    layout: &InstanceLayout,
) -> Result<Vec<Transform>, LayoutError> {
    match layout {
        InstanceLayout::Linear { origin, direction, spacing, count } => {
            validate_count(*count)?;
            validate_direction(*direction)?;
            validate_positive_finite(*spacing)?;
            let direction = normalize(*direction);
            Ok((0..*count)
                .map(|ordinal| {
                    let mut transform = Transform::IDENTITY;
                    transform.translation = Vec3 {
                        x: origin.x + direction.x * (*spacing) * ordinal as f32,
                        y: origin.y + direction.y * (*spacing) * ordinal as f32,
                        z: origin.z + direction.z * (*spacing) * ordinal as f32,
                    };
                    transform
                })
                .collect())
        }
        InstanceLayout::Grid {
            origin,
            x_direction,
            z_direction,
            rows,
            columns,
            row_spacing,
            column_spacing,
        } => {
            validate_count(*rows)?;
            validate_count(*columns)?;
            let total = (*rows).checked_mul(*columns).ok_or(LayoutError::TooManyItems)?;
            if total > MAX_INSTANCE_SET_ITEMS {
                return Err(LayoutError::TooManyItems);
            }
            validate_direction(*x_direction)?;
            validate_direction(*z_direction)?;
            validate_positive_finite(*row_spacing)?;
            validate_positive_finite(*column_spacing)?;
            let x = normalize(*x_direction);
            let z = normalize(*z_direction);
            let mut placements = Vec::with_capacity(total as usize);
            for row in 0..*rows {
                for column in 0..*columns {
                    let mut transform = Transform::IDENTITY;
                    transform.translation = Vec3 {
                        x: origin.x
                            + x.x * (*column_spacing) * column as f32
                            + z.x * (*row_spacing) * row as f32,
                        y: origin.y
                            + x.y * (*column_spacing) * column as f32
                            + z.y * (*row_spacing) * row as f32,
                        z: origin.z
                            + x.z * (*column_spacing) * column as f32
                            + z.z * (*row_spacing) * row as f32,
                    };
                    placements.push(transform);
                }
            }
            Ok(placements)
        }
        InstanceLayout::Radial {
            centre,
            radius,
            start_angle_radians,
            count,
        } => {
            validate_count(*count)?;
            validate_positive_finite(*radius)?;
            if !start_angle_radians.is_finite() {
                return Err(LayoutError::Invalid);
            }
            Ok((0..*count)
                .map(|ordinal| {
                    let angle = *start_angle_radians
                        + std::f32::consts::TAU * ordinal as f32 / *count as f32;
                    let mut transform = Transform::IDENTITY;
                    transform.translation = Vec3 {
                        x: centre.x + (*radius) * angle.cos(),
                        y: centre.y,
                        z: centre.z + (*radius) * angle.sin(),
                    };
                    transform.rotation = glam::Quat::from_rotation_y(-angle).to_array();
                    transform
                })
                .collect())
        }
        InstanceLayout::Path { placements } => {
            if placements.is_empty() || placements.len() > MAX_INSTANCE_SET_ITEMS as usize {
                return Err(LayoutError::Invalid);
            }
            if placements.iter().any(|transform| {
                !transform.translation.x.is_finite()
                    || !transform.translation.y.is_finite()
                    || !transform.translation.z.is_finite()
                    || transform.rotation.iter().any(|value| !value.is_finite())
                    || !transform.scale.x.is_finite()
                    || !transform.scale.y.is_finite()
                    || !transform.scale.z.is_finite()
                    || transform.scale.x <= 0.0
                    || transform.scale.y <= 0.0
                    || transform.scale.z <= 0.0
            }) {
                return Err(LayoutError::Invalid);
            }
            Ok(placements.clone())
        }
    }
}
```

Grid ordinals iterate row first and column second. Radial instances face tangentially along the
ring and preserve `y = centre.y`. Path preserves the supplied transform.

Update `evaluate_large_scene` to expand `NodeContent::InstanceSet`. Merge each ordinal's
`shared_parameters` and `item_overrides`, then instantiate the referenced definition. Use
`"{set_node_id}/{ordinal:06}/{part_id}"` for part identity.

- [ ] **Step 4: Run the focused tests**

Run:

```bash
cargo test -p studio-core --test instance_sets
cargo test -p studio-core --test large_scene_evaluation
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-core/src/large_scene crates/studio-core/tests/instance_sets.rs
git commit -m "feat(studio-core): expand deterministic instance sets"
```

---

### Task 5: Resolve ports and generate routed connection sweeps

**Files:**
- Create: `crates/studio-core/src/large_scene/route.rs`
- Modify: `crates/studio-core/src/large_scene/mod.rs`
- Modify: `crates/studio-core/src/large_scene/evaluate.rs`
- Modify: `crates/studio-core/src/large_scene/validate.rs`
- Test: `crates/studio-core/tests/connections.rs`

**Interfaces:**
- Consumes: explicit instance and set-item expansion from Tasks 3 and 4.
- Produces: `resolve_port(...) -> Result<ResolvedPort, ConnectionError>`
- Produces: `route_connection(...) -> Result<Vec<[f32; 3]>, ConnectionError>`
- Produces derived connection node ID `"{connection_node_id}/route"`.

- [ ] **Step 1: Write failing connection tests**

Create `crates/studio-core/tests/connections.rs`:

```rust
use studio_core::large_scene::route::{ResolvedPort, route_between_ports};
use studio_core::large_scene::connection::RoutePolicy;
use studio_core::math::Vec3;

fn port(position: [f32; 3], direction: [f32; 3], medium: &str) -> ResolvedPort {
    ResolvedPort {
        position: Vec3 { x: position[0], y: position[1], z: position[2] },
        direction: Vec3 { x: direction[0], y: direction[1], z: direction[2] },
        medium: medium.into(),
        diameter: 0.2,
    }
}

#[test]
fn an_orthogonal_route_starts_and_ends_at_the_ports() {
    let points = route_between_ports(
        &port([0.0, 1.0, 0.0], [-1.0, 0.0, 0.0], "water"),
        &port([10.0, 2.0, 4.0], [1.0, 0.0, 0.0], "water"),
        &RoutePolicy::Orthogonal,
    )
    .unwrap();
    assert_eq!(points.len(), 5);
    assert_eq!(points[0], [0.0, 1.0, 0.0]);
    assert_eq!(points[4], [10.0, 2.0, 4.0]);
}

#[test]
fn rejects_incompatible_media_and_degenerate_routes() {
    assert!(route_between_ports(
        &port([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], "water"),
        &port([1.0, 0.0, 0.0], [-1.0, 0.0, 0.0], "air"),
        &RoutePolicy::Direct,
    )
    .is_err());
    assert!(route_between_ports(
        &port([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], "water"),
        &port([0.0, 0.0, 0.0], [-1.0, 0.0, 0.0], "water"),
        &RoutePolicy::Direct,
    )
    .is_err());
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run:

```bash
cargo test -p studio-core --test connections
```

Expected: FAIL because the routing module does not exist.

- [ ] **Step 3: Implement endpoint resolution and routing**

Create `large_scene/route.rs` with:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedPort {
    pub position: Vec3,
    pub direction: Vec3,
    pub medium: String,
    pub diameter: f32,
}

#[derive(Debug, thiserror::Error)]
pub enum ConnectionError {
    #[error("connection endpoint does not exist")]
    MissingEndpoint,
    #[error("connection endpoint port does not exist")]
    MissingPort,
    #[error("connection media are incompatible")]
    IncompatibleMedia,
    #[error("connection route is degenerate")]
    Degenerate,
    #[error("connection route policy is invalid")]
    InvalidPolicy,
}

pub fn route_between_ports(
    from: &ResolvedPort,
    to: &ResolvedPort,
    policy: &RoutePolicy,
) -> Result<Vec<[f32; 3]>, ConnectionError> {
    if from.medium != to.medium {
        return Err(ConnectionError::IncompatibleMedia);
    }
    if distance(from.position, to.position) <= f32::EPSILON {
        return Err(ConnectionError::Degenerate);
    }
    let elevation = match policy {
        RoutePolicy::Direct => None,
        RoutePolicy::Orthogonal => Some(from.position.y.max(to.position.y) + 0.5),
        RoutePolicy::Buried { depth } if depth.is_finite() && *depth >= 0.0 => {
            Some(-*depth)
        }
        RoutePolicy::Rack { elevation } if elevation.is_finite() => Some(*elevation),
        _ => return Err(ConnectionError::InvalidPolicy),
    };
    Ok(build_route(from.position, to.position, elevation))
}

fn build_route(start: Vec3, end: Vec3, elevation: Option<f32>) -> Vec<[f32; 3]> {
    let Some(elevation) = elevation else {
        return vec![
            [start.x, start.y, start.z],
            [end.x, end.y, end.z],
        ];
    };
    let mut points = Vec::new();
    push_unique(&mut points, [start.x, start.y, start.z]);
    push_unique(&mut points, [start.x, elevation, start.z]);
    push_unique(&mut points, [start.x, elevation, end.z]);
    push_unique(&mut points, [end.x, elevation, end.z]);
    push_unique(&mut points, [end.x, end.y, end.z]);
    points
}

fn push_unique(points: &mut Vec<[f32; 3]>, point: [f32; 3]) {
    if points.last() != Some(&point) {
        points.push(point);
    }
}
```

The test's orthogonal route therefore returns five points. Direct routes emit start and end.

Update evaluation to resolve ports through the expanded instance metadata. Apply instance world
transform to the component-local port position and rotation. Generate the connection sweep with
the declared radius (`diameter / 2.0`) and append it under `"{connection_node_id}/route"`.

- [ ] **Step 4: Run the focused tests**

Run:

```bash
cargo test -p studio-core --test connections
cargo test -p studio-core --test large_scene_validation
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-core/src/large_scene crates/studio-core/tests/connections.rs
git commit -m "feat(studio-core): route typed equipment connections"
```

---

### Task 6: Add component, instance, instance-set, and connection Operations

**Files:**
- Modify: `crates/studio-core/src/operation.rs`
- Modify: `crates/studio-core/src/apply.rs`
- Modify: `crates/studio-core/src/diff.rs`
- Modify: `crates/studio-core/src/validation.rs`
- Test: `crates/studio-core/tests/large_scene_transactions.rs`

**Interfaces:**
- Consumes: all large-scene types from Tasks 1–5.
- Produces: twelve public `Operation` variants:
  `ComponentDefine`, `ComponentUpdate`, `ComponentDelete`, `InstanceCreate`, `InstanceUpdate`,
  `InstanceDelete`, `InstanceSetCreate`, `InstanceSetUpdate`, `InstanceSetDelete`,
  `ConnectionCreate`, `ConnectionUpdate`, and `ConnectionDelete`.
- Produces: component IDs in `DocumentDiff`.

- [ ] **Step 1: Write a failing atomic transaction test**

Create `crates/studio-core/tests/large_scene_transactions.rs`:

```rust
use std::collections::BTreeMap;

use studio_core::apply::{ApplyRequest, apply_transaction};
use studio_core::document::Document;
use studio_core::ids::{ComponentId, NodeId, Revision};
use studio_core::large_scene::component::*;
use studio_core::math::Transform;
use studio_core::operation::Operation;

#[test]
fn applies_a_component_and_instance_in_one_revision() {
    let mut document = Document::new("transaction");
    let definition = ComponentDefinition {
        id: ComponentId::from("component-000001"),
        name: "Pump".into(),
        parameters: Vec::new(),
        parts: vec![ComponentPart {
            id: "body".into(),
            parent_id: None,
            transform: PartTransform::default(),
            content: PartContent::Box {
                width: ScalarBinding::Literal(1.0),
                height: ScalarBinding::Literal(1.0),
                depth: ScalarBinding::Literal(1.0),
            },
            material_role: None,
            metadata: BTreeMap::new(),
            visible: true,
        }],
        ports: Vec::new(),
        metadata: BTreeMap::new(),
    };
    let operations = vec![
        Operation::ComponentDefine { definition },
        Operation::InstanceCreate {
            node_id: NodeId::from("node-000001"),
            parent_id: None,
            name: "Pump A".into(),
            transform: Transform::IDENTITY,
            instance: ComponentInstance {
                definition_id: ComponentId::from("component-000001"),
                parameters: BTreeMap::new(),
                overrides: BTreeMap::new(),
                metadata: BTreeMap::new(),
            },
            material_id: None,
            metadata: BTreeMap::new(),
        },
    ];
    let result = apply_transaction(
        &mut document,
        ApplyRequest {
            expected_revision: Revision::new(0),
            dry_run: false,
            idempotency_key: "create-pumps".into(),
            operations,
        },
    )
    .unwrap();
    assert_eq!(result.revision, Revision::new(1));
    assert_eq!(document.components.len(), 1);
    assert_eq!(document.scene.nodes().len(), 1);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
cargo test -p studio-core --test large_scene_transactions
```

Expected: FAIL because the new Operation variants do not exist.

- [ ] **Step 3: Implement Operations, diffs, and application**

Add the Operation variants with `serde` and `ts_rs` names:

```rust
#[serde(rename = "component.define")]
ComponentDefine { definition: ComponentDefinition },
#[serde(rename = "component.update")]
ComponentUpdate { definition: ComponentDefinition },
#[serde(rename = "component.delete")]
ComponentDelete { component_id: ComponentId },
#[serde(rename = "instance.create")]
InstanceCreate {
    node_id: NodeId,
    parent_id: Option<NodeId>,
    name: String,
    transform: Transform,
    instance: ComponentInstance,
    material_id: Option<MaterialId>,
    metadata: BTreeMap<String, String>,
},
#[serde(rename = "instance.update")]
InstanceUpdate {
    node_id: NodeId,
    instance: ComponentInstance,
},
#[serde(rename = "instance.delete")]
InstanceDelete { node_id: NodeId },
#[serde(rename = "instanceSet.create")]
InstanceSetCreate {
    node_id: NodeId,
    parent_id: Option<NodeId>,
    name: String,
    transform: Transform,
    set: InstanceSet,
    material_id: Option<MaterialId>,
    metadata: BTreeMap<String, String>,
},
#[serde(rename = "instanceSet.update")]
InstanceSetUpdate { node_id: NodeId, set: InstanceSet },
#[serde(rename = "instanceSet.delete")]
InstanceSetDelete { node_id: NodeId },
#[serde(rename = "connection.create")]
ConnectionCreate {
    node_id: NodeId,
    parent_id: Option<NodeId>,
    name: String,
    transform: Transform,
    connection: ConnectionIntent,
    metadata: BTreeMap<String, String>,
},
#[serde(rename = "connection.update")]
ConnectionUpdate { node_id: NodeId, connection: ConnectionIntent },
#[serde(rename = "connection.delete")]
ConnectionDelete { node_id: NodeId },
```

Add `added_component_ids`, `updated_component_ids`, and `removed_component_ids` to
`DocumentDiff` with matching `DiffBuilder` methods.

Apply semantics:

- Define fails when the component ID already exists.
- Update fails when the component ID is absent.
- Delete fails when any Node or nested definition references the component.
- Instance and set create fail on duplicate Node IDs.
- Instance/set update requires the existing Node to have the same content variant.
- Connection create requires the linked Relation to be present in the same candidate Document
  after applying all earlier Operations; use `connection.create` followed by
  `relation.create` in test fixtures when the relation does not exist.
- Connection delete removes the linked Relation if it still exists.
- Every successful operation marks the owning Node or component updated/added/removed in the
  diff.

Reject deletion of Definitions or Nodes while another required reference remains. Use a shared
reference scan over `Document.components`, `Document.scene.nodes`, and `Document.relations`.

- [ ] **Step 4: Run focused transaction and existing regression tests**

Run:

```bash
cargo test -p studio-core --test large_scene_transactions
cargo test -p studio-core --test transactions
cargo test -p studio-core --test changeset_evaluation
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-core/src/operation.rs crates/studio-core/src/apply.rs \
  crates/studio-core/src/diff.rs crates/studio-core/src/validation.rs \
  crates/studio-core/tests/large_scene_transactions.rs
git commit -m "feat(studio-core): add large-scene authoring operations"
```

---

### Task 7: Add the CPU reference evaluator and persistent mesh cache

**Files:**
- Create: `crates/studio-core/src/large_scene/cache.rs`
- Modify: `crates/studio-core/src/large_scene/evaluate.rs`
- Modify: `crates/studio-core/src/large_scene/mod.rs`
- Create: `crates/studio-project/src/geometry_cache.rs`
- Modify: `crates/studio-project/src/lib.rs`
- Modify: `crates/studio-project/src/store.rs`
- Test: `crates/studio-project/tests/geometry_cache.rs`
- Test: `crates/studio-core/tests/large_scene_incremental.rs`

**Interfaces:**
- Produces: `trait GeometryStore { fn load(&self, key: &str) -> Option<EvaluatedMesh>; fn
  store(&mut self, key: String, mesh: EvaluatedMesh); }`
- Produces: `evaluate_large_scene_with_cache<S: GeometryStore>(document: &Document, store: &mut S)
  -> Result<EvaluatedScene, LargeSceneError>`
- Produces: `GeometryDiskCache::new(project_dir: &Path) -> Result<Self, GeometryCacheError>`
- Produces stats: `cache_hits`, `cache_misses`, `generated_meshes`.

- [ ] **Step 1: Write failing cache and invalidation tests**

Create `crates/studio-project/tests/geometry_cache.rs`:

```rust
use studio_core::geometry::{Bounds, EvaluatedMesh, Vertex};
use studio_core::math::Vec3;
use studio_project::geometry_cache::GeometryDiskCache;

#[test]
fn round_trips_an_evaluated_mesh_and_rejects_corruption() {
    let project = tempfile::tempdir().unwrap();
    let mut cache = GeometryDiskCache::new(project.path()).unwrap();
    let mesh = EvaluatedMesh {
        vertices: vec![Vertex { position: [0.0, 0.0, 0.0], normal: [0.0, 1.0, 0.0] }],
        indices: vec![0, 0, 0],
        bounds: Bounds {
            min: Vec3 { x: 0.0, y: 0.0, z: 0.0 },
            max: Vec3 { x: 0.0, y: 0.0, z: 0.0 },
        },
    };
    cache.store("abc".into(), mesh.clone());
    assert_eq!(cache.load("abc"), Some(mesh));

    std::fs::write(project.path().join("cache/geometry/abc.mesh"), b"broken").unwrap();
    assert_eq!(cache.load("abc"), None);
}
```

Create `crates/studio-core/tests/large_scene_incremental.rs`:

```rust
mod support;

use studio_core::large_scene::cache::MemoryGeometryStore;
use studio_core::large_scene::evaluate::evaluate_large_scene_with_cache;
use studio_core::math::Vec3;
use support::shared_pump_document;

#[test]
fn transform_only_change_reuses_the_unchanged_shared_mesh() {
    let mut store = MemoryGeometryStore::default();
    let first_document = shared_pump_document();
    let first = evaluate_large_scene_with_cache(&first_document, &mut store).unwrap();

    let mut second_document = first_document;
    second_document.scene.nodes_mut()[1].transform.translation = Vec3 {
        x: 9.0,
        y: 0.0,
        z: 0.0,
    };
    let second = evaluate_large_scene_with_cache(&second_document, &mut store).unwrap();

    assert_eq!(second.stats.cache_hits, 1);
    assert_eq!(second.stats.generated_meshes, 0);
    assert_eq!(
        first.meshes.keys().collect::<Vec<_>>(),
        second.meshes.keys().collect::<Vec<_>>()
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo test -p studio-core --test large_scene_incremental
cargo test -p studio-project --test geometry_cache
```

Expected: FAIL because the cache trait and disk cache do not exist.

- [ ] **Step 3: Implement the cache interface and binary storage**

Add `large_scene/cache.rs`:

```rust
pub trait GeometryStore {
    fn load(&self, key: &str) -> Option<EvaluatedMesh>;
    fn store(&mut self, key: String, mesh: EvaluatedMesh);
}

#[derive(Default)]
pub struct MemoryGeometryStore {
    entries: BTreeMap<String, EvaluatedMesh>,
}

impl GeometryStore for MemoryGeometryStore {
    fn load(&self, key: &str) -> Option<EvaluatedMesh> {
        self.entries.get(key).cloned()
    }

    fn store(&mut self, key: String, mesh: EvaluatedMesh) {
        self.entries.entry(key).or_insert(mesh);
    }
}
```

Modify `LargeSceneStats`:

```rust
pub cache_hits: usize,
pub cache_misses: usize,
pub generated_meshes: usize,
```

`evaluate_large_scene_with_cache` must look up each geometry key before evaluating it. On a miss,
evaluate and call `store`. Never mutate a mesh obtained from the store.

Implement `GeometryDiskCache` in `studio-project` with one file per key at
`cache/geometry/<key>.mesh`. Use this deterministic format:

```text
bytes  0..8   ASCII "ICE3GEO1"
bytes  8..16  little-endian u64 vertex count
bytes 16..24  little-endian u64 index count
then   vertices, each 6 little-endian f32 values
then   indices, each little-endian u32
then   6 little-endian f32 bounds min/max
```

Write to `<key>.mesh.tmp`, sync/close, and rename atomically. Reject paths containing separators,
incorrect magic, truncated data, non-finite values, invalid indices, oversized counts, and files
larger than the configured cache limit. On corruption, remove the invalid file and return `None`.
Implement `studio_core::large_scene::cache::GeometryStore` for `GeometryDiskCache`, with
`store` using the atomic file format above.

Call `std::fs::create_dir_all(project/cache/geometry)` from `ProjectStore::create` and
`ProjectStore::open`.

- [ ] **Step 4: Run cache and regression tests**

Run:

```bash
cargo test -p studio-core --test large_scene_incremental
cargo test -p studio-project --test geometry_cache
cargo test -p studio-project --test project_store
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-core/src/large_scene crates/studio-core/tests/large_scene_incremental.rs \
  crates/studio-project/src/lib.rs crates/studio-project/src/store.rs \
  crates/studio-project/src/geometry_cache.rs crates/studio-project/tests/geometry_cache.rs
git commit -m "feat(studio-core): reuse evaluated component geometry"
```

---

### Task 8: Export shared meshes and expose evaluated large scenes through the API

**Files:**
- Modify: `crates/studio-core/src/export/glb.rs`
- Modify: `crates/studio-server/src/rpc.rs`
- Modify: `crates/studio-server/src/preview.rs`
- Modify: `crates/studio-server/src/capabilities.rs` if method discovery is duplicated there
- Test: `crates/studio-core/tests/large_scene_glb.rs`
- Test: `crates/studio-server/tests/large_scene_rpc.rs`

**Interfaces:**
- Consumes: `evaluate_large_scene` and stable derived IDs from Tasks 3–7.
- Produces: `export_glb_from_scene(document, evaluated: &EvaluatedScene) -> Result<Vec<u8>,
  ExportError>`
- Keeps `export_glb(document)` as a compatibility wrapper that evaluates with an empty cache.
- `evaluation.evaluate` and `scene.bounds` include derived instance, set-item, part, and route IDs.

- [ ] **Step 1: Write failing export and RPC tests**

Create `crates/studio-core/tests/large_scene_glb.rs`:

```rust
mod support;

use studio_core::export::glb::export_glb;
use serde_json::Value;
use support::shared_pump_document;

fn read_glb_json(bytes: &[u8]) -> Value {
    assert_eq!(&bytes[0..4], b"glTF");
    let json_length = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    let chunk_type = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
    assert_eq!(chunk_type, 0x4E4F_534A);
    serde_json::from_slice(&bytes[20..20 + json_length]).unwrap()
}

#[test]
fn exports_identical_instances_through_one_shared_mesh() {
    let document = shared_pump_document();
    let bytes = export_glb(&document).unwrap();
    let json = read_glb_json(&bytes);
    assert_eq!(json["meshes"].as_array().unwrap().len(), 1);
    assert_eq!(json["nodes"].as_array().unwrap().len(), 2);
    assert_eq!(json["nodes"][0]["mesh"], 0);
    assert_eq!(json["nodes"][1]["mesh"], 0);
    assert_eq!(json["nodes"][0]["extras"]["ice3"]["id"], "node-000001/body");
}
```

Create `crates/studio-server/tests/large_scene_rpc.rs` using the existing test server helper.
Submit one `component.define`, one explicit instance, and one instance set through
`document.apply`; then assert:

```rust
assert!(evaluated["result"]["node-000001/body"].is_object());
assert!(evaluated["result"]["node-000010/000000/body"].is_object());
assert!(bounds["result"]["node-000001/body"].is_object());
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo test -p studio-core --test large_scene_glb
cargo test -p studio-server --test large_scene_rpc
```

Expected: FAIL because export and RPC still iterate only legacy Node content.

- [ ] **Step 3: Refactor export and RPC around `EvaluatedScene`**

Modify `export_glb`:

```rust
pub fn export_glb(document: &Document) -> Result<Vec<u8>, ExportError> {
    let evaluated = crate::large_scene::evaluate::evaluate_large_scene(document)?;
    export_glb_from_scene(document, &evaluated)
}
```

`export_glb_from_scene` must:

1. Emit one glTF mesh per `EvaluatedScene.meshes` key.
2. Emit one glTF Node per `EvaluatedScene.nodes` entry.
3. Point equal derived identities at the same mesh index.
4. Write `extras.ice3.id`, `ownerId`, `name`, `metadata`, `visible`, and source instance identity.
5. Preserve existing materials, hierarchy, and relationship metadata.
6. Fail if a derived Node's `parent_id` does not resolve.

Update `evaluation.evaluate`, `scene.bounds`, and bounds filtering in `scene.query` under
`crates/studio-server/src/rpc.rs` to open the project `GeometryDiskCache`, evaluate the full
Document once per request, and key results by `EvaluatedSceneNode.id`. Build bounds from each
Node's transformed mesh bounds. Do not evaluate each source component instance twice.

Update `export_source` in `crates/studio-server/src/preview.rs` to evaluate through the same
project disk cache and call `export_glb_from_scene`. Keep the `export_glb(document)` compatibility
wrapper for pure Core callers and tests.

Keep `export.sourceGlb` unchanged at the HTTP level. It now receives shared-mesh export.

- [ ] **Step 4: Run focused and legacy tests**

Run:

```bash
cargo test -p studio-core --test large_scene_glb
cargo test -p studio-core --test glb_export
cargo test -p studio-server --test large_scene_rpc
cargo test -p studio-server --test rpc_contract
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/studio-core/src/export/glb.rs crates/studio-core/tests/large_scene_glb.rs \
  crates/studio-server/src/rpc.rs crates/studio-server/src/preview.rs \
  crates/studio-server/src/capabilities.rs \
  crates/studio-server/tests/large_scene_rpc.rs
git commit -m "feat(studio-server): expose shared large-scene geometry"
```

---

### Task 9: Add generated parameter forms to the Workbench

**Files:**
- Modify: `apps/viewer/src/api/StudioClient.ts`
- Modify: `apps/viewer/src/app/Workbench.tsx`
- Create: `apps/viewer/src/forms/instanceParameters.ts`
- Modify: `apps/viewer/src/forms/FormRenderer.tsx`
- Test: `apps/viewer/tests/instance-parameters.test.ts`

**Interfaces:**
- Consumes: generated `ComponentDefinition`, `ComponentInstance`, and `ParameterValue` types.
- Produces: `compileParameterChanges(definition, instance, draft) -> Operation[]`
- Human edits produce the same `instance.update` Operation shape as Agent edits.

- [ ] **Step 1: Write a failing form compiler test**

Create `apps/viewer/tests/instance-parameters.test.ts`:

```typescript
import assert from 'node:assert/strict';
import test from 'node:test';
import type { ComponentDefinition, ComponentInstance } from '../src/api/StudioClient';
import { compileParameterChanges } from '../src/forms/instanceParameters';

test('compiles only changed typed parameters into instance.update', () => {
  const definition: ComponentDefinition = {
    id: 'component-000001',
    name: 'Pump',
    parameters: [{
      name: 'width',
      valueType: { kind: 'number', minimum: 1, maximum: 10 },
      default: { kind: 'number', value: 2 },
      unit: 'm',
    }],
  };
  const instance: ComponentInstance = {
    definitionId: 'component-000001',
    parameters: { width: { kind: 'number', value: 2 } },
    overrides: {},
    metadata: {},
  };
  const operations = compileParameterChanges(
    definition,
    'node-000001',
    instance,
    { width: '4' },
  );
  assert.deepEqual(operations, [{
    type: 'instance.update',
    node_id: 'node-000001',
    instance: {
      ...instance,
      parameters: { width: { kind: 'number', value: 4 } },
    },
  }]);
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
source ~/.nvm/nvm.sh && nvm use 24.22.1 >/dev/null
npm --prefix apps/viewer test
```

Expected: build fails because `instanceParameters.ts` does not exist.

- [ ] **Step 3: Implement typed form compilation and rendering**

In `StudioClient.ts`, add precise types mirroring generated declarations:

```typescript
export type ParameterValue =
  | { kind: 'number'; value: number }
  | { kind: 'vector3'; value: { x: number; y: number; z: number } }
  | { kind: 'color'; value: [number, number, number, number] }
  | { kind: 'enumeration'; value: string };

export type ComponentDefinition = {
  id: string;
  name: string;
  parameters: Array<{
    name: string;
    valueType: Record<string, unknown> & { kind: string };
    default: ParameterValue;
    unit?: string | null;
  }>;
};

export type ComponentInstance = {
  definitionId: string;
  parameters: Record<string, ParameterValue>;
  overrides: Record<string, unknown>;
  metadata: Record<string, string>;
};

export type StudioDocument = {
  projectId: string;
  revision: number;
  scene: { nodes: StudioNode[] };
  components: ComponentDefinition[];
};
```

Implement `compileParameterChanges` to:

- reject unknown names;
- parse and range-check numbers before staging;
- emit `{ kind: 'number', value }` instead of strings;
- clone unchanged instance fields;
- return `[]` when no value changed.

Extend `FormRenderer` with a `definition?: ComponentDefinition` prop and render Number, Vector3,
Color, and Enumeration controls when the selected Node has a component instance. In
`Workbench.tsx`, resolve the selected instance's `definitionId` against
`session.document.components` and pass that definition to `FormRenderer`. Keep existing name and
visibility controls for all Nodes. Pass staged Operations through the existing ChangeSet path. Do
not add a direct document mutation call.

- [ ] **Step 4: Run Viewer tests and typecheck**

Run:

```bash
source ~/.nvm/nvm.sh && nvm use 24.22.1 >/dev/null
npm --prefix apps/viewer test
npm --prefix apps/viewer run typecheck
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/viewer/src/api/StudioClient.ts apps/viewer/src/app/Workbench.tsx apps/viewer/src/forms \
  apps/viewer/tests/instance-parameters.test.ts
git commit -m "feat(studio-workbench): edit typed component parameters"
```

---

### Task 10: Generate contracts, document the capability, and run the complete gate

**Files:**
- Modify: `schemas/studio-api.schema.json`
- Modify: `schemas/studio-types.ts`
- Modify: generated files under `crates/studio-core/bindings/`
- Modify: `docs/reference/api.md`
- Modify: `docs/reference/diagnostics.md`
- Modify: `docs/architecture.md`
- Modify: `docs/README.md`
- Modify: `docs/specs/2026-10-30-large-scene-authoring-design.md`
- Modify: `docs/plans/2026-10-30-large-scene-authoring-foundation.md`

**Interfaces:**
- Consumes all M1 APIs and tests.
- Produces synchronized schemas, accurate documentation, and a fully green repository gate.

- [ ] **Step 1: Regenerate schemas and TypeScript declarations**

Run:

```bash
cargo run -q -p studio-cli -- schema --output schemas
```

Expected: generated schema and declarations include all large-scene Operations and types.

- [ ] **Step 2: Update implemented documentation**

Update `docs/reference/api.md` with the new Operation table, parameter limits, derived identity
format, dry-run behavior, and one compact JSON example for a parameterized component and two
instances.

Update `docs/reference/diagnostics.md` with `ICE3_STUDIO_COMPONENT_INVALID`,
`ICE3_STUDIO_PARAMETER_INVALID`, `ICE3_STUDIO_INSTANCE_INVALID`,
`ICE3_STUDIO_INSTANCE_SET_INVALID`, `ICE3_STUDIO_CONNECTION_INVALID`, and
`ICE3_STUDIO_GEOMETRY_CACHE_FAILED`.

Update `docs/architecture.md` to add the `large_scene` Core module and shared-mesh cache boundary.

This plan originally kept `ICE3-STUDIO-SPEC-005` at `Proposed` because GPU evaluation, the large
fixture and Runtime streaming were not complete. Later plans delivered M2–M4; the spec and this
plan are now marked `Implemented`, with native GPU execution remaining experimental and default-off.

- [ ] **Step 3: Run the complete repository gate**

Run:

```bash
source ~/.nvm/nvm.sh && nvm use 24.22.1 >/dev/null
npm run verify
```

Expected:

- Rust format, lint, tests, schema check, AI fixture API, Viewer unit, typecheck, build, E2E, and
  dependency audit all pass.
- Existing `wastewater-mini` output retains its prior visual and Selection behavior.
- No generated model, cache, or customer asset appears in `git status`.

- [ ] **Step 4: Audit staged content and commit**

Run:

```bash
git status --short
rg -n "TBD|TODO|FIXME|PLACEHOLDER" docs schemas crates apps || true
git diff --check
git add schemas crates/studio-core/bindings docs
git diff --cached --check
git commit -m "docs(studio): document large-scene authoring foundation"
```

Expected: the commit contains generated contracts and documentation only; no cache, GLB, output,
credential, absolute path, or customer asset is staged.

---

## Plan Self-Review

### Spec Coverage

| Spec area | Plan tasks |
|---|---|
| Schema version and migration | 1 |
| Component definition and parameters | 1, 2, 3 |
| Component instances and stable identity | 1, 3 |
| Instance sets and layouts | 1, 4 |
| Connections and routes | 1, 5 |
| Public Operations and transaction semantics | 6 |
| CPU evaluation, shared geometry, cache, invalidation | 3, 4, 7 |
| GLB sharing and API projections | 8 |
| Generated human forms | 9 |
| Contracts, diagnostics, docs, regression gate | 10 |
| Native GPU | Separate M3 plan after M1 acceptance |
| Toolchain partition and Viewer streaming | Separate M4 plan after M1 acceptance |
| Large industrial 5.5M–7.5M triangle fixture | Separate M2 plan after M1 acceptance |

### Type Consistency

- `ComponentId` is introduced in Task 1 and used unchanged thereafter.
- `ComponentInstance` is stored in `NodeContent::ComponentInstance` and updated through
  `instance.update`.
- `InstanceSet` is stored in `NodeContent::InstanceSet`; derived identities use six-digit ordinal
  padding.
- `EvaluatedSceneNode.id` is a `String`; source entity IDs remain typed.
- `ParameterError`, `LayoutError`, `ConnectionError`, and `LargeSceneError` have one owner each
  and are re-exported through the task that defines them.

### Scope Check

This plan produces independently usable scale-out CPU authoring. It deliberately excludes the
large fixture, GPU kernels, Toolchain publication, and Tileset streaming so each later plan can
ship and prove its own behavior.
