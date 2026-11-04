import type { SceneAdapter } from './SceneAdapter';

/**
 * Exploded view control: spread the scene's top-level nodes away from its
 * centre so internal parts separate. Factor 0 restores the assembly.
 */
export class ExplodeTool {
  private readonly adapter: SceneAdapter;
  private readonly toolbar: HTMLDivElement;
  private readonly toggle: HTMLButtonElement;
  private readonly slider: HTMLInputElement;
  private active = false;

  constructor(container: HTMLElement, adapter: SceneAdapter) {
    this.adapter = adapter;

    this.toolbar = document.createElement('div');
    this.toolbar.className = 'explode-toolbar';

    this.toggle = document.createElement('button');
    this.toggle.type = 'button';
    this.toggle.className = 'measure-button';
    this.toggle.textContent = 'Explode';
    this.toggle.dataset.active = 'false';
    this.toggle.setAttribute('aria-pressed', 'false');

    this.slider = document.createElement('input');
    this.slider.type = 'range';
    this.slider.min = '0.05';
    this.slider.max = '1';
    this.slider.step = '0.01';
    this.slider.value = '0.35';
    this.slider.className = 'section-slider';
    this.slider.setAttribute('aria-label', 'Explode amount');
    this.slider.hidden = true;

    this.toggle.addEventListener('click', () => this.setActive(!this.active));
    this.slider.addEventListener('input', () => {
      if (this.active) this.adapter.setExplodeFactor(Number(this.slider.value));
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
    this.adapter.setExplodeFactor(active ? Number(this.slider.value) : 0);
  }
}
