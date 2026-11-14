import { launchBrowser, newPage, url, sleep } from "./lib.mjs";
const [mode, route, w, h] = process.argv.slice(2);
const browser = await launchBrowser();
try {
  const { page } = await newPage(browser, { width: +w, height: +h, theme: "light", mode, daemon: null });
  await page.goto(url({ mode: mode === "fixture" ? "fixture" : "x", route }), { waitUntil: "load" });
  await sleep(800);
  console.log(JSON.stringify(await page.evaluate(() => {
    const o = {};
    for (const s of ["html", "body", "#root", ".shell", ".shell-strip", ".shell-body", ".shell-content", ".shell-resources"]) {
      const e = document.querySelector(s); if (!e) continue; const r = e.getBoundingClientRect(); const cs = getComputedStyle(e);
      o[s] = [Math.round(r.top), Math.round(r.bottom), cs.position, cs.overflowY, cs.height];
    }
    return o;
  }), null, 1));
} finally { await browser.close(); }
