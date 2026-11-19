import type { Diagnostic, DocumentDiff } from '../api/StudioClient';

export type DiffEntity = 'Node' | 'Material' | 'Relation' | 'Constraint';
export type DiffChange = 'added' | 'updated' | 'removed';

export type DiffGroup = {
  entity: DiffEntity;
  change: DiffChange;
  ids: string[];
};

export type OperationField = {
  label: string;
  value: string;
};

export function operationTarget(operation: Record<string, unknown>): string | null {
  const definition = operation.definition;
  const definitionId = isRecord(definition) && typeof definition.id === 'string'
    ? definition.id
    : null;
  const target = operation.nodeId
    ?? operation.materialId
    ?? operation.relationId
    ?? operation.constraintId
    ?? operation.componentId
    ?? definitionId;
  return typeof target === 'string' ? target : null;
}

export function summarizeDiff(diff: DocumentDiff): DiffGroup[] {
  const definitions: Array<[DiffEntity, DiffChange, keyof DocumentDiff]> = [
    ['Node', 'added', 'addedNodeIds'],
    ['Node', 'updated', 'updatedNodeIds'],
    ['Node', 'removed', 'removedNodeIds'],
    ['Material', 'added', 'addedMaterialIds'],
    ['Material', 'updated', 'updatedMaterialIds'],
    ['Material', 'removed', 'removedMaterialIds'],
    ['Relation', 'added', 'addedRelationIds'],
    ['Relation', 'updated', 'updatedRelationIds'],
    ['Relation', 'removed', 'removedRelationIds'],
    ['Constraint', 'added', 'addedConstraintIds'],
    ['Constraint', 'updated', 'updatedConstraintIds'],
    ['Constraint', 'removed', 'removedConstraintIds'],
  ];
  return definitions.flatMap(([entity, change, key]) => {
    const ids = diff[key];
    return ids.length > 0 ? [{ entity, change, ids }] : [];
  });
}

export function canMutateChangeSet(status: string): boolean {
  return status.toLowerCase() === 'draft'
    || status.toLowerCase() === 'ready'
    || status.toLowerCase() === 'blocked';
}

export function mergeDiagnostics(
  ...groups: ReadonlyArray<readonly Diagnostic[]>
): Diagnostic[] {
  const unique = new Map<string, Diagnostic>();
  for (const diagnostic of groups.flat()) {
    const key = [
      diagnostic.code,
      diagnostic.operationIndex ?? '',
      diagnostic.objectId ?? '',
      diagnostic.pointer,
      diagnostic.message,
    ].join('|');
    unique.set(key, diagnostic);
  }
  return [...unique.values()];
}

export function operationFieldSummary(operation: Record<string, unknown>): OperationField[] {
  const fields: OperationField[] = [];
  if (typeof operation.name === 'string') {
    fields.push({ label: 'Name', value: operation.name });
  }
  if (typeof operation.visible === 'boolean') {
    fields.push({ label: 'Visible', value: operation.visible ? 'shown' : 'hidden' });
  }
  return fields;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
