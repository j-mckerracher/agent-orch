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

  it("pins the strip to the bottom edge only in the narrow, scrolling layout, with a height cap", () => {
    const sticky = bodies.filter((body) => /position\s*:\s*sticky/.test(body));
    expect(sticky).toHaveLength(1);
    expect(sticky[0]).toMatch(/bottom\s*:\s*0/);
    expect(sticky[0]).toMatch(/max-height\s*:\s*15dvh/);
    expect(shellCss.indexOf(sticky[0])).toBeGreaterThan(shellCss.indexOf("@media (max-width: 960px)"));
    expect(shellCss.indexOf(sticky[0])).toBeLessThan(shellCss.indexOf("@media (max-width: 760px)"));
    for (const body of bodies.filter((entry) => entry !== sticky[0])) {
      expect(body).not.toMatch(/position\s*:\s*(sticky|fixed|absolute)/);
    }
  });

  it("keeps the strip a grid area of the content column", () => {
    expect(bodies.some((body) => /grid-area\s*:\s*res\b/.test(body))).toBe(true);
  });
});
