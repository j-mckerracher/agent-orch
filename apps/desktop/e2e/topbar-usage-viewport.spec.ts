import { expect, test } from "@playwright/test";

const vitePort = Number(process.env.VITE_PORT);
if (!Number.isInteger(vitePort) || vitePort < 1 || vitePort > 65_535) {
  throw new Error("Playwright did not capture Vite's assigned port");
}

test.use({ baseURL: `http://127.0.0.1:${vitePort}` });

const WIDTHS = [760, 960, 1280, 1440];
const SHOTS = process.env.USAGE_SHOTS;

for (const width of WIDTHS) {
  test(`keeps the TopBar and usage popover inside a ${width} CSS-pixel window`, async ({ page }) => {
    await page.setViewportSize({ width, height: 700 });
    await page.goto("/?fixture=1");
    const popup = page.getByTestId("top-usage-info");
    await expect(popup.getByTestId("top-usage-estimate")).toBeAttached();
    await page.locator(".top-usage-info > summary").click();
    await expect(page.locator("details.top-usage-info")).toHaveAttribute("open", "");
    const optIn = popup.getByRole("checkbox");
    if (SHOTS) await optIn.check(); // expands the "Other AI tools" section for the screenshots

    // Below 760px the shell body is the scroller and the popover sits below the sidebar, so measure it once scrolled into view.
    const measured = await page.evaluate(() => {
      const element = document.querySelector<HTMLElement>("[data-testid=top-usage-info]")!;
      element.scrollIntoView({ block: "nearest", inline: "nearest" });
      const rect = element.getBoundingClientRect();
      const bar = document.querySelector(".shell-topbar")!.getBoundingClientRect();
      return {
        innerWidth,
        innerHeight,
        pageOverflow: document.documentElement.scrollWidth - innerWidth,
        rect: { top: rect.top, right: rect.right, bottom: rect.bottom, left: rect.left, width: rect.width, height: rect.height },
        barRight: bar.right,
        barOverflow: document.querySelector(".shell-topbar")!.scrollWidth - document.querySelector(".shell-topbar")!.clientWidth,
      };
    });
    console.info(JSON.stringify({ width, ...measured }));
    if (SHOTS) await page.screenshot({ path: `${SHOTS}/${width}.png` });

    expect(measured.innerWidth).toBe(width);
    expect(measured.rect.width).toBeGreaterThan(0);
    expect(measured.rect.height).toBeGreaterThan(0);
    expect(measured.rect.left).toBeGreaterThanOrEqual(0);
    expect(measured.rect.top).toBeGreaterThanOrEqual(0);
    expect(measured.rect.bottom).toBeLessThanOrEqual(measured.innerHeight + 1); // scroll extents round to whole pixels
    expect(measured.rect.right).toBeLessThanOrEqual(measured.innerWidth - 8);
    expect(measured.barRight).toBeLessThanOrEqual(measured.innerWidth);
    expect(measured.pageOverflow).toBeLessThanOrEqual(0);
    expect(measured.barOverflow).toBeLessThanOrEqual(0);
  });

  test(`keeps the search results panel inside a ${width} CSS-pixel window`, async ({ page }) => {
    await page.setViewportSize({ width, height: 700 });
    await page.goto("/?fixture=1");
    await page.getByTestId("top-search-input").fill("a");
    const rect = await page.getByTestId("top-search-panel").evaluate((element) => {
      const { left, right } = element.getBoundingClientRect();
      return { left, right, innerWidth };
    });
    expect(rect.left).toBeGreaterThanOrEqual(0);
    expect(rect.right).toBeLessThanOrEqual(rect.innerWidth);
  });
}
