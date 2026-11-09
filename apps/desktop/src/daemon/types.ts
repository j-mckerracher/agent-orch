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
  | "workspaces"
  | "delivery"
  | "agents"
  | "coordinator"
  | "review";

export const ALL_CAPABILITIES: readonly Capability[] = [
  "health",
  "projects",
  "tasks",
  "sessions",
  "events",
  "approvals",
  "workspaces",
  "delivery",
  "agents",
  "coordinator",
  "review",
];

/** The capabilities the UI requests: every capability the daemon offers (M5-02, M5-02b, M5-02c). */
export const M5_03_CAPABILITIES: readonly Capability[] = ALL_CAPABILITIES;

export interface Welcome {
  version: number;
  capabilities: Capability[];
  /** Names the durable journal; absent from daemons without a store epoch. */
  journal_epoch?: string;
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
}
