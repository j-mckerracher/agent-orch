import { existsSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
import type { Page } from "@playwright/test";

export const APP = `http://127.0.0.1:${process.env.AUDIT_PORT}`;
export const WIREFRAME =
  process.env.WIREFRAME ?? resolve(HERE, "../../../docs/operator-experience/wireframes.html");
if (!existsSync(WIREFRAME)) throw new Error(`wireframe not found: ${WIREFRAME} (set WIREFRAME=)`);
export const WF_URL = `file://${WIREFRAME}`;
export const OUT = resolve(HERE, "out");

export const BREAKPOINTS = {
  desktop: { width: 1440, height: 900 },
  tablet: { width: 960, height: 900 },
  mobile: { width: 760, height: 800 },
} as const;
export type Bp = keyof typeof BREAKPOINTS;

/** App route ↔ wireframe screen. */
export const SCREENS = [
  { key: "workspace", hash: "#/workspace", wf: "command-center" },
  { key: "workstreams", hash: "#/workstreams", wf: "workstreams" },
  { key: "agents", hash: "#/agents", wf: "agents-sessions" },
  { key: "review", hash: "#/review", wf: "review-decisions" },
] as const;

export type Scenario = "populated" | "empty" | "loading" | "offline";

/**
 * Opens the app. `populated|empty|loading` use fixture mode; empty/loading rewrite the
 * served createServices module so fixture sources/surfaces run that scenario (test-only).
 */
export async function openApp(page: Page, hash: string, scenario: Scenario = "populated", bp: Bp = "desktop") {
  await page.setViewportSize(BREAKPOINTS[bp]);
  await page.addInitScript(() => {
    try {
      localStorage.setItem("agent-orch.theme", "dark");
    } catch {}
  });
  if (scenario === "empty" || scenario === "loading") {
    await page.route(/\/src\/ui\/createServices\.ts/, async (route) => {
      const res = await route.fetch();
      const body = (await res.text())
        .replace(/createFixtureSources\(\)/g, `createFixtureSources({ scenario: "${scenario}" })`)
        .replace(/createFixtureSurfaces\(\)/g, `createFixtureSurfaces({ scenario: "${scenario}" })`);
      await route.fulfill({ response: res, body });
    });
  }
  const query = scenario === "offline" ? "" : "?fixture=1";
  await page.goto(`${APP}/${query}${hash}`);
  await page.locator("h1").first().waitFor({ timeout: 15_000 });
  await page.waitForTimeout(600);
}

export async function openWireframe(page: Page, screen: string, bp: Bp = "desktop") {
  await page.setViewportSize(BREAKPOINTS[bp]);
  await page.goto(`${WF_URL}#${screen}`);
  await page.evaluate(() => document.documentElement.setAttribute("data-theme", "dark"));
  await page.waitForTimeout(400);
}

/** Generic layout map: landmarks, headings, labeled regions, with boxes and key computed styles. */
export async function layoutMap(page: Page) {
  return page.evaluate(() => {
    const vis = (el: Element) => {
      const r = el.getBoundingClientRect();
      const cs = getComputedStyle(el);
      return r.width > 0 && r.height > 0 && cs.visibility !== "hidden" && cs.display !== "none";
    };
    const box = (el: Element) => {
      const r = el.getBoundingClientRect();
      return { x: Math.round(r.x), y: Math.round(r.y + scrollY), w: Math.round(r.width), h: Math.round(r.height) };
    };
    const name = (el: Element) =>
      el.getAttribute("aria-label") ??
      (el.getAttribute("aria-labelledby")
        ? el
            .getAttribute("aria-labelledby")!
            .split(/\s+/)
            .map((id) => document.getElementById(id)?.textContent?.trim())
            .join(" ")
        : null);
    const sel =
      "header,nav,main,aside,footer,section[aria-label],section[aria-labelledby],[role=region],[role=navigation],[role=complementary],[role=banner],[role=main],form,details,textarea,[role=meter]";
    const regions = [...document.querySelectorAll(sel)].filter(vis).map((el) => ({
      tag: el.tagName.toLowerCase(),
      role: el.getAttribute("role"),
      name: name(el)?.slice(0, 80) ?? null,
      cls: (el.getAttribute("class") ?? "").slice(0, 80),
      open: el.tagName === "DETAILS" ? (el as HTMLDetailsElement).open : undefined,
      box: box(el),
    }));
    const headings = [...document.querySelectorAll("h1,h2,h3,summary")].filter(vis).map((el) => ({
      tag: el.tagName.toLowerCase(),
      text: el.textContent!.trim().replace(/\s+/g, " ").slice(0, 80),
      box: box(el),
    }));
    return {
      viewport: { w: innerWidth, h: innerHeight },
      docH: document.documentElement.scrollHeight,
      docW: document.documentElement.scrollWidth,
      regions,
      headings,
    };
  });
}
