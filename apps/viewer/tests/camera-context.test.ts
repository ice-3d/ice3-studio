import assert from 'node:assert/strict';
import test from 'node:test';
import { cameraContextFrom } from '../src/viewport/SceneAdapter';

test('captures a plain camera context for Agent submission', () => {
  assert.deepEqual(cameraContextFrom({
    fov: 42,
    getPosition: () => ({ x: 45, y: 32, z: 55 }),
    getTarget: () => ({ x: 0, y: 1, z: 0 }),
  }), {
    position: [45, 32, 55],
    target: [0, 1, 0],
    fov: 42,
  });
});
