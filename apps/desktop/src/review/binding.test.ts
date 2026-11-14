import { describe, expect, it } from "vitest";
import { acceptBlockers, deliveryPlan, digestHex, evidenceBlockers, rejectBlockers, type ActionContext } from "./binding";
import { approval, evidenceFor, taskRecord } from "./testing";

function ctx(overrides: Partial<ActionContext> = {}): ActionContext {
  const a = approval();
  return {
    connected: true,
    loading: false,
    stale: false,
    refreshRequired: false,
    approval: a,
    task: taskRecord(),
    diff: { status: "ok", workspaceId: "ws-1", text: "diff" },
    evidence: { state: "available", evidence: evidenceFor(a) },
    ...overrides,
  };
}

describe("Accept policy", () => {
  it("is enabled only for bound, verified, daemon-provenance evidence", () => {
    expect(acceptBlockers(ctx())).toEqual([]);
  });

  it("is blocked by fixture evidence even when every field matches", () => {
    const a = approval();
    const c = ctx({ evidence: { state: "available", evidence: evidenceFor(a, { provenance: "fixture" }) } });
    expect(acceptBlockers(c).join(" ")).toMatch(/PLACEHOLDER DATA/);
  });

  it.each([
    ["task", () => ({ taskId: "other" })],
    ["approval", () => ({ approvalId: "other" })],
    ["candidate commit", () => ({ candidate: { commit: "c".repeat(40), baseCommit: "b".repeat(40) } })],
    ["workspace", () => ({ workspaceId: "ws-other" })],
    ["digest", (a: ReturnType<typeof approval>) => ({ action: { ...evidenceFor(a).action, digestHex: "00" } })],
    ["action kind", (a: ReturnType<typeof approval>) => ({ action: { ...evidenceFor(a).action, kind: "ReportPublish" } })],
    ["verification commit", (a: ReturnType<typeof approval>) => ({ verification: { ...evidenceFor(a).verification, commit: "d".repeat(40) } })],
    ["verification outcome", (a: ReturnType<typeof approval>) => ({ verification: { ...evidenceFor(a).verification, outcome: "failed" as const } })],
  ])("is blocked by a mismatched %s", (_name, mutate) => {
    const a = approval();
    const c = ctx({ evidence: { state: "available", evidence: evidenceFor(a, mutate(a)) } });
    expect(acceptBlockers(c).length).toBeGreaterThan(0);
  });

  it.each([
    ["missing", { state: "missing" as const, reason: "none", provenance: "daemon" as const }],
    ["error", { state: "error" as const, reason: "boom", provenance: "daemon" as const }],
    ["unavailable", { state: "unavailable" as const, reason: "no D7", provenance: "fixture" as const }],
    ["not loaded", null],
  ])("is blocked when evidence is %s", (_n, evidence) => {
    expect(acceptBlockers(ctx({ evidence })).length).toBeGreaterThan(0);
  });

  it("is blocked by too-large diffs, stale state, disconnection, loading, failed mutations and non-pending state", () => {
    expect(acceptBlockers(ctx({ diff: { status: "too_large" } })).join(" ")).toMatch(/too large/);
    expect(acceptBlockers(ctx({ stale: true })).join(" ")).toMatch(/stale/);
    expect(acceptBlockers(ctx({ connected: false })).join(" ")).toMatch(/connection/);
    expect(acceptBlockers(ctx({ loading: true })).join(" ")).toMatch(/loading/);
    expect(acceptBlockers(ctx({ refreshRequired: true })).join(" ")).toMatch(/refresh/);
    const done = { ...approval(), state: { state: "rejected" as const } };
    expect(acceptBlockers(ctx({ approval: done, evidence: { state: "available", evidence: evidenceFor(done) } })).join(" ")).toMatch(/no longer pending/);
  });

  it("is blocked for policy-change approvals", () => {
    const policy = { ...approval(), action: { ...approval().action, kind: { type: "policy_change" as const } } };
    expect(acceptBlockers(ctx({ approval: policy })).join(" ")).toMatch(/not a delivery/);
  });
});

describe("Reject policy", () => {
  it("does not need real evidence", () => {
    expect(rejectBlockers(ctx({ evidence: null, diff: { status: "too_large" } }))).toEqual([]);
  });
  it("still needs a live, fresh, pending decision", () => {
    expect(rejectBlockers(ctx({ stale: true })).length).toBeGreaterThan(0);
    expect(rejectBlockers(ctx({ connected: false })).length).toBeGreaterThan(0);
  });
});

describe("delivery plan", () => {
  const approved = () => ({ ...approval(), state: { state: "approved" as const, by: { kind: "user" as const, id: "u" } } });
  const withApproved = (overrides: Partial<ActionContext> = {}) => {
    const a = approved();
    return ctx({ approval: a, evidence: { state: "available", evidence: evidenceFor(a) }, ...overrides });
  };

  it("builds a local_review request from daemon evidence only", () => {
    expect(deliveryPlan(withApproved()).request).toEqual({
      task_id: "task-1",
      workspace_id: "ws-1",
      approval_id: "approval-1",
      verification_run_id: "run-1",
      action: "local_review",
      target_ref: "refs/heads/review/task-1",
    });
  });

  it("is never available from fixture evidence (fixture run ids cannot authorize delivery)", () => {
    const a = approved();
    const plan = deliveryPlan(
      withApproved({ evidence: { state: "available", evidence: evidenceFor(a, { provenance: "fixture" }) } }),
    );
    expect(plan.request).toBeNull();
    expect(plan.blockers.join(" ")).toMatch(/PLACEHOLDER DATA/);
  });

  it("requires an approved decision, not a merely requested one", () => {
    const a = approval();
    const plan = deliveryPlan(ctx({ approval: a }));
    expect(plan.request).toBeNull();
    expect(plan.blockers.join(" ")).toMatch(/needs an approved/);
  });

  it("requires the task's delivery policy to match the action", () => {
    expect(deliveryPlan(withApproved({ task: taskRecord("task-1", "p", "ReportOnly") })).request).toBeNull();
    expect(deliveryPlan(withApproved({ task: null })).request).toBeNull();
  });

  it("maps ReportPublish to report_only without a target and rejects PullRequestCreate", () => {
    const report = approved();
    report.action.kind = { type: "delivery", kind: "ReportPublish" };
    const e = evidenceFor(report, { action: { ...evidenceFor(report).action, target: null } });
    const plan = deliveryPlan(
      ctx({ approval: report, task: taskRecord("task-1", "p", "ReportOnly"), evidence: { state: "available", evidence: e } }),
    );
    expect(plan.request).toMatchObject({ action: "report_only" });
    expect(plan.request).not.toHaveProperty("target_ref");

    const pr = approved();
    pr.action.kind = { type: "delivery", kind: "PullRequestCreate" };
    expect(deliveryPlan(ctx({ approval: pr, evidence: { state: "available", evidence: evidenceFor(pr) } })).request).toBeNull();
  });

  it("needs a bound target ref for local review", () => {
    const a = approved();
    const e = evidenceFor(a, { action: { ...evidenceFor(a).action, target: null } });
    expect(deliveryPlan(ctx({ approval: a, evidence: { state: "available", evidence: e } })).request).toBeNull();
  });
});

describe("digestHex / evidenceBlockers", () => {
  it("renders bytes as lowercase hex", () => {
    expect(digestHex([0, 15, 255])).toBe("000fff");
  });
  it("reports nothing when no approval is selected", () => {
    expect(evidenceBlockers(ctx({ approval: null }))).toEqual([]);
  });
});


describe("A1b bound candidate policy", () => {
  it.each(["fixture:commit", "fixture0001", "  FIXTURE0001  "])("blocks accept, reject and delivery for candidate %j", (commit) => {
    const a = approval({ commit });
    const context = ctx({ approval: a, evidence: { state: "available", evidence: evidenceFor(a) } });
    expect(acceptBlockers(context).join(" ")).toMatch(/PLACEHOLDER DATA/);
    expect(rejectBlockers(context).join(" ")).toMatch(/PLACEHOLDER DATA/);
    expect(deliveryPlan(context).request).toBeNull();
  });
});
