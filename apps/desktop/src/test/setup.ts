import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

// A saved choice (DPES-1105) lives in the browser storage, which outlives a test: clear it so no test starts with another's choice.
// Node-environment tests have no window, so only DOM tests have storage to clear.
afterEach(() => {
  cleanup();
  if (typeof window !== "undefined") window.localStorage.clear();
});
