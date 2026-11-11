import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { ConnectionManager } from "../src/daemon/connection";
import { DaemonSession } from "../src/daemon/session";
import { M5_03_CAPABILITIES } from "../src/daemon/types";
import { createLiveMetrics } from "../src/sources/surfaces/liveMetrics";
import type { Measured, MeterValue, SurfaceResult, SystemMetrics } from "../src/sources/surfaces/types";
import { NodeConnector, startDaemon, type RunningDaemon } from "./harness";

let daemon: RunningDaemon;
let connector: NodeConnector;

beforeAll(async () => {
  daemon = await startDaemon();
  connector = new NodeConnector(daemon.dataDir);
});

afterAll(async () => {
  await daemon?.stop();
});

async function until<T>(read: () => Promise<T | undefined | false>, label: string, ms = 15_000): Promise<T> {
  const deadline = Date.now() + ms;
  for (;;) {
    const value = await read();
    if (value) return value;
    if (Date.now() > deadline) throw new Error(`timed out waiting for ${label}`);
    await new Promise((r) => setTimeout(r, 250));
  }
}

async function connected(): Promise<ConnectionManager> {
  const manager = new ConnectionManager({ connector, backoffMs: [50], requestTimeoutMs: 5000 });
  manager.start();
  await until(async () => manager.getSnapshot().status === "connected", "connected");
  return manager;
}

const measuredPercent = (meter: Measured<MeterValue>): number => {
  if (meter.status !== "ok") throw new Error(`expected a measured meter, got ${meter.status}`);
  expect(meter.value.unit).toBe("%");
  expect(meter.ageMs).toBeGreaterThanOrEqual(0);
  return meter.value.value;
};

describe("real daemon: host.metrics through the live adapter", () => {
  it("negotiates the host capability", async () => {
    const session = new DaemonSession(await connector.open(M5_03_CAPABILITIES));
    expect(session.welcome.capabilities).toContain("host");
    session.close();
  });

  it("reports measured CPU, memory and disk once the first CPU sample has aged, with GPU never zero-filled", async () => {
    const manager = await connected();
    const source = createLiveMetrics(manager);
    try {
      const result = await until<SurfaceResult<SystemMetrics>>(async () => {
        const next = await source.metrics();
        return next.status === "ok" && next.data.cpu.status === "ok" ? next : false;
      }, "a measured CPU reading");

      if (result.status !== "ok") throw new Error("unreachable");
      expect(result.provenance).toMatchObject({ kind: "daemon", id: "D13" });
      for (const meter of [result.data.cpu, result.data.memory, result.data.disk]) {
        const percent = measuredPercent(meter);
        expect(percent).toBeGreaterThanOrEqual(0);
        expect(percent).toBeLessThanOrEqual(100);
      }
      // Memory and disk keep their absolute figures in the label, so the percentage is not the whole story.
      expect(result.data.memory.status === "ok" && result.data.memory.value.label).toMatch(/GB of .* GB/);
      expect(result.data.disk.status === "ok" && result.data.disk.value.label).toMatch(/GB of .* GB/);

      const gpu = result.data.gpu;
      if (gpu.status === "ok") {
        expect(Number.isFinite(gpu.value.value)).toBe(true);
      } else {
        expect(["unsupported", "unavailable", "unknown"]).toContain(gpu.status);
        expect(gpu).toHaveProperty("reason");
      }
    } finally {
      manager.stop();
    }
  });

  it("delivers repeated readings through watch at the daemon's interval", async () => {
    const manager = await connected();
    const source = createLiveMetrics(manager);
    const results: Array<SurfaceResult<SystemMetrics>> = [];
    const stop = source.watch!((result) => results.push(result));
    try {
      await until(async () => results.length >= 2, "two watched readings", 20_000);
      expect(results.every((result) => result.status !== "unknown")).toBe(true);
    } finally {
      stop();
      manager.stop();
    }
  });

  it("is unsupported, not zero, when the session did not negotiate host", async () => {
    const session = new DaemonSession(await connector.open(["health"]));
    await expect(session.request("host.metrics")).rejects.toMatchObject({ kind: "capability-not-negotiated" });
    session.close();
  });
});
