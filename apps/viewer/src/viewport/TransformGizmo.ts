import type { StudioTransform, StudioVec3 } from '../api/StudioClient';
import { snapValue } from '../editor/transformMath';
import type { GizmoMode, TransformSpace } from '../editor/types';
import type { SceneAdapter } from './SceneAdapter';
import {
  angleDegreesFromScreen,
  projectAxisHandle,
  screenDragToWorldDelta,
  type ProjectedHandle,
} from './gizmoMath';

const SVG = 'http://www.w3.org/2000/svg';
const HANDLE_LENGTH_PX = 72;
const ROTATE_RADII = [72, 62, 52] as const;

type Axis = 'x' | 'y' | 'z' | 'free';

type TransformGizmoOptions = {
  beginPreview(): void;
  preview(transform: StudioTransform): void;
  finishPreview(transform: StudioTransform): void;
  cancelPreview(): void;
};

type ActiveDrag = {
  pointerId: number;
  axis: Axis;
  startX: number;
  startY: number;
  base: StudioTransform;
  parentMatrix: number[];
};

export class TransformGizmo {
  private readonly svg: SVGSVGElement;
  private readonly adapter: SceneAdapter;
  private readonly options: TransformGizmoOptions;
  private active = false;
  private nodeId: string | null = null;
  private baseTransform: StudioTransform | null = null;
  private transform: StudioTransform | null = null;
  private mode: GizmoMode = 'move';
  private space: TransformSpace = 'local';
  private drag: ActiveDrag | null = null;
  private frame = 0;
  private disposed = false;

  constructor(
    container: HTMLElement,
    adapter: SceneAdapter,
    options: TransformGizmoOptions,
  ) {
    this.adapter = adapter;
    this.options = options;
    this.svg = document.createElementNS(SVG, 'svg');
    this.svg.setAttribute('class', 'transform-gizmo');
    this.svg.setAttribute('role', 'application');
    this.svg.setAttribute('aria-label', 'Transform gizmo');
    container.append(this.svg);
    this.svg.addEventListener('pointerdown', this.onPointerDown);
    this.svg.addEventListener('pointermove', this.onPointerMove);
    this.svg.addEventListener('pointerup', this.onPointerUp);
    this.svg.addEventListener('pointercancel', this.onPointerCancel);
    window.addEventListener('keydown', this.onKeyDown);
    this.scheduleRender();
  }

  setActive(active: boolean): void {
    this.active = active;
    this.svg.style.display = active ? '' : 'none';
    this.scheduleRender();
  }

  setTarget(
    nodeId: string | null,
    baseTransform: StudioTransform | null,
    transform: StudioTransform | null,
  ): void {
    this.nodeId = nodeId;
    this.baseTransform = baseTransform ? structuredClone(baseTransform) : null;
    this.transform = transform ? structuredClone(transform) : null;
    if (!this.drag && nodeId && this.baseTransform && this.transform) {
      this.adapter.setTransformOverride(nodeId, this.baseTransform, this.transform);
    }
    this.scheduleRender();
  }

  setMode(mode: GizmoMode): void {
    this.mode = mode;
    this.scheduleRender();
  }

  setSpace(space: TransformSpace): void {
    this.space = space;
    this.scheduleRender();
  }

  dispose(): void {
    this.disposed = true;
    cancelAnimationFrame(this.frame);
    this.svg.removeEventListener('pointerdown', this.onPointerDown);
    this.svg.removeEventListener('pointermove', this.onPointerMove);
    this.svg.removeEventListener('pointerup', this.onPointerUp);
    this.svg.removeEventListener('pointercancel', this.onPointerCancel);
    window.removeEventListener('keydown', this.onKeyDown);
    this.svg.remove();
  }

  private readonly onPointerDown = (event: PointerEvent): void => {
    if (!this.active || !this.nodeId || !this.transform) return;
    const target = event.target instanceof Element
      ? event.target.closest<SVGElement>('[data-gizmo-axis]')
      : null;
    const axis = target?.dataset.gizmoAxis as Axis | undefined;
    if (!axis || target?.getAttribute('aria-disabled') === 'true') return;
    const frame = this.adapter.getTransformFrame(this.nodeId, this.baseTransform ?? this.transform);
    if (!frame) return;
    event.preventDefault();
    event.stopPropagation();
    this.svg.setPointerCapture(event.pointerId);
    this.drag = {
      pointerId: event.pointerId,
      axis,
      startX: event.clientX,
      startY: event.clientY,
      base: structuredClone(this.transform),
      parentMatrix: frame.parentMatrix,
    };
    this.options.beginPreview();
  };

  private readonly onPointerMove = (event: PointerEvent): void => {
    if (!this.drag || this.drag.pointerId !== event.pointerId || !this.nodeId) return;
    const next = this.transformForPointer(
      this.drag,
      event.clientX,
      event.clientY,
      event.shiftKey,
    );
    if (!next) return;
    this.transform = next;
    this.adapter.setTransformOverride(
      this.nodeId,
      this.baseTransform ?? next,
      next,
    );
    this.options.preview(next);
    this.scheduleRender();
  };

  private readonly onPointerUp = (event: PointerEvent): void => {
    if (!this.drag || this.drag.pointerId !== event.pointerId) return;
    const next = this.transformForPointer(
      this.drag,
      event.clientX,
      event.clientY,
      event.shiftKey,
    );
    this.drag = null;
    this.svg.releasePointerCapture?.(event.pointerId);
    if (next && this.nodeId) {
      this.transform = next;
      this.options.finishPreview(next);
    }
  };

  private readonly onPointerCancel = (): void => {
    if (!this.drag) return;
    this.drag = null;
    this.options.cancelPreview();
    if (this.nodeId && this.transform) {
      this.adapter.setTransformOverride(
        this.nodeId,
        this.baseTransform ?? this.transform,
        this.transform,
      );
    }
  };

  private readonly onKeyDown = (event: KeyboardEvent): void => {
    if (event.key !== 'Escape' || !this.drag) return;
    event.preventDefault();
    this.drag = null;
    this.options.cancelPreview();
    if (this.nodeId && this.transform) {
      this.adapter.setTransformOverride(
        this.nodeId,
        this.baseTransform ?? this.transform,
        this.transform,
      );
    }
    this.scheduleRender();
  };

  private scheduleRender(): void {
    if (this.frame || this.disposed) return;
    this.frame = requestAnimationFrame(() => {
      this.frame = 0;
      this.render();
      if (this.active && this.nodeId && this.transform) {
        this.scheduleRender();
      }
    });
  }

  private render(): void {
    this.svg.replaceChildren();
    if (!this.active || !this.nodeId || !this.transform) return;
    const frame = this.adapter.getTransformFrame(
      this.nodeId,
      this.baseTransform ?? this.transform,
    );
    if (!frame) return;
    const origin = this.adapter.projectPoint(frame.origin);
    if (!origin.visible) return;
    if (this.mode === 'rotate') {
      this.renderRotate(origin);
    } else {
      this.renderAxes(frame.origin, origin, frame.parentMatrix);
      this.renderCenter(origin);
    }
    this.renderModeLabel(origin);
  }

  private renderAxes(
    worldOrigin: readonly [number, number, number],
    origin: { x: number; y: number; visible: boolean },
    parentMatrix: readonly number[],
  ): void {
    for (const axis of ['x', 'y', 'z'] as const) {
      const endpoint = this.axisEndpoint(worldOrigin, axis, parentMatrix);
      const handle = projectAxisHandle(origin, this.adapter.projectPoint(endpoint.point));
      const group = this.createElement('g', {
        'data-gizmo-axis': axis,
        'aria-disabled': String(!handle.enabled),
        class: `transform-gizmo__handle transform-gizmo__handle--${axis}`,
      });
      group.append(
        this.createElement('line', {
          x1: String(handle.startX),
          y1: String(handle.startY),
          x2: String(handle.endX),
          y2: String(handle.endY),
        }),
        this.createElement('circle', {
          cx: String(handle.endX),
          cy: String(handle.endY),
          r: this.mode === 'scale' ? '4' : '3',
        }),
        this.createElement('text', {
          x: String(handle.endX + 7),
          y: String(handle.endY - 7),
        }, axis.toUpperCase()),
      );
      this.svg.append(group);
    }
  }

  private renderRotate(
    origin: { x: number; y: number; visible: boolean },
  ): void {
    (['x', 'y', 'z'] as const).forEach((axis, index) => {
      const group = this.createElement('g', {
        'data-gizmo-axis': axis,
        'aria-disabled': 'false',
        class: `transform-gizmo__handle transform-gizmo__handle--${axis} transform-gizmo__ring`,
      });
      group.append(
        this.createElement('circle', {
          cx: String(origin.x),
          cy: String(origin.y),
          r: String(ROTATE_RADII[index]!),
          'stroke-dasharray': index === 0 ? '8 5' : index === 1 ? '2 4' : '',
        }),
        this.createElement('text', {
          x: String(origin.x + ROTATE_RADII[index]! + 5),
          y: String(origin.y - 5),
        }, axis.toUpperCase()),
      );
      this.svg.append(group);
    });
  }

  private renderCenter(
    origin: { x: number; y: number; visible: boolean },
  ): void {
    const group = this.createElement('g', {
      'data-gizmo-axis': 'free',
      'aria-disabled': String(!origin.visible),
      class: 'transform-gizmo__handle transform-gizmo__center',
    });
    group.append(this.createElement('circle', {
      cx: String(origin.x),
      cy: String(origin.y),
      r: '5',
    }));
    this.svg.append(group);
  }

  private renderModeLabel(
    origin: { x: number; y: number; visible: boolean },
  ): void {
    const text = this.createElement('text', {
      x: String(origin.x + 10),
      y: String(origin.y + 20),
      class: 'transform-gizmo__label',
    }, `${this.mode} · ${this.space}`);
    this.svg.append(text);
  }

  private axisEndpoint(
    origin: readonly [number, number, number],
    axis: 'x' | 'y' | 'z',
    parentMatrix: readonly number[],
  ): { point: [number, number, number]; worldLength: number } {
    const localAxis = axis === 'x'
      ? { x: 1, y: 0, z: 0 }
      : axis === 'y'
        ? { x: 0, y: 1, z: 0 }
        : { x: 0, y: 0, z: 1 };
    const worldAxis = this.space === 'local'
      ? rotateVector(
          rotateVector(localAxis, this.worldRotation()),
          quatFromMatrix(parentMatrix, false),
        )
      : localAxis;
    const projectedOrigin = this.adapter.projectPoint(origin);
    const probe = this.adapter.projectPoint([
      origin[0] + worldAxis.x,
      origin[1] + worldAxis.y,
      origin[2] + worldAxis.z,
    ]);
    const pixelLength = Math.max(1, Math.hypot(probe.x - projectedOrigin.x, probe.y - projectedOrigin.y));
    const worldLength = HANDLE_LENGTH_PX / pixelLength;
    return {
      point: [
        origin[0] + worldAxis.x * worldLength,
        origin[1] + worldAxis.y * worldLength,
        origin[2] + worldAxis.z * worldLength,
      ],
      worldLength,
    };
  }

  private transformForPointer(
    drag: ActiveDrag,
    clientX: number,
    clientY: number,
    shift: boolean,
  ): StudioTransform | null {
    if (!this.nodeId) return null;
    const frame = this.adapter.getTransformFrame(
      this.nodeId,
      this.baseTransform ?? drag.base,
    );
    if (!frame) return null;
    const origin = this.adapter.projectPoint(frame.origin);
    const next = structuredClone(drag.base);
    const dragPoint = {
      startX: drag.startX,
      startY: drag.startY,
      currentX: clientX,
      currentY: clientY,
    };
    if (this.mode === 'rotate') {
      if (drag.axis === 'free') return next;
      const angle = angleDegreesFromScreen(
        origin,
        { x: drag.startX, y: drag.startY },
        { x: clientX, y: clientY },
      );
      const degrees = shift ? snapValue(angle, 15) : angle;
      const axis = axisVector(drag.axis);
      const localAxis = this.space === 'local'
        ? rotateVector(axis, drag.base.rotation)
        : axis;
      const delta = quatFromAxisAngleDegrees(localAxis, degrees);
      next.rotation = this.space === 'world'
        ? multiplyQuaternions(
          multiplyQuaternions(
            quatFromMatrix(drag.parentMatrix, true),
            delta,
          ),
          quatFromMatrix(drag.parentMatrix, false),
        )
        : delta;
      next.rotation = multiplyQuaternions(next.rotation, drag.base.rotation);
      return next;
    }
    if (drag.axis === 'free') {
      if (this.mode === 'scale') {
        const factor = Math.max(0.01, 1 + (clientX - drag.startX - (clientY - drag.startY)) / 120);
        const snapped = shift ? Math.max(0.01, snapValue(factor, 0.1)) : factor;
        next.scale = {
          x: Math.max(0.01, drag.base.scale.x * snapped),
          y: Math.max(0.01, drag.base.scale.y * snapped),
          z: Math.max(0.01, drag.base.scale.z * snapped),
        };
      }
      return next;
    }
    const endpoint = this.axisEndpoint(frame.origin, drag.axis, frame.parentMatrix);
    const axisScreen = projectAxisHandle(
      origin,
      this.adapter.projectPoint(endpoint.point),
    );
    let distance = screenDragToWorldDelta(dragPoint, axisScreen, endpoint.worldLength);
    if (shift) distance = snapValue(distance, 0.1);
    if (this.mode === 'move') {
      const localAxis = this.space === 'local'
        ? rotateVector(axisVector(drag.axis), drag.base.rotation)
        : worldVectorToParent(axisVector(drag.axis), drag.parentMatrix);
      next.translation = {
        x: drag.base.translation.x + localAxis.x * distance,
        y: drag.base.translation.y + localAxis.y * distance,
        z: drag.base.translation.z + localAxis.z * distance,
      };
    } else if (this.mode === 'scale') {
      const factor = Math.max(0.01, 1 + distance / Math.max(0.1, endpoint.worldLength));
      const snapped = shift ? Math.max(0.01, snapValue(factor, 0.1)) : factor;
      next.scale[drag.axis] = Math.max(0.01, drag.base.scale[drag.axis] * snapped);
    }
    return next;
  }

  private worldRotation(): [number, number, number, number] {
    return this.transform?.rotation ?? [0, 0, 0, 1];
  }

  private createElement<K extends keyof SVGElementTagNameMap>(
    tag: K,
    attributes: Record<string, string>,
    text?: string,
  ): SVGElementTagNameMap[K] {
    const element = document.createElementNS(SVG, tag);
    for (const [name, value] of Object.entries(attributes)) {
      element.setAttribute(name, value);
    }
    if (text !== undefined) element.textContent = text;
    return element;
  }
}

function axisVector(axis: 'x' | 'y' | 'z'): StudioVec3 {
  return axis === 'x'
    ? { x: 1, y: 0, z: 0 }
    : axis === 'y'
      ? { x: 0, y: 1, z: 0 }
      : { x: 0, y: 0, z: 1 };
}

function rotateVector(
  vector: StudioVec3,
  quaternion: readonly [number, number, number, number],
): StudioVec3 {
  const [x, y, z, w] = quaternion;
  const ix = w * vector.x + y * vector.z - z * vector.y;
  const iy = w * vector.y + z * vector.x - x * vector.z;
  const iz = w * vector.z + x * vector.y - y * vector.x;
  const iw = -x * vector.x - y * vector.y - z * vector.z;
  return {
    x: ix * w + iw * -x + iy * -z - iz * -y,
    y: iy * w + iw * -y + iz * -x - ix * -z,
    z: iz * w + iw * -z + ix * -y - iy * -x,
  };
}

function quatFromAxisAngleDegrees(
  axis: StudioVec3,
  degrees: number,
): [number, number, number, number] {
  const half = (degrees * Math.PI / 180) / 2;
  const length = Math.max(1e-9, Math.hypot(axis.x, axis.y, axis.z));
  const sine = Math.sin(half) / length;
  return [axis.x * sine, axis.y * sine, axis.z * sine, Math.cos(half)];
}

function multiplyQuaternions(
  left: readonly [number, number, number, number],
  right: readonly [number, number, number, number],
): [number, number, number, number] {
  return [
    left[3] * right[0] + left[0] * right[3] + left[1] * right[2] - left[2] * right[1],
    left[3] * right[1] - left[0] * right[2] + left[1] * right[3] + left[2] * right[0],
    left[3] * right[2] + left[0] * right[1] - left[1] * right[0] + left[2] * right[3],
    left[3] * right[3] - left[0] * right[0] - left[1] * right[1] - left[2] * right[2],
  ];
}

function quatFromMatrix(
  matrix: readonly number[],
  conjugate: boolean,
): [number, number, number, number] {
  const x = normalize([matrix[0]!, matrix[1]!, matrix[2]!]);
  const y = normalize([matrix[4]!, matrix[5]!, matrix[6]!]);
  const z = normalize([matrix[8]!, matrix[9]!, matrix[10]!]);
  const trace = x[0] + y[1] + z[2];
  const quaternion: [number, number, number, number] = trace > 0
    ? [
        (y[2] - z[1]) / Math.sqrt(trace + 1) / 2,
        (z[0] - x[2]) / Math.sqrt(trace + 1) / 2,
        (x[1] - y[0]) / Math.sqrt(trace + 1) / 2,
        Math.sqrt(trace + 1) / 2,
      ]
    : x[0] > y[1] && x[0] > z[2]
      ? [
          0.5 * Math.sqrt(1 + x[0] - y[1] - z[2]),
          (x[1] + y[0]) / Math.sqrt(1 + x[0] - y[1] - z[2]) / 2,
          (x[2] + z[0]) / Math.sqrt(1 + x[0] - y[1] - z[2]) / 2,
          (y[2] - z[1]) / Math.sqrt(1 + x[0] - y[1] - z[2]) / 2,
        ]
      : y[1] > z[2]
        ? [
            (x[1] + y[0]) / Math.sqrt(1 + y[1] - x[0] - z[2]) / 2,
            0.5 * Math.sqrt(1 + y[1] - x[0] - z[2]),
            (y[2] + z[1]) / Math.sqrt(1 + y[1] - x[0] - z[2]) / 2,
            (z[0] - x[2]) / Math.sqrt(1 + y[1] - x[0] - z[2]) / 2,
          ]
        : [
            (x[2] + z[0]) / Math.sqrt(1 + z[2] - x[0] - y[1]) / 2,
            (y[2] + z[1]) / Math.sqrt(1 + z[2] - x[0] - y[1]) / 2,
            0.5 * Math.sqrt(1 + z[2] - x[0] - y[1]),
            (x[1] - y[0]) / Math.sqrt(1 + z[2] - x[0] - y[1]) / 2,
          ];
  return conjugate
    ? [-quaternion[0], -quaternion[1], -quaternion[2], quaternion[3]]
    : quaternion;
}

function worldVectorToParent(
  vector: StudioVec3,
  parentMatrix: readonly number[],
): StudioVec3 {
  const x = normalize([parentMatrix[0]!, parentMatrix[1]!, parentMatrix[2]!]);
  const y = normalize([parentMatrix[4]!, parentMatrix[5]!, parentMatrix[6]!]);
  const z = normalize([parentMatrix[8]!, parentMatrix[9]!, parentMatrix[10]!]);
  return {
    x: dot(vector, x),
    y: dot(vector, y),
    z: dot(vector, z),
  };
}

function normalize(vector: readonly number[]): [number, number, number] {
  const length = Math.max(1e-9, Math.hypot(vector[0]!, vector[1]!, vector[2]!));
  return [vector[0]! / length, vector[1]! / length, vector[2]! / length];
}

function dot(left: StudioVec3, right: readonly [number, number, number]): number {
  return left.x * right[0] + left.y * right[1] + left.z * right[2];
}
