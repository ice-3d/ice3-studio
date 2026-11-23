import assert from 'node:assert/strict';
import test from 'node:test';
import {
  eulerDegreesToQuaternion,
  quatFromAxisAngleDegrees,
  quaternionToEulerDegrees,
  snapValue,
  transformsEqual,
} from '../src/editor/transformMath';

const almost = (actual: number, expected: number, tolerance = 1e-5): void => {
  assert.ok(
    Math.abs(actual - expected) <= tolerance,
    `${actual} != ${expected} within ${tolerance}`,
  );
};

test('round-trips a 90 degree Y rotation without changing quaternion sign semantics', () => {
  const quaternion = quatFromAxisAngleDegrees({ x: 0, y: 1, z: 0 }, 90);
  const euler = quaternionToEulerDegrees(quaternion);
  almost(euler.x, 0);
  almost(euler.y, 90);
  almost(euler.z, 0);
  const rebuilt = eulerDegreesToQuaternion(euler);
  almost(Math.abs(rebuilt[0]), Math.abs(quaternion[0]));
  almost(Math.abs(rebuilt[1]), Math.abs(quaternion[1]));
  almost(Math.abs(rebuilt[2]), Math.abs(quaternion[2]));
  almost(Math.abs(rebuilt[3]), Math.abs(quaternion[3]));
});

test('ignores f32 noise inside transform equality epsilon', () => {
  assert.equal(transformsEqual(
    {
      translation: { x: 1, y: 2, z: 3 },
      rotation: [0, 0, 0, 1],
      scale: { x: 1, y: 1, z: 1 },
    },
    {
      translation: { x: 1.0000004, y: 2, z: 3 },
      rotation: [0, 0, 0, 1.0000002],
      scale: { x: 1, y: 1, z: 1 },
    },
  ), true);
});

test('snaps positive and negative values deterministically', () => {
  assert.equal(snapValue(1.27, 0.1), 1.3);
  assert.equal(snapValue(-1.27, 0.1), -1.3);
  assert.equal(snapValue(14.9, 15), 15);
});
