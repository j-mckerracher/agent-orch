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
  | "delivery";

export const ALL_CAPABILITIES: readonly Capability[] = [
  "health",
  "projects",
  "tasks",
  "sessions",
  "events",
  "approvals",
  "workspaces",
  "delivery",
];

/** The capabilities the UI requests: M5-03 screens plus the S6 review ones (workspaces, delivery). */
export const M5_03_CAPABILITIES: readonly Capability[] = [
  "health",
  "projects",
  "tasks",
  "sessions",
  "events",
  "approvals",
  "workspaces",
  "delivery",
];

export interface Welcome {
  version: number;
  capabilities: Capability[];
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
