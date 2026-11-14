// Diagnose covered focus stops: node diag-cover.mjs <out.png-prefix> <mode> <route> <w> <h> <theme>
import { launchBrowser, newPage, startDaemon, url, sleep } from "./lib.mjs";
const [prefix, mode, route, w, h, theme] = process.argv.slice(2);
const browser = await launchBrowser();
const daemon = mode === "live" ? await startDaemon() : null;
try {
  const { page } = await newPage(browser, { width: +w, height: +h, theme, mode, daemon });
  await page.goto(url({ mode: mode === "fixture" ? "fixture" : "x", route }), { waitUntil: "load" });
  await sleep(1200);
  for (let i = 0; i < 60; i++) {
    await page.keyboard.press("Tab");
    const info = await page.evaluate(() => {
      const el = document.activeElement;
      if (!el || el === document.body) return null;
      const r = el.getBoundingClientRect();
      const cx = Math.min(Math.max(r.left + r.width / 2, 0), innerWidth - 1);
      const cy = Math.min(Math.max(r.top + r.height / 2, 0), innerHeight - 1);
      const top = document.elementFromPoint(cx, cy);
      const covered = !!top && !(el === top || el.contains(top) || top.contains(el));
      const chain = [];
      for (let p = el.parentElement; p; p = p.parentElement) {
        const cs = getComputedStyle(p);
        if (/(auto|scroll|hidden)/.test(cs.overflowY)) chain.push(`${p.tagName.toLowerCase()}.${String(p.className).slice(0, 30)}[${cs.overflowY}] ${Math.round(p.getBoundingClientRect().top)}-${Math.round(p.getBoundingClientRect().bottom)}`);
      }
      return { covered, el: `${el.tagName}.${String(el.className).slice(0, 40)} "${(el.innerText || "").slice(0, 30)}"`, rect: [r.top, r.bottom, r.left, r.right].map(Math.round), top: top ? `${top.tagName}.${String(top.className).slice(0, 50)} ${JSON.stringify(top.getBoundingClientRect().toJSON?.() ?? {}, (k, v) => (typeof v === "number" ? Math.round(v) : v))}` : null, chain };
    });
    if (info?.covered) {
      console.log(i, JSON.stringify(info, null, 1));
      await page.screenshot({ path: `${prefix}-${i}.png` });
    }
  }
} finally {
  await browser.close();
  if (daemon) await daemon.stop();
}
