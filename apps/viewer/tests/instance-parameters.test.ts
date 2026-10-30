import assert from 'node:assert/strict';
import test from 'node:test';
import type { ComponentDefinition, ComponentInstance } from '../src/api/StudioClient';
import { compileParameterChanges } from '../src/forms/instanceParameters';

test('compiles only changed typed parameters into instance.update', () => {
  const definition: ComponentDefinition = {
    id: 'component-000001',
    name: 'Pump',
    parameters: [{
      name: 'width',
      valueType: { kind: 'number', minimum: 1, maximum: 10 },
      default: { kind: 'number', value: 2 },
      unit: 'm',
    }],
  };
  const instance: ComponentInstance = {
    definitionId: 'component-000001',
    parameters: { width: { kind: 'number', value: 2 } },
    overrides: {},
    metadata: {},
  };
  const operations = compileParameterChanges(
    definition,
    'node-000001',
    instance,
    { width: '4' },
  );
  assert.deepEqual(operations, [{
    type: 'instance.update',
    node_id: 'node-000001',
    instance: {
      ...instance,
      parameters: { width: { kind: 'number', value: 4 } },
    },
  }]);
});
