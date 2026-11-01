import type {
  ComponentDefinition,
  ComponentInstance,
  ParameterValue,
  StudioVec3,
} from '../api/StudioClient';
import type { Operation } from './operationCompiler';

export type ParameterDraft = Record<string, string>;

type LegacyTypedParameterValue =
  | { kind: 'number'; value: number }
  | { kind: 'vector3'; value: StudioVec3 }
  | { kind: 'color'; value: [number, number, number, number] }
  | { kind: 'enumeration'; value: string };

type ParameterValueLike = ParameterValue | LegacyTypedParameterValue;

export function compileParameterChanges(
  definition: ComponentDefinition,
  nodeId: string,
  instance: ComponentInstance,
  draft: ParameterDraft,
): Operation[] {
  let changed = false;
  const parameters = { ...instance.parameters };
  for (const [name, raw] of Object.entries(draft)) {
    const parameter = definition.parameters.find((candidate) => candidate.name === name);
    if (!parameter) throw new Error(`Unknown parameter: ${name}`);
    const current = parameters[name] ?? parameter.default;
    const next = parseParameter(parameter.valueType, raw);
    if (!parameterValuesEqual(current, next)) {
      parameters[name] = next;
      changed = true;
    }
  }
  return changed
    ? [{
        type: 'instance.update',
        node_id: nodeId,
        instance: { ...instance, parameters },
      }]
    : [];
}

function parseParameter(
  valueType: Record<string, unknown> & { kind: string },
  raw: string,
): ParameterValue {
  if (valueType.kind === 'number') {
    const value = Number(raw);
    if (!Number.isFinite(value)) throw new Error('Parameter must be a finite number');
    const minimum = typeof valueType.minimum === 'number' ? valueType.minimum : undefined;
    const maximum = typeof valueType.maximum === 'number' ? valueType.maximum : undefined;
    if (minimum !== undefined && value < minimum) throw new Error('Parameter is below its minimum');
    if (maximum !== undefined && value > maximum) throw new Error('Parameter is above its maximum');
    return value;
  }
  const components = raw.split(',').map((value) => Number(value.trim()));
  if (valueType.kind === 'vector3') {
    if (components.length !== 3 || !components.every(Number.isFinite)) {
      throw new Error('Vector parameter requires three finite components');
    }
    return { x: components[0]!, y: components[1]!, z: components[2]! };
  }
  if (valueType.kind === 'color') {
    if (components.length !== 4 || !components.every((value) => (
      Number.isFinite(value) && value >= 0 && value <= 1
    ))) {
      throw new Error('Color parameter requires four components between zero and one');
    }
    return [components[0]!, components[1]!, components[2]!, components[3]!];
  }
  if (valueType.kind === 'enumeration') {
    const values = Array.isArray(valueType.values)
      ? valueType.values.filter((value): value is string => typeof value === 'string')
      : [];
    if (!values.includes(raw)) throw new Error('Parameter is not an allowed enumeration value');
    return raw;
  }
  throw new Error(`Unsupported parameter type: ${valueType.kind}`);
}

export function formatParameter(value: ParameterValueLike): string {
  const normalized = normalizeParameterValue(value);
  if (typeof normalized === 'number') return formatNumber(normalized);
  if (typeof normalized === 'string') return normalized;
  if (Array.isArray(normalized)) return normalized.map(formatNumber).join(', ');
  return [normalized.x, normalized.y, normalized.z].map(formatNumber).join(', ');
}

function normalizeParameterValue(value: ParameterValueLike): ParameterValue {
  if (typeof value !== 'object' || value === null) return value;
  if ('kind' in value) {
    if (value.kind === 'number') return value.value;
    if (value.kind === 'vector3') return value.value;
    if (value.kind === 'color') return value.value;
    return value.value;
  }
  return value;
}

function formatNumber(value: number): string {
  return String(Number(value.toFixed(3)));
}

function parameterValuesEqual(left: ParameterValueLike, right: ParameterValueLike): boolean {
  const normalizedLeft = normalizeParameterValue(left);
  const normalizedRight = normalizeParameterValue(right);
  if (typeof normalizedLeft === 'number' && typeof normalizedRight === 'number') {
    return approximatelyEqual(normalizedLeft, normalizedRight);
  }
  if (Array.isArray(normalizedLeft) && Array.isArray(normalizedRight)) {
    return normalizedLeft.length === normalizedRight.length
      && normalizedLeft.every((value, index) => approximatelyEqual(value, normalizedRight[index]!));
  }
  if (
    isVec3(normalizedLeft)
    && isVec3(normalizedRight)
  ) {
    return approximatelyEqual(normalizedLeft.x, normalizedRight.x)
      && approximatelyEqual(normalizedLeft.y, normalizedRight.y)
      && approximatelyEqual(normalizedLeft.z, normalizedRight.z);
  }
  return normalizedLeft === normalizedRight;
}

function approximatelyEqual(left: number, right: number): boolean {
  return Math.abs(left - right) <= Math.max(1, Math.abs(left), Math.abs(right)) * 1e-6;
}

function isVec3(value: ParameterValue): value is StudioVec3 {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
