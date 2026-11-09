/**
 * Frontend boundary interfaces for the services D1-D6.
 *
 * These are NOT daemon methods. Each interface is injected at the screen
 * boundary (`SourcesProvider`). The daemon-backed adapter (`daemon.ts`) returns
 * the same view models with `provenance.kind === "daemon"` and the fixture
 * adapter with `"fixture"`; no component or route changes. Fixture adapters
 * live in this directory, never import the daemon client, and mint ids
 * prefixed `fixture:` that `DaemonApi` refuses to send. D3 (recovery status)
 * has no daemon RPC and always uses the fixture adapter.
 */
import type { AttemptState, TaskState } from "../daemon/types";

export type SourceId = "D1" | "D2" | "D3" | "D4" | "D5" | "D6";

export type Provenance =
  | { kind: "fixture"; id: SourceId; fixture: string }
  | { kind: "daemon"; id: SourceId };

export type SourceResult<T> =
  | { state: "ready"; data: T }
  | { state: "empty" }
  | { state: "unavailable"; reason: string }
  | { state: "error"; message: string };

/** A task as the screens know it: either a real daemon task or a fixture preview. */
export type TaskRef =
  | { origin: "real"; taskId: string; state: TaskState }
  | { origin: "fixture"; id: string };

export interface SimulatedReceipt {
  simulated: true;
  provenance: Provenance;
  notice: string;
  at: string;
}

// D1 -- AgentCapabilitiesSource
export interface AgentCapability {
  id: string;
  name: string;
  availability: "installed" | "available" | "unsupported" | "unknown";
  features: string[];
  discoveredAt: string;
  note?: string;
}
export interface AgentCapabilitiesSource {
  readonly provenance: Provenance;
  listAgents(): Promise<SourceResult<AgentCapability[]>>;
}

// D2 -- TaskExecutionSource
export interface ExecutionAvailability {
  launchAvailable: boolean;
  reason: string;
}
export interface ExecutionOutcome {
  state: string;
  attemptId: string | null;
  sessionId: string | null;
  launchOutcome: string;
  cancelOutcome: string | null;
}
export interface PreviewTask {
  id: string;
  objective: string;
  agent: string;
  state: TaskState;
  attempts: Array<{ id: string; state: AttemptState }>;
  sessions: Array<{ id: string; state: string }>;
  timeline: Array<{ at: string; kind: string; detail: string }>;
  outcome: ExecutionOutcome;
}
export interface TaskExecutionSource {
  readonly provenance: Provenance;
  availability(): Promise<SourceResult<ExecutionAvailability>>;
  listPreviewTasks(): Promise<SourceResult<PreviewTask[]>>;
  getPreviewTask(id: string): Promise<SourceResult<PreviewTask>>;
  outcomeFor(ref: TaskRef): Promise<SourceResult<ExecutionOutcome>>;
  simulateLaunch(id: string): Promise<SimulatedReceipt>;
  simulateCancel(id: string): Promise<SimulatedReceipt>;
}

// D3 -- RecoveryStatusSource
export interface RecoveryStatus {
  phase: string;
  checks: { process: string; workspace: string; delivery: string };
  classification: "recoverable" | "blocked" | "completed" | "needs-inspection";
  reason: string;
}
export interface RecoveryStatusSource {
  readonly provenance: Provenance;
  recoveryFor(ref: TaskRef): Promise<SourceResult<RecoveryStatus>>;
}

// D4 -- TaskRetrySource
export interface RetryAvailability {
  available: boolean;
  reason: string;
  previousAttemptId: string | null;
}
export interface TaskRetrySource {
  readonly provenance: Provenance;
  retryFor(ref: TaskRef): Promise<SourceResult<RetryAvailability>>;
  simulateRetry(ref: TaskRef): Promise<SimulatedReceipt & { newAttemptId: string }>;
  /** Daemon-backed adapters only: calls `tasks.retry`. Rejects with DaemonError. */
  retry?(ref: TaskRef): Promise<{ state: TaskState; attempts: number }>;
}

// D5 -- CoordinatorSource
export interface CoordinatorMessage {
  id: string;
  role: "user" | "coordinator";
  text: string;
  at: string;
}
export interface ProposedTask {
  id: string;
  objective: string;
  dependencies: string[];
  validation: "valid" | "invalid";
  validationErrors: string[];
  suggestedDelivery: string;
}
export interface CoordinatorConversation {
  projectId: string;
  messages: CoordinatorMessage[];
  proposals: ProposedTask[];
  outstandingDecisions: string[];
  /** Real ids only when a daemon-backed adapter returned them. */
  relatedRealIds: string[];
}
export interface CoordinatorSource {
  readonly provenance: Provenance;
  conversation(projectId: string): Promise<SourceResult<CoordinatorConversation>>;
  previewReply(projectId: string, draft: string): Promise<SimulatedReceipt & { message: CoordinatorMessage }>;
  previewProposal(projectId: string, draft: string): Promise<SimulatedReceipt & { proposal: ProposedTask }>;
  /** Daemon-backed adapters only: `coordinator.send_message`. Records a message; nothing acts on it. */
  send?(projectId: string, text: string): Promise<void>;
  /** Daemon-backed adapters only: `coordinator.propose`. Validates a JSON proposal; creates no task. */
  propose?(projectId: string, proposalJson: string): Promise<void>;
}

// D6 -- JournalIdentitySource
export interface JournalIdentity {
  storeIdentity: string | null;
  cursorRelationship: "same" | "reset" | "unknown";
}
export interface JournalIdentitySource {
  readonly provenance: Provenance;
  identity(): Promise<SourceResult<JournalIdentity>>;
}

export interface Sources {
  agents: AgentCapabilitiesSource;
  execution: TaskExecutionSource;
  recovery: RecoveryStatusSource;
  retry: TaskRetrySource;
  coordinator: CoordinatorSource;
  journal: JournalIdentitySource;
}
