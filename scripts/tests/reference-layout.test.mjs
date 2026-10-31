import assert from 'node:assert/strict';
import test from 'node:test';
import { compactLayoutTranslation } from
  '../../fixtures/large-industrial-site/reference-layout.mjs';

test('compacts biological and clarifier tanks into adjacent arrays', () => {
  const at101 = compactLayoutTranslation(equipment('AT-101', [90, 20, 20, 10]));
  const ax101 = compactLayoutTranslation(equipment('AX-101', [103, 20, 20, 10]));
  const ae101 = compactLayoutTranslation(equipment('AE-101', [117, 20, 20, 10]));
  assert.ok(at101);
  assert.ok(ax101);
  assert.ok(ae101);
  assert.equal(90 + at101.x, 330);
  assert.equal(103 + ax101.x, 355);
  assert.equal(117 + ae101.x, 380);

  const sc301 = compactLayoutTranslation(equipment('SC-301', [680, 100, 24, 16]));
  const sc401 = compactLayoutTranslation(equipment('SC-401', [750, 100, 24, 16]));
  assert.ok(sc301);
  assert.ok(sc401);
  assert.equal(Math.abs((750 + sc401.x) - (680 + sc301.x)), 28);
  assert.equal(108 + sc301.z, 108 + sc401.z);
});

test('leaves unclassified site equipment in its source position', () => {
  assert.equal(compactLayoutTranslation(equipment('FAN-101', [0, 0, 4, 4])), null);
});

function equipment(tag, [x, z, width, depth]) {
  return {
    tag,
    bounds: {
      min: [x - width / 2, 0, z - depth / 2],
      max: [x + width / 2, 0, z + depth / 2],
    },
  };
}
