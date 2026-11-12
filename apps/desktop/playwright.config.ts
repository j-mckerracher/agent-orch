import { defineConfig } from "@playwright/test";

const baseURL = "http://127.0.0.1:1421";

export default defineConfig({
  testDir: "./e2e",
  testMatch: "**/geometry.spec.ts",
  use: {
    baseURL,
    browserName: "chromium",
    viewport: { width: 960, height: 900 },
  },
  webServer: {
    command: "npm run dev -- --host 127.0.0.1 --port 1421",
    url: baseURL,
    reuseExistingServer: false,
    timeout: 30_000,
  },
});
