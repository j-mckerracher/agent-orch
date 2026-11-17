/**
 * Fixture adapters for D9-D16 (M7-03). They read only the JSON under
 * `fixtures/ui/` and never send daemon requests. Every result carries
 * fixture provenance so screens label it PLACEHOLDER DATA. Illustrative
 * numbers from the wireframes live in that JSON and nowhere else.
 *
 * D15 search has no fixture of its own: `createFixtureSurfaces` builds it
 * over the fixture D9 and D16 sources (`surfaces/search.ts`).
 */
import { DaemonError } from "../../daemon/errors";
import activityFixture from "../../fixtures/ui/activity.json";
import conversationFixture from "../../fixtures/ui/conversation.json";
import decisionFixture from "../../fixtures/ui/decision-queue.json";
import modelFixture from "../../fixtures/ui/model-catalog.json";
import settingsFixture from "../../fixtures/ui/settings.json";
import metricsFixture from "../../fixtures/ui/system-metrics.json";
import usageFixture from "../../fixtures/ui/usage.json";
import localToolsFixture from "../../fixtures/ui/local-tools.json";
import workstreamFixture from "../../fixtures/ui/workstreams.json";
import type { LocalToolsProviderWire, LocalToolsSnapshotWire } from "../../daemon/usageWire";
import { countStates, measured, notOk, observation, okResult } from "./result";
import { wallClockMs, zoneDayStart } from "./localDay";
import { mapLocalTools, parseLocalTools } from "./localToolsLive";
import { createSearchSource } from "./search";
import type {
  ActionProposal,
  ActivitySnapshot,
  ActivitySource,
  ConversationEntry,
  ClearReceipt,
  ConversationSource,
  CoordinatorChangeReceipt,
  ConversationTarget,
  DecisionQueue,
  DecisionQueueSource,
  LocalToolsSnapshot,
  LocalToolsSource,
  ModelCatalog,
  ModelCatalogSource,
  Decision,
  Provenance,
  ProposalItem,
  ResolveReceipt,
  SendReceipt,
  MonitoringMode,
  MonitoringSource,
  MonitoringState,
  SettingsSource,
  WakeRecord,
  SettingsSnapshot,
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

/** Every state of the other-AI-tools fixture. `ok_unpriced` is what opting in shows in fixture mode. */
export const LOCAL_TOOLS_FIXTURE_STATES = [
  "off",
  "not_started",
  "ok",
  "ok_unpriced",
  "no_calls",
  "stale",
  "not_installed",
  "node_missing",
  "node_too_old",
  "unsupported_version",
  "integrity_mismatch",
  "timeout",
  "failed",
  "bad_output",
  "output_too_large",
  "wrong_day",
  "not_bundled",
] as const;
export type LocalToolsFixtureState = (typeof LOCAL_TOOLS_FIXTURE_STATES)[number];

export interface SurfaceFixtureOptions {
  scenario?: SurfaceFixtureScenario;
  /** Clock in epoch milliseconds; fixture observation ages are subtracted from it. */
  now?: () => number;
  /** Other AI tools' state at start. Defaults to opted out. The opt-in control changes it in memory. */
  localTools?: LocalToolsFixtureState;
  /** Whether the settings fixture shows the machine load gate open (the default) or closed. */
  settingsGate?: "open" | "closed";
}

type ProviderSet = "priced" | "one_unpriced" | "no_calls" | "not_run";

interface LocalToolsFixtureFile {
  timezone: string;
  pinned_version: string;
  providerSets: Record<ProviderSet, LocalToolsProviderWire[]>;
  states: Record<
    LocalToolsFixtureState,
    {
      enabled: boolean;
      reader: Omit<LocalToolsSnapshotWire["reader"], "pinned_version">;
      stale: boolean;
      /** Age of the figures; null when there are none. */
      observed_age_ms: number | null;
      providers: ProviderSet;
    }
  >;
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
  build: (data: D, nowMs: number) => T | null,
  empty: (data: D, nowMs: number) => T | null,
): Promise<SurfaceResult<T>> {
  const prov = provenance(id, envelope);
  if (ctx.scenario === "loading") return new Promise(() => undefined);
  if (ctx.scenario === "unavailable" || ctx.scenario === "unsupported" || ctx.scenario === "unknown") {
    return Promise.resolve(notOk(prov, ctx.scenario, envelope.scenarios[ctx.scenario].reason, observation(ctx.now())));
  }
  const nowMs = ctx.now();
  const { ageMs, data } = envelope.scenarios.success;
  const obs = observation(nowMs, ageMs);
  const value = ctx.scenario === "empty" ? empty(data, nowMs) : build(data, nowMs);
  return Promise.resolve(
    value === null ? notOk(prov, "unknown", "Fixture scenario: nothing has been observed yet.", observation(nowMs)) : okResult(prov, value, obs),
  );
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
        (data, nowMs) => {
          const observed = observation(nowMs, envelope.scenarios.success.ageMs);
          return stamp(data.workstreams, nowMs).map((item) => ({
            ...item,
            counts: measured(countStates((item.tasks as WorkstreamTask[]).map((task) => task.state)), observed),
          }));
        },
        () => [],
      ),
    simulateCreate: (title) => Promise.resolve(receipt("D9", envelope, ctx, counter, `workstream "${title}"`)),
    // Real mutations never run against placeholder data; the view offers simulateCreate in fixture mode.
    create: () => notMutable(),
    update: () => notMutable(),
    addObjective: () => notMutable(),
    reviseObjective: () => notMutable(),
    setGate: () => notMutable(),
    linkTask: () => notMutable(),
    unlinkTask: () => notMutable(),
  };
}

function notMutable(): Promise<never> {
  return Promise.reject(new DaemonError("method-unavailable", "Fixture workstreams cannot be changed; nothing was sent to the daemon."));
}

/** One deterministic notice and the automatic reply it led to, in the Global fixture transcript (DPES-1105). */
const FIXTURE_WAKE_TURN = "fixture:wake-turn-1";
const FIXTURE_WAKE_ENTRIES: ConversationEntry[] = [
  {
    id: "fixture:wake-msg-1",
    kind: "message",
    role: "user",
    text: "Task fixture:t-12ab (Fix the login bug) failed.",
    at: "2026-01-01T10:00:00Z",
    turnId: FIXTURE_WAKE_TURN,
    origin: "wake",
    sequence: 3,
  },
  {
    id: `${FIXTURE_WAKE_TURN}:reply`,
    kind: "message",
    role: "coordinator",
    text: "The login task failed on a missing test fixture. Retrying it after the fixture is restored should work.",
    at: "2026-01-01T10:00:05Z",
    turnId: FIXTURE_WAKE_TURN,
    sequence: 4,
  },
];
const FIXTURE_WAKE_RECORDS: WakeRecord[] = [
  {
    id: "fixture:wake-1",
    event: "task_failed",
    subject: "fixture:t-12ab",
    notice: "Task fixture:t-12ab (Fix the login bug) failed.",
    state: "handled",
    outcome: "replied",
    skipReason: null,
    turnId: FIXTURE_WAKE_TURN,
    anchorSequence: 2,
    notify: true,
    seen: false,
    createdAt: "2026-01-01T10:00:00Z",
  },
];

type ConversationData = { global: ConversationEntry[]; project: ConversationEntry[]; proposals: ActionProposal[] };

type TaskCardFields = Pick<ActionProposal, "kind" | "params" | "target"> & Partial<Pick<ActionProposal, "state" | "errorCode" | "denial">>;

/** A Global card on the fixture reply's turn, after the two workstream cards. */
function taskCard(index: number, fields: TaskCardFields): ActionProposal {
  return {
    id: `fixture:proposal-${index + 1}`,
    turnId: "fixture:turn-1",
    index,
    digest: String(index + 1).repeat(64),
    dependsOn: null,
    state: "pending",
    resultRef: null,
    errorCode: null,
    denial: null,
    updatedAt: "2026-01-01T09:00:05Z",
    ...fields,
  };
}

/** One card per task kind, and one the app denied (DPES-1105). Their ids and titles match the fixture tasks. */
const TASK_CARDS: ActionProposal[] = [
  taskCard(2, {
    kind: "task.propose",
    params: {
      project: "p-3fa9c1d2",
      objective: "Add unit tests for the retry module.\nRun them locally and report what failed.",
      agent: "codex",
      task_kind: "Implement",
      delivery: "LocalReview",
    },
    target: { projectName: "alpha" },
  }),
  taskCard(3, {
    kind: "task.cancel",
    params: { task_id: "fixture:task-auth-2" },
    target: { taskTitle: "Add refresh-token expiry", taskState: "Running" },
  }),
  taskCard(4, {
    kind: "task.retry",
    params: { task_id: "fixture:task-docs-1" },
    target: { taskTitle: "Rewrite the install guide", taskState: "Failed" },
  }),
  taskCard(5, {
    kind: "workstream.link_task",
    params: { workstream_id: "fixture:ws-docs", task_id: "fixture:task-auth-3" },
    target: { taskTitle: "Document the session model", taskState: "Queued" },
  }),
  taskCard(6, {
    kind: "task.propose",
    params: {
      project: "p-3fa9c1d2",
      objective: "Fix the flaky login test and open a pull request.",
      agent: "claude-code",
      task_kind: "Implement",
      delivery: "PullRequest",
    },
    target: { projectName: "alpha" },
    state: "failed",
    errorCode: "action_denied",
    denial: "delivery_not_authorized",
  }),
];

/** The Global conversation's cards. The JSON holds the workstream cards, written before a card had `denial` or `target`. */
function conversationProposals(data: ConversationData): ActionProposal[] {
  return [...data.proposals.map((proposal) => ({ ...proposal, denial: proposal.denial ?? null, target: proposal.target ?? null })), ...TASK_CARDS];
}

function conversationSource(ctx: Ctx): ConversationSource {
  const envelope = conversationFixture as unknown as Envelope<ConversationData>;
  const counter = { n: 0 };
  // Simulated clears (DPES-1105) empty a fixture transcript for the rest of the session only.
  const cleared = new Set<string>();
  const keyOf = (target: ConversationTarget) => JSON.stringify(target);
  // The fixture has no workstream transcripts; project notes must not stand in for one. Proposals are Global only.
  const transcriptOf = (target: ConversationTarget, entries: ConversationEntry[], proposals: ActionProposal[] = []) =>
    target.kind === "workstream"
      ? null
      : {
          target,
          // Fixture transcript sequences are positions; the Global fixture ends with one automatic update.
          entries: cleared.has(keyOf(target))
            ? []
            : target.kind === "global"
              ? [...entries.map((entry, index) => ({ ...entry, sequence: index + 1 })), ...FIXTURE_WAKE_ENTRIES]
              : entries,
          repliesExpected: true,
          truncated: false,
          proposals: target.kind === "global" && !cleared.has(keyOf(target)) ? proposals : [],
          proposalsMore: false,
          wake: target.kind === "global" && !cleared.has(keyOf(target)) ? FIXTURE_WAKE_RECORDS : [],
          wakeMore: false,
        };
  return {
    provenance: provenance("D10", envelope),
    transcript: (target: ConversationTarget) =>
      resolve<
        ConversationData,
        {
          target: ConversationTarget;
          entries: ConversationEntry[];
          repliesExpected: boolean;
          truncated: boolean;
          proposals: ActionProposal[];
          proposalsMore: boolean;
          wake: WakeRecord[];
          wakeMore: boolean;
        }
      >(
        "D10",
        envelope,
        ctx,
        (data) => transcriptOf(target, data[target.kind === "global" ? "global" : "project"], conversationProposals(data)),
        () => transcriptOf(target, []),
      ),
    // DPES-1105. Fixture cards stay pending: nothing is sent and the daemon is never asked.
    resolve: (_proposalId: string, decision: Decision, _digest: string): Promise<ResolveReceipt> =>
      Promise.resolve({
        simulated: true,
        provenance: provenance("D10", envelope),
        notice: decision === "approve" ? "Fixture data: nothing was created." : "Fixture data: nothing was changed.",
        proposal: null,
      }),
    send: (target: ConversationTarget, text: string): Promise<SendReceipt> => {
      const where = { global: "the coordinator", project: "a project", workstream: "a workstream" }[target.kind];
      const base = receipt("D10", envelope, ctx, counter, `message to ${where}`);
      return Promise.resolve({
        simulated: true,
        provenance: base.provenance,
        notice: `${base.notice} (${text.length} characters)`,
        at: base.at,
        entryId: `fixture:sent-${counter.n}`,
      });
    },
    clear: (target: ConversationTarget): Promise<ClearReceipt> => {
      const base = receipt("D10", envelope, ctx, counter, "history clear");
      cleared.add(keyOf(target));
      return Promise.resolve({ simulated: true, provenance: base.provenance, cleared: true, notice: base.notice });
    },
    removeCoordinator: (projectId: string): Promise<CoordinatorChangeReceipt> => {
      const base = receipt("D10", envelope, ctx, counter, `removal of the ${projectId} coordinator`);
      return Promise.resolve({ simulated: true, provenance: base.provenance, changed: true, removed: true, notice: base.notice });
    },
    restoreCoordinator: (projectId: string): Promise<CoordinatorChangeReceipt> => {
      const base = receipt("D10", envelope, ctx, counter, `restore of the ${projectId} coordinator`);
      return Promise.resolve({ simulated: true, provenance: base.provenance, changed: true, removed: false, notice: base.notice });
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
      resolve<UsageSummary, UsageSummary>(
        "D12",
        envelope,
        ctx,
        (data, nowMs) => ({
          ...stamp(data, nowMs),
          // The window is "since local midnight" (operational-design.md section 22), so it follows the clock.
          windowStart: (zoneDayStart(nowMs, data.timezone) ?? new Date(nowMs)).toISOString(),
        }),
        () => null,
      ),
  };
}

/** The local day (or its stand-in) the fixture figures cover, so the window follows the clock like the usage fixture. */
function localDayBounds(nowMs: number, timeZone: string): { start: number; end: number } {
  const start = (zoneDayStart(nowMs, timeZone) ?? new Date(nowMs)).getTime();
  // 36 hours after local midnight is inside the next local day, so that day's midnight is this day's end.
  const end = (zoneDayStart(start + 36 * 3_600_000, timeZone) ?? new Date(start + 86_400_000)).getTime();
  return { start, end };
}

/** One `usage.localTools` result for a fixture state, in the daemon's wire shape. */
function localToolsWire(name: LocalToolsFixtureState, nowMs: number): LocalToolsSnapshotWire {
  const file = localToolsFixture as unknown as LocalToolsFixtureFile;
  const state = file.states[name];
  const { start, end } = localDayBounds(nowMs, file.timezone);
  const utcOffsetSeconds = (wallClockMs(start, file.timezone) - start) / 1000;
  const observed = state.observed_age_ms === null ? null : nowMs - state.observed_age_ms;
  return {
    schema_version: 1,
    enabled: state.enabled,
    window: { start_unix_ms: start, end_unix_ms: end, timezone: file.timezone, utc_offset_seconds: utcOffsetSeconds },
    data_window: observed === null ? null : { start_unix_ms: start, end_unix_ms: end, timezone: file.timezone },
    reader: { ...state.reader, pinned_version: file.pinned_version },
    refreshing: false,
    observed_at_unix_ms: observed,
    stale: state.stale,
    providers: file.providerSets[state.providers],
  };
}

/**
 * Other AI tools' usage. The opt-in choice lives in memory, as the daemon keeps it: opting in shows
 * `ok_unpriced` and opting out hides every figure. Fixture mode never sends a request.
 */
function localToolsSource(ctx: Ctx, initial: LocalToolsFixtureState): LocalToolsSource {
  const provenance: Provenance = { kind: "fixture", id: "D12", fixture: "local-tools" };
  let current = initial;
  const read = (): Promise<SurfaceResult<LocalToolsSnapshot>> => {
    if (ctx.scenario === "loading") return new Promise(() => undefined);
    if (ctx.scenario === "unavailable" || ctx.scenario === "unsupported" || ctx.scenario === "unknown") {
      return Promise.resolve(notOk(provenance, ctx.scenario, `Fixture scenario: other AI tools are ${ctx.scenario}.`, observation(ctx.now())));
    }
    const nowMs = ctx.now();
    const snapshot = mapLocalTools(parseLocalTools(localToolsWire(current, nowMs)));
    return Promise.resolve(okResult(provenance, snapshot, observation(nowMs)));
  };
  return {
    provenance,
    local: read,
    setEnabled: (enabled) => {
      if (!enabled) current = "off";
      else if (current === "off") current = "ok_unpriced";
      return read();
    },
    refresh: read,
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
    // DPES-1105: fixture mode never runs the billable Copilot check. Say so instead of inventing a state.
    checkCopilotSignIn: () =>
      Promise.resolve(
        notOk(provenance("D14", envelope), "unavailable", "Fixture mode runs no Copilot sign-in check, so no call is spent.", observation(Date.now())),
      ),
    copilotSignInStatus: () =>
      Promise.resolve(
        notOk(provenance("D14", envelope), "unavailable", "Fixture mode has no Copilot sign-in record.", observation(Date.now())),
      ),
  };
}

/** The Global conversation's pending cards, as the queue lists them. The fixture has no workstream conversations. */
function pendingProposals(): ProposalItem[] {
  const { data } = (conversationFixture as unknown as Envelope<ConversationData>).scenarios.success;
  return conversationProposals(data)
    .filter((proposal) => proposal.state === "pending")
    .map((proposal): ProposalItem => ({ proposal, scope: { kind: "global" } }));
}

function decisionSource(ctx: Ctx): DecisionQueueSource {
  const envelope = decisionFixture as unknown as Envelope<Omit<DecisionQueue, "proposals">>;
  const counter = { n: 0 };
  return {
    provenance: provenance("D16", envelope),
    queue: () =>
      resolve<Omit<DecisionQueue, "proposals">, DecisionQueue>(
        "D16",
        envelope,
        ctx,
        (data) => ({ ...data, proposals: pendingProposals() }),
        (data) => ({ ...data, items: [], proposals: [] }),
      ),
    simulatePostpone: (id) => Promise.resolve(receipt("D16", envelope, ctx, counter, `postpone of ${id}`)),
    simulateDismiss: (id) => Promise.resolve(receipt("D16", envelope, ctx, counter, `dismissal of ${id}`)),
  };
}

/**
 * DPES-1105. Fixture mode shows the open gate, or the closed-gate example when asked, and saves nothing.
 * It never reaches a daemon, so a typed limit changes only what the page holds.
 */
function settingsSource(ctx: Ctx, gate: "open" | "closed"): SettingsSource {
  const envelope = settingsFixture as unknown as Envelope<Record<"open" | "closed", SettingsSnapshot>>;
  // The Phase 3 groups are the daemon's defaults (10 minutes, 1 minute, 120 minutes; 6 and 40 replies, no budget).
  const snapshotOf = (data: Record<"open" | "closed", SettingsSnapshot>): SettingsSnapshot => ({
    ...data[gate],
    supervision: { baseIntervalSecs: 600, minIntervalSecs: 60, attemptWallMinutes: 120 },
    wakeLimits: { hourlyCap: 6, dailyCap: 40, dailyTokenBudget: null },
  });
  const simulated = () =>
    Promise.resolve({
      simulated: true,
      provenance: provenance("D17", envelope),
      snapshot: snapshotOf(envelope.scenarios.success.data),
    });
  return {
    provenance: provenance("D17", envelope),
    get: () =>
      resolve<Record<"open" | "closed", SettingsSnapshot>, SettingsSnapshot>("D17", envelope, ctx, snapshotOf, snapshotOf),
    set: simulated,
    setGroups: simulated,
  };
}

/** Fixture monitoring: the mode lives in memory for the session, and nothing reaches a daemon. */
function monitoringSource(ctx: Ctx): MonitoringSource {
  const envelope = conversationFixture as unknown as Envelope<ConversationData>;
  const prov = provenance("D10", envelope);
  const state: MonitoringState = { mode: "on", paused: [], unseen: [{ target: { kind: "global" }, count: 1 }] };
  return {
    provenance: prov,
    get: () => resolve<ConversationData, MonitoringState>("D10", envelope, ctx, () => ({ ...state }), () => ({ ...state, unseen: [] })),
    set: (mode: MonitoringMode) => {
      state.mode = mode;
      return Promise.resolve({ simulated: true, provenance: prov, state: { ...state } });
    },
    ack: () => {
      state.unseen = [];
      return Promise.resolve();
    },
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
    localTools: localToolsSource(ctx, options.localTools ?? "off"),
    metrics: metricsSource(ctx),
    models: modelSource(ctx),
    search: createSearchSource(workstreams, decisions, ctx.now),
    decisions,
    settings: settingsSource(ctx, options.settingsGate ?? "open"),
    monitoring: monitoringSource(ctx),
  };
}
