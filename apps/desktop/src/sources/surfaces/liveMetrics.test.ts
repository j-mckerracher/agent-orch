import { afterEach, describe, expect, it } from "vitest";
import { ConnectionManager } from "../../daemon/connection";
import type { HostMeterWire, HostMetricsWire } from "../../daemon/hostMetrics";
import { ALL_CAPABILITIES } from "../../daemon/types";
import { FakeDaemon, until } from "../../test/fakeDaemon";
import { createLiveMetrics, mapHostMetrics, pollIntervalMs, type Visibility } from "./liveMetrics";
import { HOST_METRICS_WIRE, NOW, clock } from "./testkit";
import type { Measured, MeterValue, SurfaceResult, SystemMetrics } from "./types";

const OBSERVED = "2026-01-01T11:59:58.000Z";

function meter(overrides: Partial<HostMeterWire>): HostMeterWire {
  return {
    meter: "cpu",
    state: "ok",
    kind: "utilization",
    unit: "percent",
    scope: "all logical CPUs",
    value: 10,
    total: null,
    observed_at: OBSERVED,
    sample_interval_ms: 2000,
    reason: null,
    ...overrides,
  };
}

const unavailable = (resource: string, reason: string): HostMeterWire =>
  meter({ meter: resource, state: "unavailable", scope: null, value: null, observed_at: null, reason });

const wire = (meters: HostMeterWire[], extra: Partial<HostMetricsWire> = {}): HostMetricsWire => ({
  schema_version: 1,
  sample_interval_ms: 2000,
  meters,
  ...extra,
});

function data(result: SurfaceResult<SystemMetrics>): SystemMetrics {
  if (result.status !== "ok") throw new Error(`expected ok, got ${result.status}`);
  return result.data;
}

const okValue = (measured: Measured<MeterValue>): MeterValue => {
  if (measured.status !== "ok") throw new Error(`expected ok, got ${measured.status}`);
  return measured.value;
};

describe("mapHostMetrics", () => {
  it("maps a full snapshot, ages each meter from observed_at, and keeps GPU unsupported", () => {
    const result = mapHostMetrics(HOST_METRICS_WIRE, NOW);
    expect(result).toMatchObject({ status: "ok", provenance: { kind: "daemon", id: "D13", methods: ["host.metrics"] }, ageMs: 2000 });
    const mapped = data(result);
    expect(mapped.cpu).toMatchObject({ status: "ok", ageMs: 2000, observedAt: OBSERVED, value: { value: 12.5, unit: "%", kind: "utilization", label: "all logical CPUs" } });
    expect(okValue(mapped.memory)).toMatchObject({ value: 25, unit: "%", kind: "capacity" });
    expect(okValue(mapped.memory).label).toBe("physical memory · 8 GB of 32 GB");
    expect(okValue(mapped.disk)).toMatchObject({ value: 20, unit: "%", kind: "capacity" });
    expect(okValue(mapped.disk).label).toBe("/ · 100 GB of 500 GB");
    expect(mapped.gpu).toEqual({ status: "unsupported", reason: "No vetted GPU source on this platform." });
  });

  it("keeps unavailable and unsupported states as text with the daemon's reason, never a number", () => {
    const mapped = data(
      mapHostMetrics(
        wire([
          unavailable("cpu", "no sample has completed yet"),
          meter({ meter: "memory", state: "unsupported", kind: "capacity", unit: "bytes", scope: null, value: null, observed_at: null, reason: "no source" }),
          unavailable("disk", "stale"),
          unavailable("gpu", "no device"),
        ]),
        NOW,
      ),
    );
    expect(mapped.cpu).toEqual({ status: "unavailable", reason: "no sample has completed yet" });
    expect(mapped.memory).toEqual({ status: "unsupported", reason: "no source" });
    expect(mapped.disk).toEqual({ status: "unavailable", reason: "stale" });
    expect(mapped.gpu).toEqual({ status: "unavailable", reason: "no device" });
    for (const item of Object.values(mapped)) expect("value" in item).toBe(false);
  });

  it("uses the fetch time as the result's observation when no meter is measured", () => {
    const result = mapHostMetrics(wire([unavailable("cpu", "warming"), unavailable("memory", "x"), unavailable("disk", "x"), unavailable("gpu", "x")]), NOW);
    expect(result).toMatchObject({ status: "ok", ageMs: 0, observedAt: new Date(NOW).toISOString() });
  });

  it("uses the oldest measured meter as the result's observation", () => {
    const result = mapHostMetrics(
      wire([
        meter({ observed_at: "2026-01-01T11:59:59.000Z" }),
        meter({ meter: "memory", kind: "capacity", unit: "bytes", value: 1e9, total: 2e9, observed_at: "2026-01-01T11:59:50.000Z" }),
        unavailable("disk", "x"),
        unavailable("gpu", "x"),
      ]),
      NOW,
    );
    expect(result).toMatchObject({ status: "ok", ageMs: 10000 });
  });

  it("clamps a future observation time to age 0", () => {
    const mapped = data(mapHostMetrics(wire([meter({ observed_at: "2026-01-01T12:00:05.000Z" })]), NOW));
    expect(mapped.cpu).toMatchObject({ status: "ok", ageMs: 0 });
  });

  it("reports unknown, not a number, for readings it cannot trust", () => {
    const bad = (overrides: Partial<HostMeterWire>) => mapHostMetrics(wire([meter(overrides)]), NOW);
    expect(data(bad({ value: null })).cpu).toMatchObject({ status: "unknown" });
    expect(data(bad({ value: Number.NaN })).cpu).toMatchObject({ status: "unknown" });
    expect(data(bad({ value: -1 })).cpu).toMatchObject({ status: "unknown" });
    expect(data(bad({ value: 140 })).cpu).toMatchObject({ status: "unknown", reason: expect.stringContaining("outside 0-100%") });
    expect(data(bad({ observed_at: null })).cpu).toMatchObject({ status: "unknown" });
    expect(data(bad({ observed_at: "not a time" })).cpu).toMatchObject({ status: "unknown" });
    expect(data(bad({ unit: "furlongs" })).cpu).toMatchObject({ status: "unknown" });
    expect(data(bad({ state: "melting" })).cpu).toMatchObject({ status: "unknown" });
    expect(data(bad({ meter: "memory", kind: "capacity", unit: "bytes", value: 5, total: 4 })).memory).toMatchObject({ status: "unknown" });
  });

  it("reports a meter the daemon did not list as unknown", () => {
    const mapped = data(mapHostMetrics(wire([meter({})]), NOW));
    expect(mapped.memory).toMatchObject({ status: "unknown", reason: expect.stringContaining("no memory meter") });
    expect(mapped.gpu).toMatchObject({ status: "unknown", reason: expect.stringContaining("no gpu meter") });
  });

  it("shows absolute GB with no scale when a capacity meter has no total", () => {
    const mapped = data(mapHostMetrics(wire([meter({ meter: "memory", kind: "capacity", unit: "bytes", value: 8e9, total: null })]), NOW));
    expect(okValue(mapped.memory)).toMatchObject({ value: 8, unit: "GB", kind: "capacity" });
  });

  it("never turns a zero total into a percentage", () => {
    const mapped = data(mapHostMetrics(wire([meter({ meter: "disk", kind: "capacity", unit: "bytes", value: 5, total: 0 })]), NOW));
    expect(okValue(mapped.disk).unit).toBe("GB");
  });

  it("keeps a real zero as a measured zero", () => {
    const mapped = data(mapHostMetrics(wire([meter({ value: 0 })]), NOW));
    expect(okValue(mapped.cpu).value).toBe(0);
  });

  describe("GPU aggregation", () => {
    const gpu = (scope: string, value: number) => meter({ meter: "gpu", scope, value });
    const gpuOf = (...gpus: HostMeterWire[]) => data(mapHostMetrics(wire(gpus), NOW)).gpu;

    it("shows the busiest measured device and says how many were reported", () => {
      const result = gpuOf(gpu("GPU A", 10), gpu("GPU B", 70), unavailable("gpu", "x"));
      expect(okValue(result)).toMatchObject({ value: 70, label: "GPU B (busiest of 3 GPUs)" });
    });

    it("keeps a single device's own label", () => {
      expect(okValue(gpuOf(gpu("Apple M3 Pro", 48))).label).toBe("Apple M3 Pro");
    });

    it("prefers unavailable over unsupported when no device is measured", () => {
      const unsupported = meter({ meter: "gpu", state: "unsupported", scope: null, value: null, observed_at: null, reason: "no source" });
      expect(gpuOf(unsupported, unavailable("gpu", "read failed"))).toEqual({ status: "unavailable", reason: "read failed" });
    });
  });

  it("answers unsupported for a schema it does not know and unknown for a malformed result", () => {
    expect(mapHostMetrics(wire([], { schema_version: 2 }), NOW)).toMatchObject({ status: "unsupported", reason: expect.stringContaining("schema 2") });
    expect(mapHostMetrics({} as HostMetricsWire, NOW)).toMatchObject({ status: "unknown" });
    expect(mapHostMetrics(null as unknown as HostMetricsWire, NOW)).toMatchObject({ status: "unknown" });
  });
});

describe("pollIntervalMs", () => {
  it("follows the contract's interval, floored at the daemon's sampling floor", () => {
    expect(pollIntervalMs({ sample_interval_ms: 2000 })).toBe(2000);
    expect(pollIntervalMs({ sample_interval_ms: 5000 })).toBe(5000);
    expect(pollIntervalMs({ sample_interval_ms: 100 })).toBe(500);
  });
  it("falls back to 2 s for a missing or invalid interval", () => {
    for (const bad of [undefined, null, {}, { sample_interval_ms: 0 }, { sample_interval_ms: "fast" }, { sample_interval_ms: -5 }]) {
      expect(pollIntervalMs(bad)).toBe(2000);
    }
  });
});

describe("createLiveMetrics against the fake daemon", () => {
  const stops: Array<() => void> = [];
  afterEach(() => {
    while (stops.length) stops.pop()!();
  });

  async function connect(fake: FakeDaemon, start = true) {
    const connection = new ConnectionManager({ connector: fake, backoffMs: [5], requestTimeoutMs: 100 });
    stops.push(() => connection.stop());
    if (start) {
      connection.start();
      await until(() => connection.getSnapshot().status === "connected", "connected");
    }
    return connection;
  }

  it("requests the host capability and calls host.metrics with no params", async () => {
    const fake = new FakeDaemon();
    fake.stub("host.metrics", HOST_METRICS_WIRE);
    const connection = await connect(fake);
    const source = createLiveMetrics(connection, { now: clock });
    const result = await source.metrics();
    expect(result.status).toBe("ok");
    expect(fake.callsTo("host.metrics")).toEqual([{ method: "host.metrics", params: undefined }]);
    expect(connection.getSnapshot().capabilities).toContain("host");
  });

  it("is unsupported when the daemon has no host.metrics, and never falls back to fixtures", async () => {
    const source = createLiveMetrics(await connect(new FakeDaemon()), { now: clock });
    const result = await source.metrics();
    expect(result).toMatchObject({ status: "unsupported", provenance: { kind: "daemon", id: "D13" } });
    expect(JSON.stringify(result)).not.toMatch(/fixture|PLACEHOLDER|GiB/);
  });

  it("is unsupported when the host capability was not negotiated", async () => {
    const fake = new FakeDaemon();
    fake.stub("host.metrics", HOST_METRICS_WIRE);
    fake.capabilitiesOverride = ALL_CAPABILITIES.filter((capability) => capability !== "host");
    const result = await createLiveMetrics(await connect(fake), { now: clock }).metrics();
    expect(result).toMatchObject({ status: "unsupported", reason: expect.stringContaining("capability-not-negotiated") });
  });

  it("is unavailable when disconnected and when the daemon fails", async () => {
    const disconnected = await createLiveMetrics(await connect(new FakeDaemon(), false), { now: clock }).metrics();
    expect(disconnected).toMatchObject({ status: "unavailable", provenance: { kind: "daemon" } });

    const fake = new FakeDaemon();
    fake.stub("host.metrics", HOST_METRICS_WIRE);
    fake.failNext("host.metrics", { code: -32603, message: "boom" });
    const failed = await createLiveMetrics(await connect(fake), { now: clock }).metrics();
    expect(failed).toMatchObject({ status: "unavailable", reason: expect.stringContaining("boom") });
  });

  describe("watch", () => {
    function visibilityStub(initiallyHidden = false) {
      let hidden = initiallyHidden;
      const listeners = new Set<() => void>();
      const visibility: Visibility = {
        hidden: () => hidden,
        subscribe: (listener) => {
          listeners.add(listener);
          return () => listeners.delete(listener);
        },
      };
      return {
        visibility,
        listeners,
        set(next: boolean) {
          hidden = next;
          for (const listener of [...listeners]) listener();
        },
      };
    }

    const hostCalls = (fake: FakeDaemon) => fake.callsTo("host.metrics").length;
    const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

    /** Real timers; the contract's interval is floored at 500 ms, so these tests wait about a second. */
    async function setup(options: { hidden?: boolean } = {}) {
      const fake = new FakeDaemon();
      fake.stub("host.metrics", { ...HOST_METRICS_WIRE, sample_interval_ms: 500 });
      const connection = await connect(fake);
      const vis = visibilityStub(options.hidden);
      const source = createLiveMetrics(connection, { now: clock, visibility: vis.visibility });
      const results: Array<SurfaceResult<SystemMetrics>> = [];
      return { fake, vis, source, results };
    }

    it("reads at once, then at the contract's interval", async () => {
      const { fake, source, results } = await setup();
      const stop = source.watch!((result) => results.push(result));
      await until(() => results.length === 1, "first read");
      await sleep(250);
      expect(hostCalls(fake)).toBe(1);
      await until(() => results.length === 2, "second read", 2000);
      expect(hostCalls(fake)).toBe(2);
      stop();
    });

    it("sends no request while hidden, including at start", async () => {
      const { fake, vis, source, results } = await setup({ hidden: true });
      const stop = source.watch!((result) => results.push(result));
      await sleep(1200);
      expect(hostCalls(fake)).toBe(0);
      expect(results).toHaveLength(0);
      vis.set(false);
      await until(() => results.length === 1, "read after becoming visible");
      stop();
    });

    it("stops polling when hidden and reads again as soon as visible", async () => {
      const { fake, vis, source, results } = await setup();
      const stop = source.watch!((result) => results.push(result));
      await until(() => results.length === 1, "first read");
      vis.set(true);
      await sleep(1200);
      expect(hostCalls(fake)).toBe(1);
      vis.set(false);
      await until(() => results.length === 2, "read after becoming visible");
      stop();
    });

    it("does not deliver or reschedule after stop, and unsubscribes", async () => {
      const { fake, vis, source, results } = await setup();
      const stop = source.watch!((result) => results.push(result));
      await until(() => results.length === 1, "first read");
      stop();
      expect(vis.listeners.size).toBe(0);
      await sleep(1200);
      expect(hostCalls(fake)).toBe(1);
      expect(results).toHaveLength(1);
    });

    it("keeps polling after a failure and reports it as unavailable", async () => {
      const { fake, source, results } = await setup();
      fake.failNext("host.metrics", { code: -32603, message: "boom" });
      const stop = source.watch!((result) => results.push(result));
      await until(() => results.length === 2, "retry after failure", 3000);
      expect(results[0]).toMatchObject({ status: "unavailable" });
      expect(results[1]).toMatchObject({ status: "ok" });
      stop();
    });

    it("reads as soon as the connection is established instead of waiting out the interval", async () => {
      const fake = new FakeDaemon();
      fake.stub("host.metrics", { ...HOST_METRICS_WIRE, sample_interval_ms: 60_000 });
      const connection = await connect(fake, false);
      const source = createLiveMetrics(connection, { now: clock, visibility: visibilityStub().visibility });
      const results: Array<SurfaceResult<SystemMetrics>> = [];
      const stop = source.watch!((result) => results.push(result));
      await until(() => results.length === 1, "offline read");
      expect(results[0]).toMatchObject({ status: "unavailable" });
      connection.start();
      await until(() => results.length === 2, "read after connecting");
      expect(results[1]).toMatchObject({ status: "ok" });
      stop();
    });
  });
});

