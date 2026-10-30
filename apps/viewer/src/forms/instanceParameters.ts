import type {
  ComponentDefinition,
  ComponentInstance,
  ParameterValue,
} from '../api/StudioClient';
import type { Operation } from './operationCompiler';

export type ParameterDraft = Record<string, string>;

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
    const next = parseParameter(parameter.valueType, raw, parameter.default);
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
  fallback: ParameterValue,
): ParameterValue {
  if (valueType.kind === 'number') {
    const value = Number(raw);
    if (!Number.isFinite(value)) throw new Error('Parameter must be a finite number');
    const minimum = typeof valueType.minimum === 'number' ? valueType.minimum : undefined;
    const maximum = typeof valueType.maximum === 'number' ? valueType.maximum : undefined;
    if (minimum !== undefined && value < minimum) throw new Error('Parameter is below its minimum');
    if (maximum !== undefined && value > maximum) throw new Error('Parameter is above its maximum');
    return { kind: 'number', value };
  }
  const components = raw.split(',').map((value) => Number(value.trim()));
  if (valueType.kind === 'vector3') {
    if (components.length !== 3 || !components.every(Number.isFinite)) {
      throw new Error('Vector parameter requires three finite components');
    }
    return {
      kind: 'vector3',
      value: { x: components[0]!, y: components[1]!, z: components[2]! },
    };
  }
  if (valueType.kind === 'color') {
    if (components.length !== 4 || !components.every((value) => (
      Number.isFinite(value) && value >= 0 && value <= 1
    ))) {
      throw new Error('Color parameter requires four components between zero and one');
    }
    return {
      kind: 'color',
      value: [components[0]!, components[1]!, components[2]!, components[3]!],
    };
  }
  if (valueType.kind === 'enumeration') {
    const values = Array.isArray(valueType.values)
      ? valueType.values.filter((value): value is string => typeof value === 'string')
      : [];
    if (!values.includes(raw)) throw new Error('Parameter is not an allowed enumeration value');
    return { kind: 'enumeration', value: raw };
  }
  return fallback;
}

function parameterValuesEqual(left: ParameterValue, right: ParameterValue): boolean {
  return JSON.stringify(left) === JSON.stringify(right);
}
