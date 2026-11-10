/**
 * Ambient background engine (M7-18, decision D-9). Decorative only: it reads no
 * application state, makes no requests and never suggests live activity.
 *
 * Adapted from the workspace integration in `docs/operator-experience/animation/particle-wave.js`
 * (see THIRD-PARTY-NOTICES.md). Behaviour:
 * - `webgpu`: the vendored Agent-Native FFT ocean renderer, loaded with a dynamic `import()`
 *   (a same-origin chunk, so CSP `script-src 'self'` holds with no `unsafe-eval`). It is
 *   attempted only when `navigator.gpu` exists and falls back on any error, device loss, or
 *   a first frame that does not arrive within `gpuTimeoutMs`. WebGPU support in WKWebView
 *   and WebView2 is unverified, so nothing depends on it.
 * - `canvas2d`: a cheap point-wave approximation, capped at `fps` frames per second.
 * - `static`: one still dot field while `prefers-reduced-motion: reduce` matches.
 * - Drawing and GPU work pause while the document is hidden.
 *
 * The host element reports the active engine in `data-engine`. `destroy()` removes every
 * listener, observer, frame request and canvas it created; it is safe to call twice.
 */
import type { OceanModule, OceanRenderer } from "./vendor/ocean-renderer";

export type AmbientEngine = "webgpu" | "canvas2d" | "static";

export interface AmbientStats {
  engine: AmbientEngine;
  /** Canvas 2D frames drawn so far. This is not a GPU measurement. */
  frames: number;
  paused: boolean;
}

export interface AmbientController {
  readonly stats: AmbientStats;
  refreshColors(): void;
  destroy(): void;
}

export interface AmbientOptions {
  fps?: number;
  /** Loads the GPU renderer; replaced in tests. Not called when WebGPU is unavailable. */
  loadOcean?: () => Promise<OceanModule>;
  hasGpu?: () => boolean;
  gpuTimeoutMs?: number;
}

export const defaultLoadOcean = (): Promise<OceanModule> =>
  import("./vendor/ocean-renderer.js").then((module) => module.AgentNativeOcean);

const defaultHasGpu = () => typeof navigator !== "undefined" && "gpu" in navigator && Boolean(navigator.gpu);

type Rgb = [number, number, number];

const srgbToLinear = (value: number) => (value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4);

/** Parses `rgb()/rgba()` (0-255) or `color(srgb …)` (0-1) computed values into linear-light RGB. */
export function parseBackground(value: string, dark: boolean): Rgb {
  const numbers = value.match(/[\d.]+/g)?.map(Number);
  if (numbers && numbers.length >= 3 && numbers.slice(0, 3).every(Number.isFinite)) {
    const scale = value.trim().startsWith("color(") ? 1 : 255;
    return numbers.slice(0, 3).map((channel) => srgbToLinear(channel / scale)) as Rgb;
  }
  return dark ? [0.005, 0.01, 0.018] : [1, 1, 1];
}

interface Media {
  matches: boolean;
  addEventListener(type: "change", listener: () => void): void;
  removeEventListener(type: "change", listener: () => void): void;
}

const media = (query: string): Media => {
  const list = typeof matchMedia === "function" ? matchMedia(query) : null;
  return list ?? { matches: false, addEventListener() {}, removeEventListener() {} };
};

function withTimeout<T>(promise: Promise<T>, ms: number): Promise<T> {
  let timer: ReturnType<typeof setTimeout>;
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new Error("WebGPU renderer did not produce a frame in time")), ms);
  });
  return Promise.race([promise, timeout]).finally(() => clearTimeout(timer));
}

export function mountAmbientBackground(host: HTMLElement, options: AmbientOptions = {}): AmbientController {
  const { fps = 24, loadOcean = defaultLoadOcean, hasGpu = defaultHasGpu, gpuTimeoutMs = 6000 } = options;
  const gpuCanvas = document.createElement("canvas");
  const fallbackCanvas = document.createElement("canvas");
  gpuCanvas.setAttribute("aria-hidden", "true");
  fallbackCanvas.setAttribute("aria-hidden", "true");
  gpuCanvas.hidden = true;
  host.append(gpuCanvas, fallbackCanvas);

  const context = fallbackCanvas.getContext("2d", { alpha: true });
  const motion = media("(prefers-reduced-motion: reduce)");
  const scheme = media("(prefers-color-scheme: dark)");
  const root = document.documentElement;
  const stats: AmbientStats = { engine: "canvas2d", frames: 0, paused: false };
  let destroyed = false;
  let renderer: OceanRenderer | null = null;
  let gpuAttempted = false;
  let gpuReady = false;
  let frameId = 0;
  let lastFrame = 0;
  let width = 1;
  let height = 1;
  let points: Array<[number, number, number]> = [];
  let pointer: [number, number, number] = [0, 0, 0];
  let unbindPointer: (() => void) | null = null;
  const startTime = performance.now();

  const isDark = () => {
    const explicit = root.getAttribute("data-theme");
    return explicit ? explicit === "dark" : scheme.matches;
  };

  function colors() {
    const dark = isDark();
    return { fg: (dark ? [1, 1, 1] : [0, 0, 0]) as Rgb, bg: parseBackground(getComputedStyle(document.body).backgroundColor, dark) };
  }

  function setEngine(value: AmbientEngine) {
    stats.engine = value;
    host.dataset.engine = value;
  }

  function resize() {
    const bounds = host.getBoundingClientRect();
    width = Math.max(1, bounds.width);
    height = Math.max(1, bounds.height);
    const ratio = Math.min(window.devicePixelRatio || 1, 1.5);
    fallbackCanvas.width = Math.round(width * ratio);
    fallbackCanvas.height = Math.round(height * ratio);
    context?.setTransform(ratio, 0, 0, ratio, 0, 0);
    const columns = Math.min(148, Math.max(88, Math.round(width / 10)));
    const rows = width < 600 ? 44 : 64;
    points = [];
    for (let row = 0; row < rows; row++) {
      for (let column = 0; column < columns; column++) {
        // Fixed jitter avoids the rigid look of a regular grid.
        const seed = Math.sin(column * 127.1 + row * 311.7) * 43758.5453;
        const jitter = seed - Math.floor(seed);
        points.push([
          ((column + jitter * 0.65) / (columns - 1) - 0.5) * 440,
          -125 + ((row + jitter * 0.55) / (rows - 1)) * 179,
          jitter,
        ]);
      }
    }
    if (!gpuReady) drawFallback(motion.matches ? 0 : (performance.now() - startTime) / 1000);
  }

  function drawFallback(time: number) {
    if (!context) return;
    context.clearRect(0, 0, width, height);
    context.fillStyle = isDark() ? "#ffffff" : "#000000";
    const focal = height * 0.8;
    const t = time * 0.45;
    for (const [x, z, jitter] of points) {
      const wave =
        Math.sin(x * 0.035 + z * 0.044 + t) * 3.8 +
        Math.sin(x * 0.071 - z * 0.048 - t * 1.13) * 2.0 +
        Math.cos(x * 0.112 + z * 0.081 + t * 0.74) * 0.9;
      const relativeY = wave - 14;
      const relativeZ = z - 78;
      const depth = -relativeY * 0.101 - relativeZ * 0.995;
      if (depth < 8) continue;
      const screenX = width / 2 + (x / depth) * focal;
      const screenY = height * 0.47 - ((relativeY * 0.995 - relativeZ * 0.101) / depth) * focal;
      if (screenX < 0 || screenX > width || screenY < 0 || screenY > height) continue;
      const fade = Math.max(0, Math.min(1, (220 - depth) / 170));
      context.globalAlpha = (0.3 + jitter * 0.55) * fade;
      const size = Math.min(1.7, Math.max(0.65, 75 / depth));
      context.fillRect(screenX, screenY, size, size);
    }
    context.globalAlpha = 1;
    stats.frames++;
  }

  function tick(now: number) {
    if (destroyed || document.hidden || motion.matches || gpuReady) return;
    if (now - lastFrame >= 1000 / fps) {
      drawFallback((now - startTime) / 1000);
      lastFrame = now;
    }
    frameId = requestAnimationFrame(tick);
  }

  function reconcile() {
    cancelAnimationFrame(frameId);
    frameId = 0;
    if (destroyed) return;
    const paused = document.hidden || motion.matches;
    stats.paused = paused;
    renderer?.setPaused(paused);
    if (motion.matches) {
      gpuCanvas.hidden = true;
      fallbackCanvas.hidden = false;
      setEngine("static");
      drawFallback(0);
    } else if (gpuReady) {
      gpuCanvas.hidden = false;
      fallbackCanvas.hidden = true;
      setEngine("webgpu");
    } else {
      // While the renderer initialises its canvas must stay laid out so it can size itself.
      gpuCanvas.hidden = !renderer;
      fallbackCanvas.hidden = false;
      setEngine("canvas2d");
      if (!document.hidden && context) frameId = requestAnimationFrame(tick);
    }
  }

  function movePointer(event: PointerEvent) {
    const bounds = host.getBoundingClientRect();
    pointer = [((event.clientX - bounds.left) / width) * 2 - 1, 1 - ((event.clientY - bounds.top) / height) * 2, 1];
    renderer?.setPointer(pointer);
  }

  function releasePointer() {
    pointer = [pointer[0], pointer[1], 0];
    renderer?.setPointer(pointer);
  }

  function bindPointer() {
    document.addEventListener("pointermove", movePointer, { passive: true });
    root.addEventListener("pointerleave", releasePointer, { passive: true });
    window.addEventListener("blur", releasePointer);
    unbindPointer = () => {
      document.removeEventListener("pointermove", movePointer);
      root.removeEventListener("pointerleave", releasePointer);
      window.removeEventListener("blur", releasePointer);
      unbindPointer = null;
    };
  }

  function useFallback() {
    gpuReady = false;
    unbindPointer?.();
    try {
      renderer?.dispose();
    } catch {
      // The fallback stays usable whatever the GPU renderer does on the way out.
    }
    renderer = null;
    reconcile();
  }

  async function startOcean() {
    if (gpuAttempted || destroyed || motion.matches) return;
    gpuAttempted = true;
    if (!hasGpu()) return;
    try {
      const ocean = await loadOcean();
      if (destroyed) return;
      gpuCanvas.hidden = false;
      const created = ocean.createRenderer({ canvas: gpuCanvas, colors: colors(), fps, onError: useFallback });
      renderer = created;
      bindPointer();
      created.setPointer(pointer);
      created.setPaused(document.hidden || motion.matches);
      await withTimeout(created.ready.then(() => created.firstFrame), gpuTimeoutMs);
      if (destroyed || renderer !== created) return;
      gpuReady = true;
      reconcile();
    } catch {
      if (!destroyed) useFallback();
    }
  }

  function updateTheme() {
    renderer?.setColors(colors());
    if (!gpuReady || motion.matches) drawFallback(motion.matches ? 0 : (performance.now() - startTime) / 1000);
  }

  function motionChanged() {
    reconcile();
    if (!motion.matches) void startOcean();
  }

  const observer = new MutationObserver(updateTheme);
  observer.observe(root, { attributes: true, attributeFilter: ["data-theme", "class"] });
  const resizeObserver = typeof ResizeObserver !== "undefined" ? new ResizeObserver(resize) : null;
  resizeObserver?.observe(host);
  document.addEventListener("visibilitychange", reconcile);
  motion.addEventListener("change", motionChanged);
  scheme.addEventListener("change", updateTheme);

  const controller: AmbientController = {
    stats,
    refreshColors: updateTheme,
    destroy() {
      if (destroyed) return;
      destroyed = true;
      cancelAnimationFrame(frameId);
      observer.disconnect();
      resizeObserver?.disconnect();
      document.removeEventListener("visibilitychange", reconcile);
      motion.removeEventListener("change", motionChanged);
      scheme.removeEventListener("change", updateTheme);
      unbindPointer?.();
      try {
        renderer?.dispose();
      } finally {
        renderer = null;
        gpuCanvas.remove();
        fallbackCanvas.remove();
      }
    },
  };
  resize();
  reconcile();
  void startOcean();
  return controller;
}
