/// <reference types="node" />
import { readFileSync } from "node:fs";
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
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
  /** M7-26 fields, present only when given (an older daemon sends none of them). */
  needing?: number;
  estimatedSessions?: number;
  known?: number;
}

function bucket({ sessions = 0, withTokens = 0, withCost = 0, reported = [], estimated = [], needing, estimatedSessions, known }: BucketOptions = {}) {
  const status = (covered: number) => (covered === 0 ? "unknown" : covered < sessions ? "partial" : "complete");
  const native =
    needing === undefined || estimatedSessions === undefined
      ? {}
      : {
          sessions_needing_estimate: needing,
          sessions_with_estimated_cost: estimatedSessions,
          estimated_cost_status: estimatedSessions === 0 ? "unknown" : estimatedSessions < needing ? "partial" : "complete",
        };
  const knownFields = known === undefined ? {} : { sessions_with_known_cost: known, known_cost_status: status(known) };
  return {
    ...native,
    ...knownFields,
    sessions,
    sessions_with_tokens: withTokens,
    sessions_with_cost: withCost,
    token_status: status(withTokens),
    cost_status: status(withCost),
    reported: figures(reported, withTokens ? 10 : null),
    estimated: figures(estimated),
  };
}

function summary(totals: ReturnType<typeof bucket>, providers: Array<[string, ReturnType<typeof bucket>]> = [], price_table?: unknown) {
  return {
    window: { start_unix_ms: START, end_unix_ms: START + 86_400_000, timezone: "America/Denver", utc_offset_seconds: -25_200 },
    totals,
    providers: providers.map(([provider, value]) => ({ provider, ...value })),
    ...(price_table === undefined ? {} : { price_table }),
  };
}

const TABLE = { version: "2026-11-11.1", basis: "list_price_standard_tier", source: "LiteLLM, 2026-11-11", currency: "USD", models: 16 };

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

  it("tolerates an older daemon that sends no price table and none of the estimate fields", async () => {
    const result = await data(summary(bucket({ sessions: 1, withTokens: 1, withCost: 1, reported: [["USD", "1"]] })));
    expect(result.priceTable).toBeUndefined();
    expect(result.estimateCoverage).toBeUndefined();
    expect(result.knownCostCoverage).toBeUndefined();
    expect(result.estimated).toMatchObject({ status: "unknown", reason: expect.stringContaining("the daemon has no price table") });
  });

  it("rejects a malformed price table or a malformed new count rather than guessing", () => {
    const base = summary(bucket({ sessions: 1 }));
    expect(() => parseUsageSummary({ ...base, price_table: { version: "v1" } })).toThrow(/basis/);
    expect(() => parseUsageSummary({ ...base, price_table: "x" })).toThrow();
    expect(() => parseUsageSummary({ ...base, totals: { ...base.totals, sessions_needing_estimate: -1 } })).toThrow(/sessions_needing_estimate/);
    expect(() => parseUsageSummary({ ...base, totals: { ...base.totals, known_cost_status: "maybe" } })).toThrow(/known_cost_status/);
  });

  it("keeps a price-table estimate separate from reported, with coverage and the table version", async () => {
    const result = await data(
      summary(
        bucket({ sessions: 3, withTokens: 3, withCost: 1, reported: [["USD", "1.5"]], estimated: [["USD", "0.152"]], needing: 2, estimatedSessions: 1, known: 2 }),
        [
          ["claude-code", bucket({ sessions: 1, withTokens: 1, withCost: 1, reported: [["USD", "1.5"]], needing: 0, estimatedSessions: 0, known: 1 })],
          ["codex", bucket({ sessions: 2, withTokens: 2, estimated: [["USD", "0.152"]], needing: 2, estimatedSessions: 1, known: 1 })],
        ],
        TABLE,
      ),
    );
    expect(result.reported).toMatchObject({ status: "ok", value: 1.5 });
    expect(result.estimated).toMatchObject({ status: "ok", value: 0.152 });
    expect(result.estimateCoverage).toEqual({ status: "partial", sessionsNeedingEstimate: 2, sessionsEstimated: 1 });
    expect(result.knownCostCoverage).toEqual({ status: "partial", sessions: 3, sessionsWithKnownCost: 2 });
    expect(result.priceTable).toEqual({ version: "2026-11-11.1", basis: "list_price_standard_tier", source: "LiteLLM, 2026-11-11" });
    expect(result.costCoverage).toEqual({ status: "partial", sessions: 3, sessionsWithCost: 1 });
  });

  it("says why an estimate is unknown: unpriceable sessions, or nothing that needs one", async () => {
    const unpriced = await data(summary(bucket({ sessions: 2, withTokens: 2, needing: 2, estimatedSessions: 0, known: 0 }), [], TABLE));
    expect(unpriced.estimated).toMatchObject({
      status: "unknown",
      reason: expect.stringMatching(/2 sessions have tokens but no reported cost and could not be priced.*2026-11-11\.1/),
    });
    expect("value" in unpriced.estimated).toBe(false);
    const none = await data(
      summary(bucket({ sessions: 1, withTokens: 1, withCost: 1, reported: [["USD", "1"]], needing: 0, estimatedSessions: 0, known: 1 }), [], TABLE),
    );
    expect(none.estimated).toMatchObject({ status: "unknown", reason: expect.stringContaining("No session in this window needs an estimate (price table 2026-11-11.1)") });
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

  it("words an estimate in another currency as an estimate, not a report", async () => {
    const result = await data(
      summary(bucket({ sessions: 2, withTokens: 2, withCost: 2, reported: [["USD", "1"]], estimated: [["EUR", "9"]] })),
    );
    expect(result.estimated).toMatchObject({ reason: expect.stringMatching(/^Estimated costs were recorded in EUR 9.*only USD/) });
    expect(result.estimated).toMatchObject({ reason: expect.not.stringContaining("reported in") });
    const several = await data(
      summary(bucket({ sessions: 2, withCost: 2, reported: [["EUR", "1"], ["USD", "2"]], estimated: [["USD", "3.1"]] })),
    );
    expect(several.reported).toMatchObject({ reason: expect.stringMatching(/^Costs were reported in EUR 1, USD 2.*one currency/) });
    expect(several.estimated).toMatchObject({ status: "unsupported", reason: expect.stringMatching(/^Estimated costs were recorded in USD 3\.1/) });
  });

  it("carries the totals cost coverage so a partial headline can be flagged", async () => {
    const partial = await data(summary(bucket({ sessions: 4, withTokens: 4, withCost: 1, reported: [["USD", "2"]] })));
    expect(partial.costCoverage).toEqual({ status: "partial", sessions: 4, sessionsWithCost: 1 });
    const complete = await data(summary(bucket({ sessions: 2, withTokens: 2, withCost: 2, reported: [["USD", "2"]] })));
    expect(complete.costCoverage).toEqual({ status: "complete", sessions: 2, sessionsWithCost: 2 });
    const none = await data(summary(bucket({ sessions: 2, withTokens: 2 })));
    expect(none.costCoverage).toEqual({ status: "unknown", sessions: 2, sessionsWithCost: 0 });
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
    const view = render(<App services={services} shellSlots={topBarSlots} />);
    await connected();
    return { user, unmount: view.unmount };
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

  const valueOf = () => within(screen.getByTestId("top-usage")).getByTestId("top-usage-value");
  const oneCost = (amount: string, extra: BucketOptions = {}) =>
    summary(bucket({ sessions: 1, withTokens: 1, withCost: 1, reported: [["USD", amount]], ...extra }));

  it("refreshes the figure when the window regains focus and keeps polling machinery cleaned up", async () => {
    const fake = new FakeDaemon();
    let amount = "1.00";
    fake.stub("usage.summary", () => oneCost(amount));
    await mountLive(fake);
    await waitFor(() => expect(valueOf()).toHaveTextContent("$1.00"));
    amount = "9.00";
    act(() => void window.dispatchEvent(new Event("focus")));
    await waitFor(() => expect(valueOf()).toHaveTextContent("$9.00"));
    expect(fake.callsTo("usage.summary").length).toBeGreaterThanOrEqual(2);
  });

  it("shows a sub-cent amount as <$0.01, distinct from a true zero", async () => {
    const fake = new FakeDaemon();
    fake.stub("usage.summary", oneCost("0.004"));
    await mountLive(fake);
    await waitFor(() => expect(valueOf()).toHaveTextContent("<$0.01"));
    expect(valueOf().textContent).not.toBe("$0.00");
    expect(screen.getByTestId("top-usage-info")).toHaveTextContent("<$0.01 reported since");
  });

  it("shows nine-decimal dust as <$0.01 and a reported zero as $0.00", async () => {
    const dust = new FakeDaemon();
    dust.stub("usage.summary", oneCost("0.000000001"));
    const first = await mountLive(dust);
    await waitFor(() => expect(valueOf()).toHaveTextContent("<$0.01"));
    first.unmount();
    const zero = new FakeDaemon();
    zero.stub("usage.summary", oneCost("0"));
    await mountLive(zero);
    await waitFor(() => expect(valueOf()).toHaveTextContent(/^\$0\.00$/));
  });

  it("flags a partial headline on the pill and in the popover", async () => {
    const fake = new FakeDaemon();
    fake.stub("usage.summary", summary(bucket({ sessions: 4, withTokens: 4, withCost: 1, reported: [["USD", "2"]] })));
    await mountLive(fake);
    const pill = screen.getByTestId("top-usage");
    await waitFor(() => expect(valueOf()).toHaveTextContent("$2.00"));
    expect(pill).toHaveTextContent("reported (partial)");
    expect(screen.getByTestId("top-usage-partial")).toHaveTextContent("1 of 4 sessions reported a cost, so this is a lower bound");
  });

  it("shows an estimate-only headline as estimated with the table version, never as reported", async () => {
    const fake = new FakeDaemon();
    fake.stub(
      "usage.summary",
      summary(bucket({ sessions: 1, withTokens: 1, estimated: [["USD", "0.000152"]], needing: 1, estimatedSessions: 1, known: 1 }), [], TABLE),
    );
    await mountLive(fake);
    await waitFor(() => expect(valueOf()).toHaveTextContent("≈ <$0.01"));
    expect(screen.getByTestId("top-usage-basis")).toHaveTextContent("estimated · table 2026-11-11.1");
    expect(screen.getByTestId("top-usage-basis")).not.toHaveTextContent("partial");
    expect(screen.queryByText(/^reported( \(partial\))?$/)).toBeNull();
    expect(screen.getByTestId("top-usage-estimate")).toHaveTextContent("List-price estimate from price table 2026-11-11.1, list_price_standard_tier, not a bill");
    expect(screen.getByTestId("top-usage-price-table")).toHaveTextContent("Price table 2026-11-11.1 · basis list_price_standard_tier · LiteLLM, 2026-11-11");
    expect(screen.queryByTestId("top-usage-estimate-partial")).toBeNull();
    expect(screen.queryByTestId("top-usage-unknown")).toBeNull();
  });

  it("keeps the reported headline and does not add an estimate to it", async () => {
    const fake = new FakeDaemon();
    fake.stub(
      "usage.summary",
      summary(
        bucket({ sessions: 2, withTokens: 2, withCost: 1, reported: [["USD", "2"]], estimated: [["USD", "3"]], needing: 1, estimatedSessions: 1, known: 2 }),
        [],
        TABLE,
      ),
    );
    await mountLive(fake);
    await waitFor(() => expect(valueOf()).toHaveTextContent(/^\$2\.00$/));
    expect(screen.queryByTestId("top-usage-basis")).toBeNull();
    expect(screen.getByTestId("top-usage")).not.toHaveTextContent("$5");
    expect(screen.getByTestId("top-usage-info")).toHaveTextContent("Estimated, not reported: $3.00");
  });

  it("flags a partial estimate and the unknown sessions", async () => {
    const fake = new FakeDaemon();
    fake.stub(
      "usage.summary",
      summary(bucket({ sessions: 4, withTokens: 4, estimated: [["USD", "1"]], needing: 4, estimatedSessions: 1, known: 1 }), [], TABLE),
    );
    await mountLive(fake);
    await waitFor(() => expect(valueOf()).toHaveTextContent("≈ $1.00"));
    expect(screen.getByTestId("top-usage-basis")).toHaveTextContent("estimated (partial) · table 2026-11-11.1");
    expect(screen.getByTestId("top-usage-estimate-partial")).toHaveTextContent("1 of 4 sessions with tokens but no reported cost could be priced, so this is a lower bound");
    expect(screen.getByTestId("top-usage-unknown")).toHaveTextContent("Unknown: 3 of 4 sessions have neither a reported nor an estimated cost");
  });

  it("stays not available, with the table named, when nothing could be priced", async () => {
    const fake = new FakeDaemon();
    fake.stub("usage.summary", summary(bucket({ sessions: 2, withTokens: 2, needing: 2, estimatedSessions: 0, known: 0 }), [], TABLE));
    await mountLive(fake);
    await waitFor(() => expect(valueOf()).toHaveTextContent("not available"));
    expect(screen.getByTestId("top-usage").textContent).not.toMatch(/\$|≈/);
    expect(screen.queryByTestId("top-usage-basis")).toBeNull();
    expect(screen.getByTestId("top-usage-unknown")).toHaveTextContent("Unknown: 2 of 2 sessions");
    expect(screen.getByTestId("top-usage-price-table")).toHaveTextContent("Price table 2026-11-11.1");
  });

  it("says an older daemon reports no estimates", async () => {
    const fake = new FakeDaemon();
    fake.stub("usage.summary", oneCost("2"));
    await mountLive(fake);
    await waitFor(() => expect(valueOf()).toHaveTextContent("$2.00"));
    expect(screen.getByTestId("top-usage-price-table")).toHaveTextContent("No price table: this daemon reports no estimates.");
    expect(screen.queryByTestId("top-usage-unknown")).toBeNull();
  });

  it("does not flag a complete headline as partial", async () => {
    const fake = new FakeDaemon();
    fake.stub("usage.summary", oneCost("2"));
    await mountLive(fake);
    await waitFor(() => expect(valueOf()).toHaveTextContent("$2.00"));
    expect(screen.getByTestId("top-usage")).not.toHaveTextContent("partial");
    expect(screen.queryByTestId("top-usage-partial")).toBeNull();
  });

  it("renders duplicate provider names without a duplicate-key warning and clips long labels", async () => {
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    try {
      const fake = new FakeDaemon();
      const long = "p".repeat(300);
      fake.stub(
        "usage.summary",
        summary(bucket({ sessions: 2, withTokens: 2, withCost: 2, reported: [["USD", "1"]] }), [
          ["codex", bucket({ sessions: 1, withTokens: 1 })],
          ["codex", bucket({ sessions: 1, withTokens: 1 })],
          [long, bucket({ sessions: 1, withTokens: 1 })],
        ]),
      );
      await mountLive(fake);
      await waitFor(() => expect(valueOf()).toHaveTextContent("$1.00"));
      const items = within(screen.getByRole("list", { name: "Reporting coverage", hidden: true })).getAllByRole("listitem", { hidden: true });
      expect(items).toHaveLength(3);
      expect(items[2]!.textContent!.length).toBeLessThan(80);
      expect(items[2]).toHaveAttribute("title", long);
      expect(error.mock.calls.flat().join(" ")).not.toMatch(/same key|unique "key"/);
    } finally {
      error.mockRestore();
    }
  });
});
