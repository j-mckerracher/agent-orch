import { expect, test } from "@playwright/test";

const vitePort = Number(process.env.VITE_PORT);
if (!Number.isInteger(vitePort) || vitePort < 1 || vitePort > 65_535) {
  throw new Error("Playwright did not capture Vite's assigned port");
}

test.use({ baseURL: `http://127.0.0.1:${vitePort}` });

// Above 960px the shell main is the page's scroll container; below it the shell content scrolls instead.
const WINDOWS = [
  { width: 1000, height: 620 },
  { width: 1333, height: 829 },
  { width: 1440, height: 700 },
];

for (const { width, height } of WINDOWS) {
  test(`pins the composer to the bottom of a ${width}x${height} window`, async ({ page }) => {
    await page.setViewportSize({ width, height });
    await page.goto("/?fixture=1#/workspace");
    await expect(page.locator(".ws-composer")).toBeVisible();

    const measured = await page.evaluate(() => {
      const main = document.querySelector<HTMLElement>(".shell-main")!;
      const composer = document.querySelector<HTMLElement>(".ws-composer")!.getBoundingClientRect();
      return {
        overflow: main.scrollHeight - main.clientHeight,
        gapBelowComposer: main.getBoundingClientRect().bottom - composer.bottom,
      };
    });

    expect(measured.overflow).toBeLessThanOrEqual(1); // the main column never scrolls, so nothing passes under the composer
    expect(measured.gapBelowComposer).toBeLessThanOrEqual(36); // main's 24px bottom padding plus the 10px gap above the empty feedback row
  });
}
