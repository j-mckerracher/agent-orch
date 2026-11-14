// Matrix sweep: modes x routes x themes x widths. Axe + overflow + console errors + screenshot.
// usage: node sweep.mjs <outDir> [--modes fixture,nobridge,live] [--widths 1280,...]
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { launchBrowser, newPage, runAxe, pageMetrics, startDaemon, url, sleep, RawClient } from "./lib.mjs";

const out = process.argv[2];
const arg = (name, dflt) => {
  const i = process.argv.indexOf(`--${name}`);
  return i > 0 ? process.argv[i + 1].split(",") : dflt;
};
const modes = arg("modes", ["fixture", "nobridge", "live"]);
const widths = arg("widths", ["1280", "1150", "960", "760", "480", "390"]).map(Number);
const heights = arg("heights", ["900"]).map(Number);
const routes = arg("routes", ["workspace", "workstreams", "agents", "review"]);
const themes = ["light", "dark"];
mkdirSync(join(out, "shots"), { recursive: true });

const browser = await launchBrowser();
// The daemon exits after 60 s without a client, so a keep-alive connection holds it up between pages.
let daemon = null;
let keepAlive = null;
const results = [];
try {
  for (const mode of modes) {
    if (mode === "live" && !daemon) {
      daemon = await startDaemon();
      keepAlive = (await RawClient.connect(daemon.dataDir, ["health"])).client;
    }
    for (const route of routes) {
      for (const theme of themes) {
        for (const height of heights) {
          for (const width of widths) {
            const { page, context, errors } = await newPage(browser, { width, height, theme, mode, daemon });
            await page.goto(url({ mode: mode === "fixture" ? "fixture" : "other", route }), { waitUntil: "load" });
            await sleep(mode === "live" ? 1800 : 700);
            const metrics = await pageMetrics(page);
            const violations = await runAxe(page);
            // A target that is only partially covered by the sticky resource strip at rest is re-checked after being scrolled into view.
            const obscured = violations.filter((v) => v.id === "target-size" && v.nodes.every((n) => /obscured/.test(n.summary ?? "")));
            let afterScroll = null;
            if (obscured.length) {
              for (const v of obscured) for (const n of v.nodes) await page.evaluate((sel) => document.querySelector(sel)?.scrollIntoView({ block: "center" }), n.target[0]);
              afterScroll = await runAxe(page);
            }
            const name = `${mode}-${route}-${theme}-${width}x${height}`;
            await page.screenshot({ path: join(out, "shots", `${name}.png`), fullPage: false });
            results.push({ name, mode, route, theme, width, height, metrics, violations, afterScroll, consoleErrors: errors });
            console.log(name, "axe:", violations.length, "hscroll:", metrics.hScroll, "docY:", metrics.docOverflowY, "mainOverflow:", metrics.mainOverflowsBox, "overlap:", metrics.activityOverlapsMain, "errs:", errors.length, mode === "live" && !/Connected/.test(metrics.conn ?? "") ? "NOT-CONNECTED" : "");
            await context.close();
          }
        }
      }
    }
  }
} finally {
  keepAlive?.socket.destroy();
  await browser.close();
  if (daemon) await daemon.stop();
}
writeFileSync(join(out, "results.json"), JSON.stringify(results, null, 1));
const hard = (r) => r.violations.length && !(r.afterScroll && r.afterScroll.length === 0);
const bad = results.filter((r) => hard(r) || r.metrics.hScroll || r.metrics.docOverflowY > 1 || r.consoleErrors.length || r.metrics.mainOverflowsBox > 1 || r.metrics.activityOverlapsMain || (r.mode === "live" && !/Connected/.test(r.metrics.conn ?? "")));
console.log(`states: ${results.length}, with findings: ${bad.length}, rest-state-obscured-only: ${results.filter((r) => r.violations.length && r.afterScroll?.length === 0).length}`);
