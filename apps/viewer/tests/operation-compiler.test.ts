import assert from 'node:assert/strict';
import test from 'node:test';
import { compileNodeChanges } from '../src/forms/operationCompiler';

test('compiles generic node field changes into node.update', () => {
  const node = {
    id: 'node-000001',
    name: 'Assembly',
    parentId: null,
    visible: true,
    metadata: { status: 'draft' },
  };
  const operations = compileNodeChanges(node, {
    name: 'Assembly revised',
    visible: false,
    metadata: { status: 'approved' },
  });

  assert.deepEqual(operations, [{
    type: 'node.update',
    nodeId: 'node-000001',
    name: 'Assembly revised',
    transform: null,
    visible: false,
    metadata: { status: 'approved' },
  }]);
});
