import { launchBrowser, newPage, url, sleep } from "./lib.mjs";
const [mode, route, w, h] = process.argv.slice(2);
const browser = await launchBrowser();
try {
  const { page } = await newPage(browser, { width: +w, height: +h, theme: "light", mode, daemon: null });
  await page.goto(url({ mode: mode === "fixture" ? "fixture" : "x", route }), { waitUntil: "load" });
  await sleep(700);
  console.log(JSON.stringify(await page.evaluate(() => [...document.querySelectorAll("body *")].filter((e) => { const p = getComputedStyle(e).position; return (p === "absolute" || p === "fixed") && e.getBoundingClientRect().bottom > innerHeight; }).map((e) => `${e.tagName}.${String(e.className).slice(0, 40)}|${e.getAttribute("data-testid")}|bottom=${Math.round(e.getBoundingClientRect().bottom)}|parent=${e.parentElement.className.slice(0,30)}`).slice(0, 15), null, 1)));
} finally { await browser.close(); }
