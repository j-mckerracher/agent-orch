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
  | "orchestrator_actions"
  | "settings"
  | "monitoring"
  | "holds"
  | "landing"
  | "standing_approvals";

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
  "settings",
  "monitoring",
  "holds",
  "landing",
  "standing_approvals",
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
  "settings",
  "monitoring",
  "holds",
  "landing",
  "standing_approvals",
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
  /** DPES-673. The confirmed intake record; absent on tasks submitted without one. */
  work_item?: WorkItemWire;
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
  /** The project's delivery mode; present only for a project with saved defaults (DPES-1105). */
  mode?: string;
  /** True only for an archived project. The daemon already hides those from the list. */
  archived?: true;
}

/** A project's delivery defaults on the wire (`projects.get_defaults` and `projects.set_defaults`, DPES-1105). */
export interface ProjectDefaultsWire {
  /** `local-only`, `direct-pr` or `no-mistakes`; kept a string so a newer mode survives. */
  mode: string;
  base_branch: string | null;
  branch_prefix: string | null;
  /** Argument arrays, never a shell string. */
  checks: string[][];
  /** `merge`, `squash` or `rebase`. */
  merge_method: string;
}

/** `projects.get_defaults` and `projects.set_defaults` result. */
export interface ProjectDefaultsResultWire {
  defaults: ProjectDefaultsWire;
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

export interface OutputLine {
  /** Monotonic per session. */
  sequence: number;
  stream: "stdout" | "stderr";
  text: string;
  /** The line was cut to the daemon's per-line byte cap. */
  line_truncated: boolean;
}

/** `sessions.output_tail`: a bounded, read-only, unredacted view of a session's recent output. */
export interface SessionOutputTail {
  task_id: string;
  session_id: string;
  /** False when no output is retained: not started, evicted after ending, or the daemon restarted. */
  available: boolean;
  ended: boolean;
  limit: number;
  lines: OutputLine[];
  /** Newer-than-cursor lines exist that are not returned (dropped from the ring or beyond `limit`). */
  truncated_lines: boolean;
  last_sequence: number;
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

/** One session of the task under review: what the contract requested beside what usage receipts reported. */
export interface ReviewAttemptWire {
  session_id: string;
  agent: string;
  requested: { model: string | null; effort: string | null } | null;
  reported_model: string | null;
  /** A figure no receipt reported is null, never zero. */
  usage: {
    input_tokens: number | null;
    cache_read_tokens: number | null;
    cache_write_tokens: number | null;
    output_tokens: number | null;
    reasoning_tokens: number | null;
  };
  /** Reported cost in nano-units of `currency`. */
  cost: { amount_nanos: number; currency: string } | null;
  coverage: "reported" | "partial" | "unknown";
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
  attempts?: ReviewAttemptWire[];
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
  /** Required unless `work_class` names a class that sets the agent. */
  agent?: string;
  dependencies: string[];
  verification: VerificationPolicy;
  delivery: DeliveryPolicy;
  autonomy: AutonomyPolicy;
  max_retries: number;
  max_repairs: number;
  /** Optional; check it with `agents.validate_selection` first. At least one of model and effort. */
  model_selection?: ModelSelectionWire;
  /**
   * DPES-677. A work class whose settings fill in any of agent, model and effort the task leaves unset
   * (field by field; what the task names always wins). An unknown class, or one that no longer validates
   * against the live catalog, is refused with `invalid-params` and no task is created.
   */
  work_class?: string;
  /**
   * DPES-673. The intake record from `intake.normalize` after the user confirmed it: every criterion must have
   * `confirmed: true`. An unconfirmed item, or one with no criteria, is refused with `invalid-params` and no task is
   * created. It is stored on the task and returned in `contract.work_item`.
   */
  work_item?: WorkItemWire;
}

/** DPES-673. Where a work item came from; `reference` is a Jira key or `owner/repo#N`, recorded and never fetched. */
export interface WorkSourceWire {
  kind: "github_issue" | "jira" | "text";
  reference: string | null;
}

/** DPES-673. One acceptance criterion. `drafted` marks one the intake step wrote; the user confirms each. */
export interface CriterionWire {
  text: string;
  drafted: boolean;
  confirmed: boolean;
}

/** DPES-673. `intake.normalize` result body: a draft to show for confirmation. Every criterion starts unconfirmed. */
export interface WorkItemWire {
  source: WorkSourceWire;
  title: string;
  ask: string;
  acceptance_criteria: CriterionWire[];
}

/** DPES-673. `intake.normalize` result. */
export interface IntakeNormalizeResult {
  work_item: WorkItemWire;
}

/** DPES-677. `settings.workClasses.get` result. `invalid_configuration` and `unavailable` list no classes. */
export type WorkClassesStateWire = "off" | "ok" | "invalid_configuration" | "unavailable";

export interface WorkClassWire {
  name: string;
  agent: string | null;
  model: string | null;
  effort: string | null;
}

export interface WorkClassesWire {
  schema_version: 1;
  state: WorkClassesStateWire;
  classes: WorkClassWire[];
  max_classes: number;
}

/** One class of `settings.workClasses.set`. `name` is lowercase letters, digits and hyphens, at most 32. */
export interface WorkClassInput {
  name: string;
  agent?: string;
  model?: string;
  effort?: string;
}

/** Names the class (null when the name itself is unusable) and the field the daemon refused. */
export interface WorkClassRefusal {
  code: "invalid_work_class";
  class: string | null;
  field: "classes" | "name" | "agent" | "model" | "effort";
  reason: string;
  detail: string;
}

/** `settings.workClasses.set` replaces the whole set or, when `saved` is false, writes nothing. */
export type WorkClassesSaveResult = (WorkClassesWire & { saved: true }) | { saved: false; error: WorkClassRefusal };

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
  /** `wake` on the opening entry of an automatic turn only. Absent otherwise and from older daemons. */
  origin?: string;
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
  /** `wake` for an automatic turn, `user` otherwise. Absent from older daemons. */
  trigger?: string;
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
  | "workstream.link_task"
  | "task.revise"
  | "decision.hold"
  | "project.remove";

/** An action proposal's state on the wire (DPES-1105). Unknown states arrive as strings. */
export type ProposalStateWire = "pending" | "approved" | "executed" | "failed" | "rejected" | "superseded" | "expired";

/** What the orchestrator's action block in one reply became (DPES-1105). */
export type ActionBlockWire = "accepted" | "rejected" | "stripped";

/** Display facts the daemon read live for a task proposal: a project's folder name, or a task's title and state now. */
export interface ProposalTargetWire {
  project_name?: string | null;
  task_title?: string;
  task_state?: string;
  /** Present only on a `task.propose` card; true when the task asks for less than the project's default. */
  below_project_default?: boolean;
  /** The project's mode string; sent with `below_project_default`. */
  project_mode?: string;
  /** Present only on a `project.remove` card: the tasks kept in history and the ones still running. */
  task_count?: number;
  running_tasks?: number;
  /** Present only on a `delivery.merge` card: the pull request as the daemon last read it. */
  pull?: PullFactsWire;
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
  /**
   * Who approved it (DPES-1105 Phase 5): `{ kind: "user" }`, or a standing grant with its id. Null until approved.
   * Absent from older daemons. A bare `grant:<id>` event actor is read the same way.
   */
  approved_by?: { kind: "user" } | { kind: "grant"; grant_id: string } | string | null;
  /** The daemon sends the approving grant's id here (null when a person approved or nobody has). */
  grant_id?: string | null;
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

/** One machine meter that gates new task starts (capability `settings`, DPES-1105). GPU is never gated. */
export type LoadMeterName = "cpu" | "memory" | "disk";

/** The three machine load limits as whole percents from 50 to 100; 100 turns that meter's check off. */
export interface LoadLimitsWire {
  cpu_percent: number;
  memory_percent: number;
  disk_percent: number;
}

/** What the gate knows of one meter: a 60 second mean (whole percent), nothing yet, or a check that is off. */
export type LoadMeterWire =
  | { meter: LoadMeterName; state: "mean"; mean_percent: number }
  | { meter: LoadMeterName; state: "unknown"; reason: "warming_up" | "unavailable" }
  | { meter: LoadMeterName; state: "off" };

/** The meter that holds new starts back, with the mean that closed it and the mean at which it reopens. */
export interface LoadHoldWire {
  meter: LoadMeterName;
  mean_percent: number;
  limit_percent: number;
  reopen_percent: number;
}

/** `settings.get` and `settings.set` result. `feed` is `disabled_by_environment` only for a test daemon. */
export interface SettingsWire {
  load: LoadLimitsWire;
  gate: { state: "open" | "closed"; hold: LoadHoldWire | null; meters: LoadMeterWire[] };
  feed?: "sampler" | "disabled_by_environment";
  /** Phase 3 groups. Absent from a daemon that cannot change them (DPES-1105). */
  supervision?: SupervisionWire;
  wake_limits?: WakeLimitsWire;
  /** Whether chat may propose tasks that end in a pull request. Absent from an older daemon (DPES-1105). */
  chat_pr_delivery?: boolean;
  /** The read-only task tools for the orchestrator chat. Absent from an older daemon (DPES-1105 phase 6b). */
  mcp?: McpWire;
}

/** `settings.get` mcp group: whether the read-only task tools are on. Off by default. */
export interface McpWire {
  enabled: boolean;
}

/** `settings.get` supervision group: the stall-check intervals in seconds and the attempt wall-time bound in minutes. */
export interface SupervisionWire {
  base_interval_secs: number;
  min_interval_secs: number;
  attempt_wall_minutes: number;
}

/** `settings.get` wake_limits group. `null` is no limit. */
export interface WakeLimitsWire {
  hourly_cap: number | null;
  daily_cap: number | null;
  daily_token_budget: number | null;
}

/** `settings.set` patch: only the groups and fields that change; `null` in a wake limit clears it. */
export interface SettingsPatchWire {
  load?: Partial<LoadLimitsWire>;
  supervision?: Partial<SupervisionWire>;
  wake_limits?: Partial<WakeLimitsWire>;
  chat_pr_delivery?: boolean;
  mcp?: McpWire;
}

/** Where a wake record or a monitoring count belongs; the same shape as a conversation target. */
export type MonitoringTargetWire = ConversationWireTarget;

/** One wake record inside `conversations.get`. `state`, `outcome` and `skip_reason` stay strings so unknown values survive. */
export interface WakeRecordWire {
  id: string;
  event: string;
  subject: string;
  notice: string;
  state: string;
  outcome: string | null;
  skip_reason: string | null;
  turn_id: string | null;
  anchor_sequence: number;
  notify: boolean;
  seen: boolean;
  created_at: string;
}

/** `monitoring.get` result, and the result of `monitoring.set`. `mode` and `reason` stay strings so unknown values survive. */
export interface MonitoringWire {
  authority: "none";
  mode: string;
  paused: Array<{ target: MonitoringTargetWire; reason: string; until: string | null }>;
  unseen: Array<{ target: MonitoringTargetWire; count: number }>;
  epoch: string;
  /** Phase 5b: set while the mode is `away`. Absent from a daemon without `standing_approvals`. */
  away_since_ms?: number | null;
  brief_pending?: boolean;
}

/** Totals for the return brief window (capability `standing_approvals`, DPES-1105 Phase 5b). */
export interface BriefCountsWire {
  tasks_succeeded: number;
  tasks_failed: number;
  tasks_stalled: number;
  tasks_running: number;
  proposals_approved_by_user: number;
  proposals_approved_by_grant: number;
  executed_by_grant: number;
  failed_under_grant: number;
  proposals_waiting: number;
  deliveries_waiting: number;
  decisions_held: number;
  grant_uses: number;
  grants_expired: number;
  grants_revoked: number;
  quiet_updates: number;
  replies_paused_by_limit: number;
}

/** A standing approval used, expired or revoked in the window. `scope_label` is a project code or a cut title. */
export interface BriefGrantWire {
  id: string;
  scope_kind: "project" | "workstream" | (string & {});
  scope_label: string;
  uses_in_window: number;
  uses_used: number;
  max_uses: number;
  status: string;
}

/** One decision or problem in the brief. `kind` and `state` stay strings so a newer word survives. */
export interface BriefItemWire {
  kind: string;
  id: string;
  title: string;
  state: string;
  project: string | null;
  at_ms: number;
}

export interface BriefWire {
  from_ms: number;
  to_ms: number;
  /** True while the user is still away: the window ends now. */
  open: boolean;
  counts: BriefCountsWire;
  grants: BriefGrantWire[];
  waiting: BriefItemWire[];
  attention: BriefItemWire[];
}

/** `monitoring.brief` result: the brief, or `null` when none is due. */
export interface BriefResultWire {
  authority: "none";
  brief: BriefWire | null;
}

/** `monitoring.brief_ack` result. */
export interface BriefAckWire {
  authority: "none";
  brief_pending: boolean;
}

/** `monitoring.ack` result. */
export interface MonitoringAckWire {
  authority: "none";
  acked: number;
}

/** One decision hold (capability `holds`). `subject_kind` is `approval` (subject id is the task id) or `proposal`; both stay strings. */
export interface HoldWire {
  id: string;
  subject_kind: string;
  subject_id: string;
  /** `YYYY-MM-DD`: the day the decision returns. */
  until_date: string;
  state: string;
}

/** `holds.set` and `holds.release` result. */
export interface HoldResultWire {
  hold: HoldWire;
}

/** `holds.list` result: the holds in force now. */
export interface HoldsListWire {
  holds: HoldWire[];
}

/** One check of a pull request. `name` is remote text; `result` is the host's word for the outcome (pass, fail, pending). */
export interface PullCheckWire {
  name: string;
  result: string;
}

/** What `merge_verdict` and the merge worker last said of a task's merge request; null when none was made. */
export interface PullMergeWire {
  state: string;
  refusal: string | null;
}

/** The facts of a pull request the daemon last read, shared by a `pulls.*` row and a `delivery.merge` card's `target.pull`. */
export interface PullFactsWire {
  merge_method: string;
  required_total: number;
  required_passed: number;
  required_failed: number;
  checks: PullCheckWire[];
  observed_age_secs: number;
  /** The refusal code `merge_verdict` gives now (for example `checks_red`); null when a merge would be allowed. */
  refusal?: string | null;
}

/** One watched pull request (capability `landing`). `state` is open, merged or closed; `title`, `branch` and `base_branch` are remote text. */
export interface PullWire {
  task_id: string;
  number: number;
  state: string;
  /** The full 40 character commit. */
  head_sha: string;
  base_branch: string;
  branch: string;
  /** A boolean or the host's word; read by one mapping function. */
  mergeable: boolean | string | null;
  required_total: number;
  required_passed: number;
  required_failed: number;
  checks: PullCheckWire[];
  title: string;
  observed_age_secs: number;
  merge: PullMergeWire | null;
}

/** `pulls.list` result. */
export interface PullsListWire {
  pulls: PullWire[];
}

/** `pulls.refresh` result: the one row, after a fresh read. */
export type PullsRefreshWire = PullWire;

/** What a standing approval is bound to: one project (by its short code) or one workstream. `label` is a folder name or a title, never a path. */
export interface GrantScopeWire {
  kind: "project" | "workstream" | (string & {});
  id: string;
  label?: string;
}

/** One standing approval (capability `standing_approvals`, DPES-1105 Phase 5). `status` stays a string so a newer state survives. */
export interface GrantWire {
  id: string;
  scope: GrantScopeWire;
  kinds: string[];
  created_at_ms: number;
  expires_at_ms: number;
  max_uses: number;
  uses_used: number;
  token_budget: number;
  /** Tokens the tasks started under the grant reported so far; tasks that report none count zero. */
  tokens_spent: number;
  status: "active" | "revoked" | "expired" | "exhausted" | "budget_exhausted" | (string & {});
  revoked_at_ms: number | null;
  /** N8 merge grant only: the head branch must start with this. Null on any other grant. */
  branch_prefix?: string | null;
  /** True for an unattended merge grant. Absent from older daemons. */
  confirm_unattended_merge?: boolean;
}

/** The limits and the kinds a grant may name; the form offers nothing else. */
export interface GrantPolicyWire {
  grantable_kinds: string[];
  default_expires_in_secs: number;
  max_expires_in_secs: number;
  min_expires_in_secs: number;
  default_max_uses: number;
  max_max_uses: number;
  max_token_budget: number;
  /** N8: true when the daemon takes an unattended merge grant. The merge kind is never in `grantable_kinds`. */
  merge_kind_available?: boolean;
  merge_default_max_uses?: number;
  merge_max_max_uses?: number;
  merge_max_expires_in_secs?: number;
}

/** `grants.list` result. */
export interface GrantsListWire {
  grants: GrantWire[];
  more: boolean;
  policy: GrantPolicyWire;
}

/** `grants.create` params (user-only). `token_budget` is required. */
export interface GrantsCreateParams {
  scope: { kind: "project" | "workstream"; id: string };
  kinds: string[];
  expires_in_secs: number;
  max_uses: number;
  token_budget: number;
  /** Sent as true only for an unattended merge grant (`delivery.merge`). */
  confirm_unattended_merge?: boolean;
  branch_prefix?: string;
}

/** `grants.create` result. */
export interface GrantCreateResultWire {
  grant: GrantWire;
}

/** `grants.revoke` result (user-only). `stopped` counts approved actions that had not started and were failed. */
export interface GrantRevokeResultWire {
  grant: GrantWire;
  stopped: number;
}

/** One orchestrator note on the wire (capability `standing_approvals`, DPES-1105 5b). */
export interface NoteWire {
  position: number;
  body: string;
}

/** `notes.list` and `notes.set` result: one scope's notes in order and the limits the editor must respect. */
export interface NotesWire {
  /** `global`, or `workstream:` plus a workstream id. */
  scope: string;
  notes: NoteWire[];
  limits: { max_notes: number; max_bytes: number };
}

/** `notes.set` result: the scope and how many notes it now holds. It carries no note text (the reply is cached). */
export interface NotesSetWire {
  scope: string;
  count: number;
}

/** A task the global conversation routed to this workstream (DPES-1105 6a). `title` is the daemon's short objective. */
export interface ConversationRoutedInWire {
  task_id: string;
  title: string;
  routed_at: string;
  state: string;
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
  /** The scope's newest wake records, oldest first. Absent from daemons without `monitoring`. */
  wake_records?: WakeRecordWire[];
  wake_records_more?: boolean;
  /** Workstream target only: tasks the global conversation routed here, newest first. Absent or empty otherwise (DPES-1105). */
  routed_in?: ConversationRoutedInWire[];
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
