import { expect, test, type Page } from "@playwright/test";

const vitePort = Number(process.env.VITE_PORT);
if (!Number.isInteger(vitePort) || vitePort < 1 || vitePort > 65_535) {
  throw new Error("Playwright did not capture Vite's assigned port");
}
test.use({ baseURL: `http://127.0.0.1:${vitePort}` });

const viewport = { width: 960, height: 900 };
const resourceLabels = ["CPU", "Memory", "GPU", "Disk"] as const;

async function openFixture(page: Page, width: number) {
  await page.setViewportSize({ width, height: viewport.height });
  await page.goto("/?fixture=1#/workspace");
  await expect(page.getByTestId("fixture-banner")).toBeVisible();
}

test("expanded resources remain reachable and unclipped at 960 CSS px", async ({ page }) => {
  await openFixture(page, viewport.width);

  const region = page.getByRole("region", { name: "Device resources" });
  const disclosure = region.getByTestId("resource-strip");
  await disclosure.locator("summary").click();
  await expect(disclosure).toHaveAttribute("open", "");

  for (const name of resourceLabels) {
    await expect(region.getByText(name, { exact: true })).toBeVisible();
  }
  await expect(
    region.getByText("Scale colours (green to red) are a display aid, not an alert threshold. Disk is capacity used, not activity.", {
      exact: true,
    }),
  ).toBeVisible();

  await region.scrollIntoViewIfNeeded();
  const geometry = await region.evaluate((element) => {
    const scrollport = element.parentElement;
    if (!(scrollport instanceof HTMLElement)) throw new Error("resource region has no shell scrollport");
    const bounds = scrollport.getBoundingClientRect();
    return {
      clip: {
        left: Math.max(0, bounds.left),
        top: Math.max(0, bounds.top),
        right: Math.min(innerWidth, bounds.right),
        bottom: Math.min(innerHeight, bounds.bottom),
      },
      content: Array.from(element.querySelectorAll<HTMLElement>(".rs-item, .rs-note"), (item) => {
        const rect = item.getBoundingClientRect();
        return { left: rect.left, top: rect.top, right: rect.right, bottom: rect.bottom };
      }),
    };
  });

  expect(geometry.content).toHaveLength(resourceLabels.length + 1);
  for (const rect of geometry.content) {
    expect(rect.left).toBeGreaterThanOrEqual(geometry.clip.left - 1);
    expect(rect.top).toBeGreaterThanOrEqual(geometry.clip.top - 1);
    expect(rect.right).toBeLessThanOrEqual(geometry.clip.right + 1);
    expect(rect.bottom).toBeLessThanOrEqual(geometry.clip.bottom + 1);
  }
});

test("activity controls receive pointer input and activate at 760 CSS px", async ({ page }) => {
  await openFixture(page, 760);

  const activity = page.getByRole("complementary", { name: "Workspace activity" });
  const slots = activity.getByTestId("activity-slots");
  await slots.locator("summary").click();
  await expect(slots).toHaveAttribute("open", "");

  for (const name of ["Pause new dispatch", "Stop menu", "Queue settings"]) {
    await expect(activity.getByRole("button", { name, exact: true })).toBeVisible();
  }

  const pause = activity.getByRole("button", { name: "Pause new dispatch", exact: true });
  await expect(pause).toBeEnabled();
  await pause.scrollIntoViewIfNeeded();
  const point = await pause.evaluate((button) => {
    const rect = button.getBoundingClientRect();
    const x = rect.left + rect.width / 2;
    const y = rect.top + rect.height / 2;
    return {
      x,
      y,
      inViewport: x >= 0 && x <= innerWidth && y >= 0 && y <= innerHeight,
    };
  });

  expect(point.inViewport).toBe(true);
  await page.mouse.click(point.x, point.y);
  await expect(activity.getByTestId("simulated-receipt")).toHaveCount(1);
});
