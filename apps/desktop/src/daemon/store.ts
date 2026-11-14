/** Minimal external store compatible with React's `useSyncExternalStore`. */
export class Store<T> {
  private state: T;
  private listeners = new Set<() => void>();

  constructor(initial: T) {
    this.state = initial;
  }

  readonly subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  readonly getSnapshot = (): T => this.state;

  protected set(next: T): void {
    this.state = next;
    for (const listener of [...this.listeners]) listener();
  }

  protected update(patch: Partial<T>): void {
    this.set({ ...this.state, ...patch });
  }
}
