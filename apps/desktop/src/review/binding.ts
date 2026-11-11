// Pure policy for which S6 actions are enabled. Real evidence must be bound to
// the exact approval, candidate, action, digest and workspace being reviewed;
// fixture evidence can never enable Accept or delivery.
import type { Approval, DeliveryActionName, TaskRecord } from "../daemon/types";
import type { ReviewEvidenceResult } from "./sources";

export type DiffState =
  | { status: "loading" }
  | { status: "ok"; workspaceId: string; text: string }
  | { status: "empty"; workspaceId: string }
  | { status: "too_large" }
  | { status: "unavailable"; reason: string };

export interface ActionContext {
  connected: boolean;
  loading: boolean;
  /** A task event arrived (or a mutation failed) since the context loaded. */
  stale: boolean;
  /** A mutation failed since the last completed refresh. */
  refreshRequired: boolean;
  approval: Approval | null;
  task: TaskRecord | null;
  diff: DiffState;
  evidence: ReviewEvidenceResult | null;
}

export function digestHex(bytes: number[]): string {
  return bytes.map((b) => (b & 0xff).toString(16).padStart(2, "0")).join("");
}

export function deliveryKindOf(approval: Approval): string | null {
  return approval.action.kind.type === "delivery" ? approval.action.kind.kind : null;
}

function commonBlockers(ctx: ActionContext): string[] {
  const blockers: string[] = [];
  if (!ctx.connected) blockers.push("The daemon connection is not established; actions are unavailable.");
  if (ctx.loading) blockers.push("Review context is still loading.");
  if (ctx.stale) blockers.push("The review may be stale; refresh before acting.");
  if (ctx.refreshRequired) {
    blockers.push("A previous request failed; refresh to reconcile before retrying.");
  }
  if (!ctx.approval) blockers.push("No decision is selected.");
  else if (ctx.approval.action.kind.type !== "delivery") {
    blockers.push("This decision is not a delivery candidate review.");
  }
  return blockers;
}

/** Reasons the real evidence does not authorize (empty = bound, trusted, verified). */
export function evidenceBlockers(ctx: ActionContext): string[] {
  const { approval, evidence, diff } = ctx;
  if (!approval) return [];
  if (!evidence) return ["Review evidence has not loaded."];
  if (evidence.state !== "available") {
    return [
      `Real review evidence is ${evidence.state}: ${evidence.reason}${
        evidence.provenance === "fixture" ? " (PLACEHOLDER DATA)" : ""
      }`,
    ];
  }
  const e = evidence.evidence;
  const problems: string[] = [];
  if (e.provenance !== "daemon") {
    problems.push("Evidence is PLACEHOLDER DATA (fixture); it cannot authorize approval or delivery.");
  }
  const a = approval.action;
  if (e.taskId !== a.task_id) problems.push("Evidence task does not match the approval's task.");
  if (e.approvalId !== approval.id) problems.push("Evidence approval does not match the selected approval.");
  if (e.candidate.commit !== a.candidate_commit) {
    problems.push("Evidence candidate commit does not match the approval's candidate.");
  }
  if (e.action.kind !== deliveryKindOf(approval)) problems.push("Evidence action kind does not match the approval.");
  if (e.action.digestHex !== digestHex(a.parameter_digest)) {
    problems.push("Evidence action parameters do not match the approval's parameter digest.");
  }
  if (e.verification.commit !== a.candidate_commit) {
    problems.push("Verification run is for a different commit than the candidate.");
  }
  if (e.verification.outcome !== "passed") problems.push("Verification did not pass.");
  if (diff.status === "ok" || diff.status === "empty") {
    if (e.workspaceId !== diff.workspaceId) problems.push("Evidence workspace does not match the diff's workspace.");
  } else {
    problems.push("The scoped diff is not available to bind to the evidence.");
  }
  return problems;
}

export function acceptBlockers(ctx: ActionContext): string[] {
  const blockers = commonBlockers(ctx);
  if (ctx.approval && ctx.approval.state.state !== "requested") {
    blockers.push("This decision is no longer pending.");
  }
  if (ctx.diff.status === "too_large") {
    blockers.push("The diff is too large to review as a whole; Accept needs the full candidate diff.");
  }
  return [...blockers, ...evidenceBlockers(ctx)];
}

/** Reject is protective, so it does not require real evidence. */
export function rejectBlockers(ctx: ActionContext): string[] {
  const blockers = commonBlockers(ctx);
  if (ctx.approval && ctx.approval.state.state !== "requested") {
    blockers.push("This decision is no longer pending.");
  }
  return blockers;
}

export interface DeliveryRequestDraft {
  task_id: string;
  workspace_id: string;
  approval_id: string;
  verification_run_id: string;
  action: DeliveryActionName;
  target_ref?: string;
}

export function deliveryPlan(ctx: ActionContext): { blockers: string[]; request: DeliveryRequestDraft | null } {
  const blockers = commonBlockers(ctx);
  const { approval, task, evidence, diff } = ctx;
  let action: DeliveryActionName | null = null;
  if (approval) {
    if (approval.state.state !== "approved") blockers.push("Delivery needs an approved decision.");
    const kind = deliveryKindOf(approval);
    if (kind === "LocalReviewMerge") action = "local_review";
    else if (kind === "ReportPublish") action = "report_only";
    else if (kind !== null) blockers.push("This action kind is not supported by delivery.request.");
  }
  if (!task) blockers.push("Task context is unavailable.");
  else if (action) {
    const policy = action === "local_review" ? "LocalReview" : "ReportOnly";
    if (task.contract.delivery !== policy) blockers.push("The task's delivery policy does not allow this action.");
  }
  blockers.push(...evidenceBlockers(ctx));
  if (blockers.length > 0 || !approval || !action || evidence?.state !== "available") {
    return { blockers, request: null };
  }
  const target = evidence.evidence.action.target;
  if (action === "local_review" && !target) {
    return { blockers: [...blockers, "No bound target ref is available for local review."], request: null };
  }
  if (diff.status !== "ok" && diff.status !== "empty") return { blockers, request: null };
  return {
    blockers,
    request: {
      task_id: approval.action.task_id,
      workspace_id: diff.workspaceId,
      approval_id: approval.id,
      verification_run_id: evidence.evidence.verification.runId,
      action,
      ...(action === "local_review" && target ? { target_ref: target } : {}),
    },
  };
}
