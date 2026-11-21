import { expect, test } from "@playwright/test";

const vitePort = Number(process.env.VITE_PORT);
if (!Number.isInteger(vitePort) || vitePort < 1 || vitePort > 65_535) {
  throw new Error("Playwright did not capture Vite's assigned port");
}

test.use({ baseURL: `http://127.0.0.1:${vitePort}`, viewport: { width: 960, height: 900 } });

/** The row's controls, measured against the row. Every control must be inside the row; the row must not overflow. */
async function measureRow(page: import("@playwright/test").Page) {
  return page.evaluate(() => {
    const row = document.querySelector<HTMLElement>(".ws-composer-row")!;
    const rowBox = row.getBoundingClientRect();
    const right = (selector: string) => document.querySelector<HTMLElement>(selector)!.getBoundingClientRect().right;
    const trigger = document.querySelector<HTMLElement>(".ws-mode-root > .ws-chip-button > span:first-child")!;
    return {
      pageOverflow: document.documentElement.scrollWidth - document.documentElement.clientWidth,
      rowOverflow: row.scrollWidth - row.clientWidth,
      rowRight: rowBox.right,
      rowWidth: rowBox.width,
      modeRight: right(".ws-mode-root .ws-chip-button"),
      pillRight: right(".ws-model-root .ws-chip-button"),
      sendRight: right(".ws-composer .ws-send"),
      modeWidth: document.querySelector<HTMLElement>(".ws-mode-root")!.getBoundingClientRect().width,
      triggerTruncated: trigger.scrollWidth > trigger.clientWidth,
    };
  });
}

test("keeps the mode dropdown, model pill and send button inside the composer at 960px", async ({ page }) => {
  await page.goto("/?fixture=1#/workspace");
  await expect(page.getByRole("button", { name: /Chat mode:/ })).toBeVisible();
  await expect(page.getByRole("button", { name: /^Model settings/ })).toBeVisible();
  await expect(page.locator(".ws-composer .ws-send")).toBeVisible();

  const measured = await measureRow(page);
  expect(measured.pageOverflow).toBeLessThanOrEqual(1);
  expect(measured.rowOverflow).toBeLessThanOrEqual(1);
  expect(measured.modeRight).toBeLessThanOrEqual(measured.rowRight + 1);
  expect(measured.pillRight).toBeLessThanOrEqual(measured.rowRight + 1);
  expect(measured.sendRight).toBeLessThanOrEqual(measured.rowRight + 1);
});

test("truncates a long mode label inside its share of the row, and keeps the pill and send button in place at 960px", async ({ page }) => {
  await page.goto("/?fixture=1#/workspace");
  await expect(page.getByRole("button", { name: /Chat mode:/ })).toBeVisible();

  // Fixtures never report an agent choice, so the trigger always shows the routed label. The longest real label a chat
  // can show is a live agent's two choices (Plan then autopilot · Allow all), so the label is set to a longer run of
  // that text to exercise the row's truncation. The label is only text; the layout rules under test are the same.
  await page.evaluate(() => {
    const label = document.querySelector<HTMLElement>(".ws-mode-root > .ws-chip-button > span:first-child")!;
    label.textContent = "Plan then autopilot · Allow all · Plan then autopilot · Allow all";
  });
  await expect(page.getByRole("button", { name: /Chat mode:/ })).toBeVisible();
  await expect(page.getByRole("button", { name: /^Model settings/ })).toBeVisible();
  await expect(page.locator(".ws-composer .ws-send")).toBeVisible();

  const measured = await measureRow(page);
  expect(measured.pageOverflow).toBeLessThanOrEqual(1);
  expect(measured.rowOverflow).toBeLessThanOrEqual(1);
  expect(measured.modeRight).toBeLessThanOrEqual(measured.rowRight + 1);
  expect(measured.pillRight).toBeLessThanOrEqual(measured.rowRight + 1);
  expect(measured.sendRight).toBeLessThanOrEqual(measured.rowRight + 1);
  expect(measured.modeWidth).toBeLessThanOrEqual(measured.rowWidth * 0.4 + 1);
  expect(measured.triggerTruncated).toBe(true);
});
