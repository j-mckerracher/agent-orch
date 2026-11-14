// One-off discovery: what routes/ids are reachable in fixture + no-bridge mode.
import { chromium } from "@playwright/test";

const APP = process.env.APP_URL ?? "http://127.0.0.1:5791";
const browser = await chromium.launch({ channel: "chrome" });
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
const errors = [];
page.on("pageerror", (e) => errors.push(e.message));

async function visit(url) {
  await page.goto(url);
  await page.waitForTimeout(1200);
  const info = await page.evaluate(() => ({
    hash: location.hash,
    h1: [...document.querySelectorAll("h1")].map((h) => h.textContent.trim()),
    links: [...document.querySelectorAll("a[href^='#'], button")]
      .map((a) => (a.getAttribute("href") ?? "") + "|" + a.textContent.trim().slice(0, 50))
      .slice(0, 60),
    status: [...document.querySelectorAll("[role=status],[role=alert]")].map((s) => s.textContent.trim().slice(0, 120)),
  }));
  console.log("\n### " + url, JSON.stringify(info, null, 1));
}

for (const hash of ["#/workspace", "#/workstreams", "#/agents", "#/review", "#/projects"]) {
  await visit(`${APP}/?fixture=1${hash}`);
}
await visit(`${APP}/#/workspace`);
console.log("pageerrors", errors);
await browser.close();
