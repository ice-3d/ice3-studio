import assert from 'node:assert/strict';
import test from 'node:test';

import { formatMetres, measureBounds } from '../src/viewport/measure';

test('reports centre distance and surface clearance', () => {
  const measurement = measureBounds(
    { min: [0, 0, 0], max: [1, 1, 1] },
    { min: [3, 0, 0], max: [4, 1, 1] },
  );
  assert.deepEqual(measurement.from, [0.5, 0.5, 0.5]);
  assert.deepEqual(measurement.to, [3.5, 0.5, 0.5]);
  assert.equal(measurement.distance, 3);
  assert.equal(measurement.clearance, 2);
});

test('overlapping bounds have zero clearance', () => {
  const measurement = measureBounds(
    { min: [0, 0, 0], max: [2, 2, 2] },
    { min: [1, 1, 1], max: [3, 3, 3] },
  );
  assert.equal(measurement.clearance, 0);
  assert.ok(measurement.distance > 0);
});

test('formats metres for display', () => {
  assert.equal(formatMetres(12.345), '12.35 m');
  assert.equal(formatMetres(0.4), '40.0 cm');
  assert.equal(formatMetres(150), '150 m');
  assert.equal(formatMetres(Number.NaN), '—');
});
