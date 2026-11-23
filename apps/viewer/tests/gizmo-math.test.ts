import assert from 'node:assert/strict';
import test from 'node:test';
import {
  angleDegreesFromScreen,
  projectAxisHandle,
  screenDragToWorldDelta,
} from '../src/viewport/gizmoMath';

test('projects a visible axis and reports disabled axes', () => {
  const visible = projectAxisHandle(
    { x: 100, y: 100, visible: true },
    { x: 180, y: 100, visible: true },
  );
  assert.equal(visible.enabled, true);
  assert.equal(visible.lengthPixels, 80);
  const hidden = projectAxisHandle(
    { x: 100, y: 100, visible: true },
    { x: 103, y: 101, visible: true },
  );
  assert.equal(hidden.enabled, false);
});

test('converts pointer movement along a projected axis into world distance', () => {
  const delta = screenDragToWorldDelta(
    { startX: 100, startY: 100, currentX: 180, currentY: 100 },
    projectAxisHandle(
      { x: 100, y: 100, visible: true },
      { x: 180, y: 100, visible: true },
    ),
    8,
  );
  assert.equal(delta, 8);
});

test('computes signed screen angle in degrees', () => {
  const angle = angleDegreesFromScreen(
    { x: 0, y: 0 },
    { x: 10, y: 0 },
    { x: 0, y: 10 },
  );
  assert.equal(angle, 90);
});
