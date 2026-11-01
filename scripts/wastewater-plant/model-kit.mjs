// 过渡导出：现有 builder 实现暂时位于 site-plan，后续任务继续按职责迁移。
export {
  TAU,
  srgb,
  partTransform,
  nodeTransform,
  rotateY,
  LIE_ALONG_X,
  box,
  cylinder,
  sphere,
  createBuilder,
} from './site-plan.mjs';

const quaternionY = (yaw) => [0, Math.sin(yaw / 2), 0, Math.cos(yaw / 2)];

const offsetAlongRun = (x, z, offset, yaw) => ({
  x: x + Math.cos(yaw) * offset,
  z: z + Math.sin(yaw) * offset,
});

export function detailFlange(b, prefix, { diameter, position, rotation, material }) {
  return [
    b.part(`${prefix}-sleeve`, b.cylinder(diameter * 0.54, 0.18, 16),
      position.x, position.y, position.z, material, { rotation }),
    b.part(`${prefix}-plate`, b.cylinder(diameter * 0.75, 0.08, 16),
      position.x, position.y, position.z, material, { rotation }),
  ];
}

export function detailValve(b, prefix, { diameter, position, rotation, material }) {
  return [
    b.part(`${prefix}-body`, b.cylinder(diameter * 0.72, diameter * 1.2, 16),
      position.x, position.y, position.z, material, { rotation }),
    b.part(`${prefix}-bonnet`, b.box(diameter * 0.34, diameter * 0.5, diameter * 0.34),
      position.x, position.y + diameter * 0.72, position.z, material),
    b.part(`${prefix}-wheel`, b.cylinder(diameter * 0.55, 0.08, 18),
      position.x, position.y + diameter * 1.02, position.z, material),
  ];
}

export function detailStair(b, prefix, { x, y, z, width, height, yaw }) {
  const steps = 10;
  const run = Math.max(height * 0.78, 1.8);
  const rotation = quaternionY(yaw);
  const treadDepth = run / steps;
  const parts = [];
  for (let index = 0; index < steps; index += 1) {
    const offset = -run / 2 + (index + 0.5) * treadDepth;
    const position = offsetAlongRun(x, z, offset, yaw);
    parts.push(b.part(
      `${prefix}-tread-${index + 1}`,
      b.box(treadDepth * 0.88, 0.08, width),
      position.x,
      y + height * ((index + 1) / steps),
      position.z,
      'walkway',
      { rotation },
    ));
  }
  for (const side of [-1, 1]) {
    const sideOffset = offsetAlongRun(x, z, 0, yaw);
    const normalX = -Math.sin(yaw) * side * width * 0.48;
    const normalZ = Math.cos(yaw) * side * width * 0.48;
    parts.push(b.part(
      `${prefix}-stringer-${side > 0 ? 'a' : 'b'}`,
      b.box(run, 0.14, 0.08),
      sideOffset.x + normalX,
      y + height / 2,
      sideOffset.z + normalZ,
      'steel',
      { rotation },
    ));
  }
  return parts;
}

export function detailGuardrail(b, prefix, { x, y, z, length, yaw }) {
  const rotation = quaternionY(yaw);
  const parts = [];
  for (let index = 0; index < 3; index += 1) {
    const offset = -length / 2 + (index * length) / 2;
    const position = offsetAlongRun(x, z, offset, yaw);
    parts.push(b.part(
      `${prefix}-post-${index + 1}`,
      b.box(0.08, 1.1, 0.08),
      position.x,
      y + 0.55,
      position.z,
      'steel',
    ));
  }
  for (const [name, railY, height, depth] of [
    ['top', 1.05, 0.08, 0.08],
    ['mid', 0.55, 0.06, 0.06],
    ['toe', 0.08, 0.14, 0.06],
  ]) {
    parts.push(b.part(
      `${prefix}-${name}`,
      b.box(length, height, depth),
      x,
      y + railY,
      z,
      'steel',
      { rotation },
    ));
  }
  return parts;
}

export function detailSupport(b, prefix, { x, y, z, height, width }) {
  return [
    b.part(`${prefix}-column-a`, b.box(0.28, height, 0.28),
      x, y + height / 2, z - width / 2, 'steel'),
    b.part(`${prefix}-column-b`, b.box(0.28, height, 0.28),
      x, y + height / 2, z + width / 2, 'steel'),
    b.part(`${prefix}-beam`, b.box(0.3, 0.3, width + 0.6),
      x, y + height - 0.15, z, 'steel'),
  ];
}
