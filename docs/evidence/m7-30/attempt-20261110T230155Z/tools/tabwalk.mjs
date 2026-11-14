// Keyboard-only Tab walk: every stop must be visible, have a focus indicator, an accessible name,
// and must not be covered by another element (sticky strips). usage: node tabwalk.mjs <outDir> [--modes ..]
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { launchBrowser, newPage, startDaemon, url, sleep, RawClient } from "./lib.mjs";

const out = process.argv[2];
const arg = (name, dflt) => {
  const i = process.argv.indexOf(`--${name}`);
  return i > 0 ? process.argv[i + 1].split(",") : dflt;
};
const modes = arg("modes", ["fixture", "live"]);
const sizes = arg("sizes", ["1280x700", "960x700", "760x700", "390x700"]).map((s) => s.split("x").map(Number));
const routes = arg("routes", ["workspace", "workstreams", "agents", "review"]);
const themes = arg("themes", ["light", "dark"]);
mkdirSync(out, { recursive: true });

const probe = () => {
  const el = document.activeElement;
  if (!el || el === document.body) return { body: true };
  const r = el.getBoundingClientRect();
  const cs = getComputedStyle(el);
  const ind = (e) => {
    const c = getComputedStyle(e);
    return (c.outlineStyle !== "none" && parseFloat(c.outlineWidth) > 0) || (c.boxShadow && c.boxShadow !== "none");
  };
  let focusIndicator = !!ind(el);
  // A composer shell may draw the ring for its textarea via :focus-within.
  for (let p = el.parentElement, n = 0; p && !focusIndicator && n < 3; p = p.parentElement, n++) if (p.matches(":focus-within") && ind(p)) focusIndicator = true;
  const outline = focusIndicator;
  const shadow = false;
  const name =
    el.getAttribute("aria-label") || el.getAttribute("aria-labelledby") || (el.labels && el.labels[0]?.innerText) || el.innerText || el.getAttribute("placeholder") || el.getAttribute("title") || "";
  const cx = Math.min(Math.max(r.left + r.width / 2, 0), innerWidth - 1);
  const cy = Math.min(Math.max(r.top + r.height / 2, 0), innerHeight - 1);
  const top = document.elementFromPoint(cx, cy);
  const covered = !!top && !(el === top || el.contains(top) || top.contains(el));
  const inView = r.bottom > 0 && r.top < innerHeight && r.right > 0 && r.left < innerWidth;
  return {
    tag: el.tagName.toLowerCase(),
    id: el.id || null,
    testid: el.getAttribute("data-testid"),
    role: el.getAttribute("role"),
    name: name.trim().replace(/\s+/g, " ").slice(0, 60),
    focusIndicator: outline || !!shadow,
    covered,
    coveredBy: covered ? `${top.tagName.toLowerCase()}.${String(top.className).slice(0, 40)}` : null,
    inView,
    rect: [Math.round(r.left), Math.round(r.top), Math.round(r.width), Math.round(r.height)],
    key: `${el.tagName}#${el.id}.${String(el.className).slice(0, 30)}|${(el.innerText || "").slice(0, 20)}`,
  };
};

const browser = await launchBrowser();
// The daemon exits after 60 s without a client, so it is started lazily and held up by a keep-alive connection.
let daemon = null;
let keepAlive = null;
const results = [];
try {
  for (const mode of modes) {
    if (mode === "live" && !daemon) {
      daemon = await startDaemon();
      keepAlive = (await RawClient.connect(daemon.dataDir, ["health"])).client;
    }
    for (const route of routes)
      for (const theme of themes)
        for (const [width, height] of sizes) {
          const { page, context } = await newPage(browser, { width, height, theme, mode, daemon });
          await page.goto(url({ mode: mode === "fixture" ? "fixture" : "x", route }), { waitUntil: "load" });
          await sleep(mode === "live" ? 1500 : 500);
          const stops = [];
          const seen = new Set();
          for (let i = 0; i < 150; i++) {
            await page.keyboard.press("Tab");
            const s = await page.evaluate(probe);
            if (s.body) {
              stops.push(s);
              break;
            }
            if (seen.has(s.key) && stops.length > 3) break;
            seen.add(s.key);
            stops.push(s);
          }
          const issues = stops.filter((s) => !s.body && (!s.focusIndicator || s.covered || !s.inView || (!s.name && !["input", "textarea", "select"].includes(s.tag) && s.tag !== "div")));
          results.push({ mode, route, theme, width, height, stops: stops.length, issues });
          console.log(`${mode} ${route} ${theme} ${width}x${height}: stops=${stops.length} issues=${issues.length}`);
          await context.close();
        }
  }
} finally {
  keepAlive?.socket.destroy();
  await browser.close();
  if (daemon) await daemon.stop();
}
writeFileSync(join(out, "tabwalk.json"), JSON.stringify(results, null, 1));
const bad = results.filter((r) => r.issues.length);
console.log(`runs: ${results.length}, with issues: ${bad.length}`);
for (const r of bad.slice(0, 40)) console.log(r.mode, r.route, r.theme, r.width, JSON.stringify(r.issues.slice(0, 3).map((i) => ({ t: i.tag, id: i.id, n: i.name, fi: i.focusIndicator, cov: i.coveredBy, v: i.inView }))));
