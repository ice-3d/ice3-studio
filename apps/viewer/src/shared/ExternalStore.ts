export class ExternalStore<T> {
  private value: T;
  private readonly listeners = new Set<() => void>();

  constructor(initial: T) {
    this.value = initial;
  }

  get snapshot(): T {
    return this.value;
  }

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  protected update(next: T): void {
    this.value = next;
    for (const listener of this.listeners) listener();
  }
}
