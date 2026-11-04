import type { SceneAdapter } from './SceneAdapter';

/**
 * Horizontal section plane control: toggle a clip plane at a fraction of the
 * scene height so internals become visible.
 */
export class SectionTool {
  private readonly adapter: SceneAdapter;
  private readonly toolbar: HTMLDivElement;
  private readonly toggle: HTMLButtonElement;
  private readonly slider: HTMLInputElement;
  private active = false;

  constructor(container: HTMLElement, adapter: SceneAdapter) {
    this.adapter = adapter;

    this.toolbar = document.createElement('div');
    this.toolbar.className = 'section-toolbar';

    this.toggle = document.createElement('button');
    this.toggle.type = 'button';
    this.toggle.className = 'measure-button';
    this.toggle.textContent = 'Section';
    this.toggle.dataset.active = 'false';
    this.toggle.setAttribute('aria-pressed', 'false');

    this.slider = document.createElement('input');
    this.slider.type = 'range';
    this.slider.min = '0.05';
    this.slider.max = '0.95';
    this.slider.step = '0.01';
    this.slider.value = '0.5';
    this.slider.className = 'section-slider';
    this.slider.setAttribute('aria-label', 'Section height');
    this.slider.hidden = true;

    this.toggle.addEventListener('click', () => this.setActive(!this.active));
    this.slider.addEventListener('input', () => {
      if (this.active) this.adapter.setSectionRatio(Number(this.slider.value));
    });

    this.toolbar.append(this.toggle, this.slider);
    container.append(this.toolbar);
  }

  dispose(): void {
    this.toolbar.remove();
  }

  private setActive(active: boolean): void {
    this.active = active;
    this.toggle.dataset.active = String(active);
    this.toggle.setAttribute('aria-pressed', String(active));
    this.slider.hidden = !active;
    this.adapter.setSectionRatio(active ? Number(this.slider.value) : null);
  }
}
