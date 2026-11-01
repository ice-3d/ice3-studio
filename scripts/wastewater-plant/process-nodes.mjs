// 工艺系统中的分流、汇流、泵送、气体和材料搬运节点。

import { box, cylinder, LIE_ALONG_X } from './model-kit.mjs';

export function distributionBox(
  b,
  prefix,
  { length = 8, width = 6, depth = 2.6 } = {},
) {
  return [
    b.part(`${prefix}-base`, box(length, 0.4, width), 0, 0.2, 0, 'concrete-old'),
    b.part(`${prefix}-wall-n`, box(length, depth, 0.35),
      0, 0.5 + depth / 2, -width / 2, 'concrete-new'),
    b.part(`${prefix}-wall-s`, box(length, depth, 0.35),
      0, 0.5 + depth / 2, width / 2, 'concrete-new'),
    b.part(`${prefix}-wall-w`, box(0.35, depth, width),
      -length / 2, 0.5 + depth / 2, 0, 'concrete-new'),
    b.part(`${prefix}-wall-e`, box(0.35, depth, width),
      length / 2, 0.5 + depth / 2, 0, 'concrete-new'),
    b.part(`${prefix}-cover`, box(length - 0.7, 0.12, width - 0.7),
      0, 0.58 + depth, 0, 'walkway'),
  ];
}

export function sludgeCollectionWell(
  b,
  prefix,
  { radius = 2.6, depth = 3.2 } = {},
) {
  return [
    b.part(`${prefix}-shell`, cylinder(radius, depth, 28),
      0, depth / 2, 0, 'concrete-new'),
    b.part(`${prefix}-cover`, cylinder(radius + 0.15, 0.16, 28),
      0, depth + 0.08, 0, 'steel-dark'),
  ];
}

export function rasPumpStation(
  b,
  prefix,
  { length = 18, width = 10, height = 4.8 } = {},
) {
  const parts = [
    b.part(`${prefix}-slab`, box(length + 1, 0.5, width + 1),
      0, 0.25, 0, 'concrete'),
    b.part(`${prefix}-wall-n`, box(length, height, 0.4),
      0, 0.5 + height / 2, -width / 2, 'concrete-new'),
    b.part(`${prefix}-wall-s`, box(length, height, 0.4),
      0, 0.5 + height / 2, width / 2, 'concrete-new'),
  ];
  for (const [index, x] of [-4, 0, 4].entries()) {
    parts.push(
      b.part(`${prefix}-pump-${index + 1}`, cylinder(0.9, 1.8, 20),
        x, 1.4, 0, 'stainless'),
      b.part(`${prefix}-motor-${index + 1}`, cylinder(0.55, 1.2, 18),
        x, 3.1, 0, 'steel-dark'),
    );
  }
  return parts;
}

export function gasCollector(b, prefix, { length = 10 } = {}) {
  return [
    b.part(`${prefix}-vessel`, cylinder(0.7, length, 20),
      0, 3.0, 0, 'stainless', { rotation: LIE_ALONG_X }),
    b.part(`${prefix}-support-a`, box(0.25, 3, 0.25),
      -3.5, 1.5, 0, 'steel-dark'),
    b.part(`${prefix}-support-b`, box(0.25, 3, 0.25),
      3.5, 1.5, 0, 'steel-dark'),
  ];
}

export function gasFlare(b, prefix, { height = 12 } = {}) {
  return [
    b.part(`${prefix}-stack`, cylinder(0.35, height, 20),
      0, height / 2, 0, 'steel-dark'),
    b.part(`${prefix}-base`, cylinder(1.3, 0.4, 24),
      0, 0.2, 0, 'concrete'),
  ];
}

export function cakeConveyor(b, prefix, { length = 14 } = {}) {
  return [
    b.part(`${prefix}-trough`, box(length, 0.5, 0.9),
      0, 1.2, 0, 'steel'),
    b.part(`${prefix}-support-a`, box(0.2, 1.2, 0.6),
      -length / 3, 0.6, 0, 'steel-dark'),
    b.part(`${prefix}-support-b`, box(0.2, 1.2, 0.6),
      length / 3, 0.6, 0, 'steel-dark'),
  ];
}
