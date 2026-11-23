# 人工 Transform 与基础编辑闭环 Implementation Plan

> Document ID: ICE3-STUDIO-PLAN-035
>
> Status: Implemented
>
> Owner: ICE3 Studio maintainers

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. This repository prohibits subagents unless the user explicitly enables them; execute natively.

**Goal:** 让用户在不绕过 ChangeSet 的前提下，用数值输入和视口手柄编辑已有对象的 Transform、图元尺寸、组件参数和材质，并完成撤销、Stage、候选审阅和提交。

**Architecture:** Viewer 新增共享 `EditorDraftStore`，手势和表单只修改本地草稿；`SceneAdapter` 对选中实体应用本地 Transform override；Stage 把草稿编译为最小 typed Operations 并进入现有 ChangeSet。ICE3 Engine 的公开 API、场景结构和渲染管线不修改。

**Tech Stack:** React 19.3.1、TypeScript 7.0.2、esbuild 0.28.2、Playwright 1.64.1、Node test runner、ICE3 Engine 公开 API、Rust Studio Server。

**Spec:** [`docs/specs/2026-11-23-human-transform-editing-design.md`](../specs/2026-11-23-human-transform-editing-design.md)

## Global Constraints

- 只编辑已有对象；创建、删除、复制、重新挂载和多选不在本计划内。
- Transform 只保存 TRS；需要 world-axis shear 的缩放不提供，Scale 始终沿局部轴。
- Viewer 只通过公开 API 提交 Operation，不直接持久化 Document。
- 手势拖动期间不得发送写 RPC。
- ICE3 Engine 的渲染、拾取和公开 API 不修改。
- Stage 只能生成实际变化的字段，并按 `node.update`、`primitive.update`、`instance.update`、`material.assign` 固定顺序排列。
- Stage 后草稿保持 `staged`；commit 成功后清除；ChangeSet discard 后恢复为 dirty。
- Chrome 验证必须使用用户已安装的 Google Chrome，不使用无头浏览器或其它浏览器。
- React/browser 代码不得复制 Core 的几何求值、校验、持久化或 revision 语义。
- 文档、代码、注释、标识和 commit 不得出现禁止名称。
- Git author 和 committer 名称必须为 `damoqiongqiu`。
- 完成标准是根级 `npm run verify` 通过，不接受只跑 Viewer 测试。

## Review Focus

- 选中组件实例或派生部件时必须移动完整业务实体，不能只移动一个 mesh 或让 Selection 与手柄脱节。
- Euler/四元数往返、f32 小噪声和连续拖动不能产生虚假 dirty 或反复改变姿态。
- 编辑期间 revision 前进、对象删除或内容类型变化时，草稿必须进入 stale 而不是覆盖新文档。
- 已有活动 ChangeSet、重复 Stage、discard 和 commit 必须保持草稿与候选状态一致。
- SVG 手柄必须阻止相机旋转但保留场景拾取、Measure、Section 和键盘导航；轴识别不能只依赖颜色。

---

## File Structure

| 文件 | 责任 |
|---|---|
| `apps/viewer/src/editor/types.ts` | 草稿值、模式、状态和公开接口类型 |
| `apps/viewer/src/editor/transformMath.ts` | 向量、四元数、Euler 和吸附纯函数 |
| `apps/viewer/src/editor/draftOperations.ts` | 从 baseline 与草稿编译最小 Operation |
| `apps/viewer/src/editor/EditorDraftStore.ts` | 草稿、命令栈、preview transaction、Stage 状态 |
| `apps/viewer/src/editor/EditorDraftProvider.tsx` | React context 与 external-store hooks |
| `apps/viewer/src/editor/TransformEditor.tsx` | Transform、Geometry、Material、Draft 控件 |
| `apps/viewer/src/viewport/gizmoMath.ts` | 手柄投影、轴命中、屏幕拖动换算纯函数 |
| `apps/viewer/src/viewport/TransformGizmo.ts` | SVG 手柄和 pointer 生命周期 |
| `apps/viewer/src/viewport/SceneAdapter.ts` | 本地节点 Transform override |
| `apps/viewer/src/viewport/Viewport.tsx` | 挂载手柄、传递 override 和编辑状态 |
| `apps/viewer/src/forms/FormRenderer.tsx` | 将名称/显隐/参数表单改为共享草稿快照 |
| `apps/viewer/src/app/Workbench.tsx` | Stage、stale、commit/discard 和页面离开 |
| `apps/viewer/src/main.tsx` | 注入 EditorDraftStore |

---

### Task 1: Transform 数学与草稿领域类型

**Files:**
- Create: `apps/viewer/src/editor/types.ts`
- Create: `apps/viewer/src/editor/transformMath.ts`
- Test: `apps/viewer/tests/transform-math.test.ts`

**Interfaces:**
- Consumes: `StudioNode`、`StudioTransform`、`StudioVec3`、`ComponentDefinition`、`ComponentInstance`
- Produces:
  - `type GizmoMode = 'move' | 'rotate' | 'scale'`
  - `type TransformSpace = 'local' | 'world'`
  - `type PrimitiveValue`
  - `type EditorValues`
  - `createEditorValues(node: StudioNode, definition?: ComponentDefinition): EditorValues`
  - `quaternionToEulerDegrees(rotation: readonly [number, number, number, number]): StudioVec3`
  - `eulerDegreesToQuaternion(rotation: StudioVec3): [number, number, number, number]`
  - `quatFromAxisAngleDegrees(axis: StudioVec3, degrees: number): [number, number, number, number]`
  - `transformsEqual(left: StudioTransform, right: StudioTransform, epsilon?: number): boolean`
  - `snapValue(value: number, step: number): number`

- [ ] **Step 1: Write failing transform math tests**

Create `apps/viewer/tests/transform-math.test.ts`:

```ts
import assert from 'node:assert/strict';
import test from 'node:test';
import {
  eulerDegreesToQuaternion,
  quatFromAxisAngleDegrees,
  quaternionToEulerDegrees,
  snapValue,
  transformsEqual,
} from '../src/editor/transformMath';

const almost = (actual: number, expected: number, tolerance = 1e-5): void => {
  assert.ok(
    Math.abs(actual - expected) <= tolerance,
    `${actual} != ${expected} within ${tolerance}`,
  );
};

test('round-trips a 90 degree Y rotation without changing quaternion sign semantics', () => {
  const quaternion = quatFromAxisAngleDegrees({ x: 0, y: 1, z: 0 }, 90);
  const euler = quaternionToEulerDegrees(quaternion);
  almost(euler.x, 0);
  almost(euler.y, 90);
  almost(euler.z, 0);
  const rebuilt = eulerDegreesToQuaternion(euler);
  almost(Math.abs(rebuilt[0]), Math.abs(quaternion[0]));
  almost(Math.abs(rebuilt[1]), Math.abs(quaternion[1]));
  almost(Math.abs(rebuilt[2]), Math.abs(quaternion[2]));
  almost(Math.abs(rebuilt[3]), Math.abs(quaternion[3]));
});

test('ignores f32 noise inside transform equality epsilon', () => {
  assert.equal(transformsEqual(
    {
      translation: { x: 1, y: 2, z: 3 },
      rotation: [0, 0, 0, 1],
      scale: { x: 1, y: 1, z: 1 },
    },
    {
      translation: { x: 1.0000004, y: 2, z: 3 },
      rotation: [0, 0, 0, 1.0000002],
      scale: { x: 1, y: 1, z: 1 },
    },
  ), true);
});

test('snaps positive and negative values deterministically', () => {
  assert.equal(snapValue(1.27, 0.1), 1.3);
  assert.equal(snapValue(-1.27, 0.1), -1.3);
  assert.equal(snapValue(14.9, 15), 15);
});
```

- [ ] **Step 2: Run the test and verify it fails**

Run:

```bash
cd apps/viewer
node esbuild-tests.mjs
node --test test-dist/transform-math.test.js
```

Expected: FAIL because `src/editor/transformMath.ts` does not exist.

- [ ] **Step 3: Add the domain types**

Create `apps/viewer/src/editor/types.ts`:

```ts
import type {
  ComponentDefinition,
  ComponentInstance,
  StudioNode,
  StudioTransform,
  StudioVec3,
} from '../api/StudioClient';

export type GizmoMode = 'move' | 'rotate' | 'scale';
export type TransformSpace = 'local' | 'world';

export type PrimitiveValue =
  | { type: 'primitive'; kind: 'box'; width: number; height: number; depth: number }
  | { type: 'primitive'; kind: 'plane'; width: number; depth: number }
  | { type: 'primitive'; kind: 'cylinder'; radius: number; height: number; segments: number }
  | { type: 'primitive'; kind: 'sphere'; radius: number; segments: number; rings: number };

export type EditorValues = {
  name: string;
  visible: boolean;
  transform: StudioTransform;
  primitive: PrimitiveValue | null;
  componentParameters: Record<string, string>;
  materialId: string | null;
};

export type EditorDraftStatus = 'clean' | 'dirty' | 'staged' | 'stale' | 'failed';

export type EditorDraft = {
  nodeId: string;
  baseRevision: number;
  baseNode: StudioNode;
  values: EditorValues;
  history: EditorValues[];
  future: EditorValues[];
  status: EditorDraftStatus;
  error: string | null;
};

export function createEditorValues(
  node: StudioNode,
  definition?: ComponentDefinition,
): EditorValues {
  const instance = node.content?.type === 'component_instance'
    ? node.content as unknown as ComponentInstance
    : null;
  return {
    name: node.name,
    visible: node.visible ?? true,
    transform: structuredClone(node.transform ?? identityTransform()),
    primitive: readPrimitive(node.content),
    componentParameters: Object.fromEntries(
      definition?.parameters.map((parameter) => [
        parameter.name,
        formatDraftValue(instance?.parameters[parameter.name] ?? parameter.default),
      ]) ?? [],
    ),
    materialId: node.materialId ?? null,
  };
}

function readPrimitive(content: StudioNode['content']): PrimitiveValue | null {
  if (!content || content.type !== 'primitive' || typeof content.kind !== 'string') {
    return null;
  }
  const fields = content as Record<string, unknown>;
  const numeric = (key: string): number => {
    const value = fields[key];
    if (typeof value !== 'number') throw new Error(`Primitive ${key} is missing`);
    return value;
  };
  if (content.kind === 'box') {
    return { type: 'primitive', kind: 'box', width: numeric('width'), height: numeric('height'), depth: numeric('depth') };
  }
  if (content.kind === 'plane') {
    return { type: 'primitive', kind: 'plane', width: numeric('width'), depth: numeric('depth') };
  }
  if (content.kind === 'cylinder') {
    return { type: 'primitive', kind: 'cylinder', radius: numeric('radius'), height: numeric('height'), segments: numeric('segments') };
  }
  if (content.kind === 'sphere') {
    return { type: 'primitive', kind: 'sphere', radius: numeric('radius'), segments: numeric('segments'), rings: numeric('rings') };
  }
  return null;
}

function formatDraftValue(value: unknown): string {
  if (typeof value === 'number') return String(Number(value.toFixed(6)));
  if (typeof value === 'string') return value;
  if (Array.isArray(value)) return value.map(formatDraftValue).join(', ');
  if (value && typeof value === 'object') {
    const point = value as StudioVec3;
    if ('x' in point && 'y' in point && 'z' in point) {
      return `${formatDraftValue(point.x)}, ${formatDraftValue(point.y)}, ${formatDraftValue(point.z)}`;
    }
  }
  return String(value);
}

function identityTransform(): StudioTransform {
  return {
    translation: { x: 0, y: 0, z: 0 },
    rotation: [0, 0, 0, 1],
    scale: { x: 1, y: 1, z: 1 },
  };
}
```

- [ ] **Step 4: Implement transform math**

Create `apps/viewer/src/editor/transformMath.ts`:

```ts
import type { StudioTransform, StudioVec3 } from '../api/StudioClient';

export function quaternionToEulerDegrees(
  rotation: readonly [number, number, number, number],
): StudioVec3 {
  const [x, y, z, w] = normalizeQuaternion(rotation);
  const sinX = 2 * (w * x + y * z);
  const cosX = 1 - 2 * (x * x + y * y);
  const sinY = 2 * (w * y - z * x);
  const sinZ = 2 * (w * z + x * y);
  const cosZ = 1 - 2 * (y * y + z * z);
  return {
    x: radToDeg(Math.atan2(sinX, cosX)),
    y: radToDeg(Math.asin(clamp(sinY, -1, 1))),
    z: radToDeg(Math.atan2(sinZ, cosZ)),
  };
}

export function eulerDegreesToQuaternion(
  rotation: StudioVec3,
): [number, number, number, number] {
  const halfX = degToRad(rotation.x) / 2;
  const halfY = degToRad(rotation.y) / 2;
  const halfZ = degToRad(rotation.z) / 2;
  const cx = Math.cos(halfX);
  const sx = Math.sin(halfX);
  const cy = Math.cos(halfY);
  const sy = Math.sin(halfY);
  const cz = Math.cos(halfZ);
  const sz = Math.sin(halfZ);
  return normalizeQuaternion([
    sx * cy * cz + cx * sy * sz,
    cx * sy * cz - sx * cy * sz,
    cx * cy * sz + sx * sy * cz,
    cx * cy * cz - sx * sy * sz,
  ]);
}

export function quatFromAxisAngleDegrees(
  axis: StudioVec3,
  degrees: number,
): [number, number, number, number] {
  const length = Math.hypot(axis.x, axis.y, axis.z);
  if (!(length > 0)) return [0, 0, 0, 1];
  const half = degToRad(degrees) / 2;
  const sine = Math.sin(half) / length;
  return [axis.x * sine, axis.y * sine, axis.z * sine, Math.cos(half)];
}

export function transformsEqual(
  left: StudioTransform,
  right: StudioTransform,
  epsilon = 1e-6,
): boolean {
  return vectorEqual(left.translation, right.translation, epsilon)
    && quaternionEqual(left.rotation, right.rotation, epsilon)
    && vectorEqual(left.scale, right.scale, epsilon);
}

export function snapValue(value: number, step: number): number {
  if (!(step > 0)) throw new RangeError('snap step must be positive');
  return Number((Math.round(value / step) * step).toFixed(6));
}

function normalizeQuaternion(
  value: readonly [number, number, number, number],
): [number, number, number, number] {
  const length = Math.hypot(...value);
  if (!(length > 0)) return [0, 0, 0, 1];
  return [
    value[0] / length,
    value[1] / length,
    value[2] / length,
    value[3] / length,
  ];
}

function quaternionEqual(
  left: readonly [number, number, number, number],
  right: readonly [number, number, number, number],
  epsilon: number,
): boolean {
  const a = normalizeQuaternion(left);
  const b = normalizeQuaternion(right);
  const direct = a.every((value, index) => Math.abs(value - b[index]!) <= epsilon);
  const negated = a.every((value, index) => Math.abs(value + b[index]!) <= epsilon);
  return direct || negated;
}

function vectorEqual(left: StudioVec3, right: StudioVec3, epsilon: number): boolean {
  return Math.abs(left.x - right.x) <= epsilon
    && Math.abs(left.y - right.y) <= epsilon
    && Math.abs(left.z - right.z) <= epsilon;
}

function degToRad(value: number): number {
  return value * Math.PI / 180;
}

function radToDeg(value: number): number {
  return value * 180 / Math.PI;
}

function clamp(value: number, minimum: number, maximum: number): number {
  return Math.max(minimum, Math.min(maximum, value));
}
```

- [ ] **Step 5: Run the focused test**

Run:

```bash
cd apps/viewer
node esbuild-tests.mjs
node --test test-dist/transform-math.test.js
```

Expected: 3 tests pass.

- [ ] **Step 6: Commit**

```bash
git add apps/viewer/src/editor/types.ts apps/viewer/src/editor/transformMath.ts apps/viewer/tests/transform-math.test.ts
git commit -m "feat(editor): add transform math and draft domain types"
```

---

### Task 2: 草稿到最小 Operation 的编译

**Files:**
- Create: `apps/viewer/src/editor/draftOperations.ts`
- Test: `apps/viewer/tests/draft-operations.test.ts`
- Modify: `apps/viewer/src/forms/instanceParameters.ts`

**Interfaces:**
- Consumes:
  - `EditorValues` from Task 1
  - `compileParameterChanges` from `instanceParameters.ts`
- Produces:
  - `compileDraftOperations(base: StudioNode, values: EditorValues, definition?: ComponentDefinition): Operation[]`
  - `validateEditorValues(values: EditorValues): string | null`

- [ ] **Step 1: Write failing Operation compiler tests**

Create `apps/viewer/tests/draft-operations.test.ts`:

```ts
import assert from 'node:assert/strict';
import test from 'node:test';
import type { ComponentDefinition, StudioNode } from '../src/api/StudioClient';
import { compileDraftOperations, validateEditorValues } from '../src/editor/draftOperations';
import { createEditorValues } from '../src/editor/types';

const node: StudioNode = {
  id: 'node-000001',
  name: 'Tank',
  parentId: null,
  visible: true,
  materialId: 'material-000001',
  metadata: {},
  transform: {
    translation: { x: 1, y: 2, z: 3 },
    rotation: [0, 0, 0, 1],
    scale: { x: 1, y: 1, z: 1 },
  },
  content: {
    type: 'primitive',
    kind: 'box',
    width: 2,
    height: 3,
    depth: 4,
  },
};

test('emits one node update and one primitive update for changed fields', () => {
  const values = createEditorValues(node);
  values.name = 'Tank revised';
  values.transform.translation.x = 5;
  if (values.primitive?.kind !== 'box') throw new Error('box expected');
  values.primitive.width = 6;
  assert.deepEqual(compileDraftOperations(node, values), [
    {
      type: 'node.update',
      nodeId: 'node-000001',
      name: 'Tank revised',
      transform: {
        translation: { x: 5, y: 2, z: 3 },
        rotation: [0, 0, 0, 1],
        scale: { x: 1, y: 1, z: 1 },
      },
      visible: null,
      metadata: null,
    },
    {
      type: 'primitive.update',
      nodeId: 'node-000001',
      primitive: { kind: 'box', width: 6, height: 3, depth: 4 },
    },
  ]);
});

test('emits material assignment only when material changes', () => {
  const values = createEditorValues(node);
  values.materialId = 'material-000002';
  assert.deepEqual(compileDraftOperations(node, values), [{
    type: 'material.assign',
    nodeId: 'node-000001',
    materialId: 'material-000002',
  }]);
});

test('emits component parameter operations after geometry operations', () => {
  const definition: ComponentDefinition = {
    id: 'component-000001',
    name: 'Pump',
    parameters: [{
      name: 'radius',
      valueType: { kind: 'number', minimum: 1, maximum: 10 },
      default: 2,
      unit: 'm',
    }],
  };
  const instanceNode: StudioNode = {
    ...node,
    content: {
      type: 'component_instance',
      definitionId: definition.id,
      parameters: { radius: 2 },
      overrides: {},
      metadata: {},
    },
  };
  const values = createEditorValues(instanceNode, definition);
  values.componentParameters.radius = '4';
  assert.equal(compileDraftOperations(instanceNode, values, definition)[0]?.type, 'instance.update');
});

test('rejects non-finite transform, invalid scale and non-positive primitive dimensions', () => {
  const values = createEditorValues(node);
  values.transform.translation.x = Number.NaN;
  assert.match(validateEditorValues(values) ?? '', /finite/i);
  values.transform.translation.x = 0;
  values.transform.scale.x = 0;
  assert.match(validateEditorValues(values) ?? '', /scale/i);
  values.transform.scale.x = 1;
  if (values.primitive?.kind === 'box') values.primitive.width = 0;
  assert.match(validateEditorValues(values) ?? '', /dimension/i);
});
```

- [ ] **Step 2: Run the test and verify it fails**

Run:

```bash
cd apps/viewer
node esbuild-tests.mjs
node --test test-dist/draft-operations.test.js
```

Expected: FAIL because `draftOperations.ts` does not exist.

- [ ] **Step 3: Export parameter value compilation**

Modify `apps/viewer/src/forms/instanceParameters.ts` to add:

```ts
export function compileTypedParameterChanges(
  definition: ComponentDefinition,
  nodeId: string,
  instance: ComponentInstance,
  draft: ParameterDraft,
): Operation[] {
  return compileParameterChanges(definition, nodeId, instance, draft);
}
```

The implementation remains the existing `compileParameterChanges`; no duplicate parser is allowed.

- [ ] **Step 4: Implement draft Operation compilation**

Create `apps/viewer/src/editor/draftOperations.ts`:

```ts
import type {
  ComponentDefinition,
  ComponentInstance,
  StudioNode,
} from '../api/StudioClient';
import { compileTypedParameterChanges } from '../forms/instanceParameters';
import { compileNodeChanges, type Operation } from '../forms/operationCompiler';
import { transformsEqual } from './transformMath';
import type { EditorValues } from './types';

export function compileDraftOperations(
  base: StudioNode,
  values: EditorValues,
  definition?: ComponentDefinition,
): Operation[] {
  const error = validateEditorValues(values);
  if (error) throw new Error(error);
  const operations: Operation[] = [];
  const transformChanged = !transformsEqual(
    base.transform ?? identityTransform(),
    values.transform,
  );
  const nodeChanges = compileNodeChanges(base, {
    name: values.name,
    visible: values.visible,
  });
  if (nodeChanges.length > 0 || transformChanged) {
    const existing = nodeChanges[0] ?? {
      type: 'node.update',
      nodeId: base.id,
      name: null,
      transform: null,
      visible: null,
      metadata: null,
    };
    operations.push({
      ...existing,
      transform: transformChanged ? values.transform : null,
    });
  }
  if (values.primitive && !sameJson(base.content, values.primitive)) {
    operations.push({
      type: 'primitive.update',
      nodeId: base.id,
      primitive: {
        kind: values.primitive.kind,
        ...Object.fromEntries(Object.entries(values.primitive).filter(([key]) => key !== 'type' && key !== 'kind')),
      },
    });
  }
  const instance = base.content?.type === 'component_instance'
    ? base.content as unknown as ComponentInstance
    : null;
  if (instance && definition) {
    operations.push(...compileTypedParameterChanges(
      definition,
      base.id,
      instance,
      values.componentParameters,
    ));
  }
  if ((base.materialId ?? null) !== values.materialId) {
    operations.push({
      type: 'material.assign',
      nodeId: base.id,
      materialId: values.materialId,
    });
  }
  return operations;
}

export function validateEditorValues(values: EditorValues): string | null {
  const numbers = [
    values.transform.translation.x,
    values.transform.translation.y,
    values.transform.translation.z,
    ...values.transform.rotation,
    values.transform.scale.x,
    values.transform.scale.y,
    values.transform.scale.z,
  ];
  if (!numbers.every(Number.isFinite)) return 'Transform values must be finite';
  if (
    values.transform.scale.x <= 0
    || values.transform.scale.y <= 0
    || values.transform.scale.z <= 0
  ) {
    return 'Scale values must be positive';
  }
  if (values.primitive) {
    const positive = Object.entries(values.primitive)
      .filter(([key]) => key !== 'type' && key !== 'kind' && key !== 'segments' && key !== 'rings')
      .map(([, value]) => value);
    if (!positive.every((value) => typeof value === 'number' && Number.isFinite(value) && value > 0)) {
      return 'Primitive dimensions must be finite and positive';
    }
  }
  return null;
}

function sameJson(left: unknown, right: unknown): boolean {
  return JSON.stringify(left) === JSON.stringify(right);
}

function identityTransform() {
  return {
    translation: { x: 0, y: 0, z: 0 },
    rotation: [0, 0, 0, 1] as [number, number, number, number],
    scale: { x: 1, y: 1, z: 1 },
  };
}
```

- [ ] **Step 5: Run focused tests and typecheck**

```bash
cd apps/viewer
node esbuild-tests.mjs
node --test test-dist/draft-operations.test.js
npm run typecheck
```

Expected: all new tests pass and typecheck exits 0.

- [ ] **Step 6: Commit**

```bash
git add apps/viewer/src/editor/draftOperations.ts apps/viewer/tests/draft-operations.test.ts apps/viewer/src/forms/instanceParameters.ts
git commit -m "feat(editor): compile draft values into minimal operations"
```

---

### Task 3: 草稿 Store 与本地撤销/重做

**Files:**
- Create: `apps/viewer/src/editor/EditorDraftStore.ts`
- Test: `apps/viewer/tests/editor-draft-store.test.ts`

**Interfaces:**
- Consumes:
  - `EditorDraft`、`EditorValues`、`createEditorValues`
- Produces:
  - `EditorDraftSnapshot { drafts: Readonly<Record<string, EditorDraft>> }`
  - `EditorDraftStore.ensure(node: StudioNode, revision: number): EditorValues`
  - `EditorDraftStore.commit(nodeId: string, next: EditorValues): void`
  - `EditorDraftStore.beginPreview(nodeId: string): void`
  - `EditorDraftStore.updatePreview(nodeId: string, next: EditorValues): void`
  - `EditorDraftStore.finishPreview(nodeId: string): void`
  - `EditorDraftStore.cancelPreview(nodeId: string): void`
  - `EditorDraftStore.undo(nodeId: string): void`
  - `EditorDraftStore.redo(nodeId: string): void`
  - `EditorDraftStore.markStaged(nodeId: string): void`
  - `EditorDraftStore.markStale(nodeId: string, message: string): void`
  - `EditorDraftStore.clear(nodeId: string): void`
  - `EditorDraftStore.discard(nodeId: string): void`

- [ ] **Step 1: Write failing store tests**

Create `apps/viewer/tests/editor-draft-store.test.ts`:

```ts
import assert from 'node:assert/strict';
import test from 'node:test';
import type { StudioNode } from '../src/api/StudioClient';
import { EditorDraftStore } from '../src/editor/EditorDraftStore';
import { createEditorValues } from '../src/editor/types';

const node: StudioNode = {
  id: 'node-000001',
  name: 'Tank',
  parentId: null,
  visible: true,
  metadata: {},
  transform: {
    translation: { x: 0, y: 0, z: 0 },
    rotation: [0, 0, 0, 1],
    scale: { x: 1, y: 1, z: 1 },
  },
  content: { type: 'primitive', kind: 'box', width: 1, height: 1, depth: 1 },
};

test('commits clean, undo and redo values', () => {
  const store = new EditorDraftStore();
  const initial = store.ensure(node, 1);
  store.commit(node.id, { ...initial, name: 'Tank A' });
  assert.equal(store.snapshot.drafts[node.id]?.values.name, 'Tank A');
  assert.equal(store.snapshot.drafts[node.id]?.status, 'dirty');
  store.undo(node.id);
  assert.equal(store.snapshot.drafts[node.id]?.values.name, 'Tank');
  store.redo(node.id);
  assert.equal(store.snapshot.drafts[node.id]?.values.name, 'Tank A');
});

test('records one history entry for a continuous preview gesture', () => {
  const store = new EditorDraftStore();
  const initial = store.ensure(node, 1);
  store.beginPreview(node.id);
  store.updatePreview(node.id, {
    ...initial,
    transform: { ...initial.transform, translation: { x: 1, y: 0, z: 0 } },
  });
  store.updatePreview(node.id, {
    ...initial,
    transform: { ...initial.transform, translation: { x: 2, y: 0, z: 0 } },
  });
  store.finishPreview(node.id);
  store.undo(node.id);
  assert.equal(
    store.snapshot.drafts[node.id]?.values.transform.translation.x,
    0,
  );
});

test('cancel preview restores the gesture start without history', () => {
  const store = new EditorDraftStore();
  const initial = store.ensure(node, 1);
  store.beginPreview(node.id);
  store.updatePreview(node.id, {
    ...initial,
    transform: { ...initial.transform, translation: { x: 9, y: 0, z: 0 } },
  });
  store.cancelPreview(node.id);
  assert.equal(store.snapshot.drafts[node.id]?.values.transform.translation.x, 0);
  assert.equal(store.snapshot.drafts[node.id]?.history.length, 0);
});

test('marks revision mismatch stale and retains staged values until clear', () => {
  const store = new EditorDraftStore();
  store.ensure(node, 1);
  store.markStaged(node.id);
  assert.equal(store.snapshot.drafts[node.id]?.status, 'staged');
  store.markStale(node.id, 'Project advanced');
  assert.equal(store.snapshot.drafts[node.id]?.status, 'stale');
  store.clear(node.id);
  assert.equal(store.snapshot.drafts[node.id], undefined);
});

test('uses createEditorValues as the baseline helper', () => {
  assert.deepEqual(createEditorValues(node).transform, node.transform);
});
```

- [ ] **Step 2: Run the test and verify it fails**

```bash
cd apps/viewer
node esbuild-tests.mjs
node --test test-dist/editor-draft-store.test.js
```

Expected: FAIL because `EditorDraftStore.ts` does not exist.

- [ ] **Step 3: Implement the store**

Create `apps/viewer/src/editor/EditorDraftStore.ts` using `ExternalStore` and the following core methods:

```ts
export type EditorDraftSnapshot = {
  drafts: Readonly<Record<string, EditorDraft>>;
};

export class EditorDraftStore extends ExternalStore<EditorDraftSnapshot> {
  private readonly previewOrigins = new Map<string, EditorValues>();

  constructor() {
    super({ drafts: {} });
  }

  ensure(node: StudioNode, revision: number): EditorValues {
    const current = this.snapshot.drafts[node.id];
    if (current) {
      if (current.baseRevision !== revision && current.status !== 'stale') {
        this.markStale(node.id, 'Project revision changed');
      }
      return structuredClone(this.snapshot.drafts[node.id]!.values);
    }
    const draft: EditorDraft = {
      nodeId: node.id,
      baseRevision: revision,
      baseNode: structuredClone(node),
      values: createEditorValues(node),
      history: [],
      future: [],
      status: 'clean',
      error: null,
    };
    this.setDraft(draft);
    return structuredClone(draft.values);
  }

  commit(nodeId: string, next: EditorValues): void {
    const draft = this.requireDraft(nodeId);
    this.setDraft({
      ...draft,
      history: [...draft.history, draft.values].slice(-100),
      future: [],
      values: structuredClone(next),
      status: 'dirty',
      error: null,
    });
  }

  beginPreview(nodeId: string): void {
    this.previewOrigins.set(nodeId, structuredClone(this.requireDraft(nodeId).values));
  }

  updatePreview(nodeId: string, next: EditorValues): void {
    const draft = this.requireDraft(nodeId);
    if (!this.previewOrigins.has(nodeId)) {
      this.previewOrigins.set(nodeId, structuredClone(draft.values));
    }
    this.setDraft({
      ...draft,
      values: structuredClone(next),
      status: 'dirty',
      error: null,
    });
  }

  finishPreview(nodeId: string): void {
    const origin = this.previewOrigins.get(nodeId);
    this.previewOrigins.delete(nodeId);
    const draft = this.requireDraft(nodeId);
    if (!origin || JSON.stringify(origin) === JSON.stringify(draft.values)) return;
    this.setDraft({
      ...draft,
      history: [...draft.history, origin].slice(-100),
      future: [],
    });
  }

  cancelPreview(nodeId: string): void {
    const origin = this.previewOrigins.get(nodeId);
    this.previewOrigins.delete(nodeId);
    if (!origin) return;
    const draft = this.requireDraft(nodeId);
    this.setDraft({ ...draft, values: origin, status: 'dirty' });
  }

  undo(nodeId: string): void {
    const draft = this.requireDraft(nodeId);
    const previous = draft.history.at(-1);
    if (!previous) return;
    this.setDraft({
      ...draft,
      values: structuredClone(previous),
      history: draft.history.slice(0, -1),
      future: [draft.values, ...draft.future].slice(0, 100),
      status: 'dirty',
    });
  }

  redo(nodeId: string): void {
    const draft = this.requireDraft(nodeId);
    const next = draft.future[0];
    if (!next) return;
    this.setDraft({
      ...draft,
      values: structuredClone(next),
      history: [...draft.history, draft.values].slice(-100),
      future: draft.future.slice(1),
      status: 'dirty',
    });
  }

  markStaged(nodeId: string): void {
    const draft = this.requireDraft(nodeId);
    this.setDraft({ ...draft, status: 'staged', error: null });
  }

  markStale(nodeId: string, message: string): void {
    const draft = this.requireDraft(nodeId);
    this.setDraft({ ...draft, status: 'stale', error: message });
  }

  discard(nodeId: string): void {
    this.clear(nodeId);
  }

  clear(nodeId: string): void {
    const drafts = { ...this.snapshot.drafts };
    delete drafts[nodeId];
    this.previewOrigins.delete(nodeId);
    this.update({ drafts });
  }

  private requireDraft(nodeId: string): EditorDraft {
    const draft = this.snapshot.drafts[nodeId];
    if (!draft) throw new Error(`Editor draft does not exist: ${nodeId}`);
    return draft;
  }

  private setDraft(draft: EditorDraft): void {
    this.update({
      drafts: { ...this.snapshot.drafts, [draft.nodeId]: draft },
    });
  }
}
```

- [ ] **Step 4: Add discard/dirty helpers used by Workbench**

Add these public methods:

```ts
restoreStagedAsDirty(): void {
  const drafts = Object.fromEntries(
    Object.entries(this.snapshot.drafts).map(([nodeId, draft]) => [
      nodeId,
      draft.status === 'staged' ? { ...draft, status: 'dirty' as const } : draft,
    ]),
  );
  this.update({ drafts });
}

hasDirtyDrafts(): boolean {
  return Object.values(this.snapshot.drafts).some((draft) => (
    draft.status === 'dirty' || draft.status === 'staged'
  ));
}
```

- [ ] **Step 5: Run focused tests and typecheck**

```bash
cd apps/viewer
node esbuild-tests.mjs
node --test test-dist/editor-draft-store.test.js
npm run typecheck
```

Expected: all store tests pass; typecheck exits 0.

- [ ] **Step 6: Commit**

```bash
git add apps/viewer/src/editor/EditorDraftStore.ts apps/viewer/tests/editor-draft-store.test.ts
git commit -m "feat(editor): add shared draft history store"
```

---

### Task 4: SceneAdapter 本地 Transform Preview

**Files:**
- Modify: `apps/viewer/src/viewport/SceneAdapter.ts`
- Test: `apps/viewer/tests/scene-transform-override.test.ts`

**Interfaces:**
- Consumes: `StudioTransform`、`StudioVec3`、engine `ICEScene`、`ICENode`
- Produces:
  - `applySceneTransformOverride(scene: ICEScene, nodeId: string, transform: StudioTransform, originals: Map<ICENode, number[]>): ICENode | null`
  - `restoreSceneTransformOverrides(scene: ICEScene, originals: Map<ICENode, number[]>): void`
  - `SceneAdapter.setTransformOverride(nodeId: string | null, transform: StudioTransform | null): void`
  - `SceneAdapter.clearTransformOverride(): void`
  - `SceneAdapter.getTransformFrame(nodeId: string): { origin: [number, number, number]; parentMatrix: number[] } | null`

- [ ] **Step 1: Write failing scene override tests**

Create `apps/viewer/tests/scene-transform-override.test.ts`:

```ts
import assert from 'node:assert/strict';
import test from 'node:test';
import { ICENode, ICEScene } from 'ice3';
import {
  applySceneTransformOverride,
  restoreSceneTransformOverrides,
} from '../src/viewport/SceneAdapter';

test('moves the exact selected scene node and restores its matrix', () => {
  const scene = new ICEScene({ id: 'test-scene' });
  const node = new ICENode({ id: 'node-000001', displayName: 'Tank', selectionId: 'node-000001' });
  scene.addChild(node);
  const originals = new Map();
  const moved = applySceneTransformOverride(scene, 'node-000001', {
    translation: { x: 2, y: 3, z: 4 },
    rotation: [0, 0, 0, 1],
    scale: { x: 1, y: 1, z: 1 },
  }, originals);
  assert.equal(moved, node);
  assert.equal(node.getWorldPosition().x, 2);
  assert.equal(node.getWorldPosition().y, 3);
  assert.equal(node.getWorldPosition().z, 4);
  restoreSceneTransformOverrides(scene, originals);
  assert.equal(node.getWorldPosition().x, 0);
});

test('returns null for an unavailable node', () => {
  const scene = new ICEScene({ id: 'test-scene' });
  assert.equal(applySceneTransformOverride(scene, 'missing', {
    translation: { x: 0, y: 0, z: 0 },
    rotation: [0, 0, 0, 1],
    scale: { x: 1, y: 1, z: 1 },
  }, new Map()), null);
});
```

- [ ] **Step 2: Run the test and verify it fails**

```bash
cd apps/viewer
node esbuild-tests.mjs
node --test test-dist/scene-transform-override.test.js
```

Expected: FAIL because the exported helpers do not exist.

- [ ] **Step 3: Implement exported scene helpers**

Add to `SceneAdapter.ts`:

```ts
export function applySceneTransformOverride(
  scene: ICEScene,
  nodeId: string,
  transform: StudioTransform,
  originals: Map<ICENode, number[]>,
): ICENode | null {
  const node = findSceneNodeBySelectionId(scene, nodeId);
  if (!node) return null;
  if (!originals.has(node)) originals.set(node, localMatrixOf(node));
  node.setLocalMatrix(transformMatrix(transform) as never);
  node.updateWorldMatrix();
  return node;
}

export function restoreSceneTransformOverrides(
  scene: ICEScene,
  originals: Map<ICENode, number[]>,
): void {
  for (const [node, matrix] of originals) {
    node.setLocalMatrix(matrix as never);
    node.updateWorldMatrix();
  }
  originals.clear();
}

function findSceneNodeBySelectionId(scene: ICEScene, nodeId: string): ICENode | null {
  const visit = (node: ICENode): ICENode | null => {
    if (node.selectionId === nodeId) return node;
    for (const child of node.children) {
      const found = visit(child);
      if (found) return found;
    }
    return null;
  };
  for (const child of scene.children) {
    const found = visit(child);
    if (found) return found;
  }
  return null;
}

function identityMatrix(): number[] {
  return [
    1, 0, 0, 0,
    0, 1, 0, 0,
    0, 0, 1, 0,
    0, 0, 0, 1,
  ];
}
```

Add class methods:

```ts
private readonly transformOverrideOriginals = new Map<ICENode, number[]>();

setTransformOverride(nodeId: string | null, transform: StudioTransform | null): void {
  this.clearTransformOverride();
  if (!nodeId || !transform || !this.currentScene) return;
  applySceneTransformOverride(
    this.currentScene,
    nodeId,
    transform,
    this.transformOverrideOriginals,
  );
  this.engine?.requestRender('draft-transform-preview');
}

clearTransformOverride(): void {
  if (!this.currentScene || this.transformOverrideOriginals.size === 0) return;
  restoreSceneTransformOverrides(this.currentScene, this.transformOverrideOriginals);
  this.engine?.requestRender('draft-transform-clear');
}

getTransformFrame(nodeId: string): {
  origin: [number, number, number];
  parentMatrix: number[];
} | null {
  if (!this.currentScene) return null;
  const node = findSceneNodeBySelectionId(this.currentScene, nodeId);
  if (!node) return null;
  const position = node.getWorldPosition();
  return {
    origin: [position.x, position.y, position.z],
    parentMatrix: node.parent
      ? [...node.parent.updateWorldMatrix().elements]
      : identityMatrix(),
  };
}
```

`bindScene` must clear override state before adopting a new scene. If the exact group node is not found,
the method returns null and the Inspector numeric path remains usable; the final candidate remains authoritative.

- [ ] **Step 4: Ensure component entities use the source group**

Add a focused assertion using a parent group with selection ID `node-000001` and a child mesh selection ID `node-000001/part-1`:

```ts
test('moves the source entity group containing derived children', () => {
  const scene = new ICEScene({ id: 'test-scene' });
  const source = new ICENode({ id: 'source', displayName: 'Pump', selectionId: 'node-000001' });
  source.addChild(new ICENode({ id: 'part', displayName: 'Pump part', selectionId: 'node-000001/part-1' }));
  scene.addChild(source);
  const originals = new Map();
  const moved = applySceneTransformOverride(scene, 'node-000001', {
    translation: { x: 5, y: 0, z: 0 },
    rotation: [0, 0, 0, 1],
    scale: { x: 1, y: 1, z: 1 },
  }, originals);
  assert.equal(moved, source);
  assert.equal(source.getWorldPosition().x, 5);
});
```

- [ ] **Step 5: Run focused tests and typecheck**

```bash
cd apps/viewer
node esbuild-tests.mjs
node --test test-dist/scene-transform-override.test.js
npm run typecheck
```

Expected: 3 tests pass; typecheck exits 0.

- [ ] **Step 6: Commit**

```bash
git add apps/viewer/src/viewport/SceneAdapter.ts apps/viewer/tests/scene-transform-override.test.ts
git commit -m "feat(viewer): preview draft transforms in the scene"
```

---

### Task 5: EditorDraft Provider 与 Store 注入

**Files:**
- Create: `apps/viewer/src/editor/EditorDraftProvider.tsx`
- Modify: `apps/viewer/src/main.tsx`
- Test: `apps/viewer/tests/editor-draft-store.test.ts`

**Interfaces:**
- Consumes: `EditorDraftStore`
- Produces:
  - `EditorDraftProvider({ store, children })`
  - `useEditorDrafts(): EditorDraftSnapshot`
  - `useEditorDraftStore(): EditorDraftStore`

- [ ] **Step 1: Add a failing provider contract test**

Append to `editor-draft-store.test.ts`:

```ts
test('store snapshots are replaced so React consumers rerender', () => {
  const store = new EditorDraftStore();
  const before = store.snapshot;
  store.ensure(node, 1);
  assert.notEqual(store.snapshot, before);
});
```

Run:

```bash
cd apps/viewer
node esbuild-tests.mjs
node --test test-dist/editor-draft-store.test.js
```

Expected: PASS after Task 3 already replaced the snapshot.

- [ ] **Step 2: Implement provider**

Create `EditorDraftProvider.tsx`:

```tsx
import { createContext, useContext, useSyncExternalStore, type ReactNode } from 'react';
import type { EditorDraftSnapshot, EditorDraftStore } from './EditorDraftStore';

const Context = createContext<EditorDraftStore | null>(null);

export function EditorDraftProvider({
  store,
  children,
}: {
  store: EditorDraftStore;
  children: ReactNode;
}) {
  return <Context.Provider value={store}>{children}</Context.Provider>;
}

export function useEditorDraftStore(): EditorDraftStore {
  const store = useContext(Context);
  if (!store) throw new Error('EditorDraftProvider is missing');
  return store;
}

export function useEditorDrafts(): EditorDraftSnapshot {
  const store = useEditorDraftStore();
  return useSyncExternalStore(store.subscribe, () => store.snapshot);
}
```

- [ ] **Step 3: Inject one store in main**

Modify `main.tsx`:

```tsx
import { EditorDraftProvider } from './editor/EditorDraftProvider';
import { EditorDraftStore } from './editor/EditorDraftStore';

const editorDraft = new EditorDraftStore();

createRoot(root).render(
  <SessionProvider store={session}>
    <EditorDraftProvider store={editorDraft}>
      <RouteInspectionProvider store={routeInspection}>
        <ChangeSetProvider store={changeSet}>
          <Workbench />
        </ChangeSetProvider>
      </RouteInspectionProvider>
    </EditorDraftProvider>
  </SessionProvider>,
);
```

- [ ] **Step 4: Run typecheck and build**

```bash
npm --prefix apps/viewer run typecheck
npm --prefix apps/viewer run build
```

Expected: both commands exit 0.

- [ ] **Step 5: Commit**

```bash
git add apps/viewer/src/editor/EditorDraftProvider.tsx apps/viewer/src/main.tsx apps/viewer/tests/editor-draft-store.test.ts
git commit -m "feat(editor): provide shared draft state to the workbench"
```

---

### Task 6: Inspector 草稿表单

**Files:**
- Create: `apps/viewer/src/editor/TransformEditor.tsx`
- Modify: `apps/viewer/src/forms/FormRenderer.tsx`
- Modify: `apps/viewer/src/app/studioTokens.css`
- Test: `apps/viewer/e2e/viewer.spec.ts`

**Interfaces:**
- Consumes:
  - `EditorValues`
  - `EditorDraft`
  - `StudioMaterial[]`
  - `GizmoMode`
  - `TransformSpace`
- Produces:
  - `TransformEditor` props:
    - `values: EditorValues`
    - `draft: EditorDraft`
    - `materials: readonly StudioMaterial[]`
    - `mode: GizmoMode`
    - `space: TransformSpace`
    - `onValues(values: EditorValues): void`
    - `onPreviewStart(): void`
    - `onMode(mode: GizmoMode): void`
    - `onSpace(space: TransformSpace): void`
    - `onUndo(): void`
    - `onRedo(): void`
    - `onStage(): void`
    - `onDiscard(): void`

- [ ] **Step 1: Update the existing E2E with label expectations**

In `apps/viewer/e2e/viewer.spec.ts`, extend the form test after selecting `Secondary Clarifier B`:

```ts
await expect(page.getByLabel('Position X')).toBeVisible();
await expect(page.getByLabel('Rotation Y')).toBeVisible();
await expect(page.getByLabel('Scale Z')).toBeVisible();
await expect(page.getByRole('button', { name: 'Stage draft' })).toBeVisible();
await expect(page.getByRole('button', { name: 'Move mode' })).toBeVisible();
```

Run:

```bash
cd apps/viewer
PLAYWRIGHT_CHANNEL=chrome npm run test:e2e -- --grep "stages, reviews"
```

Expected before implementation: FAIL because the controls do not exist.

- [ ] **Step 2: Implement numeric Transform editing**

Create `TransformEditor.tsx` with deterministic labels and parsing:

```tsx
import { useEffect, useState } from 'react';
import type { StudioMaterial, StudioVec3 } from '../api/StudioClient';
import {
  eulerDegreesToQuaternion,
  quaternionToEulerDegrees,
} from './transformMath';
import type {
  EditorDraft,
  EditorValues,
  GizmoMode,
  TransformSpace,
} from './types';

type Props = {
  values: EditorValues;
  draft: EditorDraft;
  materials: readonly StudioMaterial[];
  mode: GizmoMode;
  space: TransformSpace;
  onValues(values: EditorValues): void;
  onPreviewStart(): void;
  onMode(mode: GizmoMode): void;
  onSpace(space: TransformSpace): void;
  onUndo(): void;
  onRedo(): void;
  onStage(): void;
  onDiscard(): void;
};

export function TransformEditor(props: Props) {
  const euler = quaternionToEulerDegrees(props.values.transform.rotation);
  const updateNumber = (
    section: 'translation' | 'rotation' | 'scale',
    axis: keyof StudioVec3,
    raw: string,
  ): void => {
    const value = Number(raw);
    if (!Number.isFinite(value)) return;
    const next = structuredClone(props.values);
    if (section === 'rotation') {
      const nextEuler = { ...euler, [axis]: value };
      next.transform.rotation = eulerDegreesToQuaternion(nextEuler);
    } else {
      next.transform[section] = { ...next.transform[section], [axis]: value };
    }
    props.onValues(next);
  };
  return (
    <section className="editor-section" aria-label="Transform editor">
      <header className="editor-section__heading">
        <span className="panel-kicker">Transform</span>
        <div className="segmented-control" aria-label="Transform mode">
          {(['move', 'rotate', 'scale'] as const).map((mode) => (
            <button key={mode} type="button" aria-label={`${capitalize(mode)} mode`} aria-pressed={props.mode === mode} onClick={() => props.onMode(mode)}>
              {capitalize(mode)}
            </button>
          ))}
        </div>
      </header>
      <TransformRow label="Position" prefix="Position" values={props.values.transform.translation} onChange={(axis, raw) => updateNumber('translation', axis, raw)} />
      <TransformRow label="Rotation" prefix="Rotation" values={euler} unit="deg" onChange={(axis, raw) => updateNumber('rotation', axis, raw)} />
      <TransformRow label="Scale" prefix="Scale" values={props.values.transform.scale} onChange={(axis, raw) => updateNumber('scale', axis, raw)} />
      <label className="field">
        <span>Transform space</span>
        <select value={props.space} onChange={(event) => props.onSpace(event.target.value as TransformSpace)}>
          <option value="local">Local</option>
          <option value="world">World</option>
        </select>
      </label>
      <DraftActions {...props} />
    </section>
  );
}
```

Add the complete `TransformRow`, primitive rows, material select and `DraftActions` implementations in the same file. Every numeric input must use `aria-label={`${prefix} ${axis.toUpperCase()}`}` and format to three decimals.

- [ ] **Step 3: Add primitive and material controls**

For primitive values render only fields belonging to the current `kind`:

```tsx
{props.values.primitive && (
  <fieldset className="parameter-group">
    <legend><span>Geometry</span><small>{props.values.primitive.kind}</small></legend>
    {primitiveFields(props.values.primitive).map(([key, value]) => (
      <label className="field" key={key}>
        <span>{formatLabel(key)}</span>
        <input
          type="number"
          aria-label={`Geometry ${formatLabel(key)}`}
          value={value}
          onChange={(event) => {
            const next = structuredClone(props.values);
            if (next.primitive) (next.primitive as Record<string, unknown>)[key] = Number(event.target.value);
            props.onValues(next);
          }}
        />
      </label>
    ))}
  </fieldset>
)}
<label className="field">
  <span>Material</span>
  <select value={props.values.materialId ?? ''} onChange={(event) => {
    const next = structuredClone(props.values);
    next.materialId = event.target.value || null;
    props.onValues(next);
  }}>
    <option value="">No material</option>
    {props.materials.map((material) => (
      <option value={material.id} key={material.id}>{material.name}</option>
    ))}
  </select>
</label>
```

- [ ] **Step 4: Replace direct-stage FormRenderer behavior**

Modify `FormRenderer.tsx` so its name/visible/parameter inputs call a supplied `onValues(values)` instead of calling `onStage(operations)`. `TransformEditor` owns Stage.

The props become:

```ts
type Props = {
  node: StudioNode | undefined;
  definition?: ComponentDefinition;
  values?: EditorValues;
  onValues(values: EditorValues): void;
};
```

`Workbench` supplies `draft.values`; `FormRenderer` never calls `changeSetStore` directly.

- [ ] **Step 5: Add editor CSS**

Add to `studioTokens.css`:

```css
.editor-section {
  display: grid;
  gap: var(--space-3);
  padding-bottom: var(--space-4);
  border-bottom: 1px solid var(--line);
}

.editor-section__heading,
.editor-actions {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
}

.transform-row {
  display: grid;
  grid-template-columns: 72px repeat(3, minmax(0, 1fr));
  gap: var(--space-2);
}

.transform-row input {
  min-width: 0;
}

.segmented-control {
  display: flex;
  gap: 2px;
  padding: 2px;
  border-radius: var(--radius-md);
  background: var(--canvas);
}

.segmented-control button[aria-pressed="true"] {
  background: var(--accent-soft);
  color: var(--accent-strong);
}
```

- [ ] **Step 6: Run typecheck, build and focused Chrome E2E**

```bash
npm --prefix apps/viewer run typecheck
npm --prefix apps/viewer run build
cd apps/viewer
PLAYWRIGHT_CHANNEL=chrome npm run test:e2e -- --grep "stages, reviews"
```

Expected: controls render, existing form flow remains usable, and no write request is introduced by input focus.

- [ ] **Step 7: Commit**

```bash
git add apps/viewer/src/editor/TransformEditor.tsx apps/viewer/src/forms/FormRenderer.tsx apps/viewer/src/app/studioTokens.css apps/viewer/e2e/viewer.spec.ts
git commit -m "feat(viewer): edit transform, primitive and material drafts"
```

---

### Task 7: 视口 Move/Rotate/Scale 手柄

**Files:**
- Create: `apps/viewer/src/viewport/gizmoMath.ts`
- Create: `apps/viewer/src/viewport/TransformGizmo.ts`
- Modify: `apps/viewer/src/viewport/Viewport.tsx`
- Test: `apps/viewer/tests/gizmo-math.test.ts`

**Interfaces:**
- Consumes:
  - `GizmoMode`
  - `TransformSpace`
  - `EditorValues`
  - `SceneAdapter.projectPoint`, `SceneAdapter.setTransformOverride`, `SceneAdapter.getTransformFrame`
- Produces:
  - `projectAxisHandle(origin: ScreenPoint, endpoint: ScreenPoint): ProjectedHandle`
  - `screenDragToWorldDelta(drag, axis, worldLength): number`
  - `angleDegreesFromScreen(pivot, start, current): number`
  - `TransformGizmo` class:
    - `setActive(active: boolean): void`
    - `setTarget(nodeId: string | null, transform: StudioTransform | null): void`
    - `setMode(mode: GizmoMode): void`
    - `setSpace(space: TransformSpace): void`
    - `dispose(): void`

- [ ] **Step 1: Write failing gizmo math tests**

Create `apps/viewer/tests/gizmo-math.test.ts`:

```ts
import assert from 'node:assert/strict';
import test from 'node:test';
import {
  angleDegreesFromScreen,
  projectAxisHandle,
  screenDragToWorldDelta,
} from '../src/viewport/gizmoMath';

test('projects a visible axis and reports disabled axes', () => {
  const visible = projectAxisHandle(
    { x: 100, y: 100, visible: true },
    { x: 180, y: 100, visible: true },
  );
  assert.equal(visible.enabled, true);
  assert.equal(visible.lengthPixels, 80);
  const hidden = projectAxisHandle(
    { x: 100, y: 100, visible: true },
    { x: 103, y: 101, visible: true },
  );
  assert.equal(hidden.enabled, false);
});

test('converts pointer movement along a projected axis into world distance', () => {
  const delta = screenDragToWorldDelta(
    { startX: 100, startY: 100, currentX: 180, currentY: 100 },
    { startX: 100, startY: 100, endX: 180, endY: 100, enabled: true },
    8,
  );
  assert.equal(delta, 8);
});

test('computes signed screen angle in degrees', () => {
  const angle = angleDegreesFromScreen(
    { x: 0, y: 0 },
    { x: 10, y: 0 },
    { x: 0, y: 10 },
  );
  assert.equal(angle, 90);
});
```

- [ ] **Step 2: Run the test and verify it fails**

```bash
cd apps/viewer
node esbuild-tests.mjs
node --test test-dist/gizmo-math.test.js
```

Expected: FAIL because `gizmoMath.ts` does not exist.

- [ ] **Step 3: Implement gizmo math**

Create `gizmoMath.ts` with pure functions:

```ts
export type ScreenPoint = { x: number; y: number; visible: boolean };
export type ProjectedHandle = {
  startX: number;
  startY: number;
  endX: number;
  endY: number;
  lengthPixels: number;
  enabled: boolean;
};

export function projectAxisHandle(
  origin: ScreenPoint,
  endpoint: ScreenPoint,
): ProjectedHandle {
  const lengthPixels = Math.hypot(endpoint.x - origin.x, endpoint.y - origin.y);
  return {
    startX: origin.x,
    startY: origin.y,
    endX: endpoint.x,
    endY: endpoint.y,
    lengthPixels,
    enabled: origin.visible && endpoint.visible && lengthPixels >= 12,
  };
}

export function screenDragToWorldDelta(
  drag: { startX: number; startY: number; currentX: number; currentY: number },
  axis: ProjectedHandle,
  worldLength: number,
): number {
  if (!axis.enabled || !(axis.lengthPixels > 0)) return 0;
  const axisX = (axis.endX - axis.startX) / axis.lengthPixels;
  const axisY = (axis.endY - axis.startY) / axis.lengthPixels;
  const pixels = (drag.currentX - drag.startX) * axisX
    + (drag.currentY - drag.startY) * axisY;
  return pixels / axis.lengthPixels * worldLength;
}

export function angleDegreesFromScreen(
  pivot: { x: number; y: number },
  start: { x: number; y: number },
  current: { x: number; y: number },
): number {
  const startAngle = Math.atan2(start.y - pivot.y, start.x - pivot.x);
  const currentAngle = Math.atan2(current.y - pivot.y, current.x - pivot.x);
  return ((currentAngle - startAngle) * 180 / Math.PI + 360) % 360;
}
```

- [ ] **Step 4: Implement SVG overlay**

Create `TransformGizmo.ts` with:

- `<svg class="transform-gizmo">` appended after the engine canvas.
- One `<g data-gizmo-axis="x|y|z|free">` per handle.
- Text labels `X`, `Y`, `Z`, and mode label.
- `pointerdown` calls `stopPropagation`, `preventDefault`, `setPointerCapture`.
- `pointermove` calls `adapter.setTransformOverride(nodeId, nextTransform)`.
- `pointerup` calls `onPreviewEnd(nextTransform)`.
- `Escape` calls `onCancel()`.
- Empty or disabled axes set `aria-disabled="true"` and `pointer-events: none`.

The component callback shape is:

```ts
type TransformGizmoOptions = {
  beginPreview(): void;
  preview(transform: StudioTransform): void;
  finishPreview(transform: StudioTransform): void;
  cancelPreview(): void;
};
```

- [ ] **Step 5: Mount and update the gizmo**

Instantiate the gizmo after `SceneAdapter` and dispose it with the existing viewport tools:

```ts
const gizmo = host.current
  ? new TransformGizmo(host.current, adapter, {
      beginPreview: () => onTransformPreviewStart?.(),
      preview: (transform) => onTransformPreview?.(transform),
      finishPreview: (transform) => onTransformPreviewEnd?.(transform),
      cancelPreview: () => onTransformPreviewCancel?.(),
    })
  : null;
```

Add props and effects:

```ts
useEffect(() => {
  gizmo?.setActive(Boolean(transformTarget) && active);
  gizmo?.setTarget(transformTarget?.nodeId ?? null, transformTarget?.transform ?? null);
}, [active, transformTarget]);

useEffect(() => {
  gizmo?.setMode(mode);
}, [mode]);

useEffect(() => {
  gizmo?.setSpace(space);
}, [space]);
```

Dispose the gizmo in the mount effect cleanup.

- [ ] **Step 6: Add gizmo CSS and run tests**

Add CSS:

```css
.transform-gizmo {
  position: absolute;
  inset: 0;
  z-index: 6;
  width: 100%;
  height: 100%;
  pointer-events: none;
  overflow: visible;
}

.transform-gizmo [data-gizmo-axis] {
  pointer-events: stroke;
  cursor: grab;
}

.transform-gizmo [data-gizmo-axis="x"] { stroke: #ef6b63; }
.transform-gizmo [data-gizmo-axis="y"] { stroke: #70c879; }
.transform-gizmo [data-gizmo-axis="z"] { stroke: #5aa7ea; }
.transform-gizmo [aria-disabled="true"] { opacity: 0.28; pointer-events: none; }
.transform-gizmo text { fill: currentColor; font-size: 11px; paint-order: stroke; }
```

Run:

```bash
cd apps/viewer
node esbuild-tests.mjs
node --test test-dist/gizmo-math.test.js
npm run typecheck
npm run build
```

Expected: math tests pass, typecheck/build exit 0.

- [ ] **Step 7: Commit**

```bash
git add apps/viewer/src/viewport/gizmoMath.ts apps/viewer/src/viewport/TransformGizmo.ts apps/viewer/src/viewport/Viewport.tsx apps/viewer/src/app/studioTokens.css apps/viewer/tests/gizmo-math.test.ts
git commit -m "feat(viewer): add draggable transform gizmo"
```

---

### Task 8: Workbench Stage、ChangeSet 与 Chrome 编辑闭环

**Files:**
- Modify: `apps/viewer/src/app/Workbench.tsx`
- Modify: `apps/viewer/src/changeset/ChangeSetStore.ts`
- Modify: `apps/viewer/src/changeset/types.ts`
- Modify: `apps/viewer/src/api/StudioClient.ts`
- Create: `apps/viewer/e2e/transform-editing.spec.ts`

**Interfaces:**
- Consumes:
  - `EditorDraftStore`
  - `compileDraftOperations`
  - `ChangeSetStore.addOperations`
  - `ChangeSetStore.discard`
- Produces:
  - `Stage draft` creates Operations and leaves the draft `staged`.
  - `Discard draft` clears local values.
  - ChangeSet discard restores staged drafts to dirty.
  - Commit clears staged drafts.

- [ ] **Step 1: Add failing Chrome E2E**

Create `apps/viewer/e2e/transform-editing.spec.ts`:

```ts
import { expect, test } from '@playwright/test';

test('edits a primitive draft, reviews the candidate and commits', async ({ page }) => {
  const writes: string[] = [];
  page.on('request', (request) => {
    if (request.method() === 'POST' && request.url().endsWith('/rpc')) {
      const body = request.postDataJSON() as { method?: string };
      if (body.method && !body.method.startsWith('system.') && !body.method.startsWith('document.get') && !body.method.startsWith('validation.') && !body.method.startsWith('document.history')) {
        writes.push(body.method);
      }
    }
  });
  await page.goto('/');
  await page.getByRole('button', { name: 'Aeration Tank A' }).click();
  await page.getByRole('tab', { name: 'Inspect' }).click();
  writes.length = 0;
  const x = page.getByLabel('Position X');
  const original = Number(await x.inputValue());
  await x.fill(String(original + 5));
  await x.press('Enter');
  await expect(page.getByLabel('Position X')).toHaveValue(String(original + 5));
  expect(writes).toEqual([]);

  await page.getByRole('button', { name: 'Stage draft' }).click();
  await expect(page.getByText('staged', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: 'Review candidate' }).click();
  await page.getByRole('button', { name: 'Approve & commit' }).click();
  await expect(page.getByRole('status')).toContainText('Revision');
  await expect(page.getByRole('button', { name: 'Stage draft' })).toBeDisabled();
});

test('undo and redo a numeric transform edit', async ({ page }) => {
  await page.goto('/');
  await page.getByRole('button', { name: 'Aeration Tank B' }).click();
  await page.getByRole('tab', { name: 'Inspect' }).click();
  const input = page.getByLabel('Scale X');
  const original = Number(await input.inputValue());
  await input.fill(String(original + 0.25));
  await input.press('Enter');
  await page.getByRole('button', { name: 'Undo draft' }).click();
  await expect(input).toHaveValue(original.toFixed(3).replace(/0+$/, '').replace(/\.$/, ''));
  await page.getByRole('button', { name: 'Redo draft' }).click();
  await expect(input).toHaveValue((original + 0.25).toFixed(3).replace(/0+$/, '').replace(/\.$/, ''));
});
```

- [ ] **Step 2: Run E2E and verify it fails**

```bash
cd apps/viewer
PLAYWRIGHT_CHANNEL=chrome npm run test:e2e -- --grep "primitive draft|numeric transform"
```

Expected: FAIL because Stage, draft controls and Workbench orchestration are absent.

- [ ] **Step 3: Add ChangeSet discard support**

Add to `StudioClient`:

```ts
discardChangeSet(changeSetId: string, expectedVersion: number): Promise<{ discarded: boolean }> {
  return this.rpc('changeset.discard', {
    changeSetId,
    expectedVersion,
    idempotencyKey: `viewer-discard-${changeSetId}-${expectedVersion}`,
  });
}
```

Add optional `discardChangeSet` to `ChangeSetClient` and `ChangeSetStore.discard()`:

```ts
async discard(): Promise<void> {
  const active = this.snapshot.active;
  if (!active || !this.client.discardChangeSet) throw new Error('No discardable ChangeSet');
  await this.client.discardChangeSet(active.changeSetId, active.version);
  this.update({
    ...this.snapshot,
    active: null,
    record: null,
    status: 'discarded',
    candidateUrl: null,
    canApprove: false,
    impact: null,
  });
}
```

- [ ] **Step 4: Wire Draft → Scene → ChangeSet in Workbench**

In `Workbench.tsx`:

```tsx
const editorDrafts = useEditorDrafts();
const editorDraftStore = useEditorDraftStore();
const definition = selected?.content?.type === 'component_instance'
  ? session.document?.components.find((item) => item.id === selected.content?.definitionId)
  : undefined;
const draft = selected ? editorDrafts.drafts[selected.id] : undefined;
const values = draft?.values;
useEffect(() => {
  if (dock === 'inspect' && selected && !editorDrafts.drafts[selected.id]) {
    editorDraftStore.ensure(selected, session.revision);
  }
}, [dock, editorDraftStore, editorDrafts.drafts, selected, session.revision]);
const commitDraftValue = (next: EditorValues, transient = false) => {
  if (!selected) return;
  if (transient) editorDraftStore.updatePreview(selected.id, next);
  else {
    editorDraftStore.ensure(selected, session.revision);
    editorDraftStore.commit(selected.id, next);
  }
};
const ensureDraft = (): EditorDraft | null => {
  if (!selected) return null;
  editorDraftStore.ensure(selected, session.revision);
  return editorDraftStore.snapshot.drafts[selected.id] ?? null;
};
const stageDraft = async () => {
  const current = draft ?? ensureDraft();
  if (!current || current.status === 'stale') return;
  const operations = compileDraftOperations(current.baseNode, current.values, definition);
  try {
    await changeSetStore.addOperations(session.revision, operations);
    editorDraftStore.markStaged(current.nodeId);
    setDock('changes');
  } catch (error) {
    editorDraftStore.markStageFailed(
      current.nodeId,
      error instanceof Error ? error.message : String(error),
    );
  }
};
```

Add `markStageFailed(nodeId, message)` to the store with status `failed` and the supplied error.

- [ ] **Step 5: Connect the TransformEditor and gizmo callbacks**

Replace the old direct `FormRenderer.onStage` path:

```tsx
{values && draft && (
  <>
    <TransformEditor
      values={values}
      draft={draft}
      materials={session.document?.materials ?? []}
      mode={gizmoMode}
      space={transformSpace}
      onValues={(next) => commitDraftValue(next)}
      onPreviewStart={() => editorDraftStore.beginPreview(selected!.id)}
      onMode={setGizmoMode}
      onSpace={setTransformSpace}
      onUndo={() => editorDraftStore.undo(selected!.id)}
      onRedo={() => editorDraftStore.redo(selected!.id)}
      onStage={() => void stageDraft()}
      onDiscard={() => editorDraftStore.discard(selected!.id)}
    />
    <FormRenderer node={selected} definition={definition} values={values} onValues={(next) => editorDraftStore.commit(selected!.id, next)} />
  </>
)}
```

Pass:

```tsx
transformTarget={dock === 'inspect' && selected && values
  ? { nodeId: selected.id, transform: values.transform }
  : null}
transformMode={gizmoMode}
transformSpace={transformSpace}
onTransformPreviewStart={() => selected && editorDraftStore.beginPreview(selected.id)}
onTransformPreview={(transform) => {
  if (!selected || !values) return;
  editorDraftStore.updatePreview(selected.id, {
    ...values,
    transform: structuredClone(transform),
  });
}}
onTransformPreviewEnd={() => selected && editorDraftStore.finishPreview(selected.id)}
onTransformPreviewCancel={() => selected && editorDraftStore.cancelPreview(selected.id)}
```

- [ ] **Step 6: Add stale, commit and page-leave handling**

Add an effect:

```tsx
useEffect(() => {
  if (!session.document) return;
  for (const draft of Object.values(editorDrafts.drafts)) {
    const current = session.document.scene.nodes.find((node) => node.id === draft.nodeId);
    if (!current) {
      editorDraftStore.markStale(draft.nodeId, 'Object no longer exists');
    } else if (draft.baseRevision !== session.revision && draft.status !== 'staged') {
      editorDraftStore.markStale(draft.nodeId, 'Project revision changed');
    }
  }
}, [editorDraftStore, editorDrafts, session.document, session.revision]);

useEffect(() => {
  const guard = (event: BeforeUnloadEvent): void => {
    if (!editorDraftStore.hasDirtyDrafts()) return;
    event.preventDefault();
    event.returnValue = '';
  };
  window.addEventListener('beforeunload', guard);
  return () => window.removeEventListener('beforeunload', guard);
}, [editorDraftStore]);
```

After successful commit:

```ts
if (selected) {
  editorDraftStore.clear(selected.id);
  editorDraftStore.ensure(selected, session.revision);
}
```

Guard `clear` against an empty ID. After `changeSetStore.discard()`, call
`editorDraftStore.restoreStagedAsDirty()`.

- [ ] **Step 7: Run focused Chrome E2E**

```bash
cd apps/viewer
PLAYWRIGHT_CHANNEL=chrome npm run test:e2e -- --grep "primitive draft|numeric transform"
```

Expected: both tests pass; numeric input generates no write request before Stage.

- [ ] **Step 8: Commit**

```bash
git add apps/viewer/src/app/Workbench.tsx apps/viewer/src/changeset/ChangeSetStore.ts apps/viewer/src/changeset/types.ts apps/viewer/src/api/StudioClient.ts apps/viewer/e2e/transform-editing.spec.ts
git commit -m "feat(viewer): complete human editing change-set flow"
```

---

### Task 9: 文档、视觉验收与完整门禁

**Files:**
- Modify: `README.md`
- Modify: `docs/reference/capabilities.md`
- Modify: `docs/architecture.md`
- Modify: `docs/README.md`
- Verify: `apps/viewer/e2e/transform-editing.spec.ts`

**Interfaces:**
- Consumes: all prior tasks
- Produces: implemented capability docs and verified branch

- [ ] **Step 1: Update capability documentation**

Add to README section 4:

```markdown
| Inspect | 人工 Transform、图元尺寸、组件参数和材质草稿，支持视口手柄、本地撤销和 Stage |
```

Add to `docs/reference/capabilities.md`:

```markdown
## Viewer 人工编辑

- Inspect 支持已有节点的 Transform、图元尺寸、组件参数和材质分配。
- Transform 手柄支持 move/rotate/scale、local/world、Shift 吸附和 Esc 取消。
- 草稿通过 `node.update`、`primitive.update`、`instance.update`、`material.assign`
  进入既有 ChangeSet；不新增写 RPC。
- 创建、删除、复制、重新挂载、多选和草稿磁盘持久化仍未实现。
```

In `docs/architecture.md`, add a short subsection under Viewer stating local drafts are not the
Document source of truth and are cleared or marked stale by revision changes.

- [ ] **Step 2: Update statuses after implementation**

Change:

- `ICE3-STUDIO-SPEC-038` from `Approved` to `Implemented`;
- this plan from `Proposed` to `Implemented`;
- index rows to the same statuses.

- [ ] **Step 3: Run focused and whole Viewer verification**

```bash
npm --prefix apps/viewer run typecheck
npm --prefix apps/viewer test
npm --prefix apps/viewer run build
cd apps/viewer
PLAYWRIGHT_CHANNEL=chrome npm run test:e2e
```

Expected: all pass, every E2E uses installed Google Chrome.

- [ ] **Step 4: Perform Chrome visual checks**

Run the latest project with `npm run dev`, then in installed Google Chrome check:

- 1440px: Inspector numeric grid, material select and all three gizmo modes;
- 1024px: right dock remains usable and gizmo does not overlap Section/Explode;
- 768px: controls remain scrollable and touch targets are at least 44px high where responsive CSS applies;
- keyboard focus on every numeric input and action button;
- Measure and Section still operate while the gizmo is visible.

Capture evidence under `output/playwright/transform-editing/`.

- [ ] **Step 5: Run documentation and repository verification**

```bash
npm run docs:check
npm run verify
```

Expected: both commands exit 0. If `npm run verify` exposes unrelated pre-existing failures,
record the exact failing check and do not mark the plan Implemented.

- [ ] **Step 6: Commit**

```bash
git add README.md docs/README.md docs/reference/capabilities.md docs/architecture.md docs/specs/2026-11-23-human-transform-editing-design.md docs/plans/2026-11-23-human-transform-editing.md
git commit -m "docs(editor): mark human editing flow implemented"
```

---

## Plan Self-Review

### Spec coverage

| Spec requirement | Task |
|---|---|
| 已有节点 Transform 数值编辑 | Task 1, Task 6 |
| Move/Rotate/Scale 视口手柄 | Task 7 |
| local/world、Shift 吸附、Esc 取消 | Task 1, Task 6, Task 7 |
| Box/Plane/Cylinder/Sphere 尺寸 | Task 2, Task 6 |
| 组件参数进入同一草稿 | Task 2, Task 6 |
| 现有材质分配 | Task 2, Task 6 |
| 本地草稿撤销/重做 | Task 3, Task 8 |
| Stage 最小 Operation 集 | Task 2, Task 8 |
| 候选预览、审阅和提交 | Task 8 |
| staged/dirty/stale/commit/discard 生命周期 | Task 3, Task 8 |
| 页面离开确认 | Task 8 |
| 拖动期间零写 RPC | Task 7, Task 8 |
| Chrome E2E 与完整门禁 | Task 8, Task 9 |
| 文档状态与索引 | Task 9 |

### Placeholder scan

The plan contains no `TBD`, `TODO`, `FIXME`, deferred implementation placeholder or unspecified
test command. Each task names exact files, interfaces, test intent and commit command.

### Type consistency

- `EditorValues` is defined once in `types.ts` and consumed by operations, store, Inspector and
  SceneAdapter callbacks.
- `GizmoMode` distinguishes UI mode; `TransformSpace` distinguishes local/world.
- `gizmoMath` projected handle fields are `startX/startY/endX/endY/lengthPixels/enabled`, matching
  the test and TransformGizmo usage.
- `SceneAdapter` transform helpers accept `StudioTransform` and return `ICENode | null`.
- `markStaged`, `markStale`, `markStageFailed`, `restoreStagedAsDirty` and `clear` are the only draft
  lifecycle methods used by Workbench.

### Review focus tests

- Derived entity movement: Task 4 parent-group test.
- f32/Euler noise: Task 1 transform equality and round-trip tests.
- revision/object stale: Task 3 revision mismatch and Task 8 Workbench effect.
- active ChangeSet/discard/commit: Task 8 stage, commit and discard flow.
- gizmo interception and axis availability: Task 7 math tests plus Task 9 Chrome interaction checks.
