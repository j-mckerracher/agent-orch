// Renders app-icon.svg to the 1024x1024 master PNG that `tauri icon` consumes.
// Uses the Playwright Chromium from devDependencies, so no extra package is needed.
// Run from apps/desktop: node src-tauri/icons/source/render-master.mjs
import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const require = createRequire(join(here, "../../../package.json"));
const { chromium } = require("playwright-core");

const svg = readFileSync(join(here, "app-icon.svg"), "utf8");
const master = join(here, "app-icon-1024.png");

const browser = await chromium.launch();
try {
  const page = await browser.newPage({ viewport: { width: 1024, height: 1024 }, deviceScaleFactor: 1 });
  // The SVG markup is inlined so the page never has to load an external image.
  await page.setContent(
    `<!doctype html><html><body style="margin:0;background:transparent;overflow:hidden">` +
      `<div style="width:1024px;height:1024px;display:block">${svg}</div></body></html>`,
  );
  await page.screenshot({
    path: master,
    omitBackground: true,
    clip: { x: 0, y: 0, width: 1024, height: 1024 },
  });
} finally {
  await browser.close();
}
console.log(`wrote ${master}`);
