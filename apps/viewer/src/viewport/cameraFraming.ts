import type { ICEGeometryBounds } from 'ice3';

// 视口边缘留一圈余量：投影后的 NDC 需要落在 ±0.88 以内。
const SAFE_NDC = 0.88;
const VIEW_DIRECTION = normalize([0.72, 0.58, 0.82]);
const WORLD_UP: Vector3Tuple = [0, 1, 0];
type Vector3Tuple = [number, number, number];

// Large site bounds stay inside the clip range while preserving useful
// depth precision for inspection at the smallest supported camera distance.
export const SCENE_CAMERA_NEAR = 0.5;
export const SCENE_CAMERA_FAR = 10_000;

export type CameraFraming = {
  readonly target: Vector3Tuple;
  readonly position: Vector3Tuple;
};

/**
 * 让包围盒完整进入视口的最小相机距离。
 *
 * 旧实现按包围球计算（半径取三维对角线的一半），对长条形厂区会退到两倍远：
 * 一个 550 米长的场地被当成直径相同的球，屏幕利用率只有三成。这里改成直接投影
 * 八个角点，二分出恰好容纳它们的最小距离。
 */
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
  const safeAspect = Number.isFinite(aspect) && aspect > 0 ? aspect : 1;
  const tanVertical = Math.tan((fovDegrees * Math.PI) / 360);
  const tanHorizontal = tanVertical * safeAspect;

  const corners: Vector3Tuple[] = [];
  for (let index = 0; index < 8; index += 1) {
    corners.push([
      index & 1 ? bounds.max[0] : bounds.min[0],
      index & 2 ? bounds.max[1] : bounds.min[1],
      index & 4 ? bounds.max[2] : bounds.min[2],
    ]);
  }

  const right = normalize(cross(WORLD_UP, VIEW_DIRECTION));
  const cameraUp = cross(VIEW_DIRECTION, right);

  const fits = (distance: number): boolean => {
    const eye: Vector3Tuple = [
      target[0] + VIEW_DIRECTION[0] * distance,
      target[1] + VIEW_DIRECTION[1] * distance,
      target[2] + VIEW_DIRECTION[2] * distance,
    ];
    for (const corner of corners) {
      const offset: Vector3Tuple = [
        corner[0] - eye[0],
        corner[1] - eye[1],
        corner[2] - eye[2],
      ];
      // 相机朝向 target，视线方向是 -VIEW_DIRECTION。
      const depth = -dot(offset, VIEW_DIRECTION);
      if (depth <= SCENE_CAMERA_NEAR) return false;
      if (Math.abs(dot(offset, right)) / depth > tanHorizontal * SAFE_NDC) return false;
      if (Math.abs(dot(offset, cameraUp)) / depth > tanVertical * SAFE_NDC) return false;
    }
    return true;
  };

  const diagonal = Math.hypot(
    bounds.max[0] - bounds.min[0],
    bounds.max[1] - bounds.min[1],
    bounds.max[2] - bounds.min[2],
  );
  let low = 1;
  let high = Math.max(2, diagonal * 3);
  for (let iteration = 0; iteration < 40; iteration += 1) {
    const mid = (low + high) / 2;
    if (fits(mid)) high = mid;
    else low = mid;
  }
  const distance = high;
  return {
    target,
    position: [
      target[0] + VIEW_DIRECTION[0] * distance,
      target[1] + VIEW_DIRECTION[1] * distance,
      target[2] + VIEW_DIRECTION[2] * distance,
    ],
  };
}

function dot(a: Vector3Tuple, b: Vector3Tuple): number {
  return a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
}

function cross(a: Vector3Tuple, b: Vector3Tuple): Vector3Tuple {
  return [
    a[1] * b[2] - a[2] * b[1],
    a[2] * b[0] - a[0] * b[2],
    a[0] * b[1] - a[1] * b[0],
  ];
}

function normalize(vector: Vector3Tuple): Vector3Tuple {
  const length = Math.hypot(...vector);
  return [
    vector[0] / length,
    vector[1] / length,
    vector[2] / length,
  ];
}
