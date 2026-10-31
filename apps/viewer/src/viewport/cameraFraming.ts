import type { ICEGeometryBounds } from 'ice3';

const SAFE_VIEW_MARGIN = 1.3;
const VIEW_DIRECTION = normalize([0.72, 0.58, 0.82]);
type Vector3Tuple = [number, number, number];

export const SCENE_CAMERA_NEAR = 0.5;
export const SCENE_CAMERA_FAR = 10_000;

export type CameraFraming = {
  readonly target: Vector3Tuple;
  readonly position: Vector3Tuple;
};

export function frameBoundsForViewport(
  bounds: ICEGeometryBounds,
  fovDegrees: number,
  aspect: number,
): CameraFraming {
  const target: Vector3Tuple = [
    (bounds.min[0] + bounds.max[0]) / 2,
    (bounds.min[1] + bounds.max[1]) / 2,
    (bounds.min[2] + bounds.max[2]) / 2,
  ];
  const radius = Math.max(1, Math.hypot(
    bounds.max[0] - bounds.min[0],
    bounds.max[1] - bounds.min[1],
    bounds.max[2] - bounds.min[2],
  ) / 2);
  const safeAspect = Number.isFinite(aspect) && aspect > 0 ? aspect : 1;
  const verticalHalfFov = (fovDegrees * Math.PI) / 360;
  const horizontalHalfFov = Math.atan(Math.tan(verticalHalfFov) * safeAspect);
  const limitingHalfFov = Math.min(verticalHalfFov, horizontalHalfFov);
  const distance = radius / Math.sin(limitingHalfFov) * SAFE_VIEW_MARGIN;
  return {
    target,
    position: [
      target[0] + VIEW_DIRECTION[0] * distance,
      target[1] + VIEW_DIRECTION[1] * distance,
      target[2] + VIEW_DIRECTION[2] * distance,
    ],
  };
}

function normalize(vector: Vector3Tuple): Vector3Tuple {
  const length = Math.hypot(...vector);
  return [
    vector[0] / length,
    vector[1] / length,
    vector[2] / length,
  ];
}
