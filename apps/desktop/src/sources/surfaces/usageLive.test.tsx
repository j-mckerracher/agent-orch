/// <reference types="node" />
import { readFileSync } from "node:fs";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import App from "../../App";
import { FakeDaemon } from "../../test/fakeDaemon";
import { connected } from "../../test/render";
import { createServices } from "../../ui/createServices";
import { topBarSlots } from "../../ui/shell/TopBar";
import { createFixtureSources } from "../fixtures";
import { connect, NOW } from "./testkit";
import { hostLocalDay, mapUsageSummary, parseUsageSummary, usageRequest, type LocalDay } from "./usageLive";
import type { UsageSource, UsageSummary } from "./types";

const stops: Array<() => void> = [];
afterEach(() => {
  while (stops.length) stops.pop()!();
  window.location.hash = "";
});

const START = Date.parse("2026-01-01T07:00:00Z");
const DAY: LocalDay = { timezone: "America/Denver", utcOffsetSeconds: -25_200, startMs: START, endMs: START + 86_400_000 };

const figures = (costs: Array<[string, string]> = [], tokens: number | null = null) => ({
  input_tokens: tokens,
  cache_read_tokens: null,
  cache_write_tokens: null,
  output_tokens: tokens,
  reasoning_tokens: null,
  costs: costs.map(([currency, amount]) => ({ currency, amount })),
});

interface BucketOptions {
  sessions?: number;
  withTokens?: number;
  withCost?: number;
  reported?: Array<[string, string]>;
  estimated?: Array<[string, string]>;
}

function bucket({ sessions = 0, withTokens = 0, withCost = 0, reported = [], estimated = [] }: BucketOptions = {}) {
  const status = (covered: number) => (covered === 0 ? "unknown" : covered < sessions ? "partial" : "complete");
  return {
    sessions,
    sessions_with_tokens: withTokens,
    sessions_with_cost: withCost,
    token_status: status(withTokens),
    cost_status: status(withCost),
    reported: figures(reported, withTokens ? 10 : null),
    estimated: figures(estimated),
  };
}

function summary(totals: ReturnType<typeof bucket>, providers: Array<[string, ReturnType<typeof bucket>]> = []) {
  return {
    window: { start_unix_ms: START, end_unix_ms: START + 86_400_000, timezone: "America/Denver", utc_offset_seconds: -25_200 },
    totals,
    providers: providers.map(([provider, value]) => ({ provider, ...value })),
  };
}

async function read(wire: unknown) {
  const fake = new FakeDaemon();
  fake.stub("usage.summary", wire);
  const { surfaces, stop } = await connect(fake);
  stops.push(stop);
  const source = surfaces.usage as UsageSource;
  return { result: await source.usage(), fake };
}

async function data(wire: unknown): Promise<UsageSummary> {
  const { result } = await read(wire);
  if (result.status !== "ok") throw new Error(`expected ok, got ${result.status}: ${result.reason}`);
  return result.data;
}

describe("live usage adapter request", () => {
  it("asks usage.summary for the local day with the client timezone and offset", async () => {
    const fake = new FakeDaemon();
    fake.stub("usage.summary", summary(bucket()));
    const { surfaces, stop } = await connect(fake);
    stops.push(stop);
    await surfaces.usage.usage();
    const calls = fake.callsTo("usage.summary");
    expect(calls).toHaveLength(1);
    const params = calls[0]!.params as Record<string, unknown>;
    expect(params).toMatchObject({ utc_offset_seconds: expect.any(Number), timezone: expect.any(String) });
    expect(Number(params.window_end_unix_ms) - Number(params.window_start_unix_ms)).toBeLessThanOrEqual(26 * 3_600_000);
    expect(Number(params.window_start_unix_ms)).toBeLessThanOrEqual(NOW);
    expect(Number(params.window_end_unix_ms)).toBeGreaterThan(NOW);
    expect(params.timezone).toBe(hostLocalDay(NOW).timezone);
  });

  it("pins the window to the supplied local day", () => {
    expect(usageRequest(DAY, START + 5)).toEqual({
      timezone: "America/Denver",
      utc_offset_seconds: -25_200,
      window_start_unix_ms: START,
      window_end_unix_ms: START + 86_400_000,
    });
  });

  it("falls back to now when the local day is longer than the daemon allows", () => {
    expect(usageRequest({ ...DAY, endMs: START + 27 * 3_600_000 }, NOW)).toEqual({
      timezone: "America/Denver",
      utc_offset_seconds: -25_200,
      now_unix_ms: NOW,
    });
  });

  it("measures 23 and 25 hour DST days from the host clock", () => {
    const zone = process.env.TZ;
    process.env.TZ = "America/Denver";
    try {
      const spring = hostLocalDay(Date.parse("2026-03-08T20:00:00Z"));
      expect((spring.endMs - spring.startMs) / 3_600_000).toBe(23);
      expect(spring).toMatchObject({ timezone: "America/Denver", utcOffsetSeconds: -21_600 });
      const autumn = hostLocalDay(Date.parse("2026-11-01T20:00:00Z"));
      expect((autumn.endMs - autumn.startMs) / 3_600_000).toBe(25);
      expect(autumn.utcOffsetSeconds).toBe(-25_200);
    } finally {
      if (zone === undefined) delete process.env.TZ;
      else process.env.TZ = zone;
    }
  });

  it("reports offset zero as +0, never -0", () => {
    const zone = process.env.TZ;
    process.env.TZ = "UTC";
    try {
      expect(Object.is(hostLocalDay(NOW).utcOffsetSeconds, 0)).toBe(true);
    } finally {
      if (zone === undefined) delete process.env.TZ;
      else process.env.TZ = zone;
    }
  });
});

describe("live usage adapter mapping", () => {
  it("shows a provider-reported cost as reported and keeps the estimate separate and unknown", async () => {
    const wire = summary(bucket({ sessions: 1, withTokens: 1, withCost: 1, reported: [["USD", "0.0086388"]] }), [
      ["claude-code", bucket({ sessions: 1, withTokens: 1, withCost: 1, reported: [["USD", "0.0086388"]] })],
    ]);
    const { result } = await read(wire);
    expect(result).toMatchObject({
      status: "ok",
      provenance: { kind: "daemon", id: "D12", methods: ["usage.summary"] },
      ageMs: 0,
    });
    if (result.status !== "ok") return;
    expect(result.data).toMatchObject({
      windowStart: "2026-01-01T07:00:00.000Z",
      timezone: "America/Denver",
      currency: "USD",
      reported: { status: "ok", value: 0.0086388, ageMs: 0 },
      estimated: { status: "unknown" },
      budget: { status: "unsupported" },
      coverage: [{ provider: "claude-code", reporting: "provider-reported" }],
    });
  });

  it("never adds estimated to reported, even in one currency", async () => {
    const result = await data(
      summary(bucket({ sessions: 2, withTokens: 2, withCost: 2, reported: [["USD", "1.5"]], estimated: [["USD", "0.25"]] }), [
        ["claude-code", bucket({ sessions: 1, withTokens: 1, withCost: 1, reported: [["USD", "1.5"]] })],
        ["pi", bucket({ sessions: 1, withTokens: 1, withCost: 1, estimated: [["USD", "0.25"]] })],
      ]),
    );
    expect(result.reported).toMatchObject({ status: "ok", value: 1.5 });
    expect(result.estimated).toMatchObject({ status: "ok", value: 0.25 });
    expect(result.coverage).toEqual([
      { provider: "claude-code", reporting: "provider-reported" },
      { provider: "pi", reporting: "estimated" },
    ]);
  });

  it("shows no reported cost as unknown, never zero, and says why", async () => {
    const result = await data(
      summary(bucket({ sessions: 2, withTokens: 1 }), [
        ["codex", bucket({ sessions: 1, withTokens: 1 })],
        ["pi", bucket({ sessions: 1 })],
      ]),
    );
    expect(result.reported.status).toBe("unknown");
    expect("value" in result.reported).toBe(false);
    expect(result.reported).toMatchObject({ reason: expect.stringContaining("2 sessions started") });
    expect(result.estimated.status).toBe("unknown");
    expect(result.coverage).toEqual([
      { provider: "codex", reporting: "tokens-only" },
      { provider: "pi", reporting: "none" },
    ]);
    expect(JSON.stringify(result)).not.toMatch(/"value":0\b/);
  });

  it("is unknown, not zero, for an empty day", async () => {
    const result = await data(summary(bucket()));
    expect(result).toMatchObject({ currency: "unknown", reported: { status: "unknown" }, coverage: [] });
  });

  it("marks a provider whose sessions only partly reported a cost as partial", async () => {
    const result = await data(
      summary(bucket({ sessions: 3, withTokens: 3, withCost: 1, reported: [["USD", "2"]] }), [
        ["claude-code", bucket({ sessions: 3, withTokens: 3, withCost: 1, reported: [["USD", "2"]] })],
      ]),
    );
    expect(result.coverage).toEqual([{ provider: "claude-code", reporting: "partial" }]);
    expect(result.reported).toMatchObject({ status: "ok", value: 2 });
  });

  it("never sums or converts across currencies", async () => {
    const result = await data(
      summary(bucket({ sessions: 2, withTokens: 2, withCost: 2, reported: [["EUR", "1.2"], ["USD", "3"]] })),
    );
    expect(result.currency).toBe("unknown");
    expect(result.reported.status).toBe("unsupported");
    expect(result.reported).toMatchObject({ reason: expect.stringMatching(/EUR 1\.2, USD 3.*never adds/) });
    expect("value" in result.reported).toBe(false);
  });

  it("does not show an estimate in another currency than the reported figure", async () => {
    const result = await data(
      summary(bucket({ sessions: 2, withTokens: 2, withCost: 2, reported: [["USD", "1"]], estimated: [["EUR", "9"]] })),
    );
    expect(result.currency).toBe("USD");
    expect(result.reported).toMatchObject({ status: "ok", value: 1 });
    expect(result.estimated).toMatchObject({ status: "unsupported", reason: expect.stringContaining("EUR 9") });
  });

  it("uses the estimate's currency when nothing was reported", async () => {
    const result = await data(summary(bucket({ sessions: 1, withCost: 1, estimated: [["EUR", "4.5"]] })));
    expect(result.currency).toBe("EUR");
    expect(result.reported.status).toBe("unknown");
    expect(result.estimated).toMatchObject({ status: "ok", value: 4.5 });
  });

  it("keeps decimal strings exact enough for a nano-unit amount", () => {
    const wire = parseUsageSummary(summary(bucket({ sessions: 1, withCost: 1, reported: [["USD", "123456.000000001"]] })));
    expect(mapUsageSummary(wire, NOW).reported).toMatchObject({ status: "ok", value: 123456.000000001 });
  });
});

describe("live usage adapter failures", () => {
  it("maps a missing method to unsupported and never serves fixture data", async () => {
    const { surfaces, stop } = await connect(new FakeDaemon());
    stops.push(stop);
    const result = await surfaces.usage.usage();
    expect(result).toMatchObject({ status: "unsupported", provenance: { kind: "daemon", id: "D12" } });
    expect("data" in result).toBe(false);
  });

  it("maps a daemon error to unavailable", async () => {
    const fake = new FakeDaemon();
    fake.failNext("usage.summary", { code: -32603, message: "boom" });
    const { surfaces, stop } = await connect(fake);
    stops.push(stop);
    expect(await surfaces.usage.usage()).toMatchObject({ status: "unavailable" });
  });

  it("maps invalid params to unavailable", async () => {
    const fake = new FakeDaemon();
    fake.failNext("usage.summary", { code: -32602, message: "invalid params" });
    const { surfaces, stop } = await connect(fake);
    stops.push(stop);
    expect(await surfaces.usage.usage()).toMatchObject({ status: "unavailable" });
  });

  it("is unavailable when not connected, without calling the daemon", async () => {
    const fake = new FakeDaemon();
    const { surfaces, stop } = await connect(fake, false);
    stops.push(stop);
    expect(await surfaces.usage.usage()).toMatchObject({ status: "unavailable" });
    expect(fake.callsTo("usage.summary")).toHaveLength(0);
  });

  it.each([
    ["a non-object result", null],
    ["a missing window", { totals: bucket(), providers: [] }],
    ["a non-decimal amount", summary(bucket({ sessions: 1, withCost: 1, reported: [["USD", "12.5e3"]] }))],
    ["a negative amount", summary(bucket({ sessions: 1, withCost: 1, reported: [["USD", "-1"]] }))],
    ["a repeated currency", summary(bucket({ sessions: 1, withCost: 1, reported: [["USD", "1"], ["USD", "2"]] }))],
    ["an unknown status", summary({ ...bucket(), cost_status: "full" } as never)],
    ["a provider without a name", { ...summary(bucket()), providers: [bucket()] }],
  ])("treats %s as unavailable rather than guessing a value", async (_label, wire) => {
    const { result } = await read(wire);
    expect(result).toMatchObject({ status: "unavailable" });
    expect("data" in result).toBe(false);
  });

  it("does not import fixtures", () => {
    const source = readFileSync("src/sources/surfaces/usageLive.ts", "utf8");
    expect(source).not.toMatch(/from "\.\/fixtures"|fixtures\/ui|createFixture/);
  });
});

describe("usage pill with the live adapter", () => {
  async function mountLive(fake: FakeDaemon) {
    const services = createServices({
      connector: fake,
      initialRoute: { name: "global", view: "workspace" },
      connection: { backoffMs: [5], requestTimeoutMs: 250 },
      data: { coalesceMs: 10 },
      sources: createFixtureSources(),
    });
    const user = userEvent.setup();
    render(<App services={services} shellSlots={topBarSlots} />);
    await connected();
    return { user };
  }

  it("shows a reported amount and the labelled window, estimate and provider coverage", async () => {
    const fake = new FakeDaemon();
    fake.stub(
      "usage.summary",
      summary(bucket({ sessions: 2, withTokens: 2, withCost: 1, reported: [["USD", "1.25"]] }), [
        ["claude-code", bucket({ sessions: 1, withTokens: 1, withCost: 1, reported: [["USD", "1.25"]] })],
        ["codex", bucket({ sessions: 1, withTokens: 1 })],
      ]),
    );
    await mountLive(fake);
    const pill = screen.getByTestId("top-usage");
    await waitFor(() => expect(within(pill).getByTestId("top-usage-value")).toHaveTextContent("$1.25"));
    expect(within(pill).queryByTestId("badge-D12")).not.toBeInTheDocument();
    const info = screen.getByTestId("top-usage-info");
    expect(info).toHaveTextContent("$1.25 reported since");
    expect(info).toHaveTextContent("America/Denver");
    expect(info).toHaveTextContent("Estimated, not reported: unknown");
    expect(info).toHaveTextContent("Budget: unsupported");
    expect(info).toHaveTextContent("claude-code: provider-reported");
    expect(info).toHaveTextContent("codex: tokens-only");
    expect(info).not.toHaveTextContent("placeholder");
  });

  it("never shows the wireframe $26.00 or a $0 figure when nothing was reported", async () => {
    const fake = new FakeDaemon();
    fake.stub("usage.summary", summary(bucket({ sessions: 1, withTokens: 1 }), [["codex", bucket({ sessions: 1, withTokens: 1 })]]));
    await mountLive(fake);
    const pill = screen.getByTestId("top-usage");
    await waitFor(() => expect(within(pill).getByTestId("top-usage-value")).toHaveTextContent("not available"));
    expect(pill.textContent).not.toMatch(/\$/);
    const info = screen.getByTestId("top-usage-info");
    expect(info).toHaveTextContent("Reported usage since");
    expect(info).toHaveTextContent("is unknown");
    expect(info.textContent).not.toMatch(/\$|\b26\.00\b/);
  });

  it("shows the daemon failure, not fixture data, when the call fails", async () => {
    const fake = new FakeDaemon();
    fake.failNext("usage.summary", { code: -32603, message: "boom" }, 5);
    await mountLive(fake);
    const pill = screen.getByTestId("top-usage");
    await waitFor(() => expect(within(pill).getByTestId("top-usage-value")).toHaveTextContent("not available"));
    expect(pill.textContent).not.toMatch(/\$|26/);
    expect(screen.getByTestId("top-usage-info")).toHaveTextContent("Usage is unavailable");
  });
});
