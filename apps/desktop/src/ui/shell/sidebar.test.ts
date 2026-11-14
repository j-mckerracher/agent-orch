// @vitest-environment node
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const shell = readFileSync(new URL("./shell.css", import.meta.url), "utf8");
const background = readFileSync(new URL("../background/background.css", import.meta.url), "utf8");

/** Body of the first rule whose selector is exactly `selector` (these rules contain no nested braces). */
function block(source: string, selector: string): string {
  const start = source.indexOf(`${selector} {`);
  if (start < 0) throw new Error(`selector not found: ${selector}`);
  const open = source.indexOf("{", start);
  return source.slice(open + 1, source.indexOf("}", open));
}

describe("sidebar surface follows the theme", () => {
  const sidebar = block(shell, ".shell-sidebar");

  it("is a translucent tint of the theme surface, not a fixed colour", () => {
    expect(sidebar).toMatch(/background:\s*color-mix\(in srgb,\s*var\(--soft\)\s*45%,\s*transparent\)/);
    expect(sidebar).not.toMatch(/#[0-9a-fA-F]{3,6}\b/);
    expect(sidebar).not.toMatch(/var\(--sidebar/);
  });

  it("takes its text colour from the theme, not a sidebar-only palette", () => {
    expect(sidebar).toMatch(/color:\s*var\(--ink\)/);
  });

  it("does not remap theme tokens to a light sidebar palette", () => {
    expect(shell).not.toMatch(/--sidebar[\w-]*/);
  });

  it("keeps the background layer visible through the shell only while it is on", () => {
    expect(background).toMatch(/:root\[data-ambient-background="on"\] \.shell\s*{\s*background:\s*transparent;/);
  });
});
