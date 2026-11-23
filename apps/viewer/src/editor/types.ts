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
    return {
      type: 'primitive',
      kind: 'box',
      width: numeric('width'),
      height: numeric('height'),
      depth: numeric('depth'),
    };
  }
  if (content.kind === 'plane') {
    return {
      type: 'primitive',
      kind: 'plane',
      width: numeric('width'),
      depth: numeric('depth'),
    };
  }
  if (content.kind === 'cylinder') {
    return {
      type: 'primitive',
      kind: 'cylinder',
      radius: numeric('radius'),
      height: numeric('height'),
      segments: numeric('segments'),
    };
  }
  if (content.kind === 'sphere') {
    return {
      type: 'primitive',
      kind: 'sphere',
      radius: numeric('radius'),
      segments: numeric('segments'),
      rings: numeric('rings'),
    };
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
      return [
        formatDraftValue(point.x),
        formatDraftValue(point.y),
        formatDraftValue(point.z),
      ].join(', ');
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
