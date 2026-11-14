// Capture phase: screenshots + ARIA snapshots + layout maps for every inventory row.
// Runs only when AUDIT_CAPTURE=1 so the check phase can run alone.
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { test, type Page } from "@playwright/test";
import { BREAKPOINTS, OUT, SCREENS, layoutMap, openApp, openWireframe, type Bp, type Scenario } from "./harness";

test.skip(process.env.AUDIT_CAPTURE !== "1", "set AUDIT_CAPTURE=1 to capture");

async function capture(page: Page, side: "app" | "wf", id: string) {
  const dir = join(OUT, side);
  mkdirSync(dir, { recursive: true });
  await page.screenshot({ path: join(dir, `${id}.png`), fullPage: true });
  writeFileSync(join(dir, `${id}.aria.yml`), await page.locator("body").ariaSnapshot());
  writeFileSync(join(dir, `${id}.layout.json`), JSON.stringify(await layoutMap(page), null, 1));
}

const bps = Object.keys(BREAKPOINTS) as Bp[];

for (const s of SCREENS) {
  for (const bp of bps) {
    test(`wf ${s.key} ${bp}`, async ({ page }) => {
      await openWireframe(page, s.wf, bp);
      await capture(page, "wf", `${s.key}.${bp}`);
    });
    test(`app ${s.key} populated ${bp}`, async ({ page }) => {
      await openApp(page, s.hash, "populated", bp);
      await capture(page, "app", `${s.key}.populated.${bp}`);
    });
  }
  for (const sc of ["empty", "loading", "offline"] as Scenario[]) {
    test(`app ${s.key} ${sc}`, async ({ page }) => {
      await openApp(page, s.hash, sc, "desktop");
      await capture(page, "app", `${s.key}.${sc}.desktop`);
    });
  }
}

// App-only routes (no wireframe screen): structure + offline/error state.
const APP_ONLY = [
  { key: "projects", hash: "#/projects" },
  { key: "project", hash: "#/project/fixture-project" },
  { key: "tasks", hash: "#/project/fixture-project/tasks/all" },
  { key: "task", hash: "#/project/fixture-project/task/fixture-task-1" },
  { key: "preview", hash: "#/project/fixture-project/preview/fixture%3Atask-running" },
  { key: "coordinator", hash: "#/project/fixture-project/coordinator" },
  { key: "project-review", hash: "#/project/fixture-project/review/fixture-task-1?fixture=1" },
];
for (const r of APP_ONLY) {
  for (const sc of ["populated", "offline"] as Scenario[]) {
    test(`app ${r.key} ${sc}`, async ({ page }) => {
      await openApp(page, r.hash, sc, "desktop");
      await capture(page, "app", `${r.key}.${sc}.desktop`);
    });
  }
}

// Interaction states on the workspace.
test("wf workspace inspector-open", async ({ page }) => {
  await openWireframe(page, "command-center");
  await page.locator(".b-inspector > summary, .b-inspector summary").first().click();
  await capture(page, "wf", "workspace.inspector-open.desktop");
});
test("wf workspace target-picker no-match", async ({ page }) => {
  await openWireframe(page, "command-center");
  await page.getByText("Choose conversation target").first().click();
  await page.locator("#target-search").fill("zzzz-no-match");
  await capture(page, "wf", "workspace.target-nomatch.desktop");
});
test("app workspace inspector-toggled", async ({ page }) => {
  await openApp(page, "#/workspace");
  await page.getByText("Workspace activity", { exact: false }).first().click();
  await capture(page, "app", "workspace.inspector-toggled.desktop");
});
test("app workspace target-picker no-match", async ({ page }) => {
  await openApp(page, "#/workspace");
  await page.getByRole("button", { name: /Choose conversation target/ }).click();
  const search = page.getByRole("searchbox").or(page.getByPlaceholder(/search/i)).last();
  if (await search.count()) await search.fill("zzzz-no-match");
  await capture(page, "app", "workspace.target-nomatch.desktop");
});
test("app workspace light spot-check", async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem("agent-orch.theme", "light"));
  await page.setViewportSize(BREAKPOINTS.desktop);
  await page.goto(`${process.env.AUDIT_PORT ? `http://127.0.0.1:${process.env.AUDIT_PORT}` : ""}/?fixture=1#/workspace`);
  await page.waitForTimeout(1200);
  await capture(page, "app", "workspace.light.desktop");
});
