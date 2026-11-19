import assert from 'node:assert/strict';
import test from 'node:test';
import {
  canMutateChangeSet,
  mergeDiagnostics,
  operationFieldSummary,
  operationTarget,
  summarizeDiff,
} from '../src/changeset/reviewModel';

test('extracts the stable target from typed operations', () => {
  assert.equal(
    operationTarget({ type: 'node.update', nodeId: 'node-000010', name: 'Clarifier' }),
    'node-000010',
  );
  assert.equal(
    operationTarget({ type: 'material.upsert', materialId: 'material-000003' }),
    'material-000003',
  );
  assert.equal(
    operationTarget({
      type: 'component.define',
      definition: { id: 'component-000020', name: 'Pump skid' },
    }),
    'component-000020',
  );
});

test('groups diff identifiers by entity and change kind', () => {
  const groups = summarizeDiff({
    addedNodeIds: ['node-000001'],
    updatedNodeIds: [],
    removedNodeIds: ['node-000002'],
    addedMaterialIds: [],
    updatedMaterialIds: ['material-000003'],
    removedMaterialIds: [],
    addedRelationIds: [],
    updatedRelationIds: [],
    removedRelationIds: [],
    addedConstraintIds: [],
    updatedConstraintIds: [],
    removedConstraintIds: [],
  });

  assert.deepEqual(groups, [
    { entity: 'Node', change: 'added', ids: ['node-000001'] },
    { entity: 'Node', change: 'removed', ids: ['node-000002'] },
    { entity: 'Material', change: 'updated', ids: ['material-000003'] },
  ]);
});

test('only mutable ChangeSet states offer entry removal', () => {
  assert.equal(canMutateChangeSet('draft'), true);
  assert.equal(canMutateChangeSet('ready'), true);
  assert.equal(canMutateChangeSet('blocked'), true);
  assert.equal(canMutateChangeSet('stale'), false);
  assert.equal(canMutateChangeSet('committed'), false);
});

test('summarizes proposed operation values without exposing raw payloads', () => {
  assert.deepEqual(
    operationFieldSummary({
      type: 'node.update',
      nodeId: 'node-000010',
      name: 'Secondary Clarifier Temporary',
      visible: false,
    }),
    [
      { label: 'Name', value: 'Secondary Clarifier Temporary' },
      { label: 'Visible', value: 'hidden' },
    ],
  );
});

test('merges active and persisted diagnostics without double counting', () => {
  const diagnostic = {
    code: 'ICE3_STUDIO_OPERATION_INVALID',
    operationIndex: 0,
    objectId: 'node-000010',
    pointer: '/nodes/node-000010/name',
    message: 'invalid name',
  };

  assert.deepEqual(mergeDiagnostics([diagnostic], [diagnostic]), [diagnostic]);
});
