import { formatMetres, type Measurement } from './measure';
import type { SceneAdapter } from './SceneAdapter';

const SVG = 'http://www.w3.org/2000/svg';

type ScreenPoint = { x: number; y: number; visible: boolean };
type LiveMeasurement = Measurement & { fromScreen: ScreenPoint; toScreen: ScreenPoint };

/**
 * Viewport measurement tool: toggle "Measure", click two objects, and read the
 * centre distance plus surface clearance. Implemented imperatively so it can
 * coexist with the canvases the engine appends to the viewport container.
 */
export class MeasureTool {
  private readonly adapter: SceneAdapter;
  private readonly toolbar: HTMLDivElement;
  private readonly toggle: HTMLButtonElement;
  private readonly clear: HTMLButtonElement;
  private readonly hint: HTMLDivElement;
  private readonly overlay: SVGSVGElement;
  private readonly line: SVGLineElement;
  private readonly startDot: SVGCircleElement;
  private readonly endDot: SVGCircleElement;
  private readonly label: HTMLDivElement;
  private readonly unsubscribe: () => void;
  private active = false;
  private picks: string[] = [];
  private current: LiveMeasurement | null = null;
  private frame = 0;

  constructor(container: HTMLElement, adapter: SceneAdapter) {
    this.adapter = adapter;

    this.toolbar = document.createElement('div');
    this.toolbar.className = 'measure-toolbar';

    this.toggle = document.createElement('button');
    this.toggle.type = 'button';
    this.toggle.className = 'measure-button';
    this.toggle.textContent = 'Measure';
    this.toggle.dataset.active = 'false';
    this.toggle.addEventListener('click', () => this.setActive(!this.active));

    this.clear = document.createElement('button');
    this.clear.type = 'button';
    this.clear.className = 'measure-button';
    this.clear.textContent = 'Clear';
    this.clear.addEventListener('click', () => this.reset());

    this.hint = document.createElement('div');
    this.hint.className = 'measure-hint';
    this.hint.textContent = 'Click two objects to measure';
    this.hint.hidden = true;

    this.toolbar.append(this.toggle, this.clear, this.hint);

    this.overlay = document.createElementNS(SVG, 'svg');
    this.overlay.setAttribute('class', 'measure-overlay');
    this.line = document.createElementNS(SVG, 'line');
    this.line.setAttribute('class', 'measure-line');
    this.startDot = document.createElementNS(SVG, 'circle');
    this.startDot.setAttribute('class', 'measure-dot');
    this.startDot.setAttribute('r', '3');
    this.endDot = document.createElementNS(SVG, 'circle');
    this.endDot.setAttribute('class', 'measure-dot');
    this.endDot.setAttribute('r', '3');
    this.overlay.append(this.line, this.startDot, this.endDot);

    this.label = document.createElement('div');
    this.label.className = 'measure-label';
    this.label.hidden = true;

    container.append(this.toolbar, this.overlay, this.label);
    this.unsubscribe = adapter.addSelectionListener((ids) => this.onSelection(ids));
    this.render();
  }

  dispose(): void {
    this.stopLoop();
    this.unsubscribe();
    this.toolbar.remove();
    this.overlay.remove();
    this.label.remove();
  }

  private setActive(active: boolean): void {
    this.active = active;
    this.toggle.dataset.active = String(active);
    this.hint.hidden = !active;
    if (!active) this.reset();
    this.render();
  }

  private onSelection(ids: readonly string[]): void {
    if (!this.active) return;
    const id = ids[0];
    if (!id) return;
    if (this.picks.length === 0 && this.current) {
      this.current = null;
      this.stopLoop();
    }
    this.picks.push(id);
    if (this.picks.length === 2) {
      const [first, second] = this.picks;
      this.picks = [];
      if (first && second) {
        this.current = this.adapter.measureBetween(first, second);
        this.startLoop();
        this.render();
      }
    } else {
      this.hint.textContent = 'Pick the second object';
      this.render();
    }
  }

  private reset(): void {
    this.picks = [];
    this.current = null;
    this.hint.textContent = 'Click two objects to measure';
    this.stopLoop();
    this.render();
  }

  private startLoop(): void {
    if (this.frame) return;
    const tick = (): void => {
      if (!this.current) {
        this.frame = 0;
        return;
      }
      this.current = {
        ...this.current,
        fromScreen: this.adapter.projectPoint(this.current.from),
        toScreen: this.adapter.projectPoint(this.current.to),
      };
      this.render();
      this.frame = requestAnimationFrame(tick);
    };
    this.frame = requestAnimationFrame(tick);
  }

  private stopLoop(): void {
    if (this.frame) cancelAnimationFrame(this.frame);
    this.frame = 0;
  }

  private render(): void {
    const measurement = this.current;
    if (!measurement || !measurement.fromScreen.visible || !measurement.toScreen.visible) {
      this.overlay.style.display = 'none';
      this.label.hidden = true;
      return;
    }
    const { fromScreen, toScreen } = measurement;
    this.overlay.style.display = '';
    this.line.setAttribute('x1', String(fromScreen.x));
    this.line.setAttribute('y1', String(fromScreen.y));
    this.line.setAttribute('x2', String(toScreen.x));
    this.line.setAttribute('y2', String(toScreen.y));
    this.startDot.setAttribute('cx', String(fromScreen.x));
    this.startDot.setAttribute('cy', String(fromScreen.y));
    this.endDot.setAttribute('cx', String(toScreen.x));
    this.endDot.setAttribute('cy', String(toScreen.y));

    this.label.hidden = false;
    this.label.textContent = `${formatMetres(measurement.distance)} · gap ${formatMetres(measurement.clearance)}`;
    this.label.style.left = `${(fromScreen.x + toScreen.x) / 2}px`;
    this.label.style.top = `${(fromScreen.y + toScreen.y) / 2}px`;
  }
}
