import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./e2e",
  testMatch: "**/geometry.spec.ts",
  use: {
    browserName: "chromium",
    channel: "chrome",
    viewport: { width: 960, height: 900 },
  },
  webServer: {
    command: "npm run dev -- --host 127.0.0.1 --port 0",
    wait: {
      stdout: /http:\/\/127\.0\.0\.1:(?<vite_port>\d+)\//,
    },
    stdout: "pipe",
    reuseExistingServer: false,
    timeout: 30_000,
  },
});
