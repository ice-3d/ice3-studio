import {
  ICE3,
  ICEAssetHandle,
  ICEAssetLoader,
  ICEOrbitController,
  ICEPreviewPacketLoader,
  ICEPerspectiveCamera,
  collectSceneItems,
  type ICEGeometryBounds,
  type ICEIndexedGeometry,
  type ICEPreviewGeometryPacket,
  type ICEScene,
} from 'ice3';
import {
  frameBoundsForViewport,
  SCENE_CAMERA_FAR,
  SCENE_CAMERA_NEAR,
} from './cameraFraming';

export interface ScenePreviewSource {
  readonly url: string;
  readonly glbUrl?: string;
  readonly packetUrl?: string | null;
}

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
  onSelection: ((ids: readonly string[]) => void) | undefined;

  async mount(container: HTMLElement): Promise<void> {
    this.viewportElement = container;
    const engine = await new ICE3({ renderer: 'webgl2' }).initAsync(container);
    engine.setBackgroundColor('#000000');
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
      this.onSelection?.(event.param?.selection ?? []);
    });
    this.engine = engine;
    this.loader = new ICEAssetLoader(engine);
    this.packetLoader = new ICEPreviewPacketLoader({
      pinGeometry: (geometry, update) => {
        engine.prepareGeometry(geometry, update);
      },
    });
    this.camera = camera;
  }

  async show(url: string): Promise<void> {
    await this.showPreview({ url });
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
    this.frameBounds(handle.bounds);
    this.orbit?.dispose();
    this.handle?.dispose();
    this.handle = handle;
    this.packet = undefined;
    for (const geometry of this.pinnedPacketGeometries.values()) {
      this.engine.releaseGeometry(geometry);
    }
    this.pinnedPacketGeometries.clear();
    this.engine.setScene(handle.scene);
    this.orbit = new ICEOrbitController(this.engine, this.camera).start();
    this.engine.requestRender('studio-preview-ready');
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
    this.engine.setScene(scene);
    this.orbit?.dispose();
    this.orbit = new ICEOrbitController(this.engine, this.camera).start();
    this.engine.requestRender('studio-preview-packet-ready');
  }

  private frameBounds(bounds: ICEGeometryBounds): void {
    if (!this.camera) return;
    const width = this.viewportElement?.clientWidth ?? 1;
    const height = this.viewportElement?.clientHeight ?? 1;
    const framing = frameBoundsForViewport(
      bounds,
      this.camera.fov,
      width / Math.max(1, height),
    );
    this.camera.setTarget(framing.target).setPosition(framing.position);
  }
}

function fallbackGlbUrl(url: string): string | null {
  const revision = /^\/preview-packets\/(\d+)(?:\?|$)/.exec(url);
  if (revision) return `/previews/${revision[1]}`;
  const candidate = /^\/preview-packets\/candidates\/([a-fA-F0-9]{64})(?:\?|$)/.exec(url);
  if (candidate) return `/previews/candidates/${candidate[1]}`;
  return url.startsWith('/previews/') ? url.split('?')[0]! : null;
}
