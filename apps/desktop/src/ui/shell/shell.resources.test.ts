import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const shellCss = readFileSync("src/ui/shell/shell.css", "utf8");

/** Every rule block (outside or inside an @media query) whose selector list names `.shell-resources`. */
function resourceRuleBodies(css: string): string[] {
  const bodies: string[] = [];
  for (const match of css.matchAll(/([^{}]*)\{([^{}]*)\}/g)) {
    if (/\.shell-resources\b/.test(match[1])) bodies.push(match[2]);
  }
  return bodies;
}

describe("shell.css resources region", () => {
  const bodies = resourceRuleBodies(shellCss);

  it("finds the resources rules", () => {
    expect(bodies.length).toBeGreaterThanOrEqual(3);
  });

  it("never pins the strip over content at any width, so an open strip cannot cover the screen", () => {
    for (const body of bodies) {
      expect(body).not.toMatch(/position\s*:\s*(sticky|fixed|absolute)/);
    }
    expect(shellCss).not.toMatch(/position\s*:\s*sticky/);
  });

  it("keeps the strip as a normal-flow grid area", () => {
    expect(bodies.some((body) => /grid-area\s*:\s*res\b/.test(body))).toBe(true);
  });
});
