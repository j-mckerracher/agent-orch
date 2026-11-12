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

const censusRoutes = [
  "#/workspace", "#/workstreams", "#/agents", "#/review", "#/projects",
  "#/project/fixture%3Aproject-alpha", "#/project/fixture%3Aproject-alpha/tasks/all",
  "#/project/fixture%3Aproject-alpha/task/fixture%3Atask-auth-1",
  "#/project/fixture%3Aproject-alpha/preview/fixture%3Atask-auth-1",
  "#/project/fixture%3Aproject-alpha/coordinator",
  "#/project/fixture%3Aproject-alpha/review/fixture%3Atask-auth-1",
];

for (const width of [760, 761, 960, 961, 1150, 1151]) {
  test(`route census and shell slot geometry at ${width} CSS px`, async ({ page }) => {
    await page.setViewportSize({ width, height: width === 760 ? 600 : 800 });
    for (const fixture of [true, false]) {
      for (const route of censusRoutes) {
        await page.goto(`/${fixture ? "?fixture=1" : ""}${route}`);
        await expect(page.locator("main")).toHaveCount(1);
        await expect(page.getByTestId("shell-connection")).toBeVisible();
        await expect(page.locator("h1")).toHaveCount(1);
        await expect(page.getByRole("navigation", { name: "Primary views" }).locator('[aria-current="page"]')).toHaveCount(1);
        if (route.includes("/review/")) {
          await expect(page.getByRole("navigation", { name: "Breadcrumb" }).locator('[aria-current="page"]')).toHaveText("Review");
        }
        await expect(page.locator(".shell")).toHaveAttribute("data-shell-tier",
          width <= 760 ? "narrow" : width <= 960 ? "stacked" : width <= 1150 ? "medium" : "full");
        const geometry = await page.evaluate(() => ({
          width: innerWidth, height: innerHeight,
          stripHeight: document.querySelector(".shell-strip")!.getBoundingClientRect().height,
          sidebarHeight: document.querySelector(".shell-sidebar")!.getBoundingClientRect().height,
          slots: [document.documentElement, ...document.querySelectorAll<HTMLElement>(
            ".shell-sidebar,.shell-topbar,.shell-main,main,.shell-activity,.shell-resources,.shell-content")]
            .map(el => ({ slot: el.className || el.tagName, overflow: el.scrollWidth - el.clientWidth })),
          asides: [...document.querySelectorAll("aside")].map(el => el.getAttribute("aria-label")),
        }));
        console.log(JSON.stringify({ route, fixture, ...geometry }));
        expect(geometry.width).toBe(width);
        for (const slot of geometry.slots) expect(slot.overflow, `${route} fixture=${fixture} ${slot.slot}`).toBeLessThanOrEqual(1);
        if (width === 760) expect(geometry.sidebarHeight).toBeLessThanOrEqual(geometry.height * 0.42 + 1);
      }
    }
  });
}

test("Back and Forward traverse canonical and non-canonical routes", async ({ page }) => {
  await page.goto("/?fixture=1#/agents");
  for (const [incoming, canonical] of [
    ["#/project/fixture:project-alpha", "#/project/fixture%3Aproject-alpha"],
    ["#/unknown", "#/workspace"],
  ]) {
    await page.evaluate(hash => { location.hash = hash; }, incoming);
    await expect(page).toHaveURL(new RegExp(canonical.replaceAll("?", "\\?" ) + "$"));
    await page.goBack();
    await expect(page).toHaveURL(/#\/agents$/);
    await page.goForward();
    await expect(page).toHaveURL(new RegExp(canonical + "$"));
    await page.getByRole("link", { name: /Agents & Sessions/ }).click();
    await expect(page).toHaveURL(/#\/agents$/);
  }
  for (const route of censusRoutes) {
    await page.evaluate(hash => { location.hash = hash; }, route);
    await expect(page).toHaveURL(new RegExp(route + "$"));
  }
  for (const route of [...censusRoutes].reverse().slice(1)) {
    await page.goBack();
    await expect(page).toHaveURL(new RegExp(route + "$"));
  }
  for (const route of censusRoutes.slice(1)) {
    await page.goForward();
    await expect(page).toHaveURL(new RegExp(route + "$"));
  }
});
