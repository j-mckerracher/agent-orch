/**
 * Fixture adapters for D9-D16 (M7-03). They read only the JSON under
 * `fixtures/ui/` and never import the daemon client. Every result carries
 * fixture provenance so screens label it PLACEHOLDER DATA. Illustrative
 * numbers from the wireframes live in that JSON and nowhere else.
 *
 * D15 search has no fixture of its own: `createFixtureSurfaces` builds it
 * over the fixture D9 and D16 sources (`surfaces/search.ts`).
 */
import activityFixture from "../../fixtures/ui/activity.json";
import conversationFixture from "../../fixtures/ui/conversation.json";
import decisionFixture from "../../fixtures/ui/decision-queue.json";
import modelFixture from "../../fixtures/ui/model-catalog.json";
import metricsFixture from "../../fixtures/ui/system-metrics.json";
import usageFixture from "../../fixtures/ui/usage.json";
import workstreamFixture from "../../fixtures/ui/workstreams.json";
import { countStates, notOk, observation, okResult } from "./result";
import { createSearchSource } from "./search";
import type {
  ActivitySnapshot,
  ActivitySource,
  ConversationEntry,
  ConversationSource,
  ConversationTarget,
  DecisionQueue,
  DecisionQueueSource,
  ModelCatalog,
  ModelCatalogSource,
  Provenance,
  SendReceipt,
  SurfaceId,
  SurfaceReceipt,
  SurfaceResult,
  SurfaceSources,
  SystemMetrics,
  SystemMetricsSource,
  UsageSource,
  UsageSummary,
  Workstream,
  WorkstreamSource,
  WorkstreamTask,
} from "./types";

export type SurfaceFixtureScenario = "success" | "empty" | "loading" | "unavailable" | "unsupported" | "unknown";

export interface SurfaceFixtureOptions {
  scenario?: SurfaceFixtureScenario;
  /** Clock in epoch milliseconds; fixture observation ages are subtracted from it. */
  now?: () => number;
}

interface Envelope<D> {
  fixture: string;
  scenarios: {
    success: { ageMs: number; data: D };
    unavailable: { reason: string };
    unsupported: { reason: string };
    unknown: { reason: string };
  };
}

type Ctx = { scenario: SurfaceFixtureScenario; now: () => number };

const provenance = (id: SurfaceId, envelope: { fixture: string }): Provenance => ({
  kind: "fixture",
  id,
  fixture: envelope.fixture,
});

/** Fixture JSON stores `ageMs`; stamp each ok `Measured` with `observedAt` from the clock. */
function stamp<T>(value: T, nowMs: number): T {
  if (Array.isArray(value)) return value.map((item) => stamp(item, nowMs)) as unknown as T;
  if (value && typeof value === "object") {
    const record = value as Record<string, unknown>;
    const out: Record<string, unknown> = {};
    for (const [key, item] of Object.entries(record)) out[key] = stamp(item, nowMs);
    if (record.status === "ok" && typeof record.ageMs === "number") {
      out.observedAt = new Date(nowMs - record.ageMs).toISOString();
    }
    return out as T;
  }
  return value;
}

function resolve<D, T>(
  id: SurfaceId,
  envelope: Envelope<D>,
  ctx: Ctx,
  build: (data: D, nowMs: number) => T,
  empty: (data: D, nowMs: number) => T | null,
): Promise<SurfaceResult<T>> {
  const prov = provenance(id, envelope);
  if (ctx.scenario === "loading") return new Promise(() => undefined);
  if (ctx.scenario === "unavailable" || ctx.scenario === "unsupported" || ctx.scenario === "unknown") {
    return Promise.resolve(notOk(prov, ctx.scenario, envelope.scenarios[ctx.scenario].reason));
  }
  const nowMs = ctx.now();
  const { ageMs, data } = envelope.scenarios.success;
  const obs = observation(nowMs, ageMs);
  if (ctx.scenario === "empty") {
    const value = empty(data, nowMs);
    return Promise.resolve(
      value === null ? notOk(prov, "unknown", "Fixture scenario: nothing has been observed yet.") : okResult(prov, value, obs),
    );
  }
  return Promise.resolve(okResult(prov, build(data, nowMs), obs));
}

function receipt(id: SurfaceId, envelope: { fixture: string }, ctx: Ctx, counter: { n: number }, what: string): SurfaceReceipt {
  counter.n += 1;
  return {
    simulated: true,
    provenance: provenance(id, envelope),
    notice: `Simulated ${what}: placeholder data, nothing was changed.`,
    at: new Date(ctx.now()).toISOString(),
    id: `fixture:receipt-${counter.n}`,
  };
}

function workstreamSource(ctx: Ctx): WorkstreamSource {
  const envelope = workstreamFixture as unknown as Envelope<{ workstreams: Array<Omit<Workstream, "counts">> }>;
  const counter = { n: 0 };
  return {
    provenance: provenance("D9", envelope),
    workstreams: () =>
      resolve<{ workstreams: Array<Omit<Workstream, "counts">> }, Workstream[]>(
        "D9",
        envelope,
        ctx,
        (data, nowMs) =>
          stamp(data.workstreams, nowMs).map((item) => ({
            ...item,
            counts: countStates((item.tasks as WorkstreamTask[]).map((task) => task.state)),
          })),
        () => [],
      ),
    simulateCreate: (title) => Promise.resolve(receipt("D9", envelope, ctx, counter, `workstream "${title}"`)),
  };
}

function conversationSource(ctx: Ctx): ConversationSource {
  type Data = { global: ConversationEntry[]; project: ConversationEntry[] };
  const envelope = conversationFixture as unknown as Envelope<Data>;
  const counter = { n: 0 };
  return {
    provenance: provenance("D10", envelope),
    transcript: (target: ConversationTarget) =>
      resolve<Data, { target: ConversationTarget; entries: ConversationEntry[]; repliesExpected: boolean; truncated: boolean }>(
        "D10",
        envelope,
        ctx,
        (data) => ({ target, entries: data[target.kind], repliesExpected: true, truncated: false }),
        () => ({ target, entries: [], repliesExpected: true, truncated: false }),
      ),
    send: (target: ConversationTarget, text: string): Promise<SendReceipt> => {
      const base = receipt("D10", envelope, ctx, counter, `message to ${target.kind === "global" ? "the coordinator" : "a project"}`);
      return Promise.resolve({
        simulated: true,
        provenance: base.provenance,
        notice: `${base.notice} (${text.length} characters)`,
        at: base.at,
        entryId: `fixture:sent-${counter.n}`,
      });
    },
  };
}

function activitySource(ctx: Ctx): ActivitySource {
  const envelope = activityFixture as unknown as Envelope<ActivitySnapshot>;
  const counter = { n: 0 };
  return {
    provenance: provenance("D11", envelope),
    activity: () =>
      resolve<ActivitySnapshot, ActivitySnapshot>(
        "D11",
        envelope,
        ctx,
        (data, nowMs) => stamp(data, nowMs),
        (data, nowMs) => ({ ...stamp(data, nowMs), running: [], queued: [], attention: [] }),
      ),
    simulatePauseDispatch: () => Promise.resolve(receipt("D11", envelope, ctx, counter, "pause of dispatch")),
    simulateStop: (taskId) => Promise.resolve(receipt("D11", envelope, ctx, counter, `stop of ${taskId}`)),
  };
}

function usageSource(ctx: Ctx): UsageSource {
  const envelope = usageFixture as unknown as Envelope<UsageSummary>;
  return {
    provenance: provenance("D12", envelope),
    usage: () =>
      resolve<UsageSummary, UsageSummary>("D12", envelope, ctx, (data, nowMs) => stamp(data, nowMs), () => null),
  };
}

function metricsSource(ctx: Ctx): SystemMetricsSource {
  const envelope = metricsFixture as unknown as Envelope<SystemMetrics>;
  return {
    provenance: provenance("D13", envelope),
    metrics: () =>
      resolve<SystemMetrics, SystemMetrics>("D13", envelope, ctx, (data, nowMs) => stamp(data, nowMs), () => null),
  };
}

function modelSource(ctx: Ctx): ModelCatalogSource {
  const envelope = modelFixture as unknown as Envelope<ModelCatalog>;
  return {
    provenance: provenance("D14", envelope),
    catalog: () =>
      resolve<ModelCatalog, ModelCatalog>(
        "D14",
        envelope,
        ctx,
        (data, nowMs) => stamp(data, nowMs),
        () => ({ providers: [] }),
      ),
  };
}

function decisionSource(ctx: Ctx): DecisionQueueSource {
  const envelope = decisionFixture as unknown as Envelope<DecisionQueue>;
  const counter = { n: 0 };
  return {
    provenance: provenance("D16", envelope),
    queue: () =>
      resolve<DecisionQueue, DecisionQueue>(
        "D16",
        envelope,
        ctx,
        (data) => data,
        (data) => ({ ...data, items: [] }),
      ),
    simulatePostpone: (id) => Promise.resolve(receipt("D16", envelope, ctx, counter, `postpone of ${id}`)),
    simulateDismiss: (id) => Promise.resolve(receipt("D16", envelope, ctx, counter, `dismissal of ${id}`)),
  };
}

export function createFixtureSurfaces(options: SurfaceFixtureOptions = {}): SurfaceSources {
  const ctx: Ctx = { scenario: options.scenario ?? "success", now: options.now ?? Date.now };
  const workstreams = workstreamSource(ctx);
  const decisions = decisionSource(ctx);
  return {
    workstreams,
    conversation: conversationSource(ctx),
    activity: activitySource(ctx),
    usage: usageSource(ctx),
    metrics: metricsSource(ctx),
    models: modelSource(ctx),
    search: createSearchSource(workstreams, decisions, ctx.now),
    decisions,
  };
}
