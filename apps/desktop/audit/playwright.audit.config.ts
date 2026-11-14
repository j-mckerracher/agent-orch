import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "@playwright/test";

const AUDIT = dirname(fileURLToPath(import.meta.url));
const DESKTOP = resolve(AUDIT, "..");

// Wireframe conformance audit. Run from apps/desktop:
//   WIREFRAME=/abs/path/to/wireframes.html npx playwright test -c audit/playwright.audit.config.ts
// Environment:
//   AUDIT_APP_DIR  apps/desktop directory to serve (default: this checkout). Point it at a fix
//                  branch's apps/desktop to run the same checks against that branch.
//   AUDIT_LABEL    suffix for the JSON results file (out/results-<label>.json).
//   AUDIT_CAPTURE  1 runs audit/capture.spec.ts instead of the checks.
// Captures land in audit/out/ (screenshots, ARIA snapshots, layout JSON).
const label = process.env.AUDIT_LABEL ? `-${process.env.AUDIT_LABEL}` : "";

export default defineConfig({
  testDir: ".",
  testMatch: ["capture.spec.ts", "checks.spec.ts"],
  outputDir: "out/test-results",
  workers: 4,
  reporter: [["list"], ["json", { outputFile: `out/results${label}.json` }]],
  use: { browserName: "chromium", channel: "chrome", colorScheme: "dark" },
  webServer: {
    command: "npx vite --host 127.0.0.1 --port 0",
    cwd: process.env.AUDIT_APP_DIR ? resolve(process.env.AUDIT_APP_DIR) : DESKTOP,
    wait: { stdout: /http:\/\/127\.0\.0\.1:(?<audit_port>\d+)\// },
    stdout: "pipe",
    reuseExistingServer: false,
    timeout: 60_000,
  },
});
