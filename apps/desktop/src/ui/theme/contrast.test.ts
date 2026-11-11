// @vitest-environment node
/// <reference types="node" />
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const css = readFileSync(new URL("./tokens.css", import.meta.url), "utf8");

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

function channel(value: number): number {
  const c = value / 255;
  return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
}

function luminance(hex: string): number {
  const match = /^#([0-9a-f]{6})$/i.exec(hex);
  if (!match) throw new Error(`expected a 6-digit hex colour, got ${hex}`);
  const n = parseInt(match[1], 16);
  return 0.2126 * channel((n >> 16) & 255) + 0.7152 * channel((n >> 8) & 255) + 0.0722 * channel(n & 255);
}

function ratio(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

const SURFACES = ["paper", "panel", "soft", "soft-strong", "sidebar"];
const TINTS: Record<string, string> = { blue: "blue-bg", green: "green-bg", amber: "amber-bg", red: "red-bg" };

/** [foreground, background, minimum ratio, why] for every colour pairing the primitives use. */
function pairs(): Array<[string, string, number, string]> {
  const out: Array<[string, string, number, string]> = [];
  for (const surface of SURFACES) {
    for (const text of ["ink", "muted", "subtle", "blue", "green", "amber", "red"]) {
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
  out.push(["on-accent", "accent", 4.5, "primary button label"]);
  out.push(["accent", "paper", 3, "primary button boundary"]);
  out.push(["accent", "panel", 3, "primary button boundary"]);
  out.push(["accent", "soft", 3, "primary button boundary"]);
  out.push(["meter-marker", "meter-marker-ring", 3, "meter marker vs ring"]);
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
      "paper", "panel", "soft", "soft-strong", "sidebar", "ink", "muted", "subtle", "on-accent", "line", "line-soft",
      "control-border", "blue", "blue-bg", "accent", "focus", "green", "green-bg", "amber", "amber-bg", "red", "red-bg",
      "meter-gradient", "meter-stop-1", "meter-stop-2", "meter-stop-3", "meter-stop-4", "meter-stop-5", "meter-track",
      "meter-marker", "meter-marker-ring", "font-sans", "font-display", "font-mono", "radius-sm", "radius-md",
      "radius-lg", "shadow-popover", "focus-width",
    ];
    for (const [name, tokens] of Object.entries(themes)) {
      for (const token of required) expect(tokens[`--${token}`], `${name} --${token}`).toBeTruthy();
    }
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
