import { expect, test } from "@playwright/test";

const vitePort = Number(process.env.VITE_PORT);
if (!Number.isInteger(vitePort) || vitePort < 1 || vitePort > 65_535) {
  throw new Error("Playwright did not capture Vite's assigned port");
}
test.use({ baseURL: `http://127.0.0.1:${vitePort}`, viewport: { width: 760, height: 700 } });

test("keeps the TopBar usage popover within 760 CSS-pixel width", async ({ page }) => {
  await page.goto("/?fixture=1");

  const details = page.locator("details.top-usage-info");
  const popup = page.getByTestId("top-usage-info");
  await expect(popup.getByTestId("top-usage-estimate")).toBeAttached();
  await page.locator(".top-usage-info > summary").click();
  await expect(details).toHaveAttribute("open", "");

  // Below 760px the shell body is the scroller and the popover sits below the sidebar, so measure it once scrolled into view.
  const measured = await popup.evaluate((element) => {
    element.scrollIntoView({ block: "nearest", inline: "nearest" });
    const rect = element.getBoundingClientRect();
    return {
      innerWidth,
      innerHeight,
      rect: { x: rect.x, y: rect.y, top: rect.top, right: rect.right, bottom: rect.bottom, left: rect.left, width: rect.width, height: rect.height },
    };
  });
  console.info(JSON.stringify({ browser: page.context().browser()?.version(), viewport: { width: measured.innerWidth, height: measured.innerHeight }, bounds: measured.rect }));

  expect(measured.innerWidth).toBe(760);
  expect(measured.rect.width).toBeGreaterThan(0);
  expect(measured.rect.height).toBeGreaterThan(0);
  expect(measured.rect.left).toBeGreaterThanOrEqual(0);
  expect(measured.rect.top).toBeGreaterThanOrEqual(0);
  expect(measured.innerHeight).toBeGreaterThan(measured.rect.height);
  expect(measured.rect.bottom).toBeLessThanOrEqual(measured.innerHeight + 1); // scroll extents round to whole pixels
  expect(measured.rect.right).toBeLessThanOrEqual(measured.innerWidth);
});
