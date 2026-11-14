// Scrolls a target into view in several ways and reports where it ends up and whether axe still flags it.
import { launchBrowser, newPage, url, sleep, runAxe } from "./lib.mjs";
const [mode, route, w, h, theme, sel] = process.argv.slice(2);
const browser = await launchBrowser();
try {
  const { page } = await newPage(browser, { width: +w, height: +h, theme, mode, daemon: null });
  await page.goto(url({ mode: mode === "fixture" ? "fixture" : "x", route }), { waitUntil: "load" });
  await sleep(800);
  const rect = () => page.evaluate((s) => { const e = document.querySelector(s); const r = e.getBoundingClientRect(); const st = document.querySelector(".shell-strip, .shell-resources, [data-testid=resource-strip]"); const sr = st?.getBoundingClientRect(); const sc = []; for (let p = e.parentElement; p; p = p.parentElement) if (/(auto|scroll)/.test(getComputedStyle(p).overflowY)) sc.push(`${p.className.slice(0,25)} top=${p.scrollTop} max=${p.scrollHeight - p.clientHeight}`); return { top: Math.round(r.top), bottom: Math.round(r.bottom), strip: sr && [Math.round(sr.top), Math.round(sr.bottom)], sc }; }, sel);
  console.log("rest", JSON.stringify(await rect()));
  for (const how of ["center", "start", "nearest"]) {
    await page.evaluate((s, b) => document.querySelector(s).scrollIntoView({ block: b }), sel, how);
    console.log(how, JSON.stringify(await rect()), (await runAxe(page)).map((v) => v.id + ":" + v.nodes.map((n) => n.target[0] + " " + n.summary).join("|")).join(" ") || "clean");
  }
  await page.evaluate((s) => document.querySelector(s).focus(), sel);
  console.log("focus", JSON.stringify(await rect()), (await runAxe(page)).map((v) => v.id).join(" ") || "clean");
} finally { await browser.close(); }
