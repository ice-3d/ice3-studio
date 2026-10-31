import assert from 'node:assert/strict';
import test from 'node:test';

import { assertReferenceAcceptance } from '../run-large-reference-acceptance.mjs';

test('accepts only a bounded reference project with complete equipment semantics', () => {
  assert.doesNotThrow(() => assertReferenceAcceptance({
    documentBytes: 480_000,
    contextBytes: 8_200,
    bindingPageBytes: 90_000,
    sourceNodes: 204,
    equipment: 204,
    effectiveTriangles: 5_757_962,
    coarseVisible: true,
    selectionMapped: true,
    portraitBoundsSafe: true,
    landscapeBoundsSafe: true,
    cameraClipped: false,
    flickerFrames: 0,
  }));
  assert.throws(() => assertReferenceAcceptance({
    documentBytes: 300_000_000,
    contextBytes: 8_200,
    bindingPageBytes: 90_000,
    sourceNodes: 5065,
    equipment: 204,
    effectiveTriangles: 5_757_962,
    coarseVisible: true,
    selectionMapped: true,
    portraitBoundsSafe: true,
    landscapeBoundsSafe: false,
    cameraClipped: true,
    flickerFrames: 3,
  }));
});
