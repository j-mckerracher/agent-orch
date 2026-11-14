import { defineConfig } from "vitest/config";

// Real-daemon e2e: needs ORCH_DAEMON_BIN. Unix sockets only (see e2e/harness.ts).
export default defineConfig({
  test: {
    environment: "node",
    include: ["e2e/**/*.e2e.test.{ts,tsx}"],
    fileParallelism: false,
    testTimeout: 30_000,
    hookTimeout: 30_000,
  },
});
