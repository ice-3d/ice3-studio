import type { StudioRouteInspection } from '../api/StudioClient';
import { ExternalStore } from '../shared/ExternalStore';

type Source = {
  routeInspection(lineId: string, revision: number): Promise<StudioRouteInspection>;
};

export type RouteInspectionSnapshot = {
  key: string | null;
  lineId: string | null;
  revision: number;
  status: 'idle' | 'loading' | 'ready' | 'error';
  inspection: StudioRouteInspection | null;
  error: string | null;
};

export class RouteInspectionStore extends ExternalStore<RouteInspectionSnapshot> {
  private generation = 0;

  constructor(private readonly source: Source) {
    super({
      key: null,
      lineId: null,
      revision: 0,
      status: 'idle',
      inspection: null,
      error: null,
    });
  }

  async load(lineId: string, revision: number): Promise<void> {
    const key = `${revision}\0${lineId}`;
    const generation = ++this.generation;
    const sameSelection = this.snapshot.key === key;
    this.update({
      key,
      lineId,
      revision,
      status: sameSelection && this.snapshot.status === 'ready' ? 'ready' : 'loading',
      inspection: sameSelection ? this.snapshot.inspection : null,
      error: null,
    });
    try {
      const inspection = await this.source.routeInspection(lineId, revision);
      if (generation !== this.generation) return;
      this.update({
        key,
        lineId,
        revision,
        status: 'ready',
        inspection,
        error: null,
      });
    } catch (cause) {
      if (generation !== this.generation) return;
      this.update({
        key,
        lineId,
        revision,
        status: 'error',
        inspection: null,
        error: cause instanceof Error ? cause.message : String(cause),
      });
    }
  }

  clear(): void {
    this.generation += 1;
    this.update({
      key: null,
      lineId: null,
      revision: 0,
      status: 'idle',
      inspection: null,
      error: null,
    });
  }
}
