import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { defaultLoadOcean, mountAmbientBackground, parseBackground, type AmbientController } from "./engine";
import type { OceanModule, OceanOptions, OceanRenderer } from "./vendor/ocean-renderer";

type Listener = () => void;

function installMedia(initial: { reduced?: boolean; dark?: boolean } = {}) {
  const state = { reduced: initial.reduced ?? false, dark: initial.dark ?? false };
  const listeners = { reduced: new Set<Listener>(), dark: new Set<Listener>() };
  const list = (key: "reduced" | "dark") => ({
    get matches() {
      return state[key];
    },
    addEventListener: (_: string, fn: Listener) => listeners[key].add(fn),
    removeEventListener: (_: string, fn: Listener) => listeners[key].delete(fn),
  });
  const reduced = list("reduced");
  const dark = list("dark");
  vi.stubGlobal(
    "matchMedia",
    vi.fn((query: string) => (query.includes("reduced-motion") ? reduced : dark)),
  );
  return {
    setReduced(value: boolean) {
      state.reduced = value;
      listeners.reduced.forEach((fn) => fn());
    },
    listenerCount: () => listeners.reduced.size + listeners.dark.size,
  };
}

function installFrames() {
  let queue = new Map<number, FrameRequestCallback>();
  let next = 1;
  let clock = 0;
  vi.stubGlobal("requestAnimationFrame", (fn: FrameRequestCallback) => {
    queue.set(next, fn);
    return next++;
  });
  vi.stubGlobal("cancelAnimationFrame", (id: number) => queue.delete(id));
  return {
    pending: () => queue.size,
    step(ms = 100) {
      clock += ms;
      const run = queue;
      queue = new Map();
      run.forEach((fn) => fn(clock));
    },
  };
}

function setHidden(hidden: boolean) {
  Object.defineProperty(document, "hidden", { configurable: true, get: () => hidden });
  document.dispatchEvent(new Event("visibilitychange"));
}

function fakeOcean() {
  let onError: OceanOptions["onError"];
  let resolveFrame!: () => void;
  const renderer: OceanRenderer = {
    ready: Promise.resolve(),
    firstFrame: new Promise<void>((resolve) => (resolveFrame = resolve)),
    dispose: vi.fn(),
    setColors: vi.fn(),
    setPaused: vi.fn(),
    setPointer: vi.fn(),
  };
  const createRenderer = vi.fn((options: OceanOptions) => {
    onError = options.onError;
    return renderer;
  });
  const module: OceanModule = { createRenderer };
  return { module, renderer, createRenderer, frame: () => resolveFrame(), fail: (error: unknown) => onError?.(error) };
}

const fillStyles: string[] = [];
let host: HTMLDivElement;
let controller: AmbientController | undefined;

beforeEach(() => {
  fillStyles.length = 0;
  const context = {
    clearRect: vi.fn(),
    fillRect: vi.fn(),
    setTransform: vi.fn(),
    set fillStyle(value: string) {
      fillStyles.push(value);
    },
    globalAlpha: 1,
  };
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(context as unknown as CanvasRenderingContext2D);
  host = document.createElement("div");
  vi.spyOn(host, "getBoundingClientRect").mockReturnValue({ width: 800, height: 600, left: 0, top: 0 } as DOMRect);
  document.body.append(host);
  document.documentElement.removeAttribute("data-theme");
});

afterEach(() => {
  controller?.destroy();
  controller = undefined;
  host.remove();
  Object.defineProperty(document, "hidden", { configurable: true, get: () => false });
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  document.documentElement.removeAttribute("data-theme");
});

const mount = (options: Parameters<typeof mountAmbientBackground>[1] = {}) =>
  (controller = mountAmbientBackground(host, { hasGpu: () => false, ...options }));

describe("Canvas 2D fallback", () => {
  it("animates, and does not touch the GPU loader when WebGPU is absent", () => {
    installMedia();
    const frames = installFrames();
    const loadOcean = vi.fn();
    mount({ loadOcean });
    expect(host.dataset.engine).toBe("canvas2d");
    const initial = controller!.stats.frames;
    frames.step();
    frames.step();
    expect(controller!.stats.frames).toBeGreaterThan(initial);
    expect(loadOcean).not.toHaveBeenCalled();
  });

  it("caps the frame rate", () => {
    installMedia();
    const frames = installFrames();
    mount({ fps: 10 });
    const before = controller!.stats.frames;
    frames.step(40);
    expect(controller!.stats.frames).toBe(before);
    frames.step(100);
    expect(controller!.stats.frames).toBe(before + 1);
  });

  it("pauses while the tab is hidden and resumes when it returns", () => {
    installMedia();
    const frames = installFrames();
    mount();
    expect(frames.pending()).toBe(1);
    setHidden(true);
    expect(frames.pending()).toBe(0);
    expect(controller!.stats.paused).toBe(true);
    const drawn = controller!.stats.frames;
    frames.step();
    expect(controller!.stats.frames).toBe(drawn);
    setHidden(false);
    expect(frames.pending()).toBe(1);
    expect(controller!.stats.paused).toBe(false);
  });

  it("draws one static dot field under reduced motion and never requests a frame", () => {
    installMedia({ reduced: true });
    const frames = installFrames();
    mount();
    expect(host.dataset.engine).toBe("static");
    expect(frames.pending()).toBe(0);
    const drawn = controller!.stats.frames;
    expect(drawn).toBeGreaterThan(0);
    frames.step();
    expect(controller!.stats.frames).toBe(drawn);
  });

  it("switches between static and animated when the preference changes", () => {
    const media = installMedia({ reduced: true });
    const frames = installFrames();
    mount();
    media.setReduced(false);
    expect(host.dataset.engine).toBe("canvas2d");
    expect(frames.pending()).toBe(1);
    media.setReduced(true);
    expect(host.dataset.engine).toBe("static");
    expect(frames.pending()).toBe(0);
  });

  it("follows the theme: white dots on dark, black on light, system theme when no explicit choice", () => {
    installMedia();
    installFrames();
    mount();
    expect(fillStyles.at(-1)).toBe("#000000");
    document.documentElement.setAttribute("data-theme", "dark");
    return vi.waitFor(() => expect(fillStyles.at(-1)).toBe("#ffffff"));
  });

  it("uses the system colour scheme when the page has no explicit theme", () => {
    installMedia({ dark: true });
    installFrames();
    mount();
    expect(fillStyles.at(-1)).toBe("#ffffff");
  });

  it("removes its canvases, observers and listeners on destroy, twice safely", () => {
    const media = installMedia();
    const frames = installFrames();
    mount();
    expect(host.querySelectorAll("canvas")).toHaveLength(2);
    expect(media.listenerCount()).toBe(2);
    controller!.destroy();
    controller!.destroy();
    expect(host.querySelectorAll("canvas")).toHaveLength(0);
    expect(media.listenerCount()).toBe(0);
    expect(frames.pending()).toBe(0);
    setHidden(false);
    expect(frames.pending()).toBe(0);
  });
});

describe("WebGPU path", () => {
  it("switches to the GPU canvas after the first frame and pauses it when hidden", async () => {
    installMedia();
    installFrames();
    const ocean = fakeOcean();
    mount({ hasGpu: () => true, loadOcean: async () => ocean.module });
    await vi.waitFor(() => expect(ocean.createRenderer).toHaveBeenCalled());
    expect(host.dataset.engine).toBe("canvas2d");
    ocean.frame();
    await vi.waitFor(() => expect(host.dataset.engine).toBe("webgpu"));
    const [gpu, fallback] = Array.from(host.querySelectorAll("canvas"));
    expect(gpu.hidden).toBe(false);
    expect(fallback.hidden).toBe(true);
    setHidden(true);
    expect(ocean.renderer.setPaused).toHaveBeenLastCalledWith(true);
    setHidden(false);
    expect(ocean.renderer.setPaused).toHaveBeenLastCalledWith(false);
  });

  it("passes theme colours to the renderer", async () => {
    installMedia();
    installFrames();
    const ocean = fakeOcean();
    mount({ hasGpu: () => true, loadOcean: async () => ocean.module });
    await vi.waitFor(() => expect(ocean.createRenderer).toHaveBeenCalled());
    expect(ocean.createRenderer.mock.calls[0][0].colors.fg).toEqual([0, 0, 0]);
    document.documentElement.setAttribute("data-theme", "dark");
    await vi.waitFor(() => expect(ocean.renderer.setColors).toHaveBeenCalledWith(expect.objectContaining({ fg: [1, 1, 1] })));
  });

  it("falls back to Canvas 2D and disposes the renderer when it reports an error", async () => {
    installMedia();
    installFrames();
    const ocean = fakeOcean();
    mount({ hasGpu: () => true, loadOcean: async () => ocean.module });
    await vi.waitFor(() => expect(ocean.createRenderer).toHaveBeenCalled());
    ocean.frame();
    await vi.waitFor(() => expect(host.dataset.engine).toBe("webgpu"));
    ocean.fail(new Error("device lost"));
    expect(host.dataset.engine).toBe("canvas2d");
    expect(ocean.renderer.dispose).toHaveBeenCalledTimes(1);
  });

  it("falls back when the module fails to load", async () => {
    installMedia();
    installFrames();
    mount({ hasGpu: () => true, loadOcean: () => Promise.reject(new Error("chunk missing")) });
    await Promise.resolve();
    expect(host.dataset.engine).toBe("canvas2d");
  });

  it("falls back when no frame arrives in time", async () => {
    installMedia();
    installFrames();
    const ocean = fakeOcean();
    mount({ hasGpu: () => true, loadOcean: async () => ocean.module, gpuTimeoutMs: 20 });
    await vi.waitFor(() => expect(ocean.renderer.dispose).toHaveBeenCalled());
    expect(host.dataset.engine).toBe("canvas2d");
  });

  it("does not attempt the GPU under reduced motion", () => {
    installMedia({ reduced: true });
    installFrames();
    const loadOcean = vi.fn();
    mount({ hasGpu: () => true, loadOcean });
    expect(loadOcean).not.toHaveBeenCalled();
    expect(host.dataset.engine).toBe("static");
  });

  it("forwards pointer position only while the GPU renderer exists, and stops on destroy", async () => {
    installMedia();
    installFrames();
    const ocean = fakeOcean();
    mount({ hasGpu: () => true, loadOcean: async () => ocean.module });
    await vi.waitFor(() => expect(ocean.createRenderer).toHaveBeenCalled());
    document.dispatchEvent(new MouseEvent("pointermove", { clientX: 400, clientY: 150 }));
    expect(ocean.renderer.setPointer).toHaveBeenLastCalledWith([0, 0.5, 1]);
    controller!.destroy();
    const calls = (ocean.renderer.setPointer as ReturnType<typeof vi.fn>).mock.calls.length;
    document.dispatchEvent(new MouseEvent("pointermove", { clientX: 10, clientY: 10 }));
    expect((ocean.renderer.setPointer as ReturnType<typeof vi.fn>).mock.calls.length).toBe(calls);
    expect(ocean.renderer.dispose).toHaveBeenCalled();
  });

  it("discards a renderer that finishes loading after destroy", async () => {
    installMedia();
    installFrames();
    const ocean = fakeOcean();
    let release!: () => void;
    const gate = new Promise<void>((resolve) => (release = resolve));
    mount({
      hasGpu: () => true,
      loadOcean: async () => {
        await gate;
        return ocean.module;
      },
    });
    controller!.destroy();
    release();
    await Promise.resolve();
    await Promise.resolve();
    expect(ocean.createRenderer).not.toHaveBeenCalled();
  });
});

describe("vendored renderer", () => {
  it("loads as an ES module chunk and exposes createRenderer without touching globals", async () => {
    const module = await defaultLoadOcean();
    expect(typeof module.createRenderer).toBe("function");
    expect("AgentNativeOcean" in globalThis).toBe(false);
  });
});

describe("parseBackground", () => {
  it("converts rgb() and color(srgb) to linear light", () => {
    expect(parseBackground("rgb(255, 255, 255)", false)).toEqual([1, 1, 1]);
    expect(parseBackground("color(srgb 1 1 1)", false)).toEqual([1, 1, 1]);
    const [r] = parseBackground("rgb(16, 26, 37)", true);
    expect(r).toBeCloseTo(0.0052, 3);
  });

  it("uses a theme default for anything it cannot parse", () => {
    expect(parseBackground("transparent", false)).toEqual([1, 1, 1]);
    expect(parseBackground("", true)).toEqual([0.005, 0.01, 0.018]);
  });
});
