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

  it("reserves scroll padding under the sticky strip so focused controls are never obscured (WCAG 2.4.11)", () => {
    const stacked = shellCss.indexOf("@media (max-width: 960px)");
    const narrow = shellCss.indexOf("@media (max-width: 760px)");
    const stackedBlock = shellCss.slice(stacked, narrow);
    const narrowBlock = shellCss.slice(narrow);
    expect(stackedBlock).toMatch(/\.shell-content\.has-activity\s*\{[^}]*scroll-padding-bottom\s*:\s*15dvh/);
    expect(narrowBlock).toMatch(/\.shell-body\s*\{[^}]*scroll-padding-bottom\s*:\s*15dvh/);
  });

  it("draws a visible focus ring on the focusable scroll container", () => {
    expect(shellCss).toMatch(/\.shell-main:focus-visible\s*\{[^}]*outline\s*:\s*3px/);
  });

  it("lets the stacked main row size to its content so the activity panel cannot cover the composer", () => {
    const stacked = shellCss.slice(shellCss.indexOf("@media (max-width: 960px)"), shellCss.indexOf("@media (max-width: 760px)"));
    expect(stacked).toMatch(/\.shell-main\s*\{[^}]*min-height\s*:\s*auto/);
    expect(stacked).toMatch(/\.shell-activity\s*\{[^}]*min-height\s*:\s*auto/);
  });

  it("makes every scroll container the containing block of absolutely positioned descendants so the document never scrolls", () => {
    for (const selector of [".shell-body", ".shell-content", ".shell-main", ".shell-activity"]) {
      const rule = new RegExp(`^${selector.replace(".", "\\.")}\\s*\\{([^}]*)\\}`, "m").exec(shellCss);
      expect(rule, selector).not.toBeNull();
      expect(rule![1], selector).toMatch(/position\s*:\s*relative/);
    }
  });
});
