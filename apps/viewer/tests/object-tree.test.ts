import assert from 'node:assert/strict';
import test from 'node:test';
import * as ObjectTreeModel from '../src/navigation/objectTreeModel';

const { buildObjectTree } = ObjectTreeModel;

test('builds a generic hierarchy without industry assumptions', () => {
  const tree = buildObjectTree([
    { id: 'node-000001', name: 'Assembly', parentId: null, metadata: {} },
    { id: 'node-000002', name: 'Part', parentId: 'node-000001', metadata: {} },
  ]);

  assert.deepEqual(tree, [{
    id: 'node-000001',
    name: 'Assembly',
    children: [{ id: 'node-000002', name: 'Part', children: [] }],
  }]);
});

test('filters nested objects while retaining matching ancestors', () => {
  const filterTree = ObjectTreeModel.filterObjectTree;
  assert.equal(typeof filterTree, 'function');
  const tree = buildObjectTree([
    { id: 'node-000001', name: 'Process area', parentId: null, metadata: {} },
    { id: 'node-000002', name: 'Pump A', parentId: 'node-000001', metadata: {} },
    { id: 'node-000003', name: 'Valve A', parentId: 'node-000001', metadata: {} },
    { id: 'node-000004', name: 'Unrelated', parentId: null, metadata: {} },
  ]);

  assert.deepEqual(filterTree?.(tree, 'pump'), [{
    id: 'node-000001',
    name: 'Process area',
    children: [{
      id: 'node-000002',
      name: 'Pump A',
      children: [],
    }],
  }]);
});
