import { describe, expect, it } from "vitest";
import type { ReviewEvidenceWire, ReviewVerificationRun } from "../../../daemon/types";
import { evidenceFromReview } from "./evidence";
import { taskCount } from "./model";

const COMMIT = "0123456789abcdef";

const run = (status: string, matches = true): ReviewVerificationRun => ({
  id: "v1",
  status,
  commit: COMMIT,
  started_at_ms: null,
  finished_at_ms: null,
  matches_candidate: matches,
  details: null,
  details_truncated: false,
});

const wire = (overrides: Partial<ReviewEvidenceWire> = {}): ReviewEvidenceWire => ({
  task_id: "t1",
  project_id: "/p",
  state: "current",
  reasons: [],
  review_state: "not_requested",
  decision_ready: false,
  approval: null,
  action: null,
  digest_relationship: "unavailable",
  candidate: { commit: COMMIT, source: "workspace", workspace_head: null },
  workspace: null,
  verification: [],
  artifacts: [],
  snapshot: null,
  change_request: null,
  ...overrides,
});

describe("taskCount", () => {
  it("is singular only for one", () => {
    expect([0, 1, 2, 150].map(taskCount)).toEqual(["0 tasks", "1 task", "2 tasks", "150 tasks"]);
  });
});

describe("evidenceFromReview", () => {
  it("asks for the daemon's missing or stale evidence, with its reasons", () => {
    expect(evidenceFromReview(wire({ state: "missing", reasons: ["no_candidate"], candidate: { commit: null, source: "none", workspace_head: null } }))).toBe(
      "Candidate evidence missing (no_candidate)",
    );
    expect(evidenceFromReview(wire({ state: "stale", reasons: ["head_moved", "new_commit"] }))).toBe(
      "Fresh candidate evidence; the recorded evidence is stale (head_moved, new_commit)",
    );
    expect(evidenceFromReview(wire({ state: "missing" }))).toMatch(/no reason reported/);
  });

  it("needs a passing verification run first, then an approval request", () => {
    expect(evidenceFromReview(wire())).toBe("Passing verification run for candidate 01234567");
    expect(evidenceFromReview(wire({ verification: [run("failed")] }))).toBe("Passing verification run for candidate 01234567");
    expect(evidenceFromReview(wire({ verification: [run("passed", false)] }))).toBe("Passing verification run for candidate 01234567");
    expect(evidenceFromReview(wire({ verification: [run("Passed")] }))).toBe("Approval request for candidate 01234567");
  });

  it("follows the review state", () => {
    const approval = { id: "ap", state: "requested" as const, kind: "LocalReviewMerge", task_id: "t1", candidate_commit: COMMIT };
    expect(evidenceFromReview(wire({ review_state: "awaiting_decision", decision_ready: true, approval }))).toBe(
      "Operator decision on LocalReviewMerge approval for candidate 01234567",
    );
    expect(evidenceFromReview(wire({ review_state: "awaiting_decision" }))).toMatch(/verification run/);
    expect(evidenceFromReview(wire({ review_state: "approved" }))).toMatch(/^None: review approved/);
    expect(evidenceFromReview(wire({ review_state: "changes_requested" }))).toMatch(/requested changes/);
    expect(evidenceFromReview(wire({ review_state: "rejected" }))).toMatch(/rejected/);
    expect(evidenceFromReview(wire({ review_state: "superseded" }))).toMatch(/superseding/);
  });
});
