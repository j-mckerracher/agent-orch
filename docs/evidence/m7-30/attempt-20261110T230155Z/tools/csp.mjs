// Loads the built app with the production tauri.conf.json CSP enforced as a response header and reports violations.
// usage: node csp.mjs <outFile.json>
import { readFileSync, writeFileSync } from "node:fs";
import { launchBrowser, url, sleep } from "./lib.mjs";

const conf = JSON.parse(readFileSync(process.argv[3] ?? "../m7-30-20261110T230155Z/apps/desktop/src-tauri/tauri.conf.json", "utf8"));
const csp = conf.app.security.csp;
const browser = await launchBrowser();
const out = { csp, runs: [] };
for (const mode of ["fixture", "nobridge"]) {
  const page = await browser.newPage();
  const violations = [];
  page.on("console", (m) => /Content Security Policy|Refused to/i.test(m.text()) && violations.push(m.text()));
  page.on("pageerror", (e) => violations.push(`pageerror: ${e.message}`));
  await page.setRequestInterception(true);
  page.on("request", async (req) => {
    if (req.resourceType() !== "document") return req.continue();
    const res = await fetch(req.url());
    req.respond({ status: res.status, contentType: "text/html", headers: { "content-security-policy": csp }, body: await res.text() });
  });
  await page.goto(url({ mode, route: "workspace" }), { waitUntil: "load" });
  await sleep(2500);
  const rendered = await page.evaluate(() => document.querySelector("#shell-main, main")?.innerText.length ?? 0);
  out.runs.push({ mode, rendered, violations });
  console.log(mode, "rendered chars", rendered, "violations", violations.length, violations.slice(0, 3));
  await page.close();
}
await browser.close();
writeFileSync(process.argv[2], JSON.stringify(out, null, 1));
