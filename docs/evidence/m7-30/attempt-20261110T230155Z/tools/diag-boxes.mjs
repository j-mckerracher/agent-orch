// Print the box chain from the composer up to the shell: node diag-boxes.mjs <mode> <route> <w> <h> <theme>
import { launchBrowser, newPage, url, sleep } from "./lib.mjs";
const [mode, route, w, h, theme] = process.argv.slice(2);
const browser = await launchBrowser();
try {
  const { page } = await newPage(browser, { width: +w, height: +h, theme, mode, daemon: null });
  await page.goto(url({ mode: mode === "fixture" ? "fixture" : "x", route }), { waitUntil: "load" });
  await sleep(1200);
  const out = await page.evaluate(() => {
    const rows = [];
    for (let el = document.querySelector(".ws-composer"); el; el = el.parentElement) {
      const cs = getComputedStyle(el);
      const r = el.getBoundingClientRect();
      rows.push(`${el.tagName.toLowerCase()}.${String(el.className).slice(0, 30)} top=${Math.round(r.top)} h=${Math.round(r.height)} sh=${el.scrollHeight} ov=${cs.overflowY} disp=${cs.display} height=${cs.height} minH=${cs.minHeight}`);
    }
    return rows;
  });
  console.log(out.join("\n"));
} finally {
  await browser.close();
}
