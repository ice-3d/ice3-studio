import assert from 'node:assert/strict';
import test from 'node:test';
import * as SceneAdapterModule from '../src/viewport/SceneAdapter';

type SelectionModule = {
  normalizeEntitySelections?: (
    selectionIds: readonly string[],
    options: {
      referenceScene: boolean;
      referenceBindings?: readonly {
        nodeId: string | null;
        selectionId: string;
      }[];
    },
  ) => string[];
  expandEntitySelections?: (
    entityIds: readonly string[],
    renderableSelectionIds: readonly string[],
    options: {
      referenceScene: boolean;
      referenceBindings?: readonly {
        nodeId: string | null;
        selectionId: string;
      }[];
    },
  ) => string[];
};

function selectionFunctions(): SelectionModule {
  return SceneAdapterModule as unknown as SelectionModule;
}

test('collapses derived preview parts to their owning source entities', () => {
  const normalize = selectionFunctions().normalizeEntitySelections;
  assert.equal(typeof normalize, 'function');
  assert.deepEqual(normalize?.(
    [
      'node-000125/sc-wall',
      'node-000125/sc-water',
      'node-000478/route',
      'node-000094',
    ],
    { referenceScene: false },
  ), ['node-000125', 'node-000478', 'node-000094']);
});

test('maps a reference business entity through its Studio binding', () => {
  const normalize = selectionFunctions().normalizeEntitySelections;
  assert.equal(typeof normalize, 'function');
  assert.deepEqual(normalize?.(
    ['site/equipment/SC-101', 'site/unknown'],
    {
      referenceScene: true,
      referenceBindings: [{
        nodeId: 'node-sc-101',
        selectionId: 'site/equipment/SC-101',
      }],
    },
  ), ['node-sc-101', 'site/unknown']);
});

test('expands an entity selection to every derived part for full highlight', () => {
  const expand = selectionFunctions().expandEntitySelections;
  assert.equal(typeof expand, 'function');
  assert.deepEqual(expand?.(
    ['node-000125'],
    [
      'node-000125/sc-wall',
      'node-000125/sc-water',
      'node-000126/sc-wall',
      'node-000094',
    ],
    { referenceScene: false },
  ), ['node-000125/sc-wall', 'node-000125/sc-water']);
});
