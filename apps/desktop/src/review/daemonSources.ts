// Daemon-backed D7 (review.evidence) and D8 (review.request_changes) adapters.
// They implement the same interfaces as the fixture adapters, so ReviewScreen
// is unchanged. Provenance is always "daemon": nothing here reads a fixture, and
// a request the daemon cannot answer is reported as unavailable or an error,
// never as made-up evidence.
import type { DaemonApi } from "../daemon/api";
import { asDaemonError, DaemonError } from "../daemon/errors";
import { IdempotencyTracker } from "../daemon/idempotency";
import type { ReviewEvidenceWire, ReviewVerificationRun } from "../daemon/types";
import { reviewCandidateBlocker } from "./binding";
import type {
  ChangeRequestHistory,
  ChangeRequestInput,
  ChangeRequestReceipt,
  ChangeRequestSource,
  ReviewEvidence,
  ReviewEvidenceResult,
  ReviewEvidenceSource,
} from "./sources";

type Outcome = "passed" | "failed" | "unknown";

const ACTION_LABELS: Record<string, { kind: string; label: string }> = {
  report_only: { kind: "ReportPublish", label: "Publish a report only" },
  local_review: { kind: "LocalReviewMerge", label: "Create a local review ref" },
};

function outcomeOf(status: unknown): Outcome {
  if (typeof status !== "string") return "unknown";
  const value = status.toLowerCase();
  if (value === "passed" || value === "pass" || value === "success" || value === "succeeded") return "passed";
  if (value === "failed" || value === "fail" || value === "error") return "failed";
  return "unknown";
}

/** The `details` shape is verifier-owned; read `checks[]` only when it has the expected form. */
function checksOf(details: unknown): { name: string; outcome: Outcome }[] {
  if (typeof details !== "object" || details === null) return [];
  const checks = (details as { checks?: unknown }).checks;
  if (!Array.isArray(checks)) return [];
  return checks.flatMap((check): { name: string; outcome: Outcome }[] => {
    if (typeof check !== "object" || check === null) return [];
    const { name, status, outcome } = check as { name?: unknown; status?: unknown; outcome?: unknown };
    return typeof name === "string" ? [{ name, outcome: outcomeOf(status ?? outcome) }] : [];
  });
}

function pickVerification(runs: ReviewVerificationRun[], candidate: string): ReviewVerificationRun | null {
  const matching = runs.filter((run) => run.commit === candidate);
  return matching.find((run) => outcomeOf(run.status) === "passed") ?? matching[matching.length - 1] ?? null;
}

export function toReviewEvidence(
  wire: ReviewEvidenceWire,
  request: { approvalId: string; digestHex: string },
): ReviewEvidenceResult {
  const provenance = "daemon" as const;
  if (wire.review_state === "changes_requested") {
    return {
      state: "unavailable",
      provenance,
      reason: "Changes were requested for this candidate; evidence for approval is withheld until a new candidate exists.",
    };
  }
  if (wire.state !== "current") {
    const reasons = wire.reasons.length > 0 ? wire.reasons.join(", ") : "no reason reported";
    return {
      state: wire.state === "missing" ? "missing" : "unavailable",
      provenance,
      reason: `The daemon reports the review evidence as ${wire.state} (${reasons}).`,
    };
  }
  const candidate = wire.candidate.commit;
  if (!wire.approval || !wire.action || !wire.workspace || !candidate) {
    return { state: "missing", provenance, reason: "The daemon returned no approval, action, workspace or candidate." };
  }
  const run = pickVerification(wire.verification, candidate);
  if (!run) {
    return { state: "missing", provenance, reason: "No verification run exists for the candidate commit." };
  }
  const known = ACTION_LABELS[wire.action.action];
  const evidence: ReviewEvidence = {
    provenance,
    taskId: wire.task_id,
    approvalId: wire.approval.id,
    workspaceId: wire.workspace.id,
    candidate: { commit: candidate, baseCommit: wire.workspace.base_commit },
    action: {
      kind: known?.kind ?? wire.action.action,
      label: known?.label ?? wire.action.action,
      target: wire.action.target_ref,
      parameters: {
        base: wire.action.base_sha,
        ...(wire.action.target_ref ? { target_ref: wire.action.target_ref } : {}),
      },
      // The daemon compares digests itself and does not return the bytes.
      digestHex: wire.digest_relationship === "matches" ? request.digestHex : "",
    },
    verification: {
      runId: run.id,
      commit: run.commit,
      outcome: outcomeOf(run.status),
      summary: `Verification run ${run.id} reported "${run.status}" for ${run.commit}.`,
      checks: checksOf(run.details),
    },
  };
  return { state: "available", evidence };
}

export function createDaemonReviewEvidenceSource(api: () => DaemonApi): ReviewEvidenceSource {
  return {
    provenance: "daemon",
    async load(request) {
      try {
        const wire = await api().reviewEvidence(request.taskId, request.approvalId);
        return toReviewEvidence(wire, request);
      } catch (error) {
        const failure = asDaemonError(error);
        const unavailable =
          failure.kind === "disconnected" ||
          failure.kind === "capability-not-negotiated" ||
          failure.kind === "capability-unavailable" ||
          failure.kind === "method-unavailable";
        return {
          state: unavailable ? "unavailable" : "error",
          provenance: "daemon",
          reason: `${failure.kind}: ${failure.message}`,
        };
      }
    },
  };
}

export function createDaemonChangeRequestSource(api: () => DaemonApi): ChangeRequestSource {
  const keys = new IdempotencyTracker();
  const toReceipt = (record: {
    id: string;
    task_id: string;
    candidate_commit: string;
    feedback: string;
    created_at: string;
  }, resultingState: string): ChangeRequestReceipt => ({
    provenance: "daemon",
    requestId: record.id,
    taskId: record.task_id,
    candidateCommit: record.candidate_commit,
    feedback: record.feedback,
    resultingState,
    submittedAt: record.created_at,
  });

  return {
    provenance: "daemon",
    labels: {
      description:
        "Records feedback and blocks approval of this candidate until a new one exists. This is not a Reject.",
      submitAction: "Send feedback",
      receiptNote: "",
    },
    async loadHistory(taskId): Promise<ChangeRequestHistory> {
      try {
        const wire = await api().reviewEvidence(taskId);
        return wire.change_request
          ? { state: "available", items: [toReceipt(wire.change_request, "changes requested")] }
          : { state: "empty", reason: "No change requests are recorded for this task." };
      } catch (error) {
        const failure = asDaemonError(error);
        // Several active approvals make the unscoped read ambiguous; that is not a failure to report as one.
        if (failure.kind === "conflict") {
          return { state: "unavailable", reason: "More than one approval is active, so the history is ambiguous." };
        }
        return { state: "error", reason: `${failure.kind}: ${failure.message}` };
      }
    },
    async submit(input: ChangeRequestInput) {
      const refusal = reviewCandidateBlocker(input.candidateCommit);
      if (refusal) throw new DaemonError("invalid-params", refusal);
      const scope = `changes:${input.approvalId}`;
      const key = keys.keyFor(scope, `${input.candidateCommit}|${input.feedback}`);
      try {
        const result = await api().reviewRequestChanges(input.approvalId, input.candidateCommit, input.feedback, key);
        keys.settle(scope, null);
        return toReceipt(result.change_request, "changes requested");
      } catch (error) {
        const failure = asDaemonError(error);
        keys.settle(scope, failure);
        throw new Error(`${failure.kind}: ${failure.message}`);
      }
    },
  };
}
