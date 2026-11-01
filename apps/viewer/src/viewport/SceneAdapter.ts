import {
  ICE3,
  ICEAssetHandle,
  ICEAssetLoader,
  ICEOrbitController,
  ICEPreviewPacketLoader,
  ICEPerspectiveCamera,
  ICETilesetHandle,
  ICETilesetLoader,
  collectSceneItems,
  type ICEGeometryBounds,
  type ICEIndexedGeometry,
  type ICEPreviewGeometryPacket,
  type ICEScene,
  type ICETilesetEvent,
} from 'ice3';
import type { StudioReferenceAsset } from '../api/StudioClient';
import type { StudioReferenceBinding } from '../session/SessionStore';
import {
  frameBoundsForViewport,
  SCENE_CAMERA_FAR,
  SCENE_CAMERA_NEAR,
  type CameraFraming,
} from './cameraFraming';

export interface ScenePreviewSource {
  readonly url: string;
  readonly glbUrl?: string;
  readonly packetUrl?: string | null;
}

type ReferenceSelectionBinding = {
  nodeId: string | null;
  selectionId: string;
};

export class SceneAdapter {
  private engine: ICE3 | null = null;
  private loader: ICEAssetLoader | null = null;
  private camera: ICEPerspectiveCamera | null = null;
  private orbit: ICEOrbitController | null = null;
  private handle: ICEAssetHandle | null = null;
  private packetLoader: ICEPreviewPacketLoader | null = null;
  private packet: ICEPreviewGeometryPacket | undefined;
  private pinnedPacketGeometries = new Map<string, ICEIndexedGeometry>();
  private viewportElement: HTMLElement | null = null;
  private tilesetLoader: ICETilesetLoader | null = null;
  private tilesetHandle: ICETilesetHandle | null = null;
  private referenceBindings: StudioReferenceBinding[] = [];
  private referenceExposure = 1;
  private selectionIds: string[] = [];
  private lastBounds: ICEGeometryBounds | null = null;
  private resizeObserver: ResizeObserver | null = null;
  private originalNodeState = new WeakMap<ImportedNode, {
    matrix: number[];
    visible: boolean;
    material?: ImportedMaterial;
  }>();
  onSelection: ((ids: readonly string[]) => void) | undefined;

  async mount(container: HTMLElement): Promise<void> {
    this.viewportElement = container;
    const engine = await new ICE3({ renderer: 'webgl2' }).initAsync(container);
    engine.setBackgroundColor('#14181c');
    const camera = new ICEPerspectiveCamera({
      id: 'studio-camera',
      fov: 42,
      near: SCENE_CAMERA_NEAR,
      far: SCENE_CAMERA_FAR,
      position: [45, 32, 55],
      target: [0, 1, 0],
    });
    engine.setCamera(camera).start();
    engine.on<{ selection: readonly string[] }>('SELECTION_CHANGE', (event) => {
      this.onSelection?.(mapReferenceSelections(
        event.param?.selection ?? [],
        this.referenceBindings,
      ));
    });
    this.engine = engine;
    this.loader = new ICEAssetLoader(engine);
    this.packetLoader = new ICEPreviewPacketLoader({
      pinGeometry: (geometry, update) => {
        engine.prepareGeometry(geometry, update);
      },
    });
    this.camera = camera;
    container.dataset.referenceState = 'idle';
    this.resizeObserver = new ResizeObserver(() => {
      if (this.lastBounds) this.frameBounds(this.lastBounds);
    });
    this.resizeObserver.observe(container);
  }

  async show(url: string): Promise<void> {
    await this.showPreview({ url });
  }

  select(nodeIds: readonly string[]): void {
    this.selectionIds = [...nodeIds];
    if (!this.engine) return;
    const bindingByNode = new Map(
      this.referenceBindings
        .filter((binding) => binding.nodeId)
        .map((binding) => [binding.nodeId!, binding.selectionId]),
    );
    const importedSelection = nodeIds.map(
      (nodeId) => bindingByNode.get(nodeId) ?? nodeId,
    );
    const currentSelection = this.engine.getSelection();
    if (
      currentSelection.length === importedSelection.length
      && currentSelection.every((id, index) => id === importedSelection[index])
    ) {
      return;
    }
    this.engine.setSelection(importedSelection);
    this.engine.requestRender('studio-selection-change');
  }

  async showReference(
    asset: StudioReferenceAsset,
    bindings: readonly StudioReferenceBinding[],
  ): Promise<void> {
    if (!this.engine || !this.camera) throw new Error('scene is not mounted');
    this.referenceBindings = [...bindings];
    // 曝光由资产声明，Viewer 只按数值执行，不做任何内容判断。
    this.referenceExposure = asset.descriptor.displayExposure ?? 1;
    if (
      asset.preparedPreviewUrl
      && new URLSearchParams(window.location.search).get('reference') === 'glb'
    ) {
      await this.showPreparedReference(asset.preparedPreviewUrl);
      return;
    }
    if (!asset.tilesetUrl) {
      if (asset.preparedPreviewUrl) {
        await this.showPreparedReference(asset.preparedPreviewUrl);
        return;
      }
      throw new Error('reference asset has no streamable tileset or prepared preview');
    }
    try {
      this.tilesetLoader ??= new ICETilesetLoader(this.engine, {
        camera: this.camera,
        maxConcurrentTiles: 3,
        maxResidentBytes: 805_306_368,
      });
      const replacement = await this.tilesetLoader.open(asset.tilesetUrl);
      this.viewportElement?.setAttribute('data-reference-state', 'loading-coarse');
      this.frameBounds(replacement.bounds);
      let activated = false;
      const activate = (): void => {
        if (activated || !this.engine || !this.camera) return;
        activated = true;
        applyReferenceOverrides(
          replacement.scene,
          this.referenceBindings,
          this.originalNodeState,
          this.referenceExposure,
        );
        this.engine.setScene(replacement.scene);
        this.select(this.selectionIds);
        const previous = this.tilesetHandle;
        this.tilesetHandle = replacement;
        this.handle?.dispose();
        this.handle = null;
        this.packet = undefined;
        previous?.dispose();
        this.orbit?.dispose();
        this.orbit = new ICEOrbitController(this.engine, this.camera).start();
        this.viewportElement?.setAttribute('data-reference-state', 'ready');
        this.engine.requestRender('studio-reference-coarse-ready');
      };
      replacement.on<ICETilesetEvent>('TILESET_TILE_LOADED', () => {
        applyReferenceOverrides(
          replacement.scene,
          this.referenceBindings,
          this.originalNodeState,
          this.referenceExposure,
        );
        activate();
      });
    } catch (error) {
      if (asset.preparedPreviewUrl) {
        await this.showPreparedReference(asset.preparedPreviewUrl);
        return;
      }
      this.viewportElement?.setAttribute('data-reference-state', 'failed');
      throw error;
    }
  }

  async showPreview(preview: ScenePreviewSource): Promise<void> {
    if (!this.engine || !this.loader || !this.camera || !this.packetLoader) {
      throw new Error('scene is not mounted');
    }
    const packetUrl = preview.packetUrl
      ?? (preview.url.startsWith('/preview-packets/') ? preview.url : null);
    if (packetUrl) {
      const result = await this.packetLoader.load(packetUrl, this.packet);
      if (result.kind === 'packet' && this.packetLoader.scene) {
        this.packet = result.packet;
        const geometries = this.packetLoader.currentGeometries;
        for (const [geometryId, geometry] of this.pinnedPacketGeometries) {
          if (!geometries.has(geometryId)) {
            this.engine.releaseGeometry(geometry);
          }
        }
        this.pinnedPacketGeometries = new Map(
          [...geometries].map(([geometryId, geometry]) => [geometryId, geometry]),
        );
        this.publishPacketScene(this.packetLoader.scene);
        return;
      }
    }
    const glbUrl = preview.glbUrl ?? fallbackGlbUrl(preview.url);
    if (!glbUrl) throw new Error('preview has no compatible GLB fallback');
    await this.showGlb(glbUrl);
  }

  private async showGlb(url: string): Promise<void> {
    if (!this.engine || !this.loader || !this.camera) throw new Error('scene is not mounted');
    const handle = await this.loader.load(`${url}?v=${Date.now()}`);
    // 必须先取景再建轨道控制器：控制器构造时记录相机状态，之后每帧写回，
    // 放在取景之后会把取景结果覆盖掉。
    this.frameBounds(handle.bounds);
    this.orbit?.dispose();
    this.handle?.dispose();
    this.handle = handle;
    this.packet = undefined;
    this.tilesetHandle?.dispose();
    this.tilesetHandle = null;
    for (const geometry of this.pinnedPacketGeometries.values()) {
      this.engine.releaseGeometry(geometry);
    }
    this.pinnedPacketGeometries.clear();
    this.engine.setScene(handle.scene);
    this.orbit = new ICEOrbitController(this.engine, this.camera).start();
    this.engine.requestRender('studio-preview-ready');
  }

  private async showPreparedReference(url: string): Promise<void> {
    await this.showGlb(url);
    if (!this.engine || !this.handle) return;
    applyReferenceOverrides(
      this.handle.scene,
      this.referenceBindings,
      this.originalNodeState,
      this.referenceExposure,
    );
    this.engine.setScene(this.handle.scene);
    this.viewportElement?.setAttribute('data-reference-state', 'fallback');
    this.engine.requestRender('studio-reference-prepared-ready');
  }

  private publishPacketScene(scene: ICEScene): void {
    if (!this.engine || !this.camera) throw new Error('scene is not mounted');
    const items = collectSceneItems(scene, { visibleOnly: true });
    const bounds = items.length > 0
      ? items.reduce<ICEGeometryBounds>((current, item) => ({
        min: [
          Math.min(current.min[0], item.bounds.min[0]),
          Math.min(current.min[1], item.bounds.min[1]),
          Math.min(current.min[2], item.bounds.min[2]),
        ],
        max: [
          Math.max(current.max[0], item.bounds.max[0]),
          Math.max(current.max[1], item.bounds.max[1]),
          Math.max(current.max[2], item.bounds.max[2]),
        ],
      }), {
        min: [Infinity, Infinity, Infinity] as const,
        max: [-Infinity, -Infinity, -Infinity] as const,
      })
      : {
        min: [0, 0, 0] as const,
        max: [0, 0, 0] as const,
      };
    this.frameBounds(bounds);
    this.handle?.dispose();
    this.handle = null;
    this.tilesetHandle?.dispose();
    this.tilesetHandle = null;
    this.engine.setScene(scene);
    this.orbit?.dispose();
    this.orbit = new ICEOrbitController(this.engine, this.camera).start();
    this.engine.requestRender('studio-preview-packet-ready');
  }

  private frameBounds(bounds: ICEGeometryBounds): void {
    if (!this.camera) return;
    this.lastBounds = bounds;
    const width = this.viewportElement?.clientWidth ?? 1;
    const height = this.viewportElement?.clientHeight ?? 1;
    const framing = applyViewOverride(frameBoundsForViewport(
      bounds,
      this.camera.fov,
      width / Math.max(1, height),
    ));
    this.camera.setTarget(framing.target).setPosition(framing.position);
    const matrix = this.camera.getViewProjectionMatrix(width / Math.max(1, height));
    let minX = Infinity;
    let maxX = -Infinity;
    let minY = Infinity;
    let maxY = -Infinity;
    let minZ = Infinity;
    let maxZ = -Infinity;
    for (const x of [bounds.min[0], bounds.max[0]]) {
      for (const y of [bounds.min[1], bounds.max[1]]) {
        for (const z of [bounds.min[2], bounds.max[2]]) {
          const clip = matrix.transformPoint(
            { x, y, z } as Parameters<typeof matrix.transformPoint>[0],
          );
          const ndcX = clip[0] / clip[3];
          const ndcY = clip[1] / clip[3];
          const ndcZ = clip[2] / clip[3];
          minX = Math.min(minX, ndcX);
          maxX = Math.max(maxX, ndcX);
          minY = Math.min(minY, ndcY);
          maxY = Math.max(maxY, ndcY);
          minZ = Math.min(minZ, ndcZ);
          maxZ = Math.max(maxZ, ndcZ);
        }
      }
    }
    this.viewportElement?.setAttribute(
      'data-bounds-safe',
      String(
        Math.max(Math.abs(minX), Math.abs(maxX), Math.abs(minY), Math.abs(maxY)) <= 0.9
        && minZ >= -1
        && maxZ <= 1
      ),
    );
    this.viewportElement?.setAttribute(
      'data-projected-bounds',
      JSON.stringify({ minX, maxX, minY, maxY, minZ, maxZ }),
    );
  }
}

export function mapReferenceSelection(
  selectionId: string,
  bindings: readonly ReferenceSelectionBinding[],
): string {
  return bindings.find((binding) => binding.selectionId === selectionId)?.nodeId ?? selectionId;
}

function mapReferenceSelections(
  selectionIds: readonly string[],
  bindings: readonly ReferenceSelectionBinding[],
): string[] {
  return [...new Set(selectionIds.map((selectionId) => (
    mapReferenceSelection(selectionId, bindings)
  )))];
}

type ImportedNode = {
  name?: string;
  selectionId?: string;
  semanticSelectionIds?: readonly string[];
  children?: readonly ImportedNode[];
  visible: boolean;
  position: { x: number; y: number; z: number };
  rotation: { x: number; y: number; z: number };
  scale: { x: number; y: number; z: number };
  localMatrixOverride?: Float32Array | null;
  material?: ImportedMaterial;
  setLocalMatrix(matrix: readonly number[]): unknown;
};

type ImportedMaterial = {
  readonly color: string;
  readonly edgeColor?: string;
  readonly shading?: 'directional';
};

export function applyReferenceOverrides(
  scene: unknown,
  bindings: readonly StudioReferenceBinding[],
  originalNodeState: WeakMap<ImportedNode, {
    matrix: number[];
    visible: boolean;
    material?: ImportedMaterial;
  }>,
  exposure = 1,
): void {
  const nodes = findImportedNodes(scene as ImportedNode);
  for (const binding of bindings) {
    if (!binding.nodeId) continue;
    for (const node of nodes) {
      if (!matchesSelection(node, binding.selectionId)) continue;
      if (!originalNodeState.has(node)) {
        originalNodeState.set(node, {
          matrix: currentLocalMatrix(node),
          visible: node.visible,
          material: node.material,
        });
      }
      const original = originalNodeState.get(node)!;
      if (isIdentityTransform(binding.transform)) {
        node.setLocalMatrix(original.matrix);
      } else {
        node.setLocalMatrix(multiplyMatrices(transformMatrix(binding.transform), original.matrix));
      }
      node.visible = binding.visible ? original.visible : false;
    }
  }

  const bindingsBySelection = new Map(
    bindings.map((binding) => [binding.selectionId, binding]),
  );
  const applyBindings = (
    node: ImportedNode,
    inheritedBinding: StudioReferenceBinding | undefined,
  ): void => {
    if (!originalNodeState.has(node)) {
      originalNodeState.set(node, {
        matrix: currentLocalMatrix(node),
        visible: node.visible,
        material: node.material,
      });
    }
    const original = originalNodeState.get(node)!;
    const directBinding = matchingMaterialBinding(node, bindingsBySelection);
    const binding = directBinding ?? inheritedBinding;
    // 可见性只由资产声明的 hidden 决定，不再解析节点名。
    node.visible = binding?.hidden ? false : original.visible;
    if (!original.material) {
      // Group nodes carry no material but still propagate semantic context.
    } else if (binding?.materialColor) {
      node.material = cloneMaterial(original.material, binding.materialColor);
    } else {
      node.material = presentReferenceMaterial(original.material, exposure);
    }
    for (const child of node.children ?? []) {
      applyBindings(child, binding);
    }
  };
  if (scene && typeof scene === 'object') {
    applyBindings(scene as ImportedNode, undefined);
  }
}


function matchingMaterialBinding(
  node: ImportedNode,
  bindingsBySelection: ReadonlyMap<string, StudioReferenceBinding>,
): StudioReferenceBinding | undefined {
  const selectionIds = [
    ...(node.selectionId ? [node.selectionId] : []),
    ...(node.semanticSelectionIds ?? []),
  ];
  const matches = [...new Set(selectionIds
    .map((selectionId) => bindingsBySelection.get(selectionId))
    .filter((binding): binding is StudioReferenceBinding => Boolean(binding)))];
  if (matches.length === 0) return undefined;
  const colors = new Set(matches.map((binding) => binding.materialColor ?? ''));
  return colors.size === 1 ? matches[0] : undefined;
}

function cloneMaterial(material: ImportedMaterial, color: string): ImportedMaterial {
  const cloned = Object.create(Object.getPrototypeOf(material)) as ImportedMaterial;
  Object.assign(cloned, material, { color });
  return cloned;
}

function presentReferenceMaterial(
  material: ImportedMaterial,
  exposure: number,
): ImportedMaterial {
  if (!(exposure > 1)) return material;
  return cloneMaterial(
    material,
    lightenReferenceColor(material.color, Math.min(0.9, exposure - 1)),
  );
}

export function lightenReferenceColor(color: string, amount = 0.42): string {
  const normalized = color.startsWith('#') ? color.slice(1) : color;
  const expanded = normalized.length === 3
    ? normalized.split('').map((character) => `${character}${character}`).join('')
    : normalized;
  if (!/^[0-9a-fA-F]{6}([0-9a-fA-F]{2})?$/.test(expanded)) return color;
  const channels = [0, 2, 4].map((offset) => {
    const value = Number.parseInt(expanded.slice(offset, offset + 2), 16);
    return Math.round(value + (255 - value) * amount)
      .toString(16)
      .padStart(2, '0');
  });
  return `#${channels.join('')}${expanded.slice(6)}`;
}


function findImportedNodes(root: ImportedNode): ImportedNode[] {
  const output: ImportedNode[] = [];
  const visit = (node: ImportedNode): void => {
    if (node.selectionId || node.semanticSelectionIds?.length) output.push(node);
    for (const child of node.children ?? []) visit(child);
  };
  visit(root);
  return output;
}

function matchesSelection(node: ImportedNode, selectionId: string): boolean {
  return node.selectionId === selectionId
    || node.semanticSelectionIds?.includes(selectionId) === true;
}

function isIdentityTransform(transform: StudioReferenceBinding['transform']): boolean {
  return transform.translation.x === 0
    && transform.translation.y === 0
    && transform.translation.z === 0
    && transform.rotation[0] === 0
    && transform.rotation[1] === 0
    && transform.rotation[2] === 0
    && transform.rotation[3] === 1
    && transform.scale.x === 1
    && transform.scale.y === 1
    && transform.scale.z === 1;
}

function currentLocalMatrix(node: ImportedNode): number[] {
  if (node.localMatrixOverride) return [...node.localMatrixOverride];
  return eulerTransformMatrix(node.position, node.rotation, node.scale);
}

function eulerTransformMatrix(
  translation: { x: number; y: number; z: number },
  rotation: { x: number; y: number; z: number },
  scale: { x: number; y: number; z: number },
): number[] {
  const cx = Math.cos(rotation.x);
  const sx = Math.sin(rotation.x);
  const cy = Math.cos(rotation.y);
  const sy = Math.sin(rotation.y);
  const cz = Math.cos(rotation.z);
  const sz = Math.sin(rotation.z);
  return [
    cy * cz * scale.x,
    (sx * sy * cz + cx * sz) * scale.x,
    (-cx * sy * cz + sx * sz) * scale.x,
    0,
    -cy * sz * scale.y,
    (-sx * sy * sz + cx * cz) * scale.y,
    (cx * sy * sz + sx * cz) * scale.y,
    0,
    sy * scale.z,
    -sx * cy * scale.z,
    cx * cy * scale.z,
    0,
    translation.x,
    translation.y,
    translation.z,
    1,
  ];
}

function transformMatrix(transform: StudioReferenceBinding['transform']): number[] {
  const [x, y, z, w] = transform.rotation;
  const x2 = x + x;
  const y2 = y + y;
  const z2 = z + z;
  const xx = x * x2;
  const xy = x * y2;
  const xz = x * z2;
  const yy = y * y2;
  const yz = y * z2;
  const zz = z * z2;
  const wx = w * x2;
  const wy = w * y2;
  const wz = w * z2;
  return [
    (1 - (yy + zz)) * transform.scale.x,
    (xy + wz) * transform.scale.x,
    (xz - wy) * transform.scale.x,
    0,
    (xy - wz) * transform.scale.y,
    (1 - (xx + zz)) * transform.scale.y,
    (yz + wx) * transform.scale.y,
    0,
    (xz + wy) * transform.scale.z,
    (yz - wx) * transform.scale.z,
    (1 - (xx + yy)) * transform.scale.z,
    0,
    transform.translation.x,
    transform.translation.y,
    transform.translation.z,
    1,
  ];
}

function multiplyMatrices(left: readonly number[], right: readonly number[]): number[] {
  const output = Array.from({ length: 16 }, () => 0);
  for (let column = 0; column < 4; column += 1) {
    for (let row = 0; row < 4; row += 1) {
      let value = 0;
      for (let index = 0; index < 4; index += 1) {
        value += left[index * 4 + row]! * right[column * 4 + index]!;
      }
      output[column * 4 + row] = value;
    }
  }
  return output;
}

function fallbackGlbUrl(url: string): string | null {
  const revision = /^\/preview-packets\/(\d+)(?:\?|$)/.exec(url);
  if (revision) return `/previews/${revision[1]}`;
  const candidate = /^\/preview-packets\/candidates\/([a-fA-F0-9]{64})(?:\?|$)/.exec(url);
  if (candidate) return `/previews/candidates/${candidate[1]}`;
  return url.startsWith('/previews/') ? url.split('?')[0]! : null;
}


/**
 * 允许用 `?view=方位角,仰角,距离` 指定相机，便于固定视角截图与分享。
 * 角度为度、距离为米，都相对场景中心；缺省或非法时沿用自动取景。
 */
export function applyViewOverride(framing: CameraFraming): CameraFraming {
  const view = new URLSearchParams(window.location.search).get('view');
  if (!view) return framing;
  const parts = view.split(',').map((value) => Number.parseFloat(value));
  if (parts.length !== 3 || !parts.every(Number.isFinite)) return framing;
  const [azimuthDegrees = 0, elevationDegrees = 0, distance = 0] = parts;
  if (distance <= 0) return framing;
  const azimuth = (azimuthDegrees * Math.PI) / 180;
  const elevation = (elevationDegrees * Math.PI) / 180;
  return {
    target: framing.target,
    position: [
      framing.target[0] + distance * Math.cos(elevation) * Math.cos(azimuth),
      framing.target[1] + distance * Math.sin(elevation),
      framing.target[2] + distance * Math.cos(elevation) * Math.sin(azimuth),
    ],
  };
}
