/**
 * Frontend boundary interfaces D9-D16 for the operator-experience surfaces
 * (M7-03; registry rules in `docs/ui/wireframe.md`).
 * Architecture: `docs/technical-implementation-plan.md` at the repository root.
 *
 * These are frontend interfaces, not additional daemon methods.
 *
 * Every source has up to three adapters behind one interface:
 * - fixture (`surfaces/fixtures.ts`): PLACEHOLDER DATA, provenance `fixture`,
 *   ids prefixed `fixture:`. Never sends a daemon request. Illustrative
 *   wireframe numbers (20/24 slots, $26.00, CPU 44%) exist only here.
 * - derived (`surfaces/derived.ts`): reads methods the daemon already has,
 *   provenance `daemon` with the methods used by the selected source or target,
 *   including its writes. Missing capabilities are unsupported; unobserved
 *   values are unknown. It never serves fixture data.
 * - local (`surfaces/search.ts`): D15 only; indexes what D9 and D16 returned.
 *
 * Failure semantics (shared by every read):
 * - Reads resolve; they do not reject. A fixture "loading" scenario never resolves.
 * - `ok`: data was obtained; `observedAt`/`ageMs` say when. A known-empty
 *   collection is `ok` with an empty array.
 * - `unavailable`: the answer could not be obtained now (disconnected, timeout,
 *   daemon error). A retry may help. A failed live call is `unavailable` with
 *   daemon provenance and is never replaced by fixture data.
 * - `unsupported`: this daemon or platform has no such source (missing method
 *   or capability, or no daemon concept at all). Retrying will not help.
 * - `unknown`: no value has been observed, or the answer cannot establish it.
 * - Unknown is never zero: absent numbers are an `unknown`/`unsupported`
 *   `Measured`, never `0`. An observed empty collection is `[]`; an unread
 *   collection is unknown. Nullable identifiers and timestamps explicitly mark
 *   missing metadata, not a measured count.
 * - Simulated actions exist on fixture adapters only. Derived adapters reject
 *   them with `DaemonError("method-unavailable")` so no fixture action is
 *   silently inert and none can reach a real mutation.
 * - Every adapter result carries an observation. Non-ok observations date the status
 *   attempt, not a successful data read; they never imply retained data is fresh.
 * - The derived mutations are D10 `send` (`coordinator.send_message` or
 *   `conversations.send`), `cancel` (`conversations.cancel`), D9 `workstreams.*`
 *   writes and D17 `settings.set`. Ids prefixed `fixture:` are refused (`invalid-params`) before any request.
 */
import type { ApprovalState, CopilotSignInRecord, DeliveryPolicy, LoadMeterName, TaskKind, TaskState } from "../../daemon/types";
import type { UsageTokenWarningsWire } from "../../daemon/usageWire";
import type { Provenance, SimulatedReceipt, SourceId } from "../types";

export type { Provenance, SimulatedReceipt, SourceId };

export type SurfaceId = Extract<SourceId, "D9" | "D10" | "D11" | "D12" | "D13" | "D14" | "D15" | "D16" | "D17">;

export type SurfaceStatus = "ok" | "unavailable" | "unsupported" | "unknown";

/** When data or a non-ok status was observed. `ageMs` is relative to the adapter's clock at read time. */
export interface Observation {
  observedAt: string;
  ageMs: number;
}

/** Result of one source read; always carries the provenance to label it. */
export type SurfaceResult<T> =
  | ({ status: "ok"; data: T; provenance: Provenance } & Observation)
  | ({ status: Exclude<SurfaceStatus, "ok">; reason: string; provenance: Provenance } & Observation);

/** One value inside a snapshot, with its own state and observation age. */
export type Measured<T> =
  | ({ status: "ok"; value: T } & Observation)
  | { status: Exclude<SurfaceStatus, "ok">; reason: string };

export type SurfaceReceipt = SimulatedReceipt & { id: string };

// D9 -- WorkstreamSource. Derived: workstreams.list / workstreams.get joined with tasks.list (M7-07 methods).
export interface WorkstreamTask {
  id: string;
  /** Owning project of the linked task, as the daemon reports it. */
  projectId: string;
  /** The objective this task is linked under; null when linked to the workstream only. */
  objectiveId: string | null;
  /** Task contract objective text; empty when `tasks.list` no longer returns the linked task. */
  objective: string;
  state: TaskState;
  agent: string;
  dependencies: string[];
}
export type WorkstreamLifecycle = "active" | "suspended" | "archived";
export type GateState = "pending" | "verified" | "failed";
export interface WorkstreamGate {
  id: string;
  label: string;
  state: GateState;
  /** Task whose result evidences the gate, when the daemon recorded one. */
  evidenceTaskId: string | null;
}
export interface WorkstreamObjective {
  id: string;
  title: string;
  description: string;
  revision: number;
  gates: WorkstreamGate[];
}
export interface Workstream {
  /** Derived: the daemon workstream id. Fixture: `fixture:` id. */
  id: string;
  title: string;
  summary: string;
  /** Advisory: the scheduler does not consult it, so suspending does not stop dispatch. */
  state: WorkstreamLifecycle;
  /** Optimistic-concurrency token required by `workstreams.update`. */
  revision: number;
  projectIds: string[];
  /** The daemon's workstream id for `conversations.*`. Absent until the workstream lookup (M7-07) supplies it. */
  workstreamId?: string;
  tasks: WorkstreamTask[];
  /** A zero is a real count only when this result is `ok`; unread counts are unknown. */
  counts: Measured<Record<TaskState, number>>;
  /**
   * Set when the daemon listed this workstream but its detail read failed. Title, summary,
   * state and revision come from the list; tasks, counts and objectives are unknown, not empty.
   */
  unreadReason?: string;
  /** A workstream holds any number of objectives; a live workstream with none is `ok` with `[]`. */
  objectives: Measured<WorkstreamObjective[]>;
}
/** The daemon's acknowledgement of one workstream mutation. */
export interface WorkstreamMutation {
  id: string;
  /** Revision after the change, when the daemon returned one. */
  revision: number | null;
  at: string;
}
/** A plan edit that queued new tasks in place of `replaced` (empty for an added task). `id` is the first created task. */
export interface WorkstreamPlanEdit extends WorkstreamMutation {
  created: string[];
  replaced: string[];
}
/** A removed task; `cancelled` is true when it had not started and the daemon cancelled it first. */
export interface WorkstreamTaskRemoval extends WorkstreamMutation {
  cancelled: boolean;
}
/** The task fields `addTask` sends; the policy defaults are the submit form's. */
export interface WorkstreamNewTask {
  projectId: string;
  objective: string;
  kind: TaskKind;
  agent: string;
  /** The workstream objective to link the task under; omitted links it to the workstream only. */
  objectiveId?: string;
}
/** Execution policy fields of a task; `setTaskPolicy` sends only the ones given, and needs at least one. */
export interface WorkstreamTaskPolicy {
  permission?: PermissionLevel;
  autonomy?: "WorkspaceWrites" | "ApprovalRequired";
  delivery?: DeliveryPolicy;
  maxRetries?: number;
  maxRepairs?: number;
}
export interface WorkstreamSource {
  readonly provenance: Provenance;
  /** Lists daemon workstreams and linked tasks; unread details keep an explicit unknown reason. */
  workstreams(): Promise<SurfaceResult<Workstream[]>>;
  /*
   * Real mutations (`workstreams.*`). Fixture adapters reject them and derived adapters refuse
   * `fixture:` ids before any request. Every mutation carries an idempotency key; writes carry
   * the revision the caller last read, and a stale one is a `conflict` the caller resolves by re-reading.
   */
  update(
    target: { id: string; revision: number },
    change: { title?: string; summary?: string; state?: WorkstreamLifecycle },
  ): Promise<WorkstreamMutation>;
  /** Moves a workstream to the top of the daemon's list order. Advisory: the scheduler does not consult it. */
  prioritize(workstreamId: string): Promise<WorkstreamMutation>;
  addObjective(workstreamId: string, objective: { title: string; description?: string; gates?: string[] }): Promise<WorkstreamMutation>;
  reviseObjective(
    target: { id: string; revision: number },
    change: { title?: string; description?: string; addGates?: string[] },
  ): Promise<WorkstreamMutation>;
  setGate(
    target: { id: string; revision: number },
    gateId: string,
    state: GateState,
    evidenceTaskId?: string,
  ): Promise<WorkstreamMutation>;
  linkTask(workstreamId: string, taskId: string, objectiveId?: string): Promise<WorkstreamMutation>;
  unlinkTask(workstreamId: string, taskId: string): Promise<WorkstreamMutation>;
  /*
   * Plan edits (OX-W9). The daemon queues the new tasks, links them under the same objective and
   * cancels a replaced task that had not started; the replaced task stays linked as history.
   */
  addTask(workstreamId: string, task: WorkstreamNewTask): Promise<WorkstreamPlanEdit>;
  splitTask(workstreamId: string, taskId: string, objectives: string[]): Promise<WorkstreamPlanEdit>;
  combineTasks(workstreamId: string, taskIds: string[], objective: string): Promise<WorkstreamPlanEdit>;
  removeTask(workstreamId: string, taskId: string): Promise<WorkstreamTaskRemoval>;
  reassignTask(workstreamId: string, taskId: string, agent: string): Promise<WorkstreamPlanEdit>;
  setTaskPolicy(workstreamId: string, taskId: string, policy: WorkstreamTaskPolicy): Promise<WorkstreamPlanEdit>;
}

// D10 -- ConversationSource. Project notes use coordinator.get/send_message; Global and workstream turns use conversations.get/send/cancel.
export type ConversationTarget =
  | { kind: "global" }
  | { kind: "project"; projectId: string }
  /** M7-24: a daemon workstream (`conversations.*`), distinct from a project coordinator. */
  | { kind: "workstream"; workstreamId: string };
/** Targets with monitoring and action proposals: Global or a workstream. */
export type LiveConversationTarget = Exclude<ConversationTarget, { kind: "project" }>;
export interface ConversationEntry {
  id: string;
  /** `status` is a daemon-recorded turn outcome (failed, canceled, empty) and never a reply. `thinking` is a turn's agent reasoning. */
  kind: "message" | "proposal" | "status" | "thinking";
  role: "user" | "coordinator" | "system";
  text: string;
  at: string;
  /** Live Global and workstream targets only. */
  turnId?: string;
  /** Stable machine code of a failed turn, e.g. `runtime_unavailable`. */
  errorCode?: string;
  /** True while the daemon has not recorded the end of this reply. */
  streaming?: boolean;
  /** `wake` on the opening entry of an automatic turn: it is event lines, not something the user wrote (DPES-1105). */
  origin?: "wake";
  /** The daemon's transcript sequence (a reply: its last chunk). A wake record is placed by it. Live targets only. */
  sequence?: number;
}
/** The permission level a chat message runs at (DPES-1105). The daemon sets the level it ran with on each turn. */
export type PermissionLevel = "restricted" | "extended" | "auto" | "full";

/** The decision a person makes on a proposed action (DPES-1105). */
export type Decision = "approve" | "reject";

/** What the orchestrator's action block in one reply became (DPES-1105). */
export type ActionBlock = "accepted" | "rejected" | "stripped";

/**
 * One proposed action (DPES-1105). `kind` and `state` stay strings so an unknown value renders as unknown, never coerced.
 * `params` is the daemon's canonical parameters with the `kind` key removed.
 */
export interface ActionProposal {
  id: string;
  turnId: string;
  index: number;
  kind: string;
  params: Readonly<Record<string, unknown>>;
  /** The daemon's digest of the proposal's canonical parameters. A decision must send it back (DPES-1105). */
  digest: string;
  /** The parent proposal this one needs executed first; null when it stands alone. */
  dependsOn: string | null;
  state: string;
  /** The record the executed action created or changed; null until it runs. */
  resultRef: string | null;
  errorCode: string | null;
  /** The envelope's reason when `errorCode` is "action_denied"; null otherwise. */
  denial: string | null;
  /** Who approved it: a user, or a standing approval by id. Absent from older daemons and null until approved. */
  approvedBy?: { kind: "user" } | { kind: "grant"; grantId: string } | null;
  /** Display facts the daemon read live; null for kinds that need none. No paths. */
  target: ProposalTarget | null;
  updatedAt: string;
}

/** What the daemon read when it listed a task proposal: the project's folder name, or the task's title and state now. */
export interface ProposalTarget {
  projectName?: string | null;
  taskTitle?: string;
  taskState?: string;
  /** True when the task asks for less than the project's default delivery (DPES-1105). */
  belowProjectDefault?: boolean;
  /** The project's mode string; sent with `belowProjectDefault`. */
  projectMode?: string;
  /** Only on a `project.remove` card: the tasks kept in history, and how many of them are running now. */
  taskCount?: number;
  runningTasks?: number;
  /** Only on a `delivery.merge` card: the pull request as the daemon last read it. */
  pull?: PullFacts;
}

/** A pending proposal and the conversation that holds its card (DPES-1105). */
export interface ProposalItem {
  proposal: ActionProposal;
  scope: { kind: "global" } | { kind: "workstream"; workstreamId: string };
}

/** Outcome of approve or reject. `simulated` receipts changed nothing; `proposal` is what the daemon confirmed. */
export interface ResolveReceipt {
  simulated: boolean;
  provenance: Provenance;
  notice: string | null;
  proposal: ActionProposal | null;
}

/** The latest turn the daemon recorded for a live target. */
export interface ConversationTurnInfo {
  id: string;
  state: "running" | "completed" | "failed" | "cancelled";
  errorCode: string | null;
  runtime: { name: string; version: string };
  /** The level the daemon reports the agent ran with. Null when none was recorded (DPES-1105). */
  permissionLevel?: PermissionLevel | null;
  /** False when the turn ran a version the daemon has not verified (DPES-1106). */
  verified: boolean;
  /** The action block outcome of this turn's reply (DPES-1105). Absent when the daemon did not send it. */
  actionBlock?: ActionBlock | null;
  /** `wake` for an automatic turn the user did not ask for. Absent from an older daemon (DPES-1105). */
  trigger?: "user" | "wake";
}
export interface ConversationTranscript {
  target: ConversationTarget;
  entries: ConversationEntry[];
  /** False for legacy coordinator notes on a daemon without project conversations. */
  repliesExpected: boolean;
  clearGeneration?: number;
  /** True when paging stopped before the end of the log. */
  truncated: boolean;
  /** Latest runtime turn; null when nothing was sent yet. */
  turn?: ConversationTurnInfo | null;
  /** The scope's proposed actions, newest first (DPES-1105). Absent when the daemon did not send them. */
  proposals?: ActionProposal[];
  /** The action block outcome of each turn in the window, by turn id (DPES-1105). Absent when the daemon did not send it. */
  actionBlocks?: Readonly<Record<string, ActionBlock>>;
  /**
   * True when the daemon held more decided proposals than it sent. Every pending proposal is always sent, so no
   * pending card is missing; only older executed, rejected or failed ones may be (DPES-1105).
   */
  proposalsMore: boolean;
  /** The scope's wake records, oldest first (DPES-1105). Absent when the daemon did not send them. */
  wake?: WakeRecord[];
  /** True when the daemon held older wake records than it sent. */
  wakeMore?: boolean;
  /** Workstream targets only: tasks the global conversation routed here, newest first (DPES-1105). Absent when none. */
  routedIn?: RoutedIn[];
}
/** A task routed to a workstream by the global conversation (DPES-1105). Titles only: no path, token or output. */
export interface RoutedIn {
  taskId: string;
  title: string;
  routedAt: string;
  state: string;
}
export interface SendReceipt {
  /** True for fixture receipts: nothing was recorded anywhere. */
  simulated: boolean;
  provenance: Provenance;
  notice: string;
  at: string;
  /** Daemon entry id when recorded; `fixture:` id when simulated. */
  entryId: string;
  /** Live Global and workstream targets: the turn this message opened. */
  turn?: ConversationTurnInfo;
  /**
   * Informational only, never a block: the turn ran a version the daemon has not verified. Set once per agent and
   * version per app session (DPES-1106).
   */
  warning?: string;
}
export interface ConversationSource {
  readonly provenance: Provenance;
  /** Reads the target log. Project logs keep the oldest 50 pages and mark truncation. */
  transcript(target: ConversationTarget): Promise<SurfaceResult<ConversationTranscript>>;
  /**
   * Derived: all targets send through `conversations.send`.
   * Rejects for `fixture:` ids. Fixture: simulated. `selection` is the model choice for this message (DPES-1105).
   * `permission` is the level the message runs at. It is sent only when the daemon advertises `permission_levels`.
   */
  send(target: ConversationTarget, text: string, selection?: ModelChoice, permission?: PermissionLevel): Promise<SendReceipt>;
  /** `conversations.cancel` for a running turn. */
  cancel?(target: ConversationTarget, turnId: string): Promise<{ applied: boolean }>;
  /**
   * DPES-1105. Approves or rejects one proposed action through `actions.resolve`. `digest` is the proposal's
   * `digest` from the transcript. Fixture: simulated, nothing sent. The receipt's `proposal` is the state the
   * daemon confirmed; the caller reloads before showing a decided card.
   */
  resolve(proposalId: string, decision: Decision, digest: string): Promise<ResolveReceipt>;
  /**
   * DPES-1105. Permanently deletes one conversation's history: Global, workstream, or a project
   * coordinator's log. Fixture targets are simulated. Refused while a reply runs (`conversation-busy`).
   */
  clear?(target: ConversationTarget): Promise<ClearReceipt>;
  /** DPES-1105. Removes a project's coordinator from the targets and deletes its log. The project stays. */
  removeCoordinator?(projectId: string): Promise<CoordinatorChangeReceipt>;
  /** DPES-1105. Restores a removed coordinator; it starts empty. */
  restoreCoordinator?(projectId: string): Promise<CoordinatorChangeReceipt>;
}

/** Outcome of a history clear. `simulated` receipts deleted nothing. */
export interface ClearReceipt {
  simulated: boolean;
  provenance: Provenance;
  /** False when there was nothing to clear. */
  cleared: boolean;
  notice: string;
}

/** Outcome of a remove or restore. `simulated` receipts changed nothing. */
export interface CoordinatorChangeReceipt {
  simulated: boolean;
  provenance: Provenance;
  /** False when the coordinator was already in the requested state. */
  changed: boolean;
  /** True after a removal, false after a restore. */
  removed: boolean;
  notice: string;
}

// D11 -- ActivitySource. queue.snapshot is primary; tasks.list and approvals.list enrich it or provide a limited fallback.
export type WaitingReason = "dependency" | "capacity" | "approval" | "paused" | "unknown";
/** Why a queued task has not started. */
export interface QueueWait {
  reason: WaitingReason;
  /** Short phrase for task list rows, for example "Waiting for disk space (89% used)". */
  summary: string;
  /** Full sentence for the task page and the activity panel. */
  detail: string;
  nextStep?: string;
}
export interface QueueEntry {
  taskId: string;
  projectId: string;
  objective: string;
  agent: string;
  state: TaskState;
  /** Null for entries that are not waiting (running). `unknown` means the daemon reports no reason. */
  waiting: QueueWait | null;
}
export type AttentionPriority = "P1" | "P2" | "unknown";
export interface AttentionItem {
  approvalId: string;
  taskId: string;
  projectId: string | null;
  kind: string;
  priority: AttentionPriority;
}
export interface SlotUsage {
  used: number;
  limit: number;
}
export interface ActivitySnapshot {
  running: QueueEntry[];
  queued: QueueEntry[];
  /** Pending approvals only. */
  attention: AttentionItem[];
  /**
   * Queue limits require `queue.snapshot`; task-list fallback reports `unsupported`.
   * Updated: 2026-11-11 03:46:08 -0700 MST.
   */
  slots: Measured<SlotUsage>;
  upNext: Measured<string>;
  dispatchPaused: Measured<boolean>;
  /**
   * Partial reads remain unknown, not empty. After `queue.snapshot` succeeds, `tasks` marks missing
   * task details and Verifying rows; `approvals` marks unknown attention. The task-list fallback
   * requires `tasks.list`, so that failure fails the result. Its `approvals.list` read can degrade
   * attention. Updated: 2026-11-11 04:34:06 -0700 MST.
   */
  degraded?: { tasks?: string; approvals?: string };
}
export interface ActivitySource {
  readonly provenance: Provenance;
  /** Reads queue and attention; missing queue support limits dispatch metadata, never invents slots. */
  activity(): Promise<SurfaceResult<ActivitySnapshot>>;
  /** Fixture only. Real stopping is `tasks.cancel` on the task screen; no global pause exists. */
  simulatePauseDispatch(): Promise<SurfaceReceipt>;
  simulateStop(taskId: string): Promise<SurfaceReceipt>;
}

// D12 -- UsageSource. The live adapter (`surfaces/usageLive.ts`) reads `usage.summary`.
export interface UsageCoverage {
  provider: string;
  /**
   * How a provider's cost was reported: `partial` means only some of its sessions reported a
   * cost; `tokens-only` means tokens were reported but no cost. Both are live-adapter values.
   */
  reporting: "provider-reported" | "estimated" | "partial" | "tokens-only" | "none";
}
/** How many of the window's sessions reported a cost. Anything short of `complete` makes the headline a lower bound. */
export interface UsageCostCoverage {
  status: "unknown" | "partial" | "complete";
  sessions: number;
  sessionsWithCost: number;
}
/** Of the sessions that have tokens but no reported cost, how many the price table could price. Live adapter only. */
export interface UsageEstimateCoverage {
  status: "unknown" | "partial" | "complete";
  sessionsNeedingEstimate: number;
  sessionsEstimated: number;
}
/** Sessions with a reported or an estimated cost; the rest are unknown. Live adapter only. */
export interface UsageKnownCostCoverage {
  status: "unknown" | "partial" | "complete";
  sessions: number;
  sessionsWithKnownCost: number;
}
/** The native price table behind `estimated`; absent when the daemon predates it. */
export interface UsagePriceTable {
  version: string;
  basis: string;
  source?: string;
}
export interface UsageSummary {
  windowStart: string;
  timezone: string;
  currency: string;
  /** Live adapter only; fixtures omit it. */
  costCoverage?: UsageCostCoverage;
  /** Live adapter only, and only from a daemon with the price table. */
  estimateCoverage?: UsageEstimateCoverage;
  knownCostCoverage?: UsageKnownCostCoverage;
  priceTable?: UsagePriceTable;
  tokenWarnings?: UsageTokenWarningsWire;
  reported: Measured<number>;
  estimated: Measured<number>;
  budget: Measured<number>;
  coverage: UsageCoverage[];
}
export interface UsageSource {
  readonly provenance: Provenance;
  usage(): Promise<SurfaceResult<UsageSummary>>;
}

// D12 -- LocalToolsSource. Other AI tools' usage, read from their local session files and opt-in.
// Live: `usage.localTools*` (localToolsLive.ts). Fixture: PLACEHOLDER figures. These are estimates from
// list prices; they are never added to UsageSummary.
export type LocalToolsReaderState =
  | "off"
  | "not_started"
  | "ok"
  | "not_installed"
  | "node_missing"
  | "node_too_old"
  | "unsupported_version"
  | "integrity_mismatch"
  | "timeout"
  | "failed"
  | "bad_output"
  | "output_too_large"
  | "wrong_day"
  | "not_bundled";
export type LocalToolsProviderId = "claude-code" | "codex" | "github-copilot";
export interface LocalToolsProvider {
  id: LocalToolsProviderId;
  label: string;
  state: "ok" | "no_calls" | "not_run";
  sessions: number | null;
  calls: number | null;
  tokens: { input: number | null; output: number | null; cacheRead: number | null; cacheWrite: number | null };
  /** List-price estimate in USD. Null means no call had a price: unknown, never zero. */
  estimatedUsd: number | null;
  /** Models with tokens but no price. Their tokens are not in `estimatedUsd`. */
  unpricedModels: number;
  unpricedTokens: number;
  /** True only when every call had a price. */
  costComplete: boolean;
}
export interface LocalToolsWindow {
  startMs: number;
  endMs: number;
  timezone: string;
}
export interface LocalToolsReader {
  state: LocalToolsReaderState;
  version: string | null;
  pinnedVersion: string;
  nodeVersion: string | null;
  exitStatus: number | null;
  /** Where the reader came from: the app's bundle, an override variable, or PATH. Null when none was used. */
  source: "bundled" | "override" | "path" | null;
}
export interface LocalToolsSnapshot {
  enabled: boolean;
  /** The local day the request asked for. */
  window: LocalToolsWindow;
  /** The window the provider figures cover; null when there are none. */
  figures: LocalToolsWindow | null;
  reader: LocalToolsReader;
  refreshing: boolean;
  /** When the figures were last read; null when there are none. */
  observedAtMs: number | null;
  /** Figures older than the daemon's limit, or covering another window. Reader problems also make them stale in the view. */
  stale: boolean;
  /** Always three, in the order Claude Code, Codex, GitHub Copilot. */
  providers: LocalToolsProvider[];
}
export interface LocalToolsSource {
  readonly provenance: Provenance;
  /** The cached snapshot. The daemon never runs a reader on this read. */
  local(): Promise<SurfaceResult<LocalToolsSnapshot>>;
  /** Opt in or out. Turning it off hides every figure at once. */
  setEnabled(enabled: boolean): Promise<SurfaceResult<LocalToolsSnapshot>>;
  /** Asks the collector to refresh now; rate limited by the daemon. */
  refresh(): Promise<SurfaceResult<LocalToolsSnapshot>>;
}

// D13 -- SystemMetricsSource. Live: `host.metrics` (liveMetrics.ts). Fixture: wireframe numbers.
export interface MeterValue {
  value: number;
  unit: string;
  kind: "utilization" | "capacity";
  label: string;
}
export interface SystemMetrics {
  cpu: Measured<MeterValue>;
  memory: Measured<MeterValue>;
  disk: Measured<MeterValue>;
  /** No cross-platform source: expect `unsupported`, never zero. */
  gpu: Measured<MeterValue>;
}
export interface SystemMetricsSource {
  readonly provenance: Provenance;
  metrics(): Promise<SurfaceResult<SystemMetrics>>;
  /**
   * Live adapter only: delivers a fresh result now and then at the source's own sample interval,
   * and sends no request while the window is hidden. Returns the stop function. Sources without
   * it (fixtures) are read once through `metrics()`.
   */
  watch?(onResult: (result: SurfaceResult<SystemMetrics>) => void): () => void;
}

// D14 -- ModelCatalogSource. agents.catalog plus agents.capabilities; agents.validate_selection checks choices.
export interface ModelOption {
  id: string;
  label: string;
  /** M7-24: efforts this model supports (`agents.catalog`); empty when the agent reports adapter-wide efforts. */
  efforts?: string[];
  defaultEffort?: string | null;
}
export interface ModelProvider {
  chatCompatibility?: Partial<Record<PermissionLevel, { retryable: boolean; reason: string }>>;
  /** Derived: the daemon agent id. */
  id: string;
  name: string;
  installed: boolean;
  launchable: boolean;
  modelSelection: "supported" | "unsupported" | "unknown";
  models: Measured<ModelOption[]>;
  /** Where the model list comes from, as the daemon states it (for example, documented aliases). Present with an `ok` list. */
  modelsNote?: string;
  /** The daemon's reason this agent is not available, from its catalog row. Present when the row is not available. */
  detail?: string;
  /** Adapter-wide efforts; per-model efforts are on each `ModelOption`. */
  efforts: Measured<string[]>;
}
export interface ModelCatalog {
  providers: ModelProvider[];
}
/** The model choice a message is sent with (DPES-1105). An absent model or effort means the agent's default. */
export interface ModelChoice {
  agent: string;
  model?: string;
  effort?: string;
}
/** `agents.validate_selection` outcome: the daemon's own reason, never a guess. */
export type ModelSelectionCheck = { valid: true } | { valid: false; reason: string; detail: string };
export interface ModelCatalogSource {
  readonly provenance: Provenance;
  catalog(): Promise<SurfaceResult<ModelCatalog>>;
  /** Derived (M7-24): `agents.validate_selection`. Absent on fixture adapters. */
  validate?(agent: string, selection: { model?: string; effort?: string }): Promise<SurfaceResult<ModelSelectionCheck>>;
  /**
   * DPES-1105. The explicit, billable GitHub Copilot sign-in check. Only ever called from a user action,
   * with its cost shown first. Absent on fixture adapters, which never spend a call.
   */
  checkCopilotSignIn?(force: boolean): Promise<SurfaceResult<CopilotSignInRecord>>;
  /** DPES-1105. Zero-cost read of the sign-in record (from `agents.capabilities`); used while a check runs. */
  copilotSignInStatus?(): Promise<SurfaceResult<CopilotSignInRecord>>;
}

// D15 -- SearchSource. A local index over what D9 and D16 loaded; there is no fixture of its own.
export interface SearchHit {
  kind: "workstream" | "task" | "decision";
  id: string;
  title: string;
  detail: string;
  origin: "real" | "fixture";
}
export interface SearchResults {
  query: string;
  hits: SearchHit[];
  truncated: boolean;
  /** Inputs that could not be indexed; their records are NOT searched, so absence is not evidence. */
  skipped: Array<{ source: SurfaceId; status: Exclude<SurfaceStatus, "ok">; reason: string }>;
}
export interface SearchSource {
  readonly provenance: Provenance;
  search(query: string): Promise<SurfaceResult<SearchResults>>;
}

// D16 -- DecisionQueueSource. Derived: approvals.list (+ tasks.list for the project join).
export interface DecisionItem {
  id: string;
  taskId: string;
  projectId: string | null;
  kind: string;
  candidateCommit: string;
  state: ApprovalState["state"];
  priority: AttentionPriority;
  /** Null when no response time was observed; the derived approval API supplies none. */
  latestResponseAt: string | null;
}
export interface ActionSupport {
  available: boolean;
  reason: string;
}
export interface DecisionQueue {
  items: DecisionItem[];
  /** Pending proposed actions (DPES-1105). The user decides on the card in the conversation; the queue only links to it. */
  proposals: ProposalItem[];
  /** Approve and reject belong to the review screen; these extra actions have no daemon decision type. */
  actions: Record<"postpone" | "dismiss" | "escalate", ActionSupport>;
}
export interface DecisionQueueSource {
  readonly provenance: Provenance;
  queue(): Promise<SurfaceResult<DecisionQueue>>;
  /** Fixture only; derived adapters reject. */
  simulatePostpone(id: string): Promise<SurfaceReceipt>;
  simulateDismiss(id: string): Promise<SurfaceReceipt>;
}

// D17 -- SettingsSource. Derived: settings.get / settings.set (capability `settings`), the machine load limits and the gate they feed.
/** The three limits as whole percents from 50 to 100; 100 turns that meter's check off. */
export type LoadLimits = Record<LoadMeterName, number>;
/** What the gate knows of one meter: a 60 second mean, nothing yet, or a check that is off. */
export type LoadReading =
  | { state: "mean"; meanPercent: number }
  | { state: "unknown"; reason: "warming_up" | "unavailable" }
  | { state: "off" };
/** The meter that holds new task starts back, and the mean at which it lets them go again. */
export interface LoadHold {
  meter: LoadMeterName;
  meanPercent: number;
  limitPercent: number;
  reopenPercent: number;
}
/** Phase 3 stall-check settings. Absent from a daemon that cannot change them. */
export interface SupervisionSettings {
  baseIntervalSecs: number;
  minIntervalSecs: number;
  attemptWallMinutes: number;
}
/** Phase 3 limits on automatic replies per conversation. `null` is no limit. */
export interface WakeLimitSettings {
  hourlyCap: number | null;
  dailyCap: number | null;
  dailyTokenBudget: number | null;
}
/** What `settings.set` sends for the two Phase 3 groups. A key left out is unchanged; `null` clears a wake limit. */
export interface GroupsPatch {
  supervision?: Partial<SupervisionSettings>;
  wakeLimits?: Partial<WakeLimitSettings>;
  /** Whether chat may propose tasks that end in a pull request. */
  chatPrDelivery?: boolean;
  /** Whether the orchestrator chat gets the read-only task tools. */
  mcpEnabled?: boolean;
  /** The folder a task runs in when no project is clear. `null` clears it; a key left out leaves it alone. */
  homeDirectory?: string | null;
  dispatchDefaults?: DispatchDefaults;
}
export interface DispatchDefaults {
  defaultAgent: string | null;
  providers: Record<string, { model?: string; effort?: string }>;
}
export interface SettingsSnapshot {
  limits: LoadLimits;
  supervision?: SupervisionSettings;
  wakeLimits?: WakeLimitSettings;
  /** Absent from a daemon that cannot change it. */
  chatPrDelivery?: boolean;
  /** Absent from a daemon that cannot change it. */
  mcpEnabled?: boolean;
  /** The folder a task runs in when no project is clear; `null` while none is set. Absent from a daemon that cannot change it. */
  homeDirectory?: string | null;
  /** Absent from older daemons. Shared by new tasks across projects. */
  dispatchDefaults?: DispatchDefaults;
  gate: { state: "open" | "closed"; hold: LoadHold | null; readings: Record<LoadMeterName, LoadReading> };
  /** `disabled_by_environment` only on a test daemon: no load check runs there, so tasks start whatever the load. */
  feed: "sampler" | "disabled_by_environment";
}
export interface SettingsReceipt {
  /** True for fixture data: nothing was sent and nothing was saved. */
  simulated: boolean;
  provenance: Provenance;
  /** What the daemon holds after the change; for a simulated receipt, the fixture as it was. */
  snapshot: SettingsSnapshot;
}
export interface SettingsSource {
  readonly provenance: Provenance;
  get(): Promise<SurfaceResult<SettingsSnapshot>>;
  /** Sends only the fields in `patch`. Derived: `settings.set`, a user-only method; it rejects on failure. Fixture: saves nothing. */
  set(patch: Partial<LoadLimits>): Promise<SettingsReceipt>;
  /** Phase 3: sends only the supervision and wake-limit fields in `patch`. Same rules as `set`. */
  setGroups(patch: GroupsPatch): Promise<SettingsReceipt>;
}

// Monitoring (DPES-1105 Phase 3). Derived: monitoring.get / monitoring.set / monitoring.ack (capability `monitoring`).
/** The modes the user can choose. The daemon's own word stays a string in `MonitoringState`. */
export type MonitoringMode = "on" | "quiet" | "off" | "away";
export interface WakeRecord {
  id: string;
  /**
   * The daemon's event word: `task_failed`, `task_stalled`, `delivery_waiting`, `sign_in_needed`, `proposal_waiting`,
   * `scout_report_ready`, `candidate_ready`, `work_check` or `daemon_start`. An unknown word is kept.
   */
  event: string;
  /** The task id for a task event. */
  subject: string;
  notice: string;
  /** `noticed`, `pending`, `running`, `handled`, `skipped` or `failed`; kept as the daemon's string. */
  state: string;
  /**
   * `replied`, `quiet` or `unanswered`; null until handled. `unanswered` is a quiet reply that did not cover an outcome
   * event, so its notice stays visible.
   */
  outcome: string | null;
  /** `mode_off`, `limit_hourly`, `limit_daily`, `limit_tokens` or `agent_failing`; null unless skipped. */
  skipReason: string | null;
  turnId: string | null;
  anchorSequence: number;
  /** True when the record counts toward the badge. */
  notify: boolean;
  seen: boolean;
  createdAt: string;
}
export interface PausedTarget {
  target: LiveConversationTarget;
  /** `hourly`, `daily` or `tokens`; an unknown word is kept. */
  reason: string;
  /** RFC 3339 instant the pause is expected to end; null for a token budget. */
  until: string | null;
}
export interface UnseenTarget {
  target: LiveConversationTarget;
  count: number;
}
export interface MonitoringState {
  /** The daemon's word. Anything but on, quiet or off is shown as unknown. */
  mode: string;
  paused: PausedTarget[];
  unseen: UnseenTarget[];
}
export interface MonitoringReceipt {
  /** True for fixture data: nothing was sent and nothing was saved. */
  simulated: boolean;
  provenance: Provenance;
  state: MonitoringState;
}
export interface MonitoringSource {
  readonly provenance: Provenance;
  get(): Promise<SurfaceResult<MonitoringState>>;
  /** Derived: `monitoring.set`, a user-only method; it rejects on failure. Fixture: changes the in-memory mode only. */
  set(mode: MonitoringMode): Promise<MonitoringReceipt>;
  /** Derived: `monitoring.ack`. Fixture: nothing. */
  ack(target: LiveConversationTarget): Promise<void>;
  /** Derived: `monitoring.brief` (capability `standing_approvals`). Null data: no brief is due. Fixture: one brief. */
  brief(): Promise<SurfaceResult<Brief | null>>;
  /** Derived: `monitoring.brief_ack`, which only marks the brief seen. Fixture: nothing is saved and `simulated` is true. */
  briefAck(): Promise<{ simulated: boolean }>;
}

// Return brief (DPES-1105 Phase 5b). Counts, ids, short titles and state words only; never text an agent wrote.
export interface BriefCounts {
  tasksSucceeded: number;
  tasksFailed: number;
  tasksStalled: number;
  tasksRunning: number;
  proposalsApprovedByUser: number;
  proposalsApprovedByGrant: number;
  executedByGrant: number;
  failedUnderGrant: number;
  proposalsWaiting: number;
  deliveriesWaiting: number;
  decisionsHeld: number;
  grantUses: number;
  grantsExpired: number;
  grantsRevoked: number;
  quietUpdates: number;
  repliesPausedByLimit: number;
}

export interface BriefGrant {
  id: string;
  scopeKind: string;
  /** A project code or a cut workstream title. */
  scopeLabel: string;
  usesInWindow: number;
  usesUsed: number;
  maxUses: number;
  status: string;
}

export interface BriefItem {
  /** `proposal_waiting`, `delivery_waiting`, `task_failed`, `task_stalled` or `sign_in_needed`; an unknown word is kept. */
  kind: string;
  id: string;
  title: string;
  state: string;
  project: string | null;
  atMs: number;
}

export interface Brief {
  fromMs: number;
  toMs: number;
  /** True while the user is still away: the window ends now. */
  open: boolean;
  counts: BriefCounts;
  grants: BriefGrant[];
  waiting: BriefItem[];
  attention: BriefItem[];
}

// Holds (DPES-1105 Phase 4d). Derived: holds.list / holds.set / holds.release (capability `holds`).
/** What a hold hides: all pending approvals of a task (`approval`, the subject id is the task id) or one action card (`proposal`). */
export type HoldSubjectKind = "approval" | "proposal";

/** One hold in force. The kind and state stay the daemon's strings so an unknown value is never coerced. */
export interface HoldItem {
  id: string;
  subjectKind: string;
  subjectId: string;
  /** `YYYY-MM-DD`: the day the decision comes back. */
  untilDate: string;
  state: string;
}

export interface HoldReceipt {
  /** True fixture data: nothing was sent and nothing was saved. */
  simulated: boolean;
  provenance: Provenance;
  /** "Fixture data: nothing was changed." for a simulated receipt; null from the daemon. */
  notice: string | null;
  hold: HoldItem;
}

// Pull requests (DPES-1105 Phase 4c). Derived: pulls.list / pulls.refresh (capability `landing`), read only.
/** One check of a pull request. `name` is remote text: show it as plain text only. */
export interface PullCheck {
  name: string;
  /** The host's word for the outcome, as sent. */
  result: string;
}

/** The facts a merge card and a list row share. */
export interface PullFacts {
  mergeMethod: string;
  requiredTotal: number;
  requiredPassed: number;
  requiredFailed: number;
  checks: PullCheck[];
  observedAgeSecs: number;
  /** The refusal code a merge would meet now (for example `checks_red`); null when it would be allowed. */
  refusal: string | null;
}

/** One watched pull request. `title`, `branch` and `baseBranch` are remote text: show them as plain text only. */
export interface PullItem {
  taskId: string;
  number: number;
  /** open, merged or closed; kept as the daemon's string so an unknown value is never coerced. */
  state: string;
  /** The full 40 character commit. */
  headSha: string;
  baseBranch: string;
  branch: string;
  mergeable: "yes" | "no" | "unknown";
  requiredTotal: number;
  requiredPassed: number;
  requiredFailed: number;
  checks: PullCheck[];
  title: string;
  observedAgeSecs: number;
  /** The task's merge request, when one was made; null otherwise. */
  merge: { state: string; refusal: string | null } | null;
}

export interface PullsSource {
  readonly provenance: Provenance;
  /** The watched pull requests. Derived: `pulls.list`. Fixture: sample rows. */
  list(): Promise<SurfaceResult<PullItem[]>>;
  /** Derived: `pulls.refresh`, a read the daemon rate limits per task; it rejects on failure. Fixture: the row as it was. */
  refresh(taskId: string): Promise<PullItem>;
}

export interface HoldsSource {
  readonly provenance: Provenance;
  /** The holds in force now. Derived: `holds.list`. Fixture: one held decision. */
  list(): Promise<SurfaceResult<HoldItem[]>>;
  /** Derived: `holds.set`, a user-only method; it rejects on failure. Fixture: changes in-memory holds only. */
  set(subjectKind: HoldSubjectKind, subjectId: string, until: string): Promise<HoldReceipt>;
  /** Derived: `holds.release`, user-only. Fixture: changes in-memory holds only. */
  release(holdId: string): Promise<HoldReceipt>;
}

// Project defaults (DPES-1105 Phase 4). Derived: projects.get_defaults / projects.set_defaults (capability `projects`).
/** A project's delivery defaults. The mode and merge method stay the daemon's strings; the form offers the known ones. */
export interface ProjectDefaults {
  /** `local-only`, `direct-pr` or `no-mistakes`. */
  mode: string;
  baseBranch: string | null;
  branchPrefix: string | null;
  /** Argument arrays; a command never reaches a shell. */
  checks: string[][];
  /** `merge`, `squash` or `rebase`. */
  mergeMethod: string;
}
export interface ProjectDefaultsReceipt {
  /** True for fixture data: nothing was sent and nothing was saved. */
  simulated: boolean;
  provenance: Provenance;
  /** What the daemon holds after the change; for a simulated receipt, what was typed. */
  defaults: ProjectDefaults;
}
export interface ProjectDefaultsSource {
  readonly provenance: Provenance;
  get(projectId: string): Promise<SurfaceResult<ProjectDefaults>>;
  /** Derived: `projects.set_defaults`, a user-only method; it rejects on failure. Fixture: saves nothing. */
  set(projectId: string, defaults: ProjectDefaults): Promise<ProjectDefaultsReceipt>;
}

// Standing approvals (DPES-1105 Phase 5). Derived: grants.list / grants.create / grants.revoke (capability `standing_approvals`).
/** One standing approval. The status stays the daemon's string so a newer state is shown as unknown, never coerced. */
export interface GrantItem {
  id: string;
  /** `project` (the id is a short code) or `workstream`. */
  scopeKind: string;
  scopeId: string;
  /** A project's folder name or a workstream's title; never a path. Null when the daemon sent none. */
  scopeLabel: string | null;
  kinds: string[];
  createdAtMs: number;
  expiresAtMs: number;
  maxUses: number;
  usesUsed: number;
  tokenBudget: number;
  /** Tokens reported so far by the tasks the grant started; tasks that report none count zero. */
  tokensSpent: number;
  /** `active`, `expired`, `revoked`, `exhausted` or `budget_exhausted`. */
  status: string;
  revokedAtMs: number | null;
  /** Merge grant only: the head branch must start with this. Null on any other grant. */
  branchPrefix?: string | null;
}

/** The limits of an unattended merge grant (N8). A limit the daemon did not send is undefined and the form falls back to the core's. */
export interface MergePolicy {
  defaultUses?: number;
  maxUses?: number;
  maxExpiryHours?: number;
}

/** The kinds and limits the daemon allows; the form offers nothing else. */
export interface GrantPolicy {
  mergePolicy?: MergePolicy | null;
  grantableKinds: string[];
  defaultExpiresInSecs: number;
  maxExpiresInSecs: number;
  minExpiresInSecs: number;
  defaultMaxUses: number;
  maxMaxUses: number;
  maxTokenBudget: number;
}

export interface GrantsSnapshot {
  grants: GrantItem[];
  policy: GrantPolicy;
}

/** What the form sends. The scope id is a project code or a workstream id. */
export interface GrantTerms {
  scopeKind: "project" | "workstream";
  scopeId: string;
  kinds: string[];
  expiresInSecs: number;
  maxUses: number;
  tokenBudget: number;
  /** Merge grant only: the person ticked the danger confirmation. */
  confirmUnattendedMerge?: boolean;
  /** Merge grant only: empty text is not sent. */
  branchPrefix?: string | null;
}

export interface GrantReceipt {
  /** True for fixture data: nothing was sent and nothing was saved. */
  simulated: boolean;
  provenance: Provenance;
  /** "Fixture data: nothing was saved." for a simulated receipt; null from the daemon. */
  notice: string | null;
  grant: GrantItem;
  /** Revoke only: approved actions that had not started and were stopped. */
  stopped: number;
}

export interface GrantsSource {
  readonly provenance: Provenance;
  /** The grants (ended ones included) and the policy. Derived: `grants.list`. Fixture: one fixture grant and the daemon's limits. */
  list(): Promise<SurfaceResult<GrantsSnapshot>>;
  /** Derived: `grants.create`, a user-only method; it rejects on failure. Fixture: saves nothing. */
  create(terms: GrantTerms): Promise<GrantReceipt>;
  /** Derived: `grants.revoke`, user-only. Fixture: saves nothing. */
  revoke(grantId: string): Promise<GrantReceipt>;
}

// Orchestrator notes (DPES-1105 5b). Derived: notes.list / notes.set (capability `standing_approvals`).
/** One scope's notes in order and the limits the editor must respect. `scope` is `global` or `workstream:` plus an id. */
export interface NotesSnapshot {
  scope: string;
  notes: string[];
  maxNotes: number;
  maxBytes: number;
}

export interface NotesReceipt {
  /** True for fixture data: nothing was sent and nothing was saved. */
  simulated: boolean;
  provenance: Provenance;
  /** "Fixture data: nothing was saved." for a simulated receipt; null from the daemon. */
  notice: string | null;
  /** The list as the daemon holds it now. Fixture: the list as sent, which was not saved. */
  snapshot: NotesSnapshot;
}

export interface NotesSource {
  readonly provenance: Provenance;
  /** Derived: `notes.list`. Fixture: two sample notes, or none for the `empty` scenario. */
  list(scope: string): Promise<SurfaceResult<NotesSnapshot>>;
  /** Derived: `notes.set`, a user-only method that replaces the whole list; it rejects on failure. Fixture: saves nothing. */
  set(scope: string, notes: string[]): Promise<NotesReceipt>;
}

export interface SurfaceSources {
  workstreams: WorkstreamSource;
  conversation: ConversationSource;
  activity: ActivitySource;
  usage: UsageSource;
  localTools: LocalToolsSource;
  metrics: SystemMetricsSource;
  models: ModelCatalogSource;
  search: SearchSource;
  decisions: DecisionQueueSource;
  settings: SettingsSource;
  monitoring: MonitoringSource;
  holds: HoldsSource;
  pulls: PullsSource;
  projectDefaults: ProjectDefaultsSource;
  grants: GrantsSource;
  notes: NotesSource;
}
