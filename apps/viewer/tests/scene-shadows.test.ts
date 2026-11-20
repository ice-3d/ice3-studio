import assert from 'node:assert/strict';
import test from 'node:test';
import { resolveSceneShadows } from '../src/viewport/SceneAdapter';

test('Studio keeps directional shadows opt-in', () => {
  assert.equal(resolveSceneShadows(''), false);
  assert.equal(resolveSceneShadows('?shadows=0'), false);
  assert.equal(resolveSceneShadows('?shadows=off'), false);
  assert.equal(resolveSceneShadows('?shadows=1'), true);
  assert.equal(resolveSceneShadows('?shadowdebug=1'), true);
});
