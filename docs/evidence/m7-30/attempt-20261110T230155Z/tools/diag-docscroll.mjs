// Does the document itself scroll vertically (it must not: the shell owns scrolling)?
import { launchBrowser, newPage, url, sleep } from "./lib.mjs";
const [mode, route, theme, ...sizes] = process.argv.slice(2);
const browser = await launchBrowser();
try {
  for (const s of sizes) {
    const [w, h] = s.split("x").map(Number);
    const { page, context } = await newPage(browser, { width: w, height: h, theme, mode, daemon: null });
    await page.goto(url({ mode: mode === "fixture" ? "fixture" : "x", route }), { waitUntil: "load" });
    await sleep(700);
    const r = await page.evaluate(() => {
      const se = document.scrollingElement;
      const culprit = [...document.querySelectorAll("body *")].filter((e) => e.getBoundingClientRect().bottom > innerHeight + 1 && !e.closest(".shell-body") && !e.closest(".shell-strip")).slice(0, 3).map((e) => `${e.tagName}.${String(e.className).slice(0, 30)} ${Math.round(e.getBoundingClientRect().bottom)}`);
      return { scrollH: se.scrollHeight, clientH: se.clientHeight, delta: se.scrollHeight - se.clientHeight, culprit };
    });
    console.log(s, JSON.stringify(r));
    await context.close();
  }
} finally { await browser.close(); }
