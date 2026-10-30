import assert from 'node:assert/strict';
import test from 'node:test';
import { buildObjectTree } from '../src/navigation/objectTreeModel';

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
