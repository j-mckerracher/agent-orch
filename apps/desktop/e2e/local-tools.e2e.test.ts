import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { ConnectionManager } from "../src/daemon/connection";
import { createLiveLocalToolsSource } from "../src/sources/surfaces/localToolsLive";
import type { LocalDay } from "../src/sources/surfaces/usageLive";
import type { LocalToolsSource } from "../src/sources/surfaces/types";
import { NodeConnector, startDaemon, type RunningDaemon } from "./harness";

/** A UTC day that brackets "now", so the test does not depend on the host's midnight. */
function aroundNow(): LocalDay {
  const now = Date.now();
  return { timezone: "UTC", utcOffsetSeconds: 0, startMs: now - 3_600_000, endMs: now + 3_600_000 };
}

/**
 * The daemon runs with an empty home (no provider session files) and a codeburn override that does
 * not exist, so the reader is missing on every machine: `not_installed` in a debug build, `not_bundled`
 * in a release build. The test reads no real usage and needs no codeburn installed.
 */
const MISSING_READER = ["not_installed", "not_bundled"];

let home: string;
let daemon: RunningDaemon;
let manager: ConnectionManager;
let source: LocalToolsSource;

beforeAll(async () => {
  home = mkdtempSync(join(tmpdir(), "mcao-home-"));
  daemon = await startDaemon({ HOME: home, AGENT_ORCH_CODEBURN_BIN: join(home, "no-such-codeburn") });
  manager = new ConnectionManager({ connector: new NodeConnector(daemon.dataDir), backoffMs: [50], requestTimeoutMs: 5000 });
  manager.start();
  const deadline = Date.now() + 10_000;
  while (manager.getSnapshot().status !== "connected") {
    if (Date.now() > deadline) throw new Error("daemon did not connect");
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  source = createLiveLocalToolsSource(manager, Date.now, aroundNow);
}, 30_000);

afterAll(async () => {
  manager?.stop();
  await daemon?.stop();
  if (home) rmSync(home, { recursive: true, force: true });
});

describe("other AI tools against the real daemon", () => {
  it("is off by default and reads nothing", async () => {
    const result = await source.local();
    expect(result.status).toBe("ok");
    if (result.status !== "ok") return;
    expect(result.data.enabled).toBe(false);
    expect(result.data.reader.state).toBe("off");
  });

  it("reports a missing reader at once when turned on, and clears it when turned off", async () => {
    const on = await source.setEnabled(true);
    expect(on.status).toBe("ok");
    if (on.status !== "ok") return;
    expect(on.data.enabled).toBe(true);
    // Not `not_started`: a reader that cannot exist is named straight away, with no figures.
    expect(MISSING_READER).toContain(on.data.reader.state);
    expect(on.data.observedAtMs).toBeNull();

    const off = await source.setEnabled(false);
    expect(off.status).toBe("ok");
    if (off.status !== "ok") return;
    expect(off.data.enabled).toBe(false);
    expect(off.data.reader.state).toBe("off");
    expect(off.data.observedAtMs).toBeNull();
  });

  it("answers the headline summary with zero sessions as unknown cost, never zero", async () => {
    const raw = (await manager.api().usageSummary({ timezone: "UTC", utc_offset_seconds: 0 })) as {
      totals: { sessions: number; cost_status: string; reported: { costs: unknown[] } };
    };
    expect(raw.totals.sessions).toBe(0);
    expect(raw.totals.cost_status).toBe("unknown");
    expect(raw.totals.reported.costs).toEqual([]);
  });
});
