import assert from 'node:assert/strict';
import test from 'node:test';
import type { ComponentDefinition, ComponentInstance } from '../src/api/StudioClient';
import {
  compileParameterChanges,
  formatParameter,
} from '../src/forms/instanceParameters';

test('compiles only changed API parameters into instance.update', () => {
  const definition: ComponentDefinition = {
    id: 'component-000001',
    name: 'Pump',
    parameters: [{
      name: 'width',
      valueType: { kind: 'number', minimum: 1, maximum: 10 },
      default: 2,
      unit: 'm',
    }],
  };
  const instance: ComponentInstance = {
    definitionId: 'component-000001',
    parameters: { width: 2 },
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
      parameters: { width: 4 },
    },
  }]);
});

test('formats API scalar values without exposing f32 noise', () => {
  assert.equal(formatParameter(15), '15');
  assert.equal(formatParameter(4.599999904632568), '4.6');
  assert.equal(formatParameter(0.44999998807907104), '0.45');
});

test('does not compile rounded, unchanged API parameters', () => {
  const definition: ComponentDefinition = {
    id: 'component-000001',
    name: 'Clarifier',
    parameters: [
      {
        name: 'radius',
        valueType: { kind: 'number', minimum: 1, maximum: 30 },
        default: 15,
        unit: 'm',
      },
      {
        name: 'depth',
        valueType: { kind: 'number', minimum: 1, maximum: 10 },
        default: 4.599999904632568,
        unit: 'm',
      },
      {
        name: 'wall',
        valueType: { kind: 'number', minimum: 0.1, maximum: 2 },
        default: 0.44999998807907104,
        unit: 'm',
      },
    ],
  };
  const instance: ComponentInstance = {
    definitionId: 'component-000001',
    parameters: {
      radius: 15,
      depth: 4.599999904632568,
      wall: 0.44999998807907104,
    },
    overrides: {},
    metadata: {},
  };

  assert.deepEqual(compileParameterChanges(
    definition,
    'node-000001',
    instance,
    {
      radius: formatParameter(instance.parameters.radius!),
      depth: formatParameter(instance.parameters.depth!),
      wall: formatParameter(instance.parameters.wall!),
    },
  ), []);
});
