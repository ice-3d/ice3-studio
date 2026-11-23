import assert from 'node:assert/strict';
import test from 'node:test';
import { ICENode, ICEScene } from 'ice3';
import {
  applySceneTransformOverride,
  restoreSceneTransformOverrides,
  sceneTransformFrame,
} from '../src/viewport/SceneAdapter';

test('moves the exact selected scene node and restores its matrix', () => {
  const scene = new ICEScene({ id: 'test-scene' });
  const node = new ICENode({
    id: 'node-000001',
    displayName: 'Tank',
    selectionId: 'node-000001',
  });
  scene.addChild(node);
  const originals = new Map<ICENode, number[]>();
  const base = {
    translation: { x: 0, y: 0, z: 0 },
    rotation: [0, 0, 0, 1] as [number, number, number, number],
    scale: { x: 1, y: 1, z: 1 },
  };
  const moved = applySceneTransformOverride(scene, 'node-000001', base, {
    translation: { x: 2, y: 3, z: 4 },
    rotation: [0, 0, 0, 1],
    scale: { x: 1, y: 1, z: 1 },
  }, originals);
  assert.equal(moved, node);
  assert.equal(node.getWorldPosition().x, 2);
  assert.equal(node.getWorldPosition().y, 3);
  assert.equal(node.getWorldPosition().z, 4);
  restoreSceneTransformOverrides(scene, originals);
  assert.equal(node.getWorldPosition().x, 0);
});

test('returns null for an unavailable node', () => {
  const scene = new ICEScene({ id: 'test-scene' });
  const base = {
    translation: { x: 0, y: 0, z: 0 },
    rotation: [0, 0, 0, 1] as [number, number, number, number],
    scale: { x: 1, y: 1, z: 1 },
  };
  assert.equal(applySceneTransformOverride(scene, 'missing', base, {
    translation: { x: 0, y: 0, z: 0 },
    rotation: [0, 0, 0, 1],
    scale: { x: 1, y: 1, z: 1 },
  }, new Map()), null);
});

test('moves the source entity group containing derived children', () => {
  const scene = new ICEScene({ id: 'test-scene' });
  const source = new ICENode({
    id: 'source',
    displayName: 'Pump',
    selectionId: 'node-000001',
  });
  source.addChild(new ICENode({
    id: 'part',
    displayName: 'Pump part',
    selectionId: 'node-000001/part-1',
  }));
  scene.addChild(source);
  const originals = new Map<ICENode, number[]>();
  const base = {
    translation: { x: 0, y: 0, z: 0 },
    rotation: [0, 0, 0, 1] as [number, number, number, number],
    scale: { x: 1, y: 1, z: 1 },
  };
  const moved = applySceneTransformOverride(scene, 'node-000001', base, {
    translation: { x: 5, y: 0, z: 0 },
    rotation: [0, 0, 0, 1],
    scale: { x: 1, y: 1, z: 1 },
  }, originals);
  assert.equal(moved, source);
  assert.equal(source.getWorldPosition().x, 5);
});

test('reports the selection origin and parent frame', () => {
  const scene = new ICEScene({ id: 'test-scene' });
  const parent = new ICENode({
    id: 'parent',
    position: [10, 0, 0],
  });
  const child = new ICENode({
    id: 'child',
    selectionId: 'node-000001',
    position: [2, 3, 4],
  });
  parent.addChild(child);
  scene.addChild(parent);
  const frame = sceneTransformFrame(scene, 'node-000001', {
    translation: { x: 0, y: 0, z: 0 },
    rotation: [0, 0, 0, 1],
    scale: { x: 1, y: 1, z: 1 },
  });
  assert.deepEqual(frame?.origin, [12, 3, 4]);
  assert.equal(frame?.parentMatrix[12], 10);
});

test('moves multiple derived roots when no source group exists', () => {
  const scene = new ICEScene({ id: 'test-scene' });
  const first = new ICENode({
    id: 'node-000001/part-1',
    selectionId: 'node-000001/part-1',
    position: [1, 0, 0],
  });
  const second = new ICENode({
    id: 'node-000001/part-2',
    selectionId: 'node-000001/part-2',
    position: [0, 2, 0],
  });
  scene.addChild(first);
  scene.addChild(second);
  const base = {
    translation: { x: 0, y: 0, z: 0 },
    rotation: [0, 0, 0, 1] as [number, number, number, number],
    scale: { x: 1, y: 1, z: 1 },
  };
  const originals = new Map<ICENode, number[]>();
  applySceneTransformOverride(scene, 'node-000001', base, {
    translation: { x: 5, y: 0, z: 0 },
    rotation: [0, 0, 0, 1],
    scale: { x: 1, y: 1, z: 1 },
  }, originals);
  assert.equal(first.getWorldPosition().x, 6);
  assert.equal(second.getWorldPosition().x, 5);
  assert.equal(second.getWorldPosition().y, 2);
});
