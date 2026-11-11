/** Types for the vendored Agent-Native ocean renderer bundle (see `ocean-renderer.js`). */
export interface OceanColors {
  /** Linear-light RGB, 0..1. */
  fg: [number, number, number];
  bg: [number, number, number];
}

export interface OceanRenderer {
  ready: Promise<unknown>;
  firstFrame: Promise<unknown>;
  dispose(): void;
  setColors(colors: OceanColors): void;
  setPaused(paused: boolean): void;
  /** `[x, y, strength]`: x and y in -1..1, strength 0 when the pointer has left. */
  setPointer(pointer: [number, number, number]): void;
}

export interface OceanOptions {
  canvas: HTMLCanvasElement;
  colors: OceanColors;
  fps?: number;
  onError?: (error: unknown) => void;
}

export interface OceanModule {
  createRenderer(options: OceanOptions): OceanRenderer;
}

export const AgentNativeOcean: OceanModule;
