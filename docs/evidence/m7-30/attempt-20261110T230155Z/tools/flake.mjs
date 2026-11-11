// Flake probe: load live workspace N times, report connection pill + strip error text.
import { launchBrowser, newPage, startDaemon, url, sleep } from "./lib.mjs";
const n = +(process.argv[2] ?? 12);
const browser = await launchBrowser();
const daemon = await startDaemon();
await sleep(+(process.env.FLAKE_SLEEP_MS ?? 0));
const tally = {};
try {
  for (let i = 0; i < n; i++) {
    const width = [1280, 960, 760, 390][i % 4];
    const { page, context, errors, bridge } = await newPage(browser, { width, height: 900, theme: "light", mode: "live", daemon });
    await page.goto(url({ mode: "x", route: "workspace" }), { waitUntil: "load" });
    await sleep(2000);
    const txt = await page.evaluate(() => document.querySelector("[data-testid='shell-connection']")?.innerText.replace(/\s+/g, " ").slice(0, 160));
    const key = txt.slice(0, 40);
    tally[key] = (tally[key] ?? 0) + 1;
    if (!/Connected/.test(txt)) console.log(i, width, txt, errors, JSON.stringify(bridge.log));
    await context.close();
  }
} finally {
  await browser.close();
  await daemon.stop();
}
console.log(tally);
