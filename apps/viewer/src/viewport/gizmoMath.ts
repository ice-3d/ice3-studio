export type ScreenPoint = { x: number; y: number; visible: boolean };

export type ProjectedHandle = {
  startX: number;
  startY: number;
  endX: number;
  endY: number;
  lengthPixels: number;
  enabled: boolean;
};

export function projectAxisHandle(
  origin: ScreenPoint,
  endpoint: ScreenPoint,
): ProjectedHandle {
  const lengthPixels = Math.hypot(endpoint.x - origin.x, endpoint.y - origin.y);
  return {
    startX: origin.x,
    startY: origin.y,
    endX: endpoint.x,
    endY: endpoint.y,
    lengthPixels,
    enabled: origin.visible && endpoint.visible && lengthPixels >= 12,
  };
}

export function screenDragToWorldDelta(
  drag: { startX: number; startY: number; currentX: number; currentY: number },
  axis: ProjectedHandle,
  worldLength: number,
): number {
  if (!axis.enabled || !(axis.lengthPixels > 0)) return 0;
  const axisX = (axis.endX - axis.startX) / axis.lengthPixels;
  const axisY = (axis.endY - axis.startY) / axis.lengthPixels;
  const pixels = (drag.currentX - drag.startX) * axisX
    + (drag.currentY - drag.startY) * axisY;
  return pixels / axis.lengthPixels * worldLength;
}

export function angleDegreesFromScreen(
  pivot: { x: number; y: number },
  start: { x: number; y: number },
  current: { x: number; y: number },
): number {
  const startAngle = Math.atan2(start.y - pivot.y, start.x - pivot.x);
  const currentAngle = Math.atan2(current.y - pivot.y, current.x - pivot.x);
  let degrees = (currentAngle - startAngle) * 180 / Math.PI;
  while (degrees < 0) degrees += 360;
  while (degrees >= 360) degrees -= 360;
  return degrees;
}
