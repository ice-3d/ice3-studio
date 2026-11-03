import assert from 'node:assert/strict';
import test from 'node:test';
import {
  ICEPerspectiveCamera,
  type ICEGeometryBounds,
} from 'ice3';
import {
  adaptiveCameraClipPlanes,
  frameBoundsForViewport,
  SCENE_CAMERA_FAR,
  SCENE_CAMERA_NEAR,
} from '../src/viewport/cameraFraming';

const largePlantBounds: ICEGeometryBounds = {
  min: [-625, -1, -390],
  max: [625, 31, 390],
};

function assertBoundsInsideViewport(
  bounds: ICEGeometryBounds,
  aspect: number,
): void {
  const framing = frameBoundsForViewport(bounds, 42, aspect);
  const camera = new ICEPerspectiveCamera({
    id: 'test-camera',
    fov: 42,
    near: SCENE_CAMERA_NEAR,
    far: SCENE_CAMERA_FAR,
    position: framing.position,
    target: framing.target,
  });
  const viewProjection = camera.getViewProjectionMatrix(aspect);
  for (const x of [bounds.min[0], bounds.max[0]]) {
    for (const y of [bounds.min[1], bounds.max[1]]) {
      for (const z of [bounds.min[2], bounds.max[2]]) {
        const clip = viewProjection.transformPoint(
          { x, y, z } as Parameters<typeof viewProjection.transformPoint>[0],
        );
        const ndcX = clip[0] / clip[3];
        const ndcY = clip[1] / clip[3];
        const ndcZ = clip[2] / clip[3];
        if (Math.abs(ndcX) > 0.9 || Math.abs(ndcY) > 0.9) {
          throw new Error(
            `corner ${x},${y},${z} escaped the safe viewport at aspect ${aspect}: `
            + `${ndcX},${ndcY}`,
          );
        }
        if (ndcZ < -1 || ndcZ > 1) {
          throw new Error(
            `corner ${x},${y},${z} crossed the camera clip planes at aspect ${aspect}: ${ndcZ}`,
          );
        }
      }
    }
  }
}

test('frames a wide plant without cropping in a portrait viewport', () => {
  assertBoundsInsideViewport(largePlantBounds, 0.72);
});

test('frames a wide plant without cropping in a landscape viewport', () => {
  assertBoundsInsideViewport(largePlantBounds, 1.65);
});

test('adapts camera clip planes as zoom distance changes', () => {
  const overview = adaptiveCameraClipPlanes(600, 1000);
  const close = adaptiveCameraClipPlanes(20, 1000);
  assert.ok(overview.far / overview.near < 5000);
  // near 有场景尺度下界（diagonal/150），拉近不再缩到亚米级——那是远处
  // 共面几何 z-fighting 闪烁的根因。far 仍随距离收缩。
  assert.ok(close.near >= 1000 / 150);
  assert.equal(close.near, overview.near);
  assert.ok(close.far < overview.far);
});

test('keeps small scenes on a small near plane', () => {
  const clip = adaptiveCameraClipPlanes(40, 60);
  assert.ok(clip.near >= 1 && clip.near <= 1.5, `near=${clip.near}`);
  assert.ok(clip.far >= 500);
});

test('caps the near floor for very large scenes', () => {
  const clip = adaptiveCameraClipPlanes(2000, 4000);
  assert.equal(clip.near, 8);
  assert.ok(clip.far / clip.near < 5000);
});
