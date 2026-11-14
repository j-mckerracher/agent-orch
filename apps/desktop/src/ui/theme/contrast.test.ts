// @vitest-environment node
/// <reference types="node" />
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const css = readFileSync(new URL("./tokens.css", import.meta.url), "utf8");
const primitives = readFileSync(new URL("./primitives.css", import.meta.url), "utf8");

type Tokens = Record<string, string>;

function declarations(block: string): Tokens {
  const out: Tokens = {};
  for (const match of block.matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)) out[match[1]] = match[2].trim();
  return out;
}

/** Body of the first rule whose selector is exactly `selector` (the rules here contain no nested braces). */
function block(source: string, selector: string): string {
  const start = source.indexOf(`${selector} {`);
  if (start < 0) throw new Error(`selector not found: ${selector}`);
  const open = source.indexOf("{", start);
  return source.slice(open + 1, source.indexOf("}", open));
}

const light = declarations(block(css, ":root"));
const darkOverrides = declarations(block(css, ':root[data-theme="dark"]'));
const mediaStart = css.indexOf("@media (prefers-color-scheme: dark)");
const systemDarkOverrides = declarations(block(css.slice(mediaStart), ':root:not([data-theme="light"])'));

const themes: Record<string, Tokens> = {
  light,
  dark: { ...light, ...darkOverrides },
};

type Rgb = [number, number, number];

function channel(value: number): number {
  const c = value / 255;
  return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
}

function channels(hex: string): Rgb {
  const match = /^#([0-9a-f]{6})$/i.exec(hex);
  if (!match) throw new Error(`expected a 6-digit hex colour, got ${hex}`);
  const n = parseInt(match[1], 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

function luminanceRgb([r, g, b]: Rgb): number {
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}

function ratio(a: string | Rgb, b: string | Rgb): number {
  const foreground = typeof a === "string" ? channels(a) : a;
  const background = typeof b === "string" ? channels(b) : b;
  const [hi, lo] = [luminanceRgb(foreground), luminanceRgb(background)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

function blend(foreground: Rgb, background: Rgb, opacity: number): Rgb {
  return foreground.map((value, index) => value * opacity + background[index] * (1 - opacity)) as Rgb;
}

const SURFACES = ["paper", "panel", "soft", "soft-strong", "control-surface"];
const TEXT = ["ink", "muted", "subtle", "blue", "green", "amber", "red", "success", "component-muted", "control-ink"];
const TINTS: Record<string, string> = {
  blue: "blue-bg",
  green: "green-bg",
  amber: "amber-bg",
  red: "red-bg",
  success: "success-bg",
};

/** [foreground, background, minimum ratio, why] for every colour pairing the primitives use. */
function pairs(): Array<[string, string, number, string]> {
  const out: Array<[string, string, number, string]> = [];
  for (const surface of SURFACES) {
    for (const text of TEXT) {
      out.push([text, surface, 4.5, "text"]);
    }
    for (const nonText of ["control-border", "focus"]) out.push([nonText, surface, 3, "non-text"]);
  }
  for (const [text, tint] of Object.entries(TINTS)) {
    out.push([text, tint, 4.5, "state text on tint"]);
    out.push(["ink", tint, 4.5, "ink on tint"]);
    out.push(["subtle", tint, 4.5, "secondary on tint"]);
    out.push(["focus", tint, 3, "focus ring on tint"]);
    out.push(["control-border", tint, 3, "control boundary on tint"]);
  }
  out.push(["sidebar-ink", "sidebar", 4.5, "sidebar text"]);
  out.push(["sidebar-link", "sidebar", 4.5, "sidebar link"]);
  out.push(["sidebar-focus", "sidebar", 3, "sidebar focus ring"]);
  out.push(["sidebar-control-border", "sidebar", 3, "sidebar control boundary"]);
  out.push(["on-accent", "accent", 4.5, "primary button label"]);
  for (const surface of SURFACES) out.push(["accent", surface, 3, "primary button boundary"]);
  out.push(["meter-marker", "meter-marker-ring", 3, "meter marker vs ring"]);
  out.push(["meter-marker", "meter-track", 3, "meter marker on unfilled track"]);
  for (let i = 1; i <= 5; i++) out.push(["meter-fill-pattern", `meter-stop-${i}`, 3, "meter fill pattern"]);
  return out;
}

describe("design tokens", () => {
  it("computes WCAG ratios correctly", () => {
    expect(ratio("#000000", "#ffffff")).toBeCloseTo(21, 5);
    expect(ratio("#777777", "#ffffff")).toBeCloseTo(4.48, 2);
    expect(ratio("#767676", "#ffffff")).toBeGreaterThanOrEqual(4.5);
  });
  it("defines every token the header contract names, in both themes", () => {
    const required = [
      "paper", "panel", "soft", "soft-strong", "sidebar", "sidebar-ink", "sidebar-link", "sidebar-focus",
      "sidebar-control-border", "control-surface", "control-ink", "component-muted", "on-accent", "ink", "muted",
      "subtle", "line", "line-soft", "control-border", "blue", "blue-bg", "accent", "focus", "success",
      "success-bg", "green", "green-bg", "amber", "amber-bg", "red", "red-bg", "wireframe-search-border",
      "wireframe-composer-border", "wireframe-picker-border", "meter-gradient", "meter-stop-1", "meter-stop-2",
      "meter-stop-3", "meter-stop-4", "meter-stop-5", "meter-track", "meter-marker", "meter-marker-ring",
      "meter-fill-pattern",
      "font-display", "font-mono", "radius-sm", "radius-md", "radius-lg", "shadow-popover",
      "focus-width",
    ];
    for (const [name, tokens] of Object.entries(themes)) {
      for (const token of required) expect(tokens[`--${token}`], `${name} --${token}`).toBeTruthy();
    }
  });

  it("keeps the meter pattern above its gradient and masks from its current level", () => {
    const track = block(primitives, ".ui-meter-track");
    const background = track.match(/background:\s*([\s\S]*?);/)?.[1]
      .replace(/\s+/g, " ")
      .trim();
    const fillLayers =
      /^repeating-linear-gradient\(.+var\(--meter-fill-pattern\).+\),\s*var\(--meter-gradient\)$/;
    expect(background).toMatch(fillLayers);

    const overlay = block(primitives, ".ui-meter-track::after");
    const inset = overlay.match(/inset:\s*([^;]+);/)?.[1].replace(/\s+/g, " ").trim();
    expect(inset).toMatch(/^0 0 0 var\(--level(?:,\s*0%)?\)$/);

    const reversed = primitives.replace(
      /background:\s*repeating-linear-gradient\([\s\S]*?\),\s*var\(--meter-gradient\);/,
      `background: var(--meter-gradient),
        repeating-linear-gradient(
          135deg,
          var(--meter-fill-pattern) 0 2px,
          transparent 2px 6px
        );`,
    );
    expect(reversed).not.toBe(primitives);
    const reversedBackground = block(reversed, ".ui-meter-track")
      .match(/background:\s*([\s\S]*?);/)?.[1]
      .replace(/\s+/g, " ")
      .trim();
    expect(reversedBackground).not.toMatch(fillLayers);

    const detached = primitives.replace("inset: 0 0 0 var(--level, 0%);", "inset: 0;");
    expect(detached).not.toBe(primitives);
    const detachedInset = block(detached, ".ui-meter-track::after")
      .match(/inset:\s*([^;]+);/)?.[1]
      .replace(/\s+/g, " ")
      .trim();
    expect(detachedInset).not.toMatch(/^0 0 0 var\(--level(?:,\s*0%)?\)$/);
  });

  it("matches the wireframe palette anchors", () => {
    expect(light["--blue"]).toBe("#0057a8");
    expect(themes.dark["--paper"]).toBe("#101a25");
    expect(themes.dark["--panel"]).toBe("#172433");
    expect(light["--font-sans"]).toMatch(/Avenir Next/);
    expect(light["--font-sans"]).toMatch(/system-ui/);
    expect(light["--font-sans"]).toMatch(/Segoe UI/);
    expect(css).not.toMatch(/@font-face/);
  });
  it("matches effective wireframe roles and keeps accessibility overrides explicit", () => {
    const expected: Record<string, Tokens> = {
      light: {
        success: "#0057a8",
        "success-bg": "#edf5fd",
        green: "#2f7052",
        "green-bg": "#e8f1e8",
        amber: "#765000",
        "amber-bg": "#fff5db",
        red: "#a32d34",
        "red-bg": "#fff0f0",
        "soft-strong": "#e4effb",
        "component-muted": "#333333",
        "control-ink": "#183c60",
        "control-surface": "#f2f7fc",
        "control-border": "#6b7f94",
        sidebar: "#f2f7fc",
        "sidebar-ink": "#111111",
        "sidebar-link": "#0057a8",
        "sidebar-focus": "#0057a8",
        "sidebar-control-border": "#6b7f94",
        "wireframe-search-border": "#bdcde0",
        "wireframe-composer-border": "#b7cbe0",
        "wireframe-picker-border": "#d4e0ed",
      },
      dark: {
        success: "#a0ceff",
        "success-bg": "#223f5b",
        green: "#a3d6b3",
        "green-bg": "#264638",
        amber: "#ffdb95",
        "amber-bg": "#4a3a20",
        red: "#ffbdc2",
        "red-bg": "#503137",
        "soft-strong": "#304a65",
        "component-muted": "#c2d2e3",
        "control-ink": "#f2f6fb",
        "control-surface": "#20364a",
        "control-border": "#829ab0",
        sidebar: "#f2f7fc",
        "sidebar-ink": "#111111",
        "sidebar-link": "#0057a8",
        "sidebar-focus": "#0057a8",
        "sidebar-control-border": "#6b7f94",
        "wireframe-search-border": "#40576c",
        "wireframe-composer-border": "#40576c",
        "wireframe-picker-border": "#40576c",
      },
    };
    for (const [themeName, roles] of Object.entries(expected)) {
      for (const [name, value] of Object.entries(roles)) {
        expect(themes[themeName][`--${name}`], `${themeName} --${name}`).toBe(value);
      }
    }
    expect(light["--success"]).not.toBe(light["--green"]);
    expect(themes.dark["--success"]).not.toBe(themes.dark["--green"]);
  });
  it("keeps review diff text readable on translucent state fills", () => {
    for (const [themeName, tokens] of Object.entries(themes)) {
      for (const tint of ["success-bg", "red-bg"]) {
        const background = blend(channels(tokens[`--${tint}`]), channels(tokens["--soft"]), 0.45);
        const value = ratio(channels(tokens["--ink"]), background);
        expect(value, `${themeName} --ink on 45% --${tint} over --soft = ${value.toFixed(2)}`).toBeGreaterThanOrEqual(4.5);
      }
    }
  });

  it("keeps the explicit dark set and the system dark set identical", () => {
    expect(systemDarkOverrides).toEqual(darkOverrides);
  });

  for (const [themeName, tokens] of Object.entries(themes)) {
    describe(`${themeName} theme contrast`, () => {
      for (const [fg, bg, min, why] of pairs()) {
        it(`${why}: --${fg} on --${bg} is at least ${min}:1`, () => {
          const value = ratio(tokens[`--${fg}`], tokens[`--${bg}`]);
          expect(value, `${tokens[`--${fg}`]} on ${tokens[`--${bg}`]} = ${value.toFixed(2)}`).toBeGreaterThanOrEqual(min);
        });
      }

      it("meter marker or its ring separates from every gradient stop at 3:1", () => {
        for (let i = 1; i <= 5; i++) {
          const stop = tokens[`--meter-stop-${i}`];
          const best = Math.max(ratio(tokens["--meter-marker"], stop), ratio(tokens["--meter-marker-ring"], stop));
          expect(best, `stop ${i} ${stop}`).toBeGreaterThanOrEqual(3);
        }
      });
    });
  }
});
