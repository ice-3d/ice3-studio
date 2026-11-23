import type {
  ComponentDefinition,
  ComponentInstance,
  StudioNode,
  StudioTransform,
} from '../api/StudioClient';
import { compileTypedParameterChanges } from '../forms/instanceParameters';
import { compileNodeChanges, type Operation } from '../forms/operationCompiler';
import { transformsEqual } from './transformMath';
import type { EditorValues, PrimitiveValue } from './types';

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
      transform: transformChanged ? structuredClone(values.transform) : null,
    });
  }
  if (values.primitive && primitiveChanged(base, values.primitive)) {
    operations.push({
      type: 'primitive.update',
      nodeId: base.id,
      primitive: primitiveOperation(values.primitive),
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
    const dimensions = primitiveDimensions(values.primitive);
    if (!dimensions.every((value) => Number.isFinite(value) && value > 0)) {
      return 'Primitive dimensions must be finite and positive';
    }
    const tessellation = primitiveTessellation(values.primitive);
    if (!tessellation.every((value) => Number.isInteger(value) && value > 0)) {
      return 'Primitive segment counts must be positive integers';
    }
  }
  return null;
}

function primitiveChanged(base: StudioNode, next: PrimitiveValue): boolean {
  const current = base.content;
  if (!current || current.type !== 'primitive' || current.kind !== next.kind) return true;
  const fields = current as Record<string, unknown>;
  return Object.entries(next).some(([key, value]) => (
    key !== 'type' && key !== 'kind' && fields[key] !== value
  ));
}

function primitiveOperation(next: PrimitiveValue): Record<string, unknown> {
  return {
    kind: next.kind,
    ...Object.fromEntries(
      Object.entries(next).filter(([key]) => key !== 'type' && key !== 'kind'),
    ),
  };
}

function primitiveDimensions(value: PrimitiveValue): number[] {
  if (value.kind === 'box') return [value.width, value.height, value.depth];
  if (value.kind === 'plane') return [value.width, value.depth];
  if (value.kind === 'cylinder') return [value.radius, value.height];
  return [value.radius];
}

function primitiveTessellation(value: PrimitiveValue): number[] {
  if (value.kind === 'cylinder') return [value.segments];
  if (value.kind === 'sphere') return [value.segments, value.rings];
  return [];
}

function identityTransform(): StudioTransform {
  return {
    translation: { x: 0, y: 0, z: 0 },
    rotation: [0, 0, 0, 1],
    scale: { x: 1, y: 1, z: 1 },
  };
}
