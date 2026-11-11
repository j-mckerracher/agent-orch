// Scroll a target to the centre and take a screenshot: node diag-shot.mjs <png> <mode> <route> <w> <h> <theme> <selector> [block]
import { launchBrowser, newPage, url, sleep } from "./lib.mjs";
const [png, mode, route, w, h, theme, sel, block = "center"] = process.argv.slice(2);
const browser = await launchBrowser();
try {
  const { page } = await newPage(browser, { width: +w, height: +h, theme, mode, daemon: null });
  await page.goto(url({ mode: mode === "fixture" ? "fixture" : "x", route }), { waitUntil: "load" });
  await sleep(800);
  await page.evaluate((s, b) => document.querySelector(s).scrollIntoView({ block: b }), sel, block);
  await sleep(200);
  await page.screenshot({ path: png });
} finally { await browser.close(); }
