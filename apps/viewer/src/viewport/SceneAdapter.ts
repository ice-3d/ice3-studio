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
import { measureBounds, type Measurement } from './measure';
import {
  adaptiveCameraClipPlanes,
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

export const SCENE_BACKGROUND_COLOR = '#14181c';

type ReferenceSelectionBinding = {
  nodeId: string | null;
  selectionId: string;
};

type EntitySelectionOptions = {
  referenceScene: boolean;
  referenceBindings?: readonly ReferenceSelectionBinding[];
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
  private referenceScene = false;
  private renderableSelectionIds: string[] = [];
  private selectionIds: string[] = [];
  private lastBounds: ICEGeometryBounds | null = null;
  // clip 滞回：near/far 变化超过该比例才提交，避免缩放过程中深度映射
  // 每帧重排造成额外的像素抖动。
  private lastClip: { near: number; far: number } | null = null;
  private static readonly CLIP_CHANGE_RATIO = 0.15;
  private resizeObserver: ResizeObserver | null = null;
  private originalNodeState = new WeakMap<ImportedNode, {
    matrix: number[];
    visible: boolean;
    material?: ImportedMaterial;
  }>();
  onSelection: ((ids: readonly string[]) => void) | undefined;
  private readonly selectionListeners = new Set<(ids: readonly string[]) => void>();
  private sectionRatio: number | null = null;

  async mount(container: HTMLElement): Promise<void> {
    this.viewportElement = container;
    const engine = await new ICE3({ renderer: 'webgl2' }).initAsync(container);
    engine.setBackgroundColor(SCENE_BACKGROUND_COLOR);
    const camera = new ICEPerspectiveCamera({
      id: 'studio-camera',
      fov: 42,
      near: SCENE_CAMERA_NEAR,
      far: SCENE_CAMERA_FAR,
      position: [45, 32, 55],
      target: [0, 1, 0],
    });
    engine.setCamera(camera).start();
    const sectionParam = new URLSearchParams(window.location.search).get('section');
    if (sectionParam !== null) {
      const ratio = Number.parseFloat(sectionParam);
      if (Number.isFinite(ratio)) {
        this.sectionRatio = Math.min(1, Math.max(0, ratio));
      }
    }
    engine.on<{ selection: readonly string[] }>('SELECTION_CHANGE', (event) => {
      const normalized = normalizeEntitySelections(
        event.param?.selection ?? [],
        {
          referenceScene: this.referenceScene,
          referenceBindings: this.referenceBindings,
        },
      );
      this.onSelection?.(normalized);
      for (const listener of this.selectionListeners) listener(normalized);
    });
    engine.on('BEFORE_RENDER', () => this.updateClipPlanes());
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
    const importedSelection = expandEntitySelections(
      nodeIds,
      this.renderableSelectionIds,
      {
        referenceScene: this.referenceScene,
        referenceBindings: this.referenceBindings,
      },
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

  /** Enable/disable the horizontal section plane (ratio 0..1 of scene height). */
  setSectionRatio(ratio: number | null): void {
    this.sectionRatio = ratio === null ? null : Math.min(1, Math.max(0, ratio));
    this.applySection();
  }

  private applySection(): void {
    if (!this.engine) return;
    if (this.sectionRatio === null) {
      this.engine.setClipPlane(null);
      this.engine.requestRender('section-off');
      return;
    }
    const range = this.verticalRange();
    if (!range) {
      this.engine.setClipPlane(null);
      return;
    }
    const cut = range.minY + (range.maxY - range.minY) * this.sectionRatio;
    // Keep everything below the cut plane (`dot(world, n) + d >= 0`).
    this.engine.setClipPlane([0, -1, 0, cut]);
    this.engine.requestRender('section');
  }

  private verticalRange(): { minY: number; maxY: number } | null {
    const items = this.engine?.queryScene() ?? [];
    if (items.length === 0) return null;
    let minY = Number.POSITIVE_INFINITY;
    let maxY = Number.NEGATIVE_INFINITY;
    for (const item of items) {
      minY = Math.min(minY, item.bounds.min[1]);
      maxY = Math.max(maxY, item.bounds.max[1]);
    }
    return Number.isFinite(minY) && Number.isFinite(maxY) ? { minY, maxY } : null;
  }

  /** Subscribe to engine selection changes (used by the measure tool). */
  addSelectionListener(listener: (ids: readonly string[]) => void): () => void {
    this.selectionListeners.add(listener);
    return () => this.selectionListeners.delete(listener);
  }

  /** Project a world point to viewport CSS pixels. */
  projectPoint(world: readonly [number, number, number]): { x: number; y: number; visible: boolean } {
    if (!this.engine) return { x: 0, y: 0, visible: false };
    const projected = this.engine.project([world[0], world[1], world[2]]);
    return { x: projected.x, y: projected.y, visible: projected.visible };
  }

  /**
   * Measure two entities by selection/node id using their world bounds.
   * Returns centre distance, surface clearance and projected screen endpoints.
   */
  measureBetween(firstId: string, secondId: string): (Measurement & {
    fromScreen: { x: number; y: number; visible: boolean };
    toScreen: { x: number; y: number; visible: boolean };
  }) | null {
    if (!this.engine) return null;
    const items = this.engine.queryScene();
    const find = (id: string) =>
      items.find((item) => item.selectionId === id || item.nodeId === id);
    const first = find(firstId);
    const second = find(secondId);
    if (!first || !second) return null;
    const measurement = measureBounds(
      { min: first.bounds.min, max: first.bounds.max },
      { min: second.bounds.min, max: second.bounds.max },
    );
    return {
      ...measurement,
      fromScreen: this.projectPoint(measurement.from),
      toScreen: this.projectPoint(measurement.to),
    };
  }

  async showReference(
    asset: StudioReferenceAsset,
    bindings: readonly StudioReferenceBinding[],
  ): Promise<void> {
    if (!this.engine || !this.camera) throw new Error('scene is not mounted');
    this.referenceScene = true;
    this.renderableSelectionIds = [];
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
        // 大场景瓦片同样存在"粗模（抽稀大三角面）逐块被细模替换"的过渡闪烁：
        // 默认 maxCommitsPerFrame=1 / maxConcurrentTiles=2 会让粗→细切换跨越
        // 多帧。这里一次性并发拉取、每帧提交全部，并收紧屏幕空间误差阈值让
        // 细模优先，消除旋转/缩放时的三角形低模弹入。
        maxConcurrentTiles: 6,
        maxCommitsPerFrame: 6,
        maxScreenSpaceErrorPixels: 1,
        maxResidentTiles: 256,
        prefetchMarginRatio: 0.6,
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
        this.applySection();
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

  private async showGlb(url: string, referenceScene = false): Promise<void> {
    if (!this.engine || !this.loader || !this.camera) throw new Error('scene is not mounted');
    const handle = await this.loader.load(`${url}?v=${Date.now()}`);
    this.referenceScene = referenceScene;
    this.renderableSelectionIds = referenceScene
      ? []
      : collectSelectionIds(handle.scene);
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
    this.applySection();
    this.orbit = new ICEOrbitController(this.engine, this.camera).start();
    this.engine.requestRender('studio-preview-ready');
  }

  private async showPreparedReference(url: string): Promise<void> {
    await this.showGlb(url, true);
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
    this.referenceScene = false;
    this.renderableSelectionIds = collectSelectionIds(scene);
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
    this.applySection();
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
    this.updateClipPlanes();
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

  private updateClipPlanes(): void {
    if (!this.camera || !this.lastBounds) return;
    const eye = this.camera.getPosition();
    const target = this.camera.getTarget();
    const distance = Math.hypot(
      eye.x - target.x,
      eye.y - target.y,
      eye.z - target.z,
    );
    const diagonal = Math.hypot(
      this.lastBounds.max[0] - this.lastBounds.min[0],
      this.lastBounds.max[1] - this.lastBounds.min[1],
      this.lastBounds.max[2] - this.lastBounds.min[2],
    );
    const clip = adaptiveCameraClipPlanes(distance, diagonal);
    const last = this.lastClip;
    const significant = last === null
      || Math.abs(clip.near - last.near) > last.near * SceneAdapter.CLIP_CHANGE_RATIO
      || Math.abs(clip.far - last.far) > last.far * SceneAdapter.CLIP_CHANGE_RATIO;
    if (!significant) return;
    this.lastClip = { near: clip.near, far: clip.far };
    this.camera.setClipPlanes(clip.near, clip.far);
  }
}

export function normalizeEntitySelections(
  selectionIds: readonly string[],
  options: EntitySelectionOptions,
): string[] {
  if (options.referenceScene) {
    return mapReferenceSelections(selectionIds, options.referenceBindings ?? []);
  }
  return [...new Set(selectionIds.map(previewOwnerId))];
}

export function expandEntitySelections(
  entityIds: readonly string[],
  renderableSelectionIds: readonly string[],
  options: EntitySelectionOptions,
): string[] {
  if (options.referenceScene) {
    const selectionByNode = new Map(
      (options.referenceBindings ?? [])
        .filter((binding) => binding.nodeId)
        .map((binding) => [binding.nodeId!, binding.selectionId]),
    );
    return [...new Set(entityIds.map(
      (nodeId) => selectionByNode.get(nodeId) ?? nodeId,
    ))];
  }
  const selectedEntities = new Set(entityIds);
  const expanded = renderableSelectionIds.filter(
    (selectionId) => selectedEntities.has(previewOwnerId(selectionId)),
  );
  const renderedEntities = new Set(expanded.map(previewOwnerId));
  for (const entityId of entityIds) {
    if (!renderedEntities.has(entityId)) expanded.push(entityId);
  }
  return [...new Set(expanded)];
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

function previewOwnerId(selectionId: string): string {
  const separator = selectionId.indexOf('/');
  return separator < 0 ? selectionId : selectionId.slice(0, separator);
}

function collectSelectionIds(scene: ICEScene): string[] {
  return collectSceneItems(scene)
    .filter((item) => item.typeId.endsWith(':Mesh'))
    .map((item) => item.selectionId);
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
 * 角度为度、距离为米，都相对视点；视点默认是场景中心，
 * 也可以用 `?target=x,y,z` 指到厂区某一角，用来近看局部。
 * 缺省或非法时沿用自动取景。
 */
export function applyViewOverride(framing: CameraFraming): CameraFraming {
  const params = new URLSearchParams(window.location.search);
  const view = params.get('view');
  if (!view) return framing;
  const parts = view.split(',').map((value) => Number.parseFloat(value));
  if (parts.length !== 3 || !parts.every(Number.isFinite)) return framing;
  const [azimuthDegrees = 0, elevationDegrees = 0, distance = 0] = parts;
  if (distance <= 0) return framing;
  const target = parseTarget(params.get('target')) ?? framing.target;
  const azimuth = (azimuthDegrees * Math.PI) / 180;
  const elevation = (elevationDegrees * Math.PI) / 180;
  return {
    target,
    position: [
      target[0] + distance * Math.cos(elevation) * Math.cos(azimuth),
      target[1] + distance * Math.sin(elevation),
      target[2] + distance * Math.cos(elevation) * Math.sin(azimuth),
    ],
  };
}

function parseTarget(raw: string | null): [number, number, number] | null {
  if (!raw) return null;
  const parts = raw.split(',').map((value) => Number.parseFloat(value));
  if (parts.length !== 3 || !parts.every(Number.isFinite)) return null;
  return [parts[0]!, parts[1]!, parts[2]!];
}
