import type { StudioTransform, StudioVec3 } from '../api/StudioClient';

export function quaternionToEulerDegrees(
  rotation: readonly [number, number, number, number],
): StudioVec3 {
  const [x, y, z, w] = normalizeQuaternion(rotation);
  const sinX = 2 * (w * x + y * z);
  const cosX = 1 - 2 * (x * x + y * y);
  const sinY = 2 * (w * y - z * x);
  const sinZ = 2 * (w * z + x * y);
  const cosZ = 1 - 2 * (y * y + z * z);
  return {
    x: radToDeg(Math.atan2(sinX, cosX)),
    y: radToDeg(Math.asin(clamp(sinY, -1, 1))),
    z: radToDeg(Math.atan2(sinZ, cosZ)),
  };
}

export function eulerDegreesToQuaternion(
  rotation: StudioVec3,
): [number, number, number, number] {
  const halfX = degToRad(rotation.x) / 2;
  const halfY = degToRad(rotation.y) / 2;
  const halfZ = degToRad(rotation.z) / 2;
  const cx = Math.cos(halfX);
  const sx = Math.sin(halfX);
  const cy = Math.cos(halfY);
  const sy = Math.sin(halfY);
  const cz = Math.cos(halfZ);
  const sz = Math.sin(halfZ);
  return normalizeQuaternion([
    sx * cy * cz + cx * sy * sz,
    cx * sy * cz - sx * cy * sz,
    cx * cy * sz + sx * sy * cz,
    cx * cy * cz - sx * sy * sz,
  ]);
}

export function quatFromAxisAngleDegrees(
  axis: StudioVec3,
  degrees: number,
): [number, number, number, number] {
  const length = Math.hypot(axis.x, axis.y, axis.z);
  if (!(length > 0)) return [0, 0, 0, 1];
  const half = degToRad(degrees) / 2;
  const sine = Math.sin(half) / length;
  return [axis.x * sine, axis.y * sine, axis.z * sine, Math.cos(half)];
}

export function transformsEqual(
  left: StudioTransform,
  right: StudioTransform,
  epsilon = 1e-6,
): boolean {
  return vectorEqual(left.translation, right.translation, epsilon)
    && quaternionEqual(left.rotation, right.rotation, epsilon)
    && vectorEqual(left.scale, right.scale, epsilon);
}

export function snapValue(value: number, step: number): number {
  if (!(step > 0)) throw new RangeError('snap step must be positive');
  return Number((Math.round(value / step) * step).toFixed(6));
}

function normalizeQuaternion(
  value: readonly [number, number, number, number],
): [number, number, number, number] {
  const length = Math.hypot(...value);
  if (!(length > 0)) return [0, 0, 0, 1];
  return [
    value[0] / length,
    value[1] / length,
    value[2] / length,
    value[3] / length,
  ];
}

function quaternionEqual(
  left: readonly [number, number, number, number],
  right: readonly [number, number, number, number],
  epsilon: number,
): boolean {
  const a = normalizeQuaternion(left);
  const b = normalizeQuaternion(right);
  const direct = a.every((value, index) => Math.abs(value - b[index]!) <= epsilon);
  const negated = a.every((value, index) => Math.abs(value + b[index]!) <= epsilon);
  return direct || negated;
}

function vectorEqual(left: StudioVec3, right: StudioVec3, epsilon: number): boolean {
  return Math.abs(left.x - right.x) <= epsilon
    && Math.abs(left.y - right.y) <= epsilon
    && Math.abs(left.z - right.z) <= epsilon;
}

function degToRad(value: number): number {
  return value * Math.PI / 180;
}

function radToDeg(value: number): number {
  return value * 180 / Math.PI;
}

function clamp(value: number, minimum: number, maximum: number): number {
  return Math.max(minimum, Math.min(maximum, value));
}
