export type Bounds3 = {
  readonly min: readonly [number, number, number];
  readonly max: readonly [number, number, number];
};

export type Measurement = {
  readonly from: [number, number, number];
  readonly to: [number, number, number];
  readonly distance: number;
  readonly clearance: number;
};

function centre(bounds: Bounds3): [number, number, number] {
  return [
    (bounds.min[0] + bounds.max[0]) / 2,
    (bounds.min[1] + bounds.max[1]) / 2,
    (bounds.min[2] + bounds.max[2]) / 2,
  ];
}

function axisGap(aMin: number, aMax: number, bMin: number, bMax: number): number {
  return Math.max(0, bMin - aMax, aMin - bMax);
}

/**
 * Distance between two world-space bounding boxes.
 *
 * `distance` is centre-to-centre; `clearance` is the closest surface gap
 * (0 when the boxes overlap), which is what clearances are judged by.
 */
export function measureBounds(a: Bounds3, b: Bounds3): Measurement {
  const from = centre(a);
  const to = centre(b);
  const distance = Math.hypot(to[0] - from[0], to[1] - from[1], to[2] - from[2]);
  const clearance = Math.hypot(
    axisGap(a.min[0], a.max[0], b.min[0], b.max[0]),
    axisGap(a.min[1], a.max[1], b.min[1], b.max[1]),
    axisGap(a.min[2], a.max[2], b.min[2], b.max[2]),
  );
  return { from, to, distance, clearance };
}

export function formatMetres(value: number): string {
  if (!Number.isFinite(value)) return "—";
  if (value >= 100) return `${value.toFixed(0)} m`;
  if (value >= 1) return `${value.toFixed(2)} m`;
  return `${(value * 100).toFixed(1)} cm`;
}
