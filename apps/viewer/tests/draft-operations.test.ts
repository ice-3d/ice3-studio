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
