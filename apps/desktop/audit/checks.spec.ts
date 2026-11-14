// Check phase: one test per remaining defect, grouped by root cause (see audit/PLAN.md).
// Every test must FAIL on main; a test that passes on main is a wrong check or a non-defect.
// Scope: defects only. Intentional and wireframe-gap findings are deliberately absent.
// Run with: npx playwright test -c audit/playwright.audit.config.ts audit/checks.spec.ts
import { expect, test, type Locator, type Page } from "@playwright/test";
import { openApp } from "./harness";

test.skip(process.env.AUDIT_CAPTURE === "1", "capture run");

/** Bounding box of a rendered element; fails with a clear message when nothing is drawn. */
async function box(loc: Locator) {
  const b = await loc.boundingBox();
  if (!b) throw new Error("element has no bounding box (not rendered)");
  return b;
}

/**
 * Open state of the <details> whose <summary> matches `name`.
 * Returns "missing" when no such summary exists, so the assertion fails with a readable value.
 */
async function detailsOpen(page: Page, name: RegExp): Promise<boolean | "missing"> {
  const summary = page.locator("summary", { hasText: name }).first();
  if ((await summary.count()) === 0) return "missing";
  return summary.evaluate((s) => (s.parentElement as HTMLDetailsElement).open);
}

/** Opens the Workspace activity trigger when the popover variant renders one. */
async function openWorkspaceActivity(page: Page) {
  const trigger = page.getByRole("button", { name: /^Workspace activity/ });
  if ((await trigger.count()) > 0) await trigger.click();
}

const RUNNING_TASK = /fixture:task-running/;
const AGENT_DETAIL = "Selected task / session detail";

// ---------------------------------------------------------------------------
// RC-01 Honest states: unavailable is never shown as loading, zero, or empty copy
// ---------------------------------------------------------------------------
test.describe("RC-01 honest states: unavailable is never loading, zero, or empty copy", () => {
  test("workspace-22: offline conversation states the error instead of staying on Loading", async ({ page }) => {
    await openApp(page, "#/workspace", "offline", "desktop");
    await expect(page.getByText(/Loading conversations/)).toHaveCount(0, { timeout: 5000 });
  });

  test("workstreams-05: offline workstreams view and sidebar do not say Loading workstreams", async ({ page }) => {
    await openApp(page, "#/workstreams", "offline", "desktop");
    await expect(page.getByText(/Loading workstreams/)).toHaveCount(0, { timeout: 5000 });
  });

  test("workstreams-20: loading state does not say workstreams are not available", async ({ page }) => {
    await openApp(page, "#/workstreams", "loading", "desktop");
    await expect(page.getByLabel(/workstreams: not available/i)).toHaveCount(0);
  });

  test("agents-16: empty task list does not read as an unavailable list with stuck counts", async ({ page }) => {
    await openApp(page, "#/agents", "empty", "desktop");
    await expect(page.getByText(/task list is not available/i)).toHaveCount(0);
    await expect(page.getByText(/\(…\)/)).toHaveCount(0);
  });

  test("review-13: offline global review does not say Decisions: loading", async ({ page }) => {
    await openApp(page, "#/review", "offline", "desktop");
    await expect(page.getByText(/Decisions: loading/)).toHaveCount(0);
  });

  test("review-17: offline project page does not show zero pending-review counts", async ({ page }) => {
    await openApp(page, "#/project/fixture-project", "offline", "desktop");
    await expect(page.getByText(/Pending review \(0\)/)).toHaveCount(0);
  });

  test("review-19: offline tasks page does not show 0 / 0 counts", async ({ page }) => {
    await openApp(page, "#/project/fixture-project/tasks/all", "offline", "desktop");
    await expect(page.getByText(/\b0\s*\/\s*0\b/)).toHaveCount(0);
  });
});

// ---------------------------------------------------------------------------
// RC-02 Workspace activity is a top-right popover, not a docked full-height panel
// ---------------------------------------------------------------------------
test.describe("RC-02 workspace activity is a top-right popover, not a docked panel", () => {
  test("workspace-05: Workspace activity is a collapsed top-right trigger", async ({ page }) => {
    await openApp(page, "#/workspace", "populated", "desktop");
    await expect(page.getByRole("button", { name: /^Workspace activity/ })).toBeVisible();
  });

  test("workspace-06: execution queue, attention and agent slots start collapsed", async ({ page }) => {
    await openApp(page, "#/workspace", "populated", "desktop");
    await openWorkspaceActivity(page);
    for (const name of [/^Execution queue/, /^Attention/, /^Agent slots/]) {
      expect(await detailsOpen(page, name), `${name} should start collapsed`).toBe(false);
    }
  });

  test("workspace-07: queue rows are compact disclosure rows (48px or less)", async ({ page }) => {
    await openApp(page, "#/workspace", "populated", "desktop");
    await openWorkspaceActivity(page);
    const queue = page.locator("summary", { hasText: /^Execution queue/ }).first();
    await queue.evaluate((s) => { (s.parentElement as HTMLDetailsElement).open = true; });
    const running = queue.locator("xpath=..").locator("summary", { hasText: /^Running/ }).first();
    await running.evaluate((s) => { (s.parentElement as HTMLDetailsElement).open = true; });
    const row = running.locator("xpath=..").locator("li").first();
    expect((await box(row)).height).toBeLessThanOrEqual(48);
  });
});

// ---------------------------------------------------------------------------
// RC-03 Sidebar workstream tree overflows and blocks its own rows
// ---------------------------------------------------------------------------
test.describe("RC-03 sidebar workstream tree overflow and pointer targets", () => {
  test("workstreams-01: sidebar workstream rows 2 and 3 receive pointer events at desktop and tablet", async ({ page }) => {
    for (const bp of ["desktop", "tablet"] as const) {
      await openApp(page, "#/workstreams", "populated", bp);
      for (const n of [2, 3]) {
        const row = page.getByRole("treeitem", { name: new RegExp(`Workstream ${n} of 3`) });
        await row.scrollIntoViewIfNeeded();
        const hit = await row.evaluate((el) => {
          const b = el.getBoundingClientRect();
          const top = document.elementFromPoint(b.x + b.width / 2, b.y + b.height / 2);
          return !!top && (el === top || el.contains(top));
        });
        expect(hit, `${bp}: workstream ${n} of 3 is not hit-testable (below the fold or covered)`).toBe(true);
      }
    }
  });

  test("workstreams-03: mobile sidebar workstream tree stays inside its nav box", async ({ page }) => {
    await openApp(page, "#/workstreams", "populated", "mobile");
    const target = page.getByRole("treeitem", { name: /Workstream 3 of 3/ });
    await target.scrollIntoViewIfNeeded();
    const nav = await box(page.getByRole("navigation", { name: "Workstreams" }));
    const row = await box(target);
    expect(row.y).toBeGreaterThanOrEqual(nav.y - 1);
    expect(row.y + row.height).toBeLessThanOrEqual(nav.y + nav.height + 1);
  });
});

// ---------------------------------------------------------------------------
// RC-04 Breakpoint parity: tablet and mobile rules diverge from the wireframe media queries
// ---------------------------------------------------------------------------
test.describe("RC-04 breakpoint parity with wireframe media queries (960 / 760)", () => {
  test("agents-01: at tablet width Selected task detail stacks below Assigned work", async ({ page }) => {
    await openApp(page, "#/agents", "populated", "tablet");
    const card = await box(page.getByRole("region", { name: "Assigned work and sessions" }));
    const detail = await box(page.getByRole("complementary", { name: AGENT_DETAIL }));
    expect(detail.y).toBeGreaterThanOrEqual(card.y + card.height - 1);
  });

  test("workstreams-02: at tablet width the workstream list and objective detail sit in two columns", async ({ page }) => {
    await openApp(page, "#/workstreams", "populated", "tablet");
    const list = await box(page.getByRole("region", { name: "Workstream list" }));
    const detail = await box(page.getByRole("region", { name: /^Workstream Auth hardening/ }));
    expect(detail.y, "detail starts below the list, so the layout stacks").toBeLessThan(list.y + list.height);
  });

  test("workstreams-15: at mobile width the workstream list uses two columns", async ({ page }) => {
    await openApp(page, "#/workstreams", "populated", "mobile");
    const a = await box(page.getByRole("button", { name: /1\. Auth hardening/ }));
    const b = await box(page.getByRole("button", { name: /2\. Docs refresh/ }));
    expect(Math.abs(a.y - b.y)).toBeLessThan(2);
  });
});

// ---------------------------------------------------------------------------
// RC-05 Six separate completion facts: missing on project review, misplaced on global review
// ---------------------------------------------------------------------------
test.describe("RC-05 six-fact candidate record: presence and placement on review routes", () => {
  test("review-and-app-only-01: project review shows the six separate facts", async ({ page }) => {
    await openApp(page, "#/project/fixture-project/review/fixture-task-1?fixture=1", "populated", "desktop");
    await expect(page.getByText(/^Six separate facts/).first()).toBeVisible();
  });

  test("review-and-app-only-02: global review places the six facts after the diff and decision layout", async ({ page }) => {
    await openApp(page, "#/review", "populated", "desktop");
    const facts = await box(page.getByText(/^Six separate facts/).first());
    const diff = await box(page.getByText("Unified diff", { exact: true }));
    expect(facts.y).toBeGreaterThan(diff.y);
  });
});

// ---------------------------------------------------------------------------
// RC-06 Sidebar dark theme and Workstreams heading box (covered by sidebar branch)
// ---------------------------------------------------------------------------
test.describe("RC-06 sidebar dark theme and Workstreams heading box", () => {
  test("workspace-01: sidebar paints a dark background in dark theme", async ({ page }) => {
    await openApp(page, "#/workspace", "populated", "desktop");
    const bg = await page.getByRole("region", { name: "Sidebar" }).evaluate((el) => getComputedStyle(el).backgroundColor);
    const [r, g, b, a = 1] = bg.match(/[\d.]+/g)!.map(Number);
    expect(a, "sidebar has no painted background (transparent)").toBeGreaterThan(0);
    expect((r + g + b) / 3).toBeLessThan(128);
  });

  test("workstreams-10: sidebar 'Workstreams · N' heading is a bordered link box", async ({ page }) => {
    await openApp(page, "#/workstreams", "populated", "desktop");
    const link = page.getByRole("link", { name: /^Workstreams · \d+/ });
    const bordered = await link.evaluate((el) => {
      // Walk up to four ancestors; the heading box is the first one with all four borders drawn.
      let node: HTMLElement | null = el.parentElement;
      for (let i = 0; node && i < 4; i++, node = node.parentElement) {
        const cs = getComputedStyle(node);
        if ([cs.borderTopWidth, cs.borderRightWidth, cs.borderBottomWidth, cs.borderLeftWidth].every((w) => parseFloat(w) > 0)) {
          return true;
        }
      }
      return false;
    });
    expect(bordered, "no bordered box around the Workstreams heading link").toBe(true);
  });
});

// ---------------------------------------------------------------------------
// RC-07 Device resource strip: bottom strip covers content; meters belong in the activity area
// ---------------------------------------------------------------------------
test.describe("RC-07 device resources placement (bottom strip vs activity area)", () => {
  // The defect is a full-width bottom band across the window (OX-A9 / OX-B6h put System Usage in the
  // activity area). A rail column or popover is narrow, so a band wider than 80% of the window fails.
  test("workspace-08: device resources are not a full-width strip along the bottom window edge", async ({ page }) => {
    await openApp(page, "#/workspace", "populated", "desktop");
    const strip = page.getByRole("region", { name: "Device resources" });
    if ((await strip.count()) > 0) {
      const s = await box(strip);
      const viewportWidth = page.viewportSize()!.width;
      expect(s.width, "device resources span the window as a bottom strip").toBeLessThan(viewportWidth * 0.8);
    }
  });

  test("agents-18: device strip does not cover the selected task detail at desktop", async ({ page }) => {
    await openApp(page, "#/agents", "populated", "desktop");
    await page.getByRole("button", { name: RUNNING_TASK }).click();
    const detail = await box(page.getByRole("complementary", { name: AGENT_DETAIL }));
    const strip = page.getByRole("region", { name: "Device resources" });
    if ((await strip.count()) > 0) {
      const s = await box(strip);
      expect(detail.y + detail.height).toBeLessThanOrEqual(s.y + 1);
    }
  });
});

// ---------------------------------------------------------------------------
// RC-08 Workstreams grouped by project (D-4 interim grouping) and list card wording
// ---------------------------------------------------------------------------
test.describe("RC-08 workstreams grouped by project (D-4) and list card", () => {
  // workstreams-04 (tree/list grouped by fixture workstream, not by project) has no check here.
  // Fixture-mode #/projects renders "Offline: no cached projects" with no rows, so the project
  // list the grouping must match is unavailable and the assertion cannot be made honestly.
  // See audit/PLAN.md "What could not be verified".

  test("workstreams-11: list card reads 'N initiatives' with derived project status lines", async ({ page }) => {
    await openApp(page, "#/workstreams", "populated", "desktop");
    await expect(page.getByText(/^\d+ initiatives$/)).toBeVisible();
    await expect(page.getByText(/\d+ projects? · /).first()).toBeVisible();
  });
});

// ---------------------------------------------------------------------------
// RC-09 Workstream detail: objective header, plan revision and Activity tab
// ---------------------------------------------------------------------------
test.describe("RC-09 workstream detail header, plan revision and tabs", () => {
  test("workstreams-07: objective header shows plan revision", async ({ page }) => {
    await openApp(page, "#/workstreams", "populated", "desktop");
    await expect(page.getByText(/Plan revision/)).toBeVisible();
  });

  test("workstreams-08: workstream detail offers the Activity tab", async ({ page }) => {
    await openApp(page, "#/workstreams", "populated", "desktop");
    await expect(page.getByRole("tab", { name: "Activity" })).toBeVisible();
  });

  test("workstreams-16: purpose and rationale paragraph is rendered", async ({ page }) => {
    await openApp(page, "#/workstreams", "populated", "desktop");
    await expect(page.getByText(/Purpose and rationale/)).toBeVisible();
  });
});

// ---------------------------------------------------------------------------
// RC-10 Workstreams page header and plan-editing controls
// ---------------------------------------------------------------------------
test.describe("RC-10 workstreams page header and plan controls", () => {
  test("workstreams-06: Create workstream is a header action beside the page heading", async ({ page }) => {
    await openApp(page, "#/workstreams", "populated", "desktop");
    const h1 = await box(page.getByRole("heading", { level: 1, name: "Workstreams" }));
    const btn = await box(page.getByRole("button", { name: /Create workstream/ }));
    expect(Math.abs(btn.y - h1.y)).toBeLessThan(80);
  });

  test("workstreams-12: plan controls Combine and Change execution policy are offered", async ({ page }) => {
    await openApp(page, "#/workstreams", "populated", "desktop");
    await expect(page.getByRole("button", { name: /^Combine/ })).toBeVisible();
    await expect(page.getByRole("button", { name: /Change execution policy/ })).toBeVisible();
  });

  test("workstreams-14: page eyebrow 'Continuing initiatives' sits above the title", async ({ page }) => {
    await openApp(page, "#/workstreams", "populated", "desktop");
    const eyebrow = page.getByText("Continuing initiatives", { exact: false }).first();
    await expect(eyebrow).toBeVisible();
    const e = await box(eyebrow);
    const h1 = await box(page.getByRole("heading", { level: 1 }));
    expect(e.y + e.height).toBeLessThanOrEqual(h1.y + 1);
  });
});

// ---------------------------------------------------------------------------
// RC-11 Workstream detail panels and task table columns
// ---------------------------------------------------------------------------
test.describe("RC-11 workstream detail panels and task table", () => {
  test("workstreams-09: Dependency and resource detail panel is present", async ({ page }) => {
    await openApp(page, "#/workstreams", "populated", "desktop");
    await expect(page.getByText("Dependency and resource detail")).toBeVisible();
  });

  test("workstreams-13: task table has one 'Objective / project' column", async ({ page }) => {
    await openApp(page, "#/workstreams", "populated", "desktop");
    const headers = await page.getByRole("columnheader").allTextContents();
    expect(headers.some((h) => /objective\s*\/\s*project/i.test(h))).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// RC-12 Agents & Sessions control placement and action set
// ---------------------------------------------------------------------------
test.describe("RC-12 agents control placement and action set", () => {
  test("agents-04: Cancel attempt sits under Session controls", async ({ page }) => {
    await openApp(page, "#/agents", "populated", "desktop");
    await page.getByRole("button", { name: RUNNING_TASK }).click();
    await expect(page.getByRole("region", { name: "Session controls" }).getByRole("button", { name: /cancel/i })).toBeVisible();
  });

  test("agents-05: task and feed actions are offered (disabled with reason if unsupported)", async ({ page }) => {
    await openApp(page, "#/agents", "populated", "desktop");
    await page.getByRole("button", { name: RUNNING_TASK }).click();
    await expect(page.getByRole("button", { name: /reassign/i })).toBeVisible();
    await page.locator("summary", { hasText: "Execution feed, side effects and supplied context" }).click();
    await expect(
      page.getByRole("button", { name: /filter raw trace/i }).or(page.getByRole("combobox", { name: /filter raw trace/i })),
    ).toBeVisible();
    await expect(page.getByRole("button", { name: /request diagnostic/i })).toBeVisible();
    await expect(page.getByRole("button", { name: /branch from retained evidence/i })).toBeVisible();
  });

  test("agents-08: Configure providers is a page-header action", async ({ page }) => {
    await openApp(page, "#/agents", "populated", "desktop");
    const h1 = await box(page.getByRole("heading", { level: 1 }));
    const btn = await box(page.getByRole("button", { name: /Configure providers/ }));
    expect(Math.abs(btn.y - h1.y)).toBeLessThan(60);
  });

  test("agents-12: Resume control reads 'Resume unavailable until capability verified'", async ({ page }) => {
    await openApp(page, "#/agents", "populated", "desktop");
    await page.getByRole("button", { name: RUNNING_TASK }).click();
    await expect(page.getByRole("region", { name: "Session controls" })).toContainText(
      "Resume unavailable until capability verified",
    );
  });
});

// ---------------------------------------------------------------------------
// RC-13 Agents assigned-work list: session counts, filters, row facts, Available / idle tab
// ---------------------------------------------------------------------------
test.describe("RC-13 agents assigned-work list and counts", () => {
  test("agents-07: Failed sessions tab counts sessions, and disconnected is not shown as 0", async ({ page }) => {
    await openApp(page, "#/agents", "populated", "desktop");
    await expect(page.getByRole("tab", { name: /^Failed sessions/ })).toBeVisible();
    await expect(page.getByText(/0 disconnected sessions/)).toHaveCount(0);
  });

  test("agents-09: Assigned work and sessions offers filter controls", async ({ page }) => {
    await openApp(page, "#/agents", "populated", "desktop");
    const card = page.getByRole("region", { name: "Assigned work and sessions" });
    const controls = card
      .getByRole("combobox")
      .or(card.getByRole("searchbox"))
      .or(card.getByRole("button", { name: /filter/i }));
    expect(await controls.count()).toBeGreaterThan(0);
  });

  test("agents-13: assigned-work rows show the attempt number", async ({ page }) => {
    await openApp(page, "#/agents", "populated", "desktop");
    await expect(page.getByRole("button", { name: RUNNING_TASK })).toContainText(/attempt \d+/i);
  });

  test("agents-14: Available tab is labelled Available / idle", async ({ page }) => {
    await openApp(page, "#/agents", "populated", "desktop");
    await expect(page.getByRole("tab", { name: /^Available \/ idle/ })).toBeVisible();
  });
});

// ---------------------------------------------------------------------------
// RC-14 Agents default state and desktop column split
// ---------------------------------------------------------------------------
test.describe("RC-14 agents default state and desktop split", () => {
  test("agents-02: configured inventory is collapsed by default", async ({ page }) => {
    await openApp(page, "#/agents", "populated", "desktop");
    expect(await detailsOpen(page, /Configured inventory/)).toBe(false);
  });

  test("agents-03: populated desktop selects a task by default", async ({ page }) => {
    await openApp(page, "#/agents", "populated", "desktop");
    await expect(page.getByRole("complementary", { name: AGENT_DETAIL })).not.toContainText("No task selected");
  });

  test("agents-11: detail column is about 545px wide at desktop", async ({ page }) => {
    await openApp(page, "#/agents", "populated", "desktop");
    const w = (await box(page.getByRole("complementary", { name: AGENT_DETAIL }))).width;
    expect(Math.abs(w - 545)).toBeLessThanOrEqual(20);
  });
});

// ---------------------------------------------------------------------------
// RC-15 Agents selected detail: TaskContract and per-attempt facts
// ---------------------------------------------------------------------------
test.describe("RC-15 agents selected detail facts", () => {
  // Blocked-on-data: the base revision field is not in the accepted OX-G5 mapping (heartbeat is
  // listed as missing there). The assertion is the auditor's claim; it fails on main and needs a
  // daemon field to be satisfied for real.
  test("agents-06: selected detail states the workspace base revision [blocked-on-data: base revision field]", async ({ page }) => {
    await openApp(page, "#/agents", "populated", "desktop");
    await page.getByRole("button", { name: RUNNING_TASK }).click();
    await expect(page.getByRole("complementary", { name: AGENT_DETAIL })).toContainText(/base revision/i);
  });
});

// ---------------------------------------------------------------------------
// RC-16 Global review queue layout: view-level tab bar and queue width
// ---------------------------------------------------------------------------
test.describe("RC-16 global review queue layout", () => {
  test("review-07: filter tabs are a view-level tab bar above the decision queue", async ({ page }) => {
    await openApp(page, "#/review", "populated", "desktop");
    const tabs = await box(page.getByRole("tablist").first());
    const queue = await box(page.getByRole("region", { name: "Decision queue" }));
    expect(tabs.y).toBeLessThan(queue.y);
  });

  test("review-08: decision queue column is wide enough for one row of six tabs (about 560px)", async ({ page }) => {
    await openApp(page, "#/review", "populated", "desktop");
    const queue = await box(page.getByRole("region", { name: "Decision queue" }));
    expect(queue.width).toBeGreaterThanOrEqual(500);
  });
});

// ---------------------------------------------------------------------------
// RC-17 Decision queue content and row actions
// ---------------------------------------------------------------------------
test.describe("RC-17 decision queue grouping receipt and row actions", () => {
  test("review-and-app-only-06: queue shows the grouping receipt (groups / raw requests / independent decisions) [blocked-on-data: grouping key]", async ({ page }) => {
    await openApp(page, "#/review", "populated", "desktop");
    await expect(page.getByText(/\d+ groups? \/ \d+ raw requests? \/ \d+ independent decisions?/)).toBeVisible();
  });

  test("review-and-app-only-09: each queue row offers its primary action", async ({ page }) => {
    await openApp(page, "#/review", "populated", "desktop");
    const queue = page.getByRole("region", { name: "Decision queue" });
    await expect(
      queue.getByRole("button", { name: /^(Compare proposals|Inspect attempts|Review exact action)$/ }).first(),
    ).toBeVisible();
  });
});

// ---------------------------------------------------------------------------
// RC-18 Candidate panel: identity header and action placement
// ---------------------------------------------------------------------------
test.describe("RC-18 candidate panel identity and decision actions", () => {
  test("review-and-app-only-03: candidate identity names the project and workstream", async ({ page }) => {
    await openApp(page, "#/review", "populated", "desktop");
    const id = page.getByRole("region", { name: "Candidate identity" });
    await expect(id).toContainText(/Project/);
    await expect(id).toContainText(/Workstream/);
  });

  test("review-and-app-only-05: Request additional checks is visible beside Request changes", async ({ page }) => {
    await openApp(page, "#/review", "populated", "desktop");
    await expect(page.getByRole("button", { name: "Request additional checks" })).toBeVisible();
  });
});

// ---------------------------------------------------------------------------
// RC-19 Daily usage pill: window label agrees with the 'Today's AI usage' heading
// ---------------------------------------------------------------------------
test.describe("RC-19 daily usage pill window label", () => {
  test("workspace-04: usage popover window does not contradict 'Today's AI usage'", async ({ page }) => {
    await openApp(page, "#/workspace", "populated", "desktop");
    const usage = page.locator('details:has(summary[aria-label="More information about usage"])');
    expect(await usage.textContent()).not.toMatch(/since Jan 1/);
  });
});

// ---------------------------------------------------------------------------
// RC-20 Primary navigation: exactly four destinations
// ---------------------------------------------------------------------------
test.describe("RC-20 primary navigation has exactly four destinations", () => {
  test("workspace-17: sidebar has no Projects destination", async ({ page }) => {
    await openApp(page, "#/workspace", "populated", "desktop");
    await expect(page.getByRole("link", { name: "Projects", exact: true })).toHaveCount(0);
  });

  test("review-and-app-only-16: Projects is not a fifth primary destination on the projects route", async ({ page }) => {
    await openApp(page, "#/projects", "populated", "desktop");
    await expect(page.getByRole("navigation").getByRole("link", { name: "Projects", exact: true })).toHaveCount(0);
  });
});

// ---------------------------------------------------------------------------
// RC-21 Workspace conversation: scope line, composer placeholder, coordinator chips, target label
// ---------------------------------------------------------------------------
test.describe("RC-21 workspace conversation copy, chips and composer", () => {
  test("workspace-11: coordinator conversations offer Request review as an enabled navigation action", async ({ page }) => {
    await openApp(page, "#/workspace", "populated", "desktop");
    await page.getByRole("button", { name: /Choose conversation target/ }).click();
    await page.getByRole("radio", { name: /Auth hardening coordinator/ }).check();
    await expect(page.getByRole("button", { name: "Request review" })).toBeEnabled();
  });

  test("workspace-12: Global scope line reads 'All workstreams · Cross-workstream coordination'", async ({ page }) => {
    await openApp(page, "#/workspace", "populated", "desktop");
    await expect(page.getByText("All workstreams · Cross-workstream coordination", { exact: true })).toBeVisible();
  });

  test("workspace-13: composer placeholder matches the wireframe", async ({ page }) => {
    await openApp(page, "#/workspace", "populated", "desktop");
    await expect(page.getByRole("textbox", { name: /^Message / }).first()).toHaveAttribute(
      "placeholder",
      "Ask a question, redirect work, or discuss the plan…",
    );
  });

  test("workspace-14: target chip label is not clipped", async ({ page }) => {
    await openApp(page, "#/workspace", "populated", "desktop");
    const label = page.getByRole("button", { name: /Choose conversation target/ }).locator("span").first();
    expect(await label.evaluate((el) => el.scrollWidth > el.clientWidth)).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// RC-22 Shell brand and top-bar search sizing
// ---------------------------------------------------------------------------
test.describe("RC-22 shell brand and top-bar search sizing", () => {
  test("workspace-02: brand wordmark is about 30px", async ({ page }) => {
    await openApp(page, "#/workspace", "populated", "desktop");
    const size = await page
      .getByText("MCAO", { exact: true })
      .first()
      .evaluate((el) => parseFloat(getComputedStyle(el).fontSize));
    expect(size).toBeGreaterThanOrEqual(28);
  });

  test("workspace-03: top-bar search is wide (about 680px) as in the wireframe", async ({ page }) => {
    await openApp(page, "#/workspace", "populated", "desktop");
    const search = page.getByRole("combobox", { name: /^Search/ }).first();
    expect((await box(search)).width).toBeGreaterThan(600);
  });
});

// ---------------------------------------------------------------------------
// RC-23 Sidebar workstream tree row markup and names
// ---------------------------------------------------------------------------
test.describe("RC-23 sidebar tree row markup and names", () => {
  test("workstreams-17: sidebar tree rows show no disclosure chevron", async ({ page }) => {
    await openApp(page, "#/workstreams", "populated", "desktop");
    await expect(page.locator(".ws-tree-chevron")).toHaveCount(0);
  });

  test("workstreams-18: sidebar tree region is named as in the wireframe", async ({ page }) => {
    await openApp(page, "#/workstreams", "populated", "desktop");
    await expect(page.getByRole("navigation", { name: "Workstreams and individual workers" })).toBeVisible();
  });

  test("workstreams-19: worker rows carry the green bullet marker", async ({ page }) => {
    await openApp(page, "#/workstreams", "populated", "desktop");
    const content = await page.locator(".ws-tree-task-name").first().evaluate((el) => getComputedStyle(el, "::before").content);
    expect(content).not.toBe("none");
  });
});
