// @vitest-environment node
/// <reference types="node" />
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const read = (relative: string) => readFileSync(new URL(relative, import.meta.url), "utf8");
const css = read("./background.css");
const tokens = read("../theme/tokens.css");
const tauri = JSON.parse(read("../../../src-tauri/tauri.conf.json"));

function block(source: string, selector: string): string {
  const start = source.indexOf(`${selector} {`);
  if (start < 0) throw new Error(`selector not found: ${selector}`);
  const open = source.indexOf("{", start);
  return source.slice(open + 1, source.indexOf("}", open));
}

const token = (body: string, name: string) => new RegExp(`${name}:\\s*(#[0-9a-fA-F]{6})`).exec(body)?.[1];
const opacityOf = (body: string) => Number(/--ambient-opacity:\s*([\d.]+)/.exec(body)?.[1]);

function channel(value: number): number {
  const c = value / 255;
  return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
}
type Rgb = [number, number, number];
const rgb = (hex: string): Rgb => {
  const n = parseInt(hex.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
};
const luminance = ([r, g, b]: Rgb) => 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
const ratio = (a: Rgb, b: Rgb) => {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
};
const mix = (fg: Rgb, bg: Rgb, alpha: number): Rgb => fg.map((c, i) => c * alpha + bg[i] * (1 - alpha)) as Rgb;

const lightTokens = block(tokens, ":root");
const darkTokens = lightTokens + block(tokens, ':root[data-theme="dark"]');
const TEXT = ["ink", "muted", "subtle", "blue", "green", "amber", "red", "success", "component-muted", "control-ink"];

describe("ambient layer opacity keeps text readable", () => {
  const reducedCss = css.slice(css.indexOf("@media (prefers-reduced-motion: reduce) {"));
  const themes = [
    { name: "light", opacity: opacityOf(block(css, ".ambient-background")), fg: [0, 0, 0] as Rgb, tokens: lightTokens },
    { name: "light, reduced motion", opacity: opacityOf(block(reducedCss, ".ambient-background")), fg: [0, 0, 0] as Rgb, tokens: lightTokens },
    {
      name: "dark",
      opacity: opacityOf(block(css, ':root[data-theme="dark"] .ambient-background')),
      fg: [255, 255, 255] as Rgb,
      tokens: darkTokens,
    },
    {
      name: "dark, reduced motion",
      opacity: opacityOf(block(reducedCss, ':root[data-theme="dark"] .ambient-background')),
      fg: [255, 255, 255] as Rgb,
      tokens: darkTokens,
    },
  ];

  for (const theme of themes) {
    it(`${theme.name}: every text token is at least 4.5:1 on a fully covered particle pixel`, () => {
      expect(theme.opacity).toBeGreaterThan(0);
      // The layer's opacity multiplies GPU and Canvas pixels alike, so a pixel the particles cover completely is the worst case.
      const paperHex = [...theme.tokens.matchAll(/--paper:\s*(#[0-9a-fA-F]{6})/g)].at(-1)![1];
      const worst = mix(theme.fg, rgb(paperHex), theme.opacity);
      for (const name of TEXT) {
        const hex = [...theme.tokens.matchAll(new RegExp(`--${name}:\\s*(#[0-9a-fA-F]{6})`, "g"))].at(-1)?.[1];
        expect(hex, name).toBeDefined();
        expect(ratio(rgb(hex!), worst), `${theme.name} --${name}`).toBeGreaterThanOrEqual(4.5);
      }
    });
  }

  it("system dark scheme uses the same opacities as explicit dark", () => {
    const system = css.slice(css.indexOf("@media (prefers-color-scheme: dark)"));
    expect(opacityOf(block(system, ':root:not([data-theme="light"]) .ambient-background'))).toBe(
      opacityOf(block(css, ':root[data-theme="dark"] .ambient-background')),
    );
  });

  it("reads token values (sanity)", () => {
    expect(token(lightTokens, "--paper")).toBe("#ffffff");
  });
});

describe("layer placement and policy", () => {
  const layer = block(css, ".ambient-background");

  it("is fixed, inert, below the UI and hidden in print", () => {
    expect(layer).toMatch(/position:\s*fixed/);
    expect(layer).toMatch(/pointer-events:\s*none/);
    expect(layer).toMatch(/z-index:\s*-1/);
    expect(css).toMatch(/@media print\s*{\s*\.ambient-background\s*{\s*display:\s*none/);
    expect(block(css, ".ambient-background canvas")).toMatch(/pointer-events:\s*none/);
  });

  it("only changes the shell while the layer is on", () => {
    expect(css).toContain(':root[data-ambient-background="on"] .shell');
  });

  it("keeps the Content Security Policy strict: no unsafe-eval, scripts only from self", () => {
    const csp: string = tauri.app.security.csp;
    expect(csp).toMatch(/script-src 'self';/);
    expect(csp).not.toContain("unsafe-eval");
    expect(csp).not.toMatch(/script-src[^;]*(unsafe-inline|https?:|data:|blob:)/);
  });

  it("vendored renderer needs no eval, Function constructor, network or inline-script construct", () => {
    const source = read("./vendor/ocean-renderer.js");
    expect(source).not.toMatch(/\beval\s*\(/);
    expect(source).not.toMatch(/\bnew\s+Function\s*\(/);
    expect(source).not.toMatch(/\bFunction\s*\(\s*["'`]/);
    expect(source).not.toMatch(/\b(fetch|XMLHttpRequest|WebSocket|importScripts)\b\s*\(/);
    expect(source).not.toMatch(/\bnew\s+(Shared)?Worker\s*\(/);
    expect(source).not.toMatch(/createObjectURL/);
  });

  it("ships licence and attribution files", () => {
    expect(read("./vendor/LICENSE.agent-native")).toContain("Builder.io");
    expect(read("./vendor/LICENSE.vgpu")).toContain("Vercel");
    expect(read("./THIRD-PARTY-NOTICES.md")).toContain("41fae733f80038c9258d797279ef87620c6f12a7");
    expect(read("./vendor/ocean-renderer.js").slice(0, 400)).toContain("Builder.io");
  });
});
