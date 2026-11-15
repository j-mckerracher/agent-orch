// Evidence capture for DPES-1105: the usage pill and its popover, dark theme, 1440x900, fixture data only.
// Runs only when EVIDENCE_DIR is set, e.g.
//   EVIDENCE_DIR=/abs/path/docs/evidence/DPES-1105-ai-usage npx playwright test -c audit/playwright.audit.config.ts audit/local-tools-evidence.spec.ts
// Writes pill-<state>.png (the pill with its chip) and popover-<state>.png (the popover open).
import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { expect, test } from "@playwright/test";
import { openApp } from "./harness";
import type { LocalToolsFixtureState } from "../src/sources/surfaces/fixtures";

const DIR = process.env.EVIDENCE_DIR;
test.skip(!DIR, "set EVIDENCE_DIR to write the local-tools screenshots");

const STATES: Array<{ slug: string; fixture: LocalToolsFixtureState }> = [
  { slug: "opted-out", fixture: "off" },
  { slug: "ok-unpriced", fixture: "ok_unpriced" },
  { slug: "not-installed", fixture: "not_installed" },
  { slug: "node-too-old", fixture: "node_too_old" },
  { slug: "unsupported-version", fixture: "unsupported_version" },
  { slug: "integrity-mismatch", fixture: "integrity_mismatch" },
  { slug: "timeout", fixture: "timeout" },
  { slug: "stale", fixture: "stale" },
  { slug: "not-bundled", fixture: "not_bundled" },
];

for (const state of STATES) {
  test(`evidence ${state.slug}`, async ({ page }) => {
    const dir = DIR!;
    mkdirSync(dir, { recursive: true });
    await openApp(page, "#/workspace", "populated", "desktop", state.fixture);

    const pill = page.getByTestId("top-usage");
    await pill.screenshot({ path: join(dir, `pill-${state.slug}.png`) });

    await page.locator('summary[aria-label="More information about usage"]').click();
    await expect(page.getByRole("region", { name: "Other AI tools on this machine" })).toBeVisible();
    await page.waitForTimeout(200);

    // The popover is absolutely positioned, so frame the pill and the popover together.
    const boxes = [await pill.boundingBox(), await page.getByTestId("top-usage-info").boundingBox()];
    const left = Math.min(...boxes.map((b) => b!.x)) - 12;
    const top = Math.min(...boxes.map((b) => b!.y)) - 12;
    const right = Math.max(...boxes.map((b) => b!.x + b!.width)) + 12;
    const bottom = Math.max(...boxes.map((b) => b!.y + b!.height)) + 12;
    await page.screenshot({
      path: join(dir, `popover-${state.slug}.png`),
      fullPage: true,
      clip: { x: left, y: top, width: right - left, height: bottom - top },
    });
  });
}
