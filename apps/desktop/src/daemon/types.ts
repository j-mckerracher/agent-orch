/**
 * Wire types for the accepted M5-02 daemon contract (source 7435ea3).
 *
 * Mirrors `crates/orchestrator-daemon/src/api.rs` and `crates/orchestrator-ipc`.
 * Rust enums use serde's default external tagging, so unit variants are bare
 * strings. Values the UI does not interpret stay `unknown` so that new daemon
 * variants render as raw records instead of being coerced.
 */

export type Capability =
  | "health"
  | "projects"
  | "tasks"
  | "sessions"
  | "events"
  | "approvals"
  | "queue"
  | "workspaces"
  | "delivery"
  | "agents"
  | "host"
  | "coordinator"
  | "review"
  | "workstreams"
  | "conversation_history"
  | "conversation_selection"
  | "permission_levels"
  | "orchestrator_actions";

export const ALL_CAPABILITIES: readonly Capability[] = [
  "health",
  "projects",
  "tasks",
  "sessions",
  "events",
  "approvals",
  "queue",
  "workspaces",
  "delivery",
  "agents",
  "host",
  "coordinator",
  "review",
  "workstreams",
  "conversation_history",
  "conversation_selection",
  "permission_levels",
  "orchestrator_actions",
];

/** The capabilities the UI requests: every capability the daemon offers (M5-02, M5-02b, M5-02c). */
export const M5_03_CAPABILITIES: readonly Capability[] = ALL_CAPABILITIES;

/**
 * Capabilities a daemon may lack without making the app unusable. The handshake asks for them, and
 * `ConnectionManager` repeats it once without them when the daemon refuses; the matching feature then
 * degrades to `unsupported`. Older daemons reject any capability name they do not know.
 */
export const OPTIONAL_CAPABILITIES: readonly Capability[] = [
  "queue",
  "host",
  "workstreams",
  "conversation_history",
  "conversation_selection",
  "permission_levels",
  "orchestrator_actions",
];

export interface Welcome {
  version: number;
  capabilities: Capability[];
  /** Names the durable journal; absent from daemons without a store epoch. */
  journal_epoch?: string;
  /** True when this session proved the approver secret; absent from older daemons. */
  approver?: boolean;
}

export type TaskState =
  | "Created"
  | "Queued"
  | "Running"
  | "Verifying"
  | "Succeeded"
  | "VerificationFailed"
  | "Failed"
  | "Cancelled";

export type AttemptState =
  | "Pending"
  | "Running"
  | "CandidateReady"
  | "Failed"
  | "Succeeded"
  | "Cancelled";

export type SessionState = "Spawning" | "Running" | "Exited" | "Aborted";

export type TaskKind = "Implement" | "Investigate";
export type DeliveryPolicy = "ReportOnly" | "LocalReview" | "PullRequest";
export type AutonomyPolicy = "ReadOnly" | "WorkspaceWrites" | "ApprovalRequired";
export type VerificationPolicy = "None" | { Commands: { commands: string[][] } };

export interface TaskContract {
  id: string;
  project_id: string;
  parent_id: string | null;
  objective: string;
  kind: TaskKind;
  agent: string;
  dependencies: string[];
  verification: VerificationPolicy;
  delivery: DeliveryPolicy;
  autonomy: AutonomyPolicy;
  max_retries: number;
  max_repairs: number;
}

export interface Attempt {
  id: string;
  task_id: string;
  attempt_number: number;
  state: AttemptState;
  /** Safe explanation for a failed attempt; absent in older task records. */
  failure_reason?: string;
}

export interface Session {
  id: string;
  attempt_id: string;
  adapter: string;
  state: SessionState;
}

export interface TaskRecord {
  contract: TaskContract;
  state: TaskState;
  attempts: Attempt[];
  sessions: Session[];
  /** Audit transitions in order; unit variants such as "TaskAdmitted". */
  events: string[];
  version: number;
}

/** `projects.list` row. The id is the canonical repository root. */
export interface ProjectRow {
  id: string;
  /** False while the project's coordinator is removed (DPES-1105). Absent from older daemons, where it is active. */
  coordinator_active?: boolean;
}

export interface SessionEntry {
  task_id: string;
  session: Session;
}

export interface DaemonEvent {
  /** Database-wide monotonic sequence. */
  sequence: number;
  task_id: string;
  session_id: string | null;
  kind: string;
  payload: unknown;
  created_at: string;
}

export interface SessionEventsPage {
  task_id: string;
  session_id: string;
  events: DaemonEvent[];
  more: boolean;
  next_after_sequence: number;
}

/** `Actor` serializes as `{kind, id}`. */
export interface Actor {
  kind: "user" | "agent" | "approved";
  id: string;
}

/** `ApprovalState` is tagged by `state`. */
export type ApprovalState =
  | { state: "requested" }
  | { state: "approved"; by: Actor }
  | { state: "rejected" }
  | { state: "superseded"; approved_by: Actor | null };

/** `ApprovalActionKind`; the delivery kind is the Rust `ActionKind` variant name. */
export type ApprovalActionKind =
  | { type: "delivery"; kind: "LocalReviewMerge" | "PullRequestCreate" | "ReportPublish" }
  | { type: "policy_change" };

export interface ApprovalAction {
  kind: ApprovalActionKind;
  /** `ParameterDigest` is `#[serde(transparent)]` over `[u8; 32]`: 32 numbers, not hex. */
  parameter_digest: number[];
  candidate_commit: string;
  task_id: string;
}

export interface Approval {
  id: string;
  action: ApprovalAction;
  state: ApprovalState;
}

export type ResolveDecision = "approve" | "reject";

/** `workspaces.diff` params; `path` plus `workspace_id` scopes the diff to one file. */
export interface WorkspaceDiffRequest {
  task_id: string;
  workspace_id?: string;
  path?: string;
}

export interface WorkspaceDiff {
  task_id: string;
  workspace_id: string;
  diff: string;
}

export type DeliveryActionName = "report_only" | "local_review";

/** `delivery.request` params (the idempotency key is passed separately). */
export interface DeliveryRequest {
  task_id: string;
  workspace_id?: string;
  approval_id: string;
  verification_run_id: string;
  action: DeliveryActionName;
  target_ref?: string;
}

export interface DeliveryResult {
  task_id: string;
  step_id: string;
  outcome: string;
}

export interface DaemonHealth {
  package_version: string;
  uptime_seconds: number;
  data_directory: string;
}

/** `agents.capabilities` row (M5-02b). The flags are null when the agent was not detected. */
export interface AgentCapabilityEntry {
  agent: string;
  installed: boolean;
  launchable: boolean;
  capabilities: {
    streaming: boolean;
    interactive_input: boolean;
    structured_output: boolean;
    cancellation: boolean;
    session_resumption: boolean;
    approvals: boolean;
    model_selection: boolean;
  } | null;
  /** DPES-1105: GitHub Copilot only. Its session sign-in record. */
  sign_in?: CopilotSignInRecord | null;
}

export interface ProposalTaskSummary {
  id: string;
  title: string;
  dependencies: string[];
  contract: Partial<TaskContract> & Record<string, unknown>;
}

export interface ProposalError {
  code: string;
  task_key: string | null;
  message: string;
}

/** `validation` is always advisory: `authority` is "none" and no task or approval is created. */
export type ProposalValidation =
  | { status: "valid"; authority: "none"; objective: string; tasks: ProposalTaskSummary[] }
  | { status: "invalid"; authority: "none"; errors: ProposalError[] };

/** `coordinator.*` entry (M5-02c). */
export interface CoordinatorEntry {
  sequence: number;
  id: string;
  project_id: string;
  kind: "message" | "proposal";
  author: "user" | "coordinator";
  body: string;
  validation: ProposalValidation | null;
  created_at: string;
}

export interface CoordinatorPage {
  project_id: string;
  entries: CoordinatorEntry[];
  more: boolean;
  next_after_sequence: number;
  /** Advances on every clear and removal (DPES-1105). Absent from daemons before that. */
  clear_generation?: number;
}

export type ReviewApprovalAction = "report_only" | "local_review";

export interface ReviewApprovalRequest {
  task_id: string;
  workspace_id?: string;
  verification_run_id: string;
  action: ReviewApprovalAction;
  target_ref?: string;
}

export interface ReviewApprovalSummary {
  id: string;
  state: "requested" | "approved" | "rejected" | "superseded";
  kind: string;
  task_id: string;
  candidate_commit: string;
}

export interface ReviewApprovalResult {
  approval: ReviewApprovalSummary;
  action: {
    action: ReviewApprovalAction;
    destination: { project_id: string };
    base_sha: string;
    target_ref: string | null;
    candidate_commit: string;
  };
  digest: string;
  existing: boolean;
}

export interface ReviewVerificationRun {
  id: string;
  status: string;
  commit: string;
  started_at_ms: number | null;
  finished_at_ms: number | null;
  matches_candidate: boolean;
  details: unknown;
  details_truncated: boolean;
}

export interface ChangeRequestRecord {
  id: string;
  task_id: string;
  approval_id: string;
  candidate_commit: string;
  feedback: string;
  requested_by: string;
  created_at: string;
}

/** `review.evidence` result. */
export interface ReviewEvidenceWire {
  task_id: string;
  project_id: string;
  state: "current" | "stale" | "missing";
  reasons: string[];
  review_state: "not_requested" | "awaiting_decision" | "changes_requested" | "approved" | "rejected" | "superseded";
  decision_ready: boolean;
  approval: ReviewApprovalSummary | null;
  action: ReviewApprovalResult["action"] | null;
  digest_relationship: "matches" | "mismatch" | "unavailable";
  candidate: { commit: string | null; source: string; workspace_head: string | null };
  workspace: { id: string; branch: string; base_commit: string } | null;
  verification: ReviewVerificationRun[];
  artifacts: Array<{ id: string; kind: string; checksum: string }>;
  snapshot: {
    base_commit: string;
    candidate_commit: string;
    files: unknown[];
    files_truncated: boolean;
    path: string | null;
    diff: string | null;
    diff_state: "complete" | "too_large" | "unavailable";
    diff_bytes: number;
  } | null;
  change_request: ChangeRequestRecord | null;
}

export interface ReviewChangesResult {
  change_request: ChangeRequestRecord;
  review_state: "changes_requested";
  approval_state: "requested";
}

export type WorkstreamStateWire = "active" | "suspended" | "archived";
export type GateStateWire = "pending" | "verified" | "failed";

/** `workstreams.*` records, mirroring `crates/orchestrator-core/src/workstream.rs`. */
export interface WorkstreamWire {
  id: string;
  title: string;
  summary: string;
  state: WorkstreamStateWire;
  revision: number;
}

export interface AcceptanceGateWire {
  id: string;
  description: string;
  state: GateStateWire;
  evidence_task_id: string | null;
}

export interface ObjectiveWire {
  id: string;
  workstream_id: string;
  title: string;
  description: string;
  gates: AcceptanceGateWire[];
  revision: number;
}

/** A task linked to a workstream, optionally under one of its objectives. */
export interface WorkstreamTaskLinkWire {
  task_id: string;
  workstream_id: string;
  objective_id: string | null;
  project_id: string;
  task_state: TaskState;
}

export interface WorkstreamDetailWire {
  workstream: WorkstreamWire;
  objectives: ObjectiveWire[];
  tasks: WorkstreamTaskLinkWire[];
  project_ids: string[];
}

export interface WorkstreamUpdateRequest {
  workstream_id: string;
  expected_revision: number;
  title?: string;
  summary?: string;
  state?: WorkstreamStateWire;
}

export interface GateDraft {
  description: string;
}

export interface WorkstreamReviseObjectiveRequest {
  objective_id: string;
  expected_revision: number;
  title?: string;
  description?: string;
  add_gates?: GateDraft[];
}

export interface WorkstreamSetGateRequest {
  objective_id: string;
  expected_revision: number;
  gate_id: string;
  state: GateStateWire;
  evidence_task_id?: string;
}

/** Body of `tasks.submit` `contract`; the daemon assigns the task id. */
export interface TaskSubmission {
  project_id: string;
  parent_id?: string | null;
  objective: string;
  kind: TaskKind;
  agent: string;
  dependencies: string[];
  verification: VerificationPolicy;
  delivery: DeliveryPolicy;
  autonomy: AutonomyPolicy;
  max_retries: number;
  max_repairs: number;
  /** Optional; check it with `agents.validate_selection` first. At least one of model and effort. */
  model_selection?: ModelSelectionWire;
}

/** `model_selection` of a task contract and params of `agents.validate_selection` (M7-09). */
export interface ModelSelectionWire {
  model?: string;
  effort?: string;
}

export type CatalogStateWire = "available" | "unknown" | "unavailable" | "unsupported" | "authentication_failed";

export interface CatalogFacet<T> {
  state: CatalogStateWire;
  detail: string;
  items: T[];
}

export interface CatalogModelOption {
  id: string;
  display_name: string;
  efforts: string[];
  default_effort: string | null;
}

/** One `agents.catalog` row. `models.items[].efforts` are per model; `efforts` is adapter-wide. */
export interface AgentCatalogEntry {
  agent: string;
  provider: string | null;
  state: CatalogStateWire;
  detail: string;
  models: CatalogFacet<CatalogModelOption>;
  efforts: CatalogFacet<string>;
}

export type SelectionReasonWire =
  | "agent_unavailable"
  | "authentication_failed"
  | "agent_state_unknown"
  | "agent_unsupported"
  | "models_unsupported"
  | "models_unknown"
  | "unknown_model"
  | "effort_requires_model"
  | "effort_unsupported"
  | "effort_unknown";

/** `agents.validate_selection` result. */
export type SelectionCheck = { valid: true } | { valid: false; reason: SelectionReasonWire; detail: string };

/** GitHub Copilot sign-in as the daemon last established it (DPES-1105). Counts are null when not reported. */
export type CopilotSignInState = "not_checked" | "checking" | "ready" | "not_signed_in" | "no_seat" | "error";

/** `agents.check_copilot_sign_in` result. Premium requests and AI credits are Copilot's units, never USD. */
export interface CopilotSignInRecord {
  state: CopilotSignInState;
  reason: string | null;
  checked_at_ms: number | null;
  premium_requests: number | null;
  ai_credits: number | null;
  /** A sanitized error a user hook reported during a completed run. Shown as a warning, never a blocker. */
  warning: string | null;
}

/** Conversation target on the wire (M7-08). The kind is `workstream`, not `project`. */
export type ConversationWireTarget = { kind: "global" } | { kind: "workstream"; workstream_id: string };

export type ConversationEntryKind =
  | "user_message"
  | "reply_chunk"
  | "turn_completed"
  | "turn_failed"
  | "turn_cancelled";

export interface ConversationWireEntry {
  sequence: number;
  id: string;
  turn_id: string;
  kind: ConversationEntryKind;
  author: "user" | "orchestrator" | "system";
  body: string;
  error_code: string | null;
  /** Set on a `turn_completed` entry (DPES-1105). Absent from older daemons; null when the turn had no action block. */
  action_block?: ActionBlockWire | null;
  /** Set on a `version_mismatch` entry: the agent and version the turn refused (DPES-1106). */
  runtime?: { name: string; version: string };
  created_at: string;
}

/** The permission level a message runs at (DPES-1105). Absent from older daemons; null when no agent launched. */
export type PermissionLevelWire = "restricted" | "extended" | "auto" | "full";

export interface ConversationWireTurn {
  id: string;
  state: "running" | "completed" | "failed" | "cancelled";
  error_code: string | null;
  runtime: { name: string; version: string };
  /** False when the daemon has not verified the version the turn ran on (DPES-1106). Absent from older daemons. */
  verified?: boolean;
  reply_bytes: number;
  created_at: string;
  updated_at: string;
  /** The level the daemon recorded the agent ran with. Null before launch and for turns from before levels. */
  permission_level?: PermissionLevelWire | null;
  permission_profile?: string | null;
  /** The outcome of the action block in this turn's reply (DPES-1105). Absent from older daemons. */
  action_block?: ActionBlockWire | null;
}

/**
 * A message a history clear deleted (DPES-1105). Only its id and sequence survive; a replayed send
 * answers with this instead of the entry. `created_at` is null when the daemon had none to return.
 */
export interface ClearedEntryReference {
  id: string;
  sequence: number;
  created_at: string | null;
  cleared: true;
}

/**
 * The agent, model and effort the picker chose for one message (DPES-1105). Sent only when the daemon
 * negotiated `conversation_selection`. The daemon refuses a choice its catalog does not offer, as a failed turn.
 */
export interface ConversationSelectionWire {
  agent: string;
  model?: string;
  effort?: string;
}

/** `conversations.send` result: the daemon recorded the message; the turn may still fail. */
export interface ConversationSendResult {
  authority: "none";
  target: ConversationWireTarget;
  entry: ConversationWireEntry | ClearedEntryReference;
  /** Null when the message was cleared, because its turn was cleared with it. */
  turn: ConversationWireTurn | null;
  epoch: string;
}

/** An action proposal's kind on the wire (DPES-1105). Unknown kinds arrive as strings and render as unknown. */
export type ActionKindWire =
  | "workstream.create"
  | "workstream.update"
  | "objective.add"
  | "objective.revise"
  | "task.propose"
  | "task.cancel"
  | "task.retry"
  | "workstream.link_task";

/** An action proposal's state on the wire (DPES-1105). Unknown states arrive as strings. */
export type ProposalStateWire = "pending" | "approved" | "executed" | "failed" | "rejected" | "superseded" | "expired";

/** What the orchestrator's action block in one reply became (DPES-1105). */
export type ActionBlockWire = "accepted" | "rejected" | "stripped";

/** Display facts the daemon read live for a task proposal: a project's folder name, or a task's title and state now. */
export interface ProposalTargetWire {
  project_name?: string | null;
  task_title?: string;
  task_state?: string;
}

/** One proposed action (DPES-1105). `params` is the canonical JSON the digest binds; it may hold `kind`. */
export interface ActionProposalWire {
  id: string;
  turn_id: string;
  index: number;
  kind: ActionKindWire | (string & {});
  params: Record<string, unknown>;
  digest: string;
  depends_on: string | null;
  state: ProposalStateWire | (string & {});
  result_ref: string | null;
  error_code: string | null;
  /** The envelope's reason when `error_code` is "action_denied"; null otherwise. Absent from older daemons. */
  denial?: string | null;
  /** Null for kinds that need no display facts. Absent from older daemons. */
  target?: ProposalTargetWire | null;
  created_at: string;
  updated_at: string;
}

/** `actions.resolve` result: the proposal as the daemon now holds it (DPES-1105). */
export interface ActionsResolveWire {
  proposal: ActionProposalWire;
}

/** The conversation that holds a proposal's card. */
export type ProposalScopeWire = { kind: "global" } | { kind: "workstream"; workstream_id: string };

/** `actions.pending` result: pending proposals only, newest first. `more` is true when any were left out. */
export interface ActionsPendingWire {
  proposals: Array<ActionProposalWire & { scope: ProposalScopeWire }>;
  more: boolean;
}

/** `conversations.get` result: cursor paging, no push stream. */
export interface ConversationWirePage {
  authority: "none";
  target: ConversationWireTarget;
  entries: ConversationWireEntry[];
  more: boolean;
  next_after_sequence: number;
  latest_sequence: number;
  /** Advances on every clear. A reader that sees a new value discards what it has read (DPES-1105). */
  clear_generation: number;
  turn: ConversationWireTurn | null;
  epoch: string;
  /** The scope's proposals, newest first, on every page (DPES-1105). Absent from older daemons. */
  proposals?: ActionProposalWire[];
  /** True when the daemon held more decided proposals than it sent (count or byte budget). Pending ones are always sent. Absent from older daemons. */
  proposals_more?: boolean;
}

/** `conversations.clear` result (DPES-1105). Idempotent: nothing to clear reports `cleared: false`. */
export interface ConversationClearResult {
  authority: "none";
  target: ConversationWireTarget;
  cleared: boolean;
  entries_deleted: number;
  turns_deleted: number;
  clear_generation: number;
  epoch: string;
}

/** `coordinator.clear` result (DPES-1105). */
export interface CoordinatorClearResult {
  project_id: string;
  cleared: boolean;
  entries_deleted: number;
  clear_generation: number;
}

/** `coordinator.remove` and `coordinator.restore` result (DPES-1105). */
export interface CoordinatorChangeResult {
  project_id: string;
  changed: boolean;
  removed: boolean;
  entries_deleted: number;
  clear_generation: number;
}

export interface ConversationCancelResult {
  authority: "none";
  target: ConversationWireTarget;
  turn: ConversationWireTurn;
  cancel_applied: boolean;
  epoch: string;
}
