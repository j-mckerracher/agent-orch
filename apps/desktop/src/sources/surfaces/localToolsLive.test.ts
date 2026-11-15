import { afterEach, describe, expect, it } from "vitest";
import { DaemonError } from "../../daemon/errors";
import { FakeDaemon } from "../../test/fakeDaemon";
import { createLiveLocalToolsSource, mapLocalTools, parseLocalTools } from "./localToolsLive";
import type { LocalDay } from "./usageLive";
import { connect, NOW } from "./testkit";

const stops: Array<() => void> = [];
afterEach(() => {
  while (stops.length) stops.pop()!();
});

const START = Date.parse("2026-01-01T07:00:00Z");
const DAY: LocalDay = { timezone: "America/Denver", utcOffsetSeconds: -25_200, startMs: START, endMs: START + 86_400_000 };

function provider(id: string, extra: Record<string, unknown> = {}) {
  return {
    id,
    state: "ok",
    sessions: 2,
    calls: 9,
    tokens: { input: 100, output: 20, cache_read: null, cache_write: null },
    estimated_costs: [{ currency: "USD", amount: "0.0126" }],
    unpriced_models: 0,
    unpriced_tokens: 0,
    cost_complete: true,
    ...extra,
  };
}

/** A valid `usage.localTools` result: Claude Code and Codex priced, GitHub Copilot with one unpriced model. */
function wire(): Record<string, any> {
  return {
    schema_version: 1,
    enabled: true,
    window: { start_unix_ms: START, end_unix_ms: START + 86_400_000, timezone: "America/Denver", utc_offset_seconds: -25_200 },
    data_window: { start_unix_ms: START, end_unix_ms: START + 86_400_000, timezone: "America/Denver" },
    reader: {
      state: "ok",
      version: "0.9.28",
      pinned_version: "0.9.28",
      node_version: "22.16.0",
      exit_status: null,
      source: "bundled",
      development_build: false,
    },
    refreshing: false,
    observed_at_unix_ms: NOW - 120_000,
    stale: false,
    providers: [
      provider("claude-code"),
      provider("codex", { estimated_costs: [{ currency: "USD", amount: "0.0100" }] }),
      provider("github-copilot", {
        estimated_costs: [{ currency: "USD", amount: "0.0050" }],
        unpriced_models: 1,
        unpriced_tokens: 4000,
        cost_complete: false,
      }),
    ],
  };
}

const clone = () => JSON.parse(JSON.stringify(wire())) as Record<string, any>;

async function source(fake: FakeDaemon) {
  const { connection, stop } = await connect(fake);
  stops.push(stop);
  return createLiveLocalToolsSource(connection, () => NOW, () => DAY);
}

describe("parseLocalTools", () => {
  it("accepts a valid result and maps it to the view model", () => {
    const snapshot = mapLocalTools(parseLocalTools(wire()));
    expect(snapshot.enabled).toBe(true);
    expect(snapshot.window).toEqual({ startMs: START, endMs: START + 86_400_000, timezone: "America/Denver" });
    expect(snapshot.figures).toEqual(snapshot.window);
    expect(snapshot.reader).toEqual({
      state: "ok",
      version: "0.9.28",
      pinnedVersion: "0.9.28",
      nodeVersion: "22.16.0",
      exitStatus: null,
      source: "bundled",
    });
    expect(snapshot.observedAtMs).toBe(NOW - 120_000);
    expect(snapshot.providers.map((p) => [p.id, p.label])).toEqual([
      ["claude-code", "Claude Code"],
      ["codex", "Codex"],
      ["github-copilot", "GitHub Copilot"],
    ]);
  });

  it("keeps an unpriced model out of the estimate and marks the cost incomplete", () => {
    const copilot = mapLocalTools(parseLocalTools(wire())).providers[2]!;
    expect(copilot.estimatedUsd).toBe(0.005);
    expect(copilot.unpricedModels).toBe(1);
    expect(copilot.unpricedTokens).toBe(4000);
    expect(copilot.costComplete).toBe(false);
  });

  it("maps a provider with no price to a null estimate, never zero", () => {
    const value = clone();
    value.providers[0].estimated_costs = [];
    expect(mapLocalTools(parseLocalTools(value)).providers[0]!.estimatedUsd).toBeNull();
  });

  it("accepts a release build's not_bundled reader, with no source", () => {
    const value = clone();
    value.data_window = null;
    value.observed_at_unix_ms = null;
    value.reader = { state: "not_bundled", version: null, pinned_version: "0.9.28", node_version: null, exit_status: null, source: null, development_build: false };
    value.providers = value.providers.map((p: Record<string, unknown>) => ({ ...p, state: "not_run", sessions: null, calls: null, estimated_costs: [] }));
    expect(mapLocalTools(parseLocalTools(value)).reader).toMatchObject({ state: "not_bundled", source: null });
  });

  it("maps a development build's PATH or override source", () => {
    for (const source of ["path", "override"] as const) {
      const value = clone();
      value.reader.source = source;
      expect(mapLocalTools(parseLocalTools(value)).reader.source).toBe(source);
    }
  });

  it("maps no figures to null, and keeps the requested window", () => {
    const value = clone();
    value.data_window = null;
    value.observed_at_unix_ms = null;
    value.reader = {
      state: "not_installed",
      version: null,
      pinned_version: "0.9.28",
      node_version: "22.16.0",
      exit_status: null,
      source: null,
      development_build: true,
    };
    value.providers = value.providers.map((p: Record<string, unknown>) => ({ ...p, state: "not_run", sessions: null, calls: null, estimated_costs: [] }));
    const snapshot = mapLocalTools(parseLocalTools(value));
    expect(snapshot.figures).toBeNull();
    expect(snapshot.observedAtMs).toBeNull();
    expect(snapshot.window.startMs).toBe(START);
  });

  const BROKEN: Array<[string, (w: Record<string, any>) => void]> = [
    ["a schema version other than 1", (w) => (w.schema_version = 2)],
    ["enabled as a string", (w) => (w.enabled = "yes")],
    ["a window that ends when it starts", (w) => (w.window.end_unix_ms = w.window.start_unix_ms)],
    ["a window with no timezone", (w) => delete w.window.timezone],
    ["figures with no observed time", (w) => (w.observed_at_unix_ms = null)],
    ["figures for a reversed window", (w) => (w.data_window.end_unix_ms = w.data_window.start_unix_ms - 1)],
    ["an unknown reader state", (w) => (w.reader.state = "running")],
    ["a reader source outside the three known ones", (w) => (w.reader.source = "cache")],
    ["a missing reader source", (w) => delete w.reader.source],
    ["a development_build flag as a string", (w) => (w.reader.development_build = "false")],
    ["a missing development_build flag", (w) => delete w.reader.development_build],
    ["a missing pinned version", (w) => delete w.reader.pinned_version],
    ["a non-integer exit status", (w) => (w.reader.exit_status = "2")],
    ["two providers instead of three", (w) => w.providers.pop()],
    ["providers out of order", (w) => w.providers.reverse()],
    ["an unknown provider", (w) => (w.providers[2].id = "cursor")],
    ["an unknown provider state", (w) => (w.providers[0].state = "partial")],
    ["a cost in another currency", (w) => (w.providers[0].estimated_costs[0].currency = "EUR")],
    ["a cost that is not a decimal string", (w) => (w.providers[0].estimated_costs[0].amount = "1,20")],
    ["a cost that is a JSON number", (w) => (w.providers[0].estimated_costs[0].amount = 0.0126)],
    ["two costs for one provider", (w) => w.providers[0].estimated_costs.push({ currency: "USD", amount: "0.01" })],
    ["a negative unpriced count", (w) => (w.providers[2].unpriced_models = -1)],
    ["a fractional session count", (w) => (w.providers[0].sessions = 1.5)],
    ["cost_complete as a string", (w) => (w.providers[0].cost_complete = "true")],
    ["missing tokens", (w) => delete w.providers[0].tokens],
    ["a refreshing flag as a number", (w) => (w.refreshing = 0)],
    ["a missing stale flag", (w) => delete w.stale],
  ];

  it.each(BROKEN)("refuses %s as a protocol failure", (_label, break_) => {
    const value = clone();
    break_(value);
    let error: unknown = null;
    try {
      parseLocalTools(value);
    } catch (caught) {
      error = caught;
    }
    expect(error).toBeInstanceOf(DaemonError);
    expect(error).toMatchObject({ kind: "protocol" });
  });

  it.each([null, [], "ok", 42])("refuses a result that is not an object (%j)", (value) => {
    expect(() => parseLocalTools(value)).toThrow(/malformed/);
  });
});

describe("the live local-tools source", () => {
  it("reads the cached snapshot for the host's local day, and sends no enabled flag", async () => {
    const fake = new FakeDaemon();
    fake.stub("usage.localTools", wire());
    const result = await (await source(fake)).local();
    expect(result).toMatchObject({ status: "ok", provenance: { kind: "daemon", id: "D12", methods: expect.arrayContaining(["usage.localTools"]) } });
    const [call] = fake.callsTo("usage.localTools");
    expect(call!.params).toEqual({
      timezone: "America/Denver",
      utc_offset_seconds: -25_200,
      window_start_unix_ms: START,
      window_end_unix_ms: START + 86_400_000,
    });
  });

  it("sends the opt-in choice with the same window when setting it", async () => {
    const fake = new FakeDaemon();
    fake.stub("usage.localTools.setEnabled", { ...wire(), enabled: false });
    const result = await (await source(fake)).setEnabled(false);
    expect(result).toMatchObject({ status: "ok", data: { enabled: false } });
    const [call] = fake.callsTo("usage.localTools.setEnabled");
    expect(call!.params).toMatchObject({ enabled: false, window_start_unix_ms: START, window_end_unix_ms: START + 86_400_000 });
  });

  it("asks the collector to refresh with the same window", async () => {
    const fake = new FakeDaemon();
    fake.stub("usage.localTools.refresh", { ...wire(), refreshing: true });
    const result = await (await source(fake)).refresh();
    expect(result).toMatchObject({ status: "ok", data: { refreshing: true } });
    expect(fake.callsTo("usage.localTools.refresh")).toHaveLength(1);
  });

  it("reports a daemon that lacks the method as unsupported, never as zero", async () => {
    const fake = new FakeDaemon();
    const result = await (await source(fake)).local();
    expect(result).toMatchObject({ status: "unsupported" });
    expect(result).not.toHaveProperty("data");
  });

  it("reports a malformed reply as unavailable with the reason, and guesses nothing", async () => {
    const fake = new FakeDaemon();
    fake.stub("usage.localTools", { ...wire(), schema_version: 2 });
    const result = await (await source(fake)).local();
    expect(result).toMatchObject({ status: "unavailable", provenance: { kind: "daemon" } });
    expect(result).not.toHaveProperty("data");
    expect(result.status !== "ok" && result.reason).toContain("malformed");
  });

  it("reports a daemon error as unavailable, never with fixture figures", async () => {
    const fake = new FakeDaemon();
    fake.failNext("usage.localTools", { code: -32603, message: "boom" });
    const result = await (await source(fake)).local();
    expect(result).toMatchObject({ status: "unavailable", provenance: { kind: "daemon", id: "D12" } });
  });
});
