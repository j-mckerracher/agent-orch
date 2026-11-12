import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";
import { expect, it } from "vitest";

const desktopRoot = fileURLToPath(new URL("..", import.meta.url));
const chromePath = process.env.CHROME_BIN || "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const viewport = { width: 760, height: 1200 };
const probe = `<script>
window.addEventListener("load", () => {
  const capture = () => {
    const summary = document.querySelector(".top-usage-info > summary");
    const details = summary?.closest("details");
    const popup = document.querySelector('[data-testid="top-usage-info"]');
    if (
      !(summary instanceof HTMLElement) ||
      !(details instanceof HTMLDetailsElement) ||
      !(popup instanceof HTMLElement) ||
      !popup.querySelector('[data-testid="top-usage-estimate"]')
    ) {
      requestAnimationFrame(capture);
      return;
    }
    summary.click();
    requestAnimationFrame(() => {
      const rect = popup.getBoundingClientRect();
      const marker = document.createElement("pre");
      marker.id = "topbar-usage-measurement";
      marker.textContent = JSON.stringify({
        open: details.open,
        innerWidth: window.innerWidth,
        innerHeight: window.innerHeight,
        rect: {
          x: rect.x,
          y: rect.y,
          top: rect.top,
          right: rect.right,
          bottom: rect.bottom,
          left: rect.left,
          width: rect.width,
          height: rect.height,
        },
      });
      document.body.append(marker);
    });
  };
  requestAnimationFrame(capture);
}, { once: true });
</script>`;

type Rect = { x: number; y: number; top: number; right: number; bottom: number; left: number; width: number; height: number };
type Measurement = { open: boolean; innerWidth: number; innerHeight: number; rect: Rect };

function isRect(value: unknown): value is Rect {
  return (
    typeof value === "object" &&
    value !== null &&
    "x" in value && typeof value.x === "number" && Number.isFinite(value.x) &&
    "y" in value && typeof value.y === "number" && Number.isFinite(value.y) &&
    "top" in value && typeof value.top === "number" && Number.isFinite(value.top) &&
    "right" in value && typeof value.right === "number" && Number.isFinite(value.right) &&
    "bottom" in value && typeof value.bottom === "number" && Number.isFinite(value.bottom) &&
    "left" in value && typeof value.left === "number" && Number.isFinite(value.left) &&
    "width" in value && typeof value.width === "number" && Number.isFinite(value.width) &&
    "height" in value && typeof value.height === "number" && Number.isFinite(value.height)
  );
}

function isMeasurement(value: unknown): value is Measurement {
  return (
    typeof value === "object" &&
    value !== null &&
    "open" in value &&
    typeof value.open === "boolean" &&
    "innerWidth" in value &&
    typeof value.innerWidth === "number" &&
    Number.isFinite(value.innerWidth) &&
    "innerHeight" in value &&
    typeof value.innerHeight === "number" &&
    Number.isFinite(value.innerHeight) &&
    "rect" in value &&
    isRect(value.rect)
  );
}

function dumpDom(args: string[]): Promise<{ code: number | null; signal: NodeJS.Signals | null; stdout: string; stderr: string }> {
  return new Promise((resolve, reject) => {
    const child = spawn(chromePath, args, { stdio: ["ignore", "pipe", "pipe"], timeout: 20_000 });
    let stdout = "";
    let stderr = "";
    child.stdout?.setEncoding("utf8");
    child.stderr?.setEncoding("utf8");
    child.stdout?.on("data", (chunk) => (stdout += String(chunk)));
    child.stderr?.on("data", (chunk) => (stderr += String(chunk)));
    child.once("error", reject);
    child.once("close", (code, signal) => resolve({ code, signal, stdout, stderr }));
  });
}

it("keeps the TopBar usage popover within 760 CSS-pixel width", async () => {
  const profile = mkdtempSync(join(tmpdir(), "topbar-usage-popover-"));
  let server: Awaited<ReturnType<typeof createServer>> | undefined;
  try {
    const browserVersion = execFileSync(chromePath, ["--version"], { encoding: "utf8" }).trim();
    server = await createServer({
      root: desktopRoot,
      configFile: resolve(desktopRoot, "vite.config.ts"),
      server: { host: "127.0.0.1", port: 0, strictPort: false, open: false, hmr: false },
      plugins: [
        {
          name: "topbar-usage-measurement",
          transformIndexHtml(html) {
            if (!html.includes("</body>")) throw new Error("Vite entry HTML has no body close tag");
            return html.replace("</body>", `${probe}</body>`);
          },
        },
      ],
    });
    await server.listen();
    const address = server.httpServer?.address();
    if (typeof address !== "object" || address === null) throw new Error("Vite did not expose a TCP address");

    const run = await dumpDom([
      "--headless=new",
      "--disable-gpu",
      "--no-first-run",
      "--no-default-browser-check",
      `--user-data-dir=${profile}`,
      `--window-size=${viewport.width},${viewport.height}`,
      "--force-device-scale-factor=1",
      "--virtual-time-budget=5000",
      "--dump-dom",
      `http://127.0.0.1:${address.port}/?fixture=1`,
    ]);
    if (run.code !== 0) throw new Error(`Chrome exited ${run.code} (${run.signal}): ${run.stderr}`);

    const text = /<pre id="topbar-usage-measurement">([\s\S]*?)<\/pre>/.exec(run.stdout)?.[1];
    if (!text) throw new Error(`Chrome did not report popover geometry: ${run.stderr}`);
    const raw: unknown = JSON.parse(text);
    if (!isMeasurement(raw)) throw new Error("Chrome returned invalid usage popover measurements");
    console.info(JSON.stringify({ browser: browserVersion, viewport: { width: raw.innerWidth, height: raw.innerHeight }, bounds: raw.rect }));

    expect(raw.open).toBe(true);
    expect(raw.innerWidth).toBe(viewport.width);
    expect(raw.rect.width).toBeGreaterThan(0);
    expect(raw.rect.height).toBeGreaterThan(0);
    expect(raw.rect.left).toBeGreaterThanOrEqual(0);
    expect(raw.rect.top).toBeGreaterThanOrEqual(0);
    expect(raw.rect.bottom).toBeLessThanOrEqual(raw.innerHeight);
    expect(raw.rect.right).toBeLessThanOrEqual(raw.innerWidth);
  } finally {
    await server?.close();
    rmSync(profile, { recursive: true, force: true });
  }
});
