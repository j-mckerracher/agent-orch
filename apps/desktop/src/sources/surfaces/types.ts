/**
 * Frontend boundary interfaces D9-D16 for the operator-experience surfaces
 * (M7-03; registry in GAP-ANALYSIS.md section 8.2, rules in `docs/ui/wireframe.md`).
 *
 * These are NOT daemon methods and add none. Plan:
 * /Users/mckerracher.joshua/Code/agent-orch-worktrees/mckerracher-joshua-mclm-friendly-guide/docs/technical-implementation-plan.md
 *
 * Every source has up to three adapters behind one interface:
 * - fixture (`surfaces/fixtures.ts`): PLACEHOLDER DATA, provenance `fixture`,
 *   ids prefixed `fixture:`. Never imports the daemon client. Illustrative
 *   wireframe numbers (20/24 slots, $26.00, CPU 41%) exist only here.
 * - derived (`surfaces/derived.ts`): reads methods the daemon already has,
 *   provenance `daemon` with the `methods` it read. Where the daemon has no
 *   data it answers `unsupported`; it never serves fixture data.
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
 * - `unknown`: the source answered but cannot establish the value.
 * - Unknown is never zero: absent numbers are an `unknown`/`unsupported`
 *   `Measured`, never `0`, `null` or an empty list standing in for "no data".
 * - Simulated actions exist on fixture adapters only. Derived adapters reject
 *   them with `DaemonError("method-unavailable")` so no fixture action is
 *   silently inert and none can reach a real mutation.
 * - The one derived mutation is D10 `send` (`coordinator.send_message`). Ids
 *   prefixed `fixture:` are refused (`invalid-params`) before any request.
 */
import type { ApprovalState, TaskState } from "../../daemon/types";
import type { Provenance, SimulatedReceipt, SourceId } from "../types";

export type { Provenance, SimulatedReceipt, SourceId };

export type SurfaceId = Extract<SourceId, "D9" | "D10" | "D11" | "D12" | "D13" | "D14" | "D15" | "D16">;

export type SurfaceStatus = "ok" | "unavailable" | "unsupported" | "unknown";

/** When a value was observed. `ageMs` is relative to the adapter's clock at read time. */
export interface Observation {
  observedAt: string;
  ageMs: number;
}

/** Result of one source read; always carries the provenance to label it. */
export type SurfaceResult<T> =
  | ({ status: "ok"; data: T; provenance: Provenance } & Observation)
  | { status: Exclude<SurfaceStatus, "ok">; reason: string; provenance: Provenance };

/** One value inside a snapshot, with its own state and observation age. */
export type Measured<T> =
  | ({ status: "ok"; value: T } & Observation)
  | { status: Exclude<SurfaceStatus, "ok">; reason: string };

export type SurfaceReceipt = SimulatedReceipt & { id: string };

// D9 -- WorkstreamSource. Derived: one workstream per project (projects.list + tasks.list).
export interface WorkstreamTask {
  id: string;
  objective: string;
  state: TaskState;
  agent: string;
  dependencies: string[];
}
export interface WorkstreamObjective {
  text: string;
  revision: number;
  gates: Array<{ id: string; label: string; passed: boolean }>;
}
export interface Workstream {
  /** Derived: the project id (canonical repository path). Fixture: `fixture:` id. */
  id: string;
  title: string;
  grouping: "project" | "objective";
  projectIds: string[];
  tasks: WorkstreamTask[];
  /** Every state is present; a zero here is a real zero over the returned tasks. */
  counts: Record<TaskState, number>;
  /** Objectives and revisions have no daemon record: derived adapters answer `unsupported`. */
  objective: Measured<WorkstreamObjective>;
}
export interface WorkstreamSource {
  readonly provenance: Provenance;
  workstreams(): Promise<SurfaceResult<Workstream[]>>;
  /** Fixture only. Derived adapters reject: the daemon cannot create workstreams. */
  simulateCreate(title: string): Promise<SurfaceReceipt>;
}

// D10 -- ConversationSource. Derived: per-project coordinator.get / coordinator.send_message.
export type ConversationTarget = { kind: "global" } | { kind: "project"; projectId: string };
export interface ConversationEntry {
  id: string;
  kind: "message" | "proposal";
  role: "user" | "coordinator";
  text: string;
  at: string;
}
export interface ConversationTranscript {
  target: ConversationTarget;
  entries: ConversationEntry[];
  /** False for the daemon's coordinator: it records messages and nothing replies. */
  repliesExpected: boolean;
  /** True when paging stopped before the end of the log. */
  truncated: boolean;
}
export interface SendReceipt {
  /** True for fixture receipts: nothing was recorded anywhere. */
  simulated: boolean;
  provenance: Provenance;
  notice: string;
  at: string;
  /** Daemon entry id when recorded; `fixture:` id when simulated. */
  entryId: string;
}
export interface ConversationSource {
  readonly provenance: Provenance;
  transcript(target: ConversationTarget): Promise<SurfaceResult<ConversationTranscript>>;
  /**
   * Derived: records a note through `coordinator.send_message` for a project
   * target; rejects for `global` and for `fixture:` ids. Fixture: simulated.
   */
  send(target: ConversationTarget, text: string): Promise<SendReceipt>;
}

// D11 -- ActivitySource (queue, attention, slots). Derived: tasks.list + approvals.list.
export type WaitingReason = "dependency" | "capacity" | "approval" | "paused" | "unknown";
export interface QueueEntry {
  taskId: string;
  projectId: string;
  objective: string;
  agent: string;
  state: TaskState;
  /** Null for entries that are not waiting (running). `unknown` means the daemon reports no reason. */
  waiting: { reason: WaitingReason; detail: string } | null;
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
  /** Scheduler limits are not exposed by the daemon: derived adapters answer `unsupported`. */
  slots: Measured<SlotUsage>;
  upNext: Measured<string>;
  dispatchPaused: Measured<boolean>;
  /**
   * Reads that failed while the queue snapshot succeeded. A listed part is unknown, not empty:
   * `tasks` means objectives, queued agents and Verifying tasks are missing; `approvals` means
   * `attention` is not known and holds nothing.
   */
  degraded?: { tasks?: string; approvals?: string };
}
export interface ActivitySource {
  readonly provenance: Provenance;
  activity(): Promise<SurfaceResult<ActivitySnapshot>>;
  /** Fixture only. Real stopping is `tasks.cancel` on the task screen; no global pause exists. */
  simulatePauseDispatch(): Promise<SurfaceReceipt>;
  simulateStop(taskId: string): Promise<SurfaceReceipt>;
}

// D12 -- UsageSource. No daemon data exists: the live adapter answers `unsupported`.
export interface UsageCoverage {
  provider: string;
  reporting: "provider-reported" | "estimated" | "none";
}
export interface UsageSummary {
  windowStart: string;
  timezone: string;
  currency: string;
  reported: Measured<number>;
  estimated: Measured<number>;
  budget: Measured<number>;
  coverage: UsageCoverage[];
}
export interface UsageSource {
  readonly provenance: Provenance;
  usage(): Promise<SurfaceResult<UsageSummary>>;
}

// D13 -- SystemMetricsSource. No daemon data exists: the live adapter answers `unsupported`.
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
}

// D14 -- ModelCatalogSource. Derived: agents.capabilities (agents only, no models).
export interface ModelOption {
  id: string;
  label: string;
}
export interface ModelProvider {
  /** Derived: the daemon agent id. */
  id: string;
  name: string;
  installed: boolean;
  launchable: boolean;
  modelSelection: "supported" | "unsupported" | "unknown";
  models: Measured<ModelOption[]>;
  efforts: Measured<string[]>;
}
export interface ModelCatalog {
  providers: ModelProvider[];
}
export interface ModelCatalogSource {
  readonly provenance: Provenance;
  catalog(): Promise<SurfaceResult<ModelCatalog>>;
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
  /** Null means no deadline is known, not that none exists. */
  latestResponseAt: string | null;
}
export interface ActionSupport {
  available: boolean;
  reason: string;
}
export interface DecisionQueue {
  items: DecisionItem[];
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

export interface SurfaceSources {
  workstreams: WorkstreamSource;
  conversation: ConversationSource;
  activity: ActivitySource;
  usage: UsageSource;
  metrics: SystemMetricsSource;
  models: ModelCatalogSource;
  search: SearchSource;
  decisions: DecisionQueueSource;
}
