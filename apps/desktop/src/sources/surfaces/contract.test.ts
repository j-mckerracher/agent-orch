import { afterEach, describe, expect, it } from "vitest";
import { FakeDaemon } from "../../test/fakeDaemon";
import { createFixtureSurfaces } from "./fixtures";
import { ALPHA, READ_METHODS, clock, connect, populate } from "./testkit";
import type { ConversationTarget, SurfaceResult, SurfaceSources } from "./types";

/**
 * Contract tests (M7-03): the same scenarios and invariants run against the
 * fixture adapters and the daemon-derived adapters behind one interface.
 */
type Scenario = "ok" | "empty" | "unavailable" | "unsupported" | "error";
type Kind = "fixture" | "daemon";

interface Built {
  surfaces: SurfaceSources;
  target: ConversationTarget;
  fake?: FakeDaemon;
}
interface Harness {
  kind: Kind;
  build(scenario: Scenario): Promise<Built>;
}

const stops: Array<() => void> = [];
afterEach(() => {
  while (stops.length) stops.pop()!();
});

const fixtureHarness: Harness = {
  kind: "fixture",
  async build(scenario) {
    const mapped = scenario === "ok" ? "success" : scenario === "error" ? "unavailable" : scenario;
    return {
      surfaces: createFixtureSurfaces({ scenario: mapped, now: clock }),
      target: { kind: "project", projectId: "fixture:project-alpha" },
    };
  },
};

/** A `usage.summary` result: one provider that reported a cost, or an empty day. */
function usageWire(withCost: boolean) {
  const figures = (costs: Array<{ currency: string; amount: string }>) => ({
    input_tokens: costs.length ? 10 : null,
    cache_read_tokens: null,
    cache_write_tokens: null,
    output_tokens: costs.length ? 5 : null,
    reasoning_tokens: null,
    costs,
  });
  const bucket = (n: number) => ({
    sessions: n,
    sessions_with_tokens: n,
    sessions_with_cost: n,
    token_status: n ? "complete" : "unknown",
    cost_status: n ? "complete" : "unknown",
    reported: figures(n ? [{ currency: "USD", amount: "0.5" }] : []),
    estimated: figures([]),
  });
  const n = withCost ? 1 : 0;
  return {
    window: { start_unix_ms: 0, end_unix_ms: 86_400_000, timezone: "UTC", utc_offset_seconds: 0 },
    totals: bucket(n),
    providers: withCost ? [{ provider: "claude-code", ...bucket(1) }] : [],
  };
}

const derivedHarness: Harness = {
  kind: "daemon",
  async build(scenario) {
    const fake = new FakeDaemon();
    if (scenario === "ok") populate(fake);
    if (scenario === "ok" || scenario === "empty") fake.stub("usage.summary", usageWire(scenario === "ok"));
    if (scenario === "empty") {
      fake.stub("agents.capabilities", []);
      fake.stub("coordinator.get", { project_id: ALPHA, entries: [], more: false, next_after_sequence: 0 });
      fake.stub("host.metrics", { schema_version: 1, sample_interval_ms: 2000, meters: [] });
    }
    if (scenario === "unsupported") {
      for (const method of [...READ_METHODS, "usage.summary"]) fake.failNext(method, { code: -32601, message: "method unavailable" }, 20);
    }
    if (scenario === "error") {
      for (const method of [...READ_METHODS, "usage.summary"]) fake.failNext(method, { code: -32603, message: "boom" }, 20);
    }
    const { surfaces, stop } = await connect(fake, scenario !== "unavailable");
    stops.push(stop);
    return { surfaces, target: { kind: "project", projectId: ALPHA }, fake };
  },
};

type Read = [string, (b: Built) => Promise<SurfaceResult<unknown>>];
const READS: Read[] = [
  ["D9", (b) => b.surfaces.workstreams.workstreams()],
  ["D10", (b) => b.surfaces.conversation.transcript(b.target)],
  ["D11", (b) => b.surfaces.activity.activity()],
  ["D12", (b) => b.surfaces.usage.usage()],
  ["D13", (b) => b.surfaces.metrics.metrics()],
  ["D14", (b) => b.surfaces.models.catalog()],
  ["D15", (b) => b.surfaces.search.search("task")],
  ["D16", (b) => b.surfaces.decisions.queue()],
];

/** D12 reads `usage.summary` and D13 reads `host.metrics`; every daemon adapter is live, so only the scenario decides. */
function expectedStatus(kind: Kind, id: string, scenario: Scenario): string {
  if (kind === "daemon" && id === "D12" && scenario === "empty") return "ok";
  if (scenario === "ok") return "ok";
  if (scenario === "empty") return id === "D12" || (kind === "fixture" && id === "D13") ? "unknown" : "ok";
  if (scenario === "unsupported") return "unsupported";
  return "unavailable";
}

function emptyCollection(id: string, data: unknown): boolean {
  const d = data as Record<string, unknown[]> & { entries?: unknown[]; providers?: unknown[]; items?: unknown[]; hits?: unknown[] };
  switch (id) {
    case "D9":
      return (data as unknown[]).length === 0;
    case "D10":
      return d.entries!.length === 0;
    case "D11":
      return d.running!.length === 0 && d.queued!.length === 0 && d.attention!.length === 0;
    case "D13": {
      const meters = data as Record<string, { status: string }>;
      return ["cpu", "memory", "disk", "gpu"].every((key) => meters[key]!.status !== "ok");
    }
    case "D12":
      return (data as { coverage: unknown[] }).coverage.length === 0;
    case "D14":
      return d.providers!.length === 0;
    case "D15":
      return d.hits!.length === 0;
    case "D16":
      return d.items!.length === 0;
    default:
      return false;
  }
}

describe.each([fixtureHarness, derivedHarness])("D9-D16 contract: $kind adapters", (harness) => {
  it.each(["ok", "empty", "unavailable", "unsupported", "error"] as Scenario[])("scenario %s", async (scenario) => {
    const built = await harness.build(scenario);
    for (const [id, read] of READS) {
      const result = await read(built);
      const label = `${id}/${scenario}`;
      expect(result.status, label).toBe(expectedStatus(harness.kind, id, scenario));
      expect(result.provenance.id, label).toBe(id);
      if (result.status === "ok") {
        expect(result.provenance.kind, label).toBe(harness.kind);
        expect(Number.isNaN(Date.parse(result.observedAt)), label).toBe(false);
        expect(result.ageMs, label).toBeGreaterThanOrEqual(0);
        expect(result.data, label).toBeDefined();
        if (scenario === "empty") expect(emptyCollection(id, result.data), label).toBe(true);
      } else {
        expect(result.provenance.kind, label).toBe(harness.kind);
        expect(result.reason.length, label).toBeGreaterThan(0);
        expect("data" in result, label).toBe(false);
      }
    }
  });

  it("returns a numeric ageMs and never zero-fills unknown or unsupported Measured values", async () => {
    const built = await harness.build("ok");
    const activity = await built.surfaces.activity.activity();
    const usage = await built.surfaces.usage.usage();
    const metrics = await built.surfaces.metrics.metrics();
    const measuredValues: unknown[] = [];
    if (activity.status === "ok") measuredValues.push(activity.data.slots, activity.data.upNext, activity.data.dispatchPaused);
    if (usage.status === "ok") measuredValues.push(usage.data.reported, usage.data.estimated, usage.data.budget);
    if (metrics.status === "ok") measuredValues.push(metrics.data.cpu, metrics.data.memory, metrics.data.disk, metrics.data.gpu);
    expect(measuredValues.length).toBeGreaterThan(0);
    for (const value of measuredValues as Array<{ status: string; value?: unknown; ageMs?: number; reason?: string }>) {
      if (value.status === "ok") {
        expect(value.value).toBeDefined();
        expect(value.ageMs).toBeGreaterThanOrEqual(0);
      } else {
        expect("value" in value).toBe(false);
        expect(value.reason).toBeTruthy();
      }
    }
  });

  it("represents tasks with every state counted and the same view-model shape", async () => {
    const built = await harness.build("ok");
    const result = await built.surfaces.workstreams.workstreams();
    expect(result.status).toBe("ok");
    if (result.status !== "ok") return;
    expect(result.data.length).toBeGreaterThan(0);
    for (const workstream of result.data) {
      expect(Object.keys(workstream.counts).sort()).toEqual(
        ["Cancelled", "Created", "Failed", "Queued", "Running", "Succeeded", "Verifying", "VerificationFailed"].sort(),
      );
      const total = Object.values(workstream.counts).reduce((a, b) => a + b, 0);
      expect(total).toBe(workstream.tasks.length);
      expect(workstream.objective.status === "ok" || "reason" in workstream.objective).toBe(true);
    }
  });

  it("finds a task by objective through D15 and marks its origin", async () => {
    const built = await harness.build("ok");
    const result = await built.surfaces.search.search("session");
    expect(result.status).toBe("ok");
    if (result.status !== "ok") return;
    const task = result.data.hits.find((hit) => hit.kind === "task");
    expect(task?.title.toLowerCase()).toContain("session");
    expect(task?.origin).toBe(harness.kind === "fixture" ? "fixture" : "real");
    expect(result.data.skipped).toEqual([]);
  });

  // `watch` (D13 polling) exists on the live adapter only; the read methods must match.
  it("exposes the same adapter methods as the other adapter set", () => {
    const fixture = createFixtureSurfaces();
    const shape = (s: SurfaceSources) =>
      Object.fromEntries(Object.entries(s).map(([name, source]) => [name, Object.keys(source).filter((key) => key !== "watch").sort()]));
    return harness.build("ok").then((built) => expect(shape(built.surfaces)).toEqual(shape(fixture)));
  });
});
