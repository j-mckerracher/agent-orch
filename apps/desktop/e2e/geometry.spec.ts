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

test("System Usage stays reachable and unclipped in the activity popover at 960 CSS px", async ({ page }) => {
  await openFixture(page, viewport.width);

  const activity = page.getByTestId("workspace-activity");
  await activity.getByRole("button", { name: /^Workspace activity/ }).click();
  const usage = activity.getByTestId("resource-strip");
  // The System Usage section opens by default, as in the wireframe.
  await expect(usage).toHaveAttribute("open", "");

  for (const name of resourceLabels) {
    await expect(usage.getByText(name, { exact: true })).toBeVisible();
  }
  await expect(
    usage.getByText("Scale colors (green to red) are a display aid, not an alert threshold. Disk is capacity used, not activity.", {
      exact: true,
    }),
  ).toBeVisible();

  await usage.scrollIntoViewIfNeeded();
  const geometry = await usage.evaluate((element) => {
    const popover = element.closest<HTMLElement>(".activity-popover");
    if (!popover) throw new Error("System Usage is not inside the activity popover");
    const bounds = popover.getBoundingClientRect();
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

  const activity = page.getByTestId("workspace-activity");
  await activity.getByRole("button", { name: /^Workspace activity/ }).click();
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
        await expect(page.locator('.shell-sidebar [aria-current="page"]').first()).toBeAttached();
        if (route.includes("/review/")) {
          await expect(page.getByRole("navigation", { name: "Breadcrumb" }).locator('[aria-current="page"]')).toHaveText("Review");
        }
        await expect(page.locator(".shell")).toHaveAttribute("data-shell-tier",
          width <= 760 ? "narrow" : width <= 960 ? "stacked" : width <= 1150 ? "medium" : "full");
        const geometry = await page.evaluate(() => ({
          width: innerWidth, height: innerHeight,
          stripHeight: document.querySelector(".shell-strip")!.getBoundingClientRect().height,
          sidebarHeight: document.querySelector(".shell-sidebar")!.getBoundingClientRect().height,
          stripLeft: document.querySelector(".shell-strip")!.getBoundingClientRect().left,
          stripBottom: document.querySelector(".shell-strip")!.getBoundingClientRect().bottom,
          sidebarTop: document.querySelector(".shell-sidebar")!.getBoundingClientRect().top,
          sidebarRight: document.querySelector(".shell-sidebar")!.getBoundingClientRect().right,
          sidebarBottom: document.querySelector(".shell-sidebar")!.getBoundingClientRect().bottom,
          slots: [document.documentElement, ...document.querySelectorAll<HTMLElement>(
            ".shell-sidebar,.shell-topbar,.shell-main,main,.shell-activity,.shell-content")]
            .map(el => ({ slot: el.className || el.tagName, overflow: el.scrollWidth - el.clientWidth })),
          asides: [...document.querySelectorAll("aside")].map(el => el.getAttribute("aria-label")),
          activityControls: document.querySelectorAll('[data-testid="shell-activity"]').length,
        }));
        console.log(JSON.stringify({ route, fixture, ...geometry }));
        expect(geometry.width).toBe(width);
        for (const slot of geometry.slots) expect.soft(slot.overflow, `${route} fixture=${fixture} ${slot.slot}`).toBeLessThanOrEqual(1);
        if (width === 760) expect(geometry.sidebarHeight).toBeLessThanOrEqual(geometry.height * 0.42 + 1);
        // The sidebar spans the window height and the connection strip sits over the content column; the narrow
        // tier stacks the strip above the sidebar instead.
        if (width > 760) {
          expect.soft(Math.abs(geometry.sidebarBottom - geometry.sidebarTop - geometry.height), `${route} sidebar height`).toBeLessThanOrEqual(1);
          expect.soft(geometry.stripLeft, `${route} strip over content column`).toBeGreaterThanOrEqual(geometry.sidebarRight - 1);
        } else {
          expect.soft(geometry.stripBottom, `${route} strip above sidebar`).toBeLessThanOrEqual(geometry.sidebarTop + 1);
        }
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


test("keyboard Tab census, reverse traversal and skip link", async ({ page }) => {
  for (const width of [760, 900, 1280]) {
    await page.setViewportSize({ width, height: width === 760 ? 600 : 800 });
    await page.goto("/?fixture=1#/workspace");
    await expect(page.getByRole("heading", { name: "Orchestrator Workspace", exact: true })).toBeVisible();
    const stops: number[] = [];
    const identity = () => page.evaluate(() => Array.from(document.querySelectorAll("*")).indexOf(document.activeElement!));
    for (let i = 0; i < 150; i++) {
      await page.keyboard.press("Tab");
      const id = await identity();
      if (stops.length && id === stops[0]) break;
      // Chrome can focus the browser chrome between document cycles.
      if (await page.evaluate(() => document.activeElement === document.body)) continue;
      expect(stops).not.toContain(id);
      stops.push(id);
    }
    expect(stops.length).toBeGreaterThan(10);
    expect(stops.length).toBeLessThan(150);
    await page.keyboard.press("Shift+Tab");
    if (await page.evaluate(() => document.activeElement === document.body)) await page.keyboard.press("Shift+Tab");
    expect(await identity()).toBe(stops.at(-1));
    console.log(JSON.stringify({ keyboardWidth: width, tabStops: stops.length }));
    await page.getByRole("link", { name: "Skip to main content" }).focus();
    await page.keyboard.press("Enter");
    await expect(page.locator("#shell-main")).toBeFocused();
    await page.getByRole("link", { name: /Agents & Sessions/ }).focus();
    await page.keyboard.press("Enter");
    await expect(page).toHaveURL(/#\/agents$/);
  }
});

test("focused main stays below the connection banner at 760x700", async ({ page }) => {
  await page.setViewportSize({ width: 760, height: 700 });
  await page.goto("/#/workspace");

  const skipLink = page.getByRole("link", { name: "Skip to main content" });
  await skipLink.focus();
  await page.keyboard.press("Enter");
  const main = page.locator("#shell-main");
  await expect(main).toBeFocused();
  const geometry = await page.evaluate(() => {
    const banner = document.querySelector<HTMLElement>(".shell-strip");
    const focusedMain = document.querySelector<HTMLElement>("#shell-main");
    if (!banner || !focusedMain) throw new Error("shell banner or main region is missing");
    return {
      bannerBottom: banner.getBoundingClientRect().bottom,
      mainTop: focusedMain.getBoundingClientRect().top,
    };
  });
  expect(geometry.mainTop, "focused main is not obscured by the connection banner").toBeGreaterThanOrEqual(
    geometry.bannerBottom,
  );
});

// DPES-1102: sidebar resizing and window bounds. The window can be as small as 760x600 (tauri.conf.json) and as
// wide as an ultrawide display; at every size, and with the sidebar at either bound or collapsed, no shell region
// may scroll horizontally.
const windowSizes = [
  { width: 760, height: 600 },
  { width: 1280, height: 800 },
  { width: 1800, height: 900 },
  { width: 2560, height: 1080 },
] as const;
const sidebarStates = ["min", "max", "collapsed"] as const;

async function horizontalOverflow(page: Page) {
  return page.evaluate(() =>
    [".shell", ".shell-body", ".shell-sidebar", ".shell-topbar", ".shell-main", ".shell-activity", ".shell-resources", ".shell-content"]
      .map((selector) => document.querySelector<HTMLElement>(selector))
      .filter((element): element is HTMLElement => element !== null)
      // scrollWidth ignores clipping; a clipped box (the collapsed 0 px sidebar) cannot scroll or show overflow.
      .filter((element) => !["hidden", "clip"].includes(getComputedStyle(element).overflowX))
      .map((element) => ({ selector: element.className, overflow: element.scrollWidth - element.clientWidth }))
      .concat([{ selector: "document", overflow: document.documentElement.scrollWidth - document.documentElement.clientWidth }]),
  );
}

for (const size of windowSizes) {
  for (const state of sidebarStates) {
    test(`no horizontal overflow at ${size.width}x${size.height} with the sidebar ${state}`, async ({ page }) => {
      await page.setViewportSize(size);
      await page.goto("/?fixture=1#/workspace");
      await expect(page.getByTestId("fixture-banner")).toBeVisible();

      // 760 px and below the sidebar is a full-width block with no splitter; the state does not apply.
      if (size.width > 760) {
        const splitter = page.getByRole("separator", { name: "Resize sidebar" });
        await splitter.focus();
        await page.keyboard.press(state === "min" ? "Home" : state === "max" ? "End" : "Enter");
      }

      for (const { selector, overflow } of await horizontalOverflow(page)) {
        expect(overflow, `${selector} scrolls horizontally`).toBeLessThanOrEqual(1);
      }
    });
  }
}

test("a dragged sidebar width is clamped to 40vw at the minimum window width and survives a reload", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/?fixture=1#/workspace");
  await page.getByRole("separator", { name: "Resize sidebar" }).focus();
  await page.keyboard.press("End");
  const sidebar = page.getByTestId("shell-sidebar");
  expect((await sidebar.boundingBox())?.width).toBeCloseTo(480, 0);

  // Narrow window: the saved 480 px must not take more than 40vw (304 px at 761 px wide).
  await page.setViewportSize({ width: 761, height: 600 });
  expect((await sidebar.boundingBox())?.width).toBeLessThanOrEqual(761 * 0.4 + 1);

  await page.setViewportSize({ width: 1280, height: 800 });
  await page.reload();
  await expect(page.getByTestId("fixture-banner")).toBeVisible();
  expect((await sidebar.boundingBox())?.width).toBeCloseTo(480, 0);
});

test("dragging the splitter resizes the sidebar and the toggle collapses it out of the tab order", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/?fixture=1#/workspace");
  const sidebar = page.getByTestId("shell-sidebar");
  const splitter = page.getByRole("separator", { name: "Resize sidebar" });
  const box = await splitter.boundingBox();
  if (!box) throw new Error("splitter has no box");
  const startX = box.x + box.width / 2;
  const startY = box.y + 200;
  await page.mouse.move(startX, startY);
  await page.mouse.down();
  await page.mouse.move(startX + 60, startY, { steps: 5 });
  await page.mouse.up();
  expect((await sidebar.boundingBox())?.width).toBeCloseTo(274, 0);

  await page.getByRole("button", { name: "Collapse sidebar" }).click();
  await expect(sidebar).toBeHidden();
  await page.getByRole("button", { name: "Expand sidebar" }).click();
  await expect(sidebar).toBeVisible();
});

test("agents and workstreams split columns follow the wireframe 1150 and 760 breakpoints", async ({ page }) => {
  const trackCount = (selector: string) =>
    page.locator(selector).evaluate((el) => getComputedStyle(el).gridTemplateColumns.split(" ").length);

  await page.setViewportSize({ width: 1150, height: viewport.height });
  await page.goto("/?fixture=1#/agents");
  await expect(page.locator(".agents-layout")).toBeVisible();
  expect(await trackCount(".agents-layout"), "agents stacks at 1150 px").toBe(1);
  await page.setViewportSize({ width: 1151, height: viewport.height });
  expect(await trackCount(".agents-layout"), "agents splits above 1150 px").toBe(2);

  await page.setViewportSize({ width: 1150, height: viewport.height });
  await page.goto("/?fixture=1#/workstreams");
  await expect(page.locator(".ws-layout")).toBeVisible();
  const listColumn = await page
    .locator(".ws-layout")
    .evaluate((el) => getComputedStyle(el).gridTemplateColumns.split(" ")[0]);
  expect(listColumn, "workstream list column is 145 px at 1150 px").toBe("145px");
  expect(await trackCount(".ws-layout"), "workstreams keep two columns at 1150 px").toBe(2);
  await page.setViewportSize({ width: 960, height: viewport.height });
  expect(await trackCount(".ws-layout"), "workstreams keep two columns at 960 px").toBe(2);

  await page.setViewportSize({ width: 760, height: viewport.height });
  expect(await trackCount(".ws-layout"), "workstreams stack at 760 px").toBe(1);
  expect(await trackCount(".ws-items"), "workstream list is two columns at 760 px").toBe(2);
});

test("the workstream list badge stays on one line and item titles do not wrap per letter at 960 px", async ({ page }) => {
  await page.setViewportSize({ width: viewport.width, height: viewport.height });
  await page.goto("/?fixture=1#/workstreams");
  const badge = page.locator(".ws-list-marker .ui-badge");
  await expect(badge).toBeVisible();

  const lineHeight = parseFloat(await badge.evaluate((el) => getComputedStyle(el).lineHeight));
  const box = await badge.boundingBox();
  expect(box, "badge is laid out").toBeTruthy();
  expect(box!.height, "badge is one line tall").toBeLessThanOrEqual(lineHeight * 1.5);
  // The rendered text, not the box: a label wider than its box overflows visibly.
  const textRight = await badge.evaluate((el) => {
    const range = document.createRange();
    range.selectNodeContents(el);
    return range.getBoundingClientRect().right;
  });
  const contentRight = await page.locator(".ws-list").evaluate((el) => {
    const style = getComputedStyle(el);
    return el.getBoundingClientRect().right - parseFloat(style.paddingRight);
  });
  expect(textRight, "badge text stays inside the list panel's content edge").toBeLessThanOrEqual(contentRight);
  const itemRights = await page.locator(".ws-item").evaluateAll((els) => els.map((el) => el.getBoundingClientRect().right));
  for (const right of itemRights) {
    expect(right, "a list item stays inside the list panel").toBeLessThanOrEqual(contentRight + 1);
  }

  const titleLine = parseFloat(await page.locator(".ws-item-title").first().evaluate((el) => getComputedStyle(el).lineHeight));
  const titles = await page.locator(".ws-item-title").evaluateAll((els) => els.map((el) => el.getBoundingClientRect().height));
  for (const height of titles) {
    expect(height, "an item title wraps at words, not per letter").toBeLessThanOrEqual(titleLine * 3);
  }
});

test("agents split is 1.1 to 1 at desktop width, with the first running task selected", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto("/?fixture=1#/agents");
  const detail = page.getByRole("complementary", { name: "Selected task / session detail" });
  await expect(detail).toContainText("fixture:task-running");
  const work = await page.getByRole("region", { name: "Assigned work and sessions" }).boundingBox();
  const selected = await detail.boundingBox();
  expect(work).not.toBeNull();
  expect(selected).not.toBeNull();
  expect(selected!.width / work!.width).toBeCloseTo(1 / 1.1, 2);
});

// DPES-1105 (RC-24): the wireframe reserves 324 px for the inspector column at 961 px and up, a 300 px card
// and a 24 px gutter. The conversation must stop short of the open card, and the column stays when collapsed.
const INSPECTOR_GAP = 24;
for (const [width, height] of [
  [1440, 900],
  [1920, 1080],
] as const) {
  test(`workspace inspector has its own column at ${width} CSS px: the conversation stops short of the open card`, async ({ page }) => {
    await page.setViewportSize({ width, height });
    await page.goto("/?fixture=1#/workspace");
    await expect(page.getByTestId("fixture-banner")).toBeVisible();
    await expect(page.locator(".ws-head h2")).toBeVisible();

    const conversation = page.locator(".ws-conversation");
    const conversationRight = async () => {
      const b = (await conversation.boundingBox())!;
      return b.x + b.width;
    };
    const collapsedRight = await conversationRight();

    await page.getByTestId("workspace-activity").getByRole("button", { name: /^Workspace activity/ }).click();
    const card = page.getByTestId("activity-popover");
    await expect(card).toBeVisible();
    const cardBox = (await card.boundingBox())!;
    expect(await conversationRight(), "conversation right edge vs inspector left edge").toBeLessThanOrEqual(cardBox.x - INSPECTOR_GAP);
    expect(await conversationRight(), "column stays reserved while the card is open").toBeCloseTo(collapsedRight, 0);

    const parts = await page.locator(".ws-conversation > *, .ws-log > *, .ws-composer-row > *").evaluateAll((els) =>
      els.map((el) => {
        const r = el.getBoundingClientRect();
        return { right: r.right, bottom: r.bottom, left: r.left, top: r.top };
      }),
    );
    expect(parts.length, "conversation parts measured").toBeGreaterThan(0);
    for (const part of parts) {
      const overlapsCard =
        part.left < cardBox.x + cardBox.width && part.right > cardBox.x && part.top < cardBox.y + cardBox.height && part.bottom > cardBox.y;
      expect(overlapsCard, "a conversation part runs under the inspector card").toBe(false);
    }
  });
}

test("the model chip's label, level and badge are whole and on one line beside the target, at 960 CSS px", async ({ page }) => {
  await openFixture(page, 960);
  const chip = page.locator(".ws-model-root > .ws-chip-button");
  const target = page.locator(".ws-composer-row > .ws-popover-root:not(.ws-model-root) > .ws-chip-button");
  const label = chip.locator(":scope > span").first();
  const level = chip.locator(".ws-chip-level");
  const badge = chip.locator(".ui-badge");
  await expect(chip).toContainText("Codex · default · medium · Auto");
  for (const [name, part] of [
    ["label", label],
    ["level", level],
    ["badge", badge],
  ] as const) {
    expect(await part.evaluate((el) => el.scrollWidth <= el.clientWidth), `${name} is not clipped`).toBe(true);
    expect((await part.boundingBox())!.height, `${name} is one line`).toBeLessThan(24);
  }
  const chipBox = (await chip.boundingBox())!;
  const targetBox = (await target.boundingBox())!;
  const sendBox = (await page.locator(".ws-composer .ws-send").boundingBox())!;
  expect(Math.abs(chipBox.y - targetBox.y), "the chip sits on the target's row, not a new one").toBeLessThan(2);
  expect(chipBox.x + chipBox.width, "the chip ends before the send button").toBeLessThanOrEqual(sendBox.x);
});

test.describe("connection strip collapse (DPES-1105)", () => {
  for (const width of [1440, 960, 760]) {
    test(`collapsing the strip shortens it and moves the content up at ${width} CSS px`, async ({ page }) => {
      await page.setViewportSize({ width, height: 900 });
      await page.goto("/?fixture=1#/workspace");
      await expect(page.getByTestId("fixture-banner")).toBeVisible();
      const strip = page.getByTestId("shell-connection");
      const content = page.locator("#shell-main");
      const measure = async () => ({
        strip: (await strip.boundingBox())!.height,
        content: (await content.boundingBox())!.y,
      });

      const expanded = await measure();
      await page.getByRole("button", { name: "Collapse connection details" }).click();
      await expect(page.getByRole("button", { name: "Expand connection details" })).toHaveAttribute("aria-expanded", "false");
      const collapsed = await measure();

      expect(collapsed.strip, "collapsed strip height").toBeLessThan(expanded.strip);
      expect(collapsed.content, "content top after collapse").toBeLessThan(expanded.content);
      expect(
        Math.abs(expanded.content - collapsed.content - (expanded.strip - collapsed.strip)),
        "content moves up by the height the strip gave back",
      ).toBeLessThan(1);
    });
  }

  test("the sidebar still spans the full window height with the strip collapsed at 1440 CSS px", async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.goto("/?fixture=1#/workspace");
    await page.getByRole("button", { name: "Collapse connection details" }).click();
    const sidebar = (await page.getByTestId("shell-sidebar").boundingBox())!;
    expect(sidebar.y, "sidebar top").toBeCloseTo(0, 0);
    expect(sidebar.y + sidebar.height, "sidebar bottom").toBeCloseTo(900, 0);
  });
});
