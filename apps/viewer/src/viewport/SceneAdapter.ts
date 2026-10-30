import {
  ICE3,
  ICEAssetHandle,
  ICEAssetLoader,
  ICEOrbitController,
  ICEPerspectiveCamera,
} from 'ice3';

export class SceneAdapter {
  private engine: ICE3 | null = null;
  private loader: ICEAssetLoader | null = null;
  private camera: ICEPerspectiveCamera | null = null;
  private orbit: ICEOrbitController | null = null;
  private handle: ICEAssetHandle | null = null;
  onSelection: ((ids: readonly string[]) => void) | undefined;

  async mount(container: HTMLElement): Promise<void> {
    const engine = await new ICE3({ renderer: 'webgl2' }).initAsync(container);
    engine.setBackgroundColor('#000000');
    const camera = new ICEPerspectiveCamera({
      id: 'studio-camera',
      fov: 42,
      near: 0.05,
      far: 3000,
      position: [45, 32, 55],
      target: [0, 1, 0],
    });
    engine.setCamera(camera).start();
    engine.on<{ selection: readonly string[] }>('SELECTION_CHANGE', (event) => {
      this.onSelection?.(event.param?.selection ?? []);
    });
    this.engine = engine;
    this.loader = new ICEAssetLoader(engine);
    this.camera = camera;
  }

  async show(url: string): Promise<void> {
    if (!this.engine || !this.loader || !this.camera) throw new Error('scene is not mounted');
    const handle = await this.loader.load(`${url}?v=${Date.now()}`);
    const { min, max } = handle.bounds;
    const center: [number, number, number] = [
      (min[0] + max[0]) / 2,
      (min[1] + max[1]) / 2,
      (min[2] + max[2]) / 2,
    ];
    const radius = Math.max(1, Math.hypot(
      max[0] - min[0],
      max[1] - min[1],
      max[2] - min[2],
    ) / 2);
    const distance = radius / Math.sin(this.camera.fov * Math.PI / 360) * 1.15;
    this.camera.setTarget(center).setPosition([
      center[0] + distance * 0.72,
      center[1] + distance * 0.48,
      center[2] + distance * 0.82,
    ]);
    this.orbit?.dispose();
    this.handle?.dispose();
    this.handle = handle;
    this.engine.setScene(handle.scene);
    this.orbit = new ICEOrbitController(this.engine, this.camera).start();
    this.engine.requestRender('studio-preview-ready');
  }
}
