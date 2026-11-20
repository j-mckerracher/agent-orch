import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { createServices } from "../ui/createServices";
import { createReviewClient } from "./client";
import { createFixtureDaemonClient } from "./fixtureDaemon";
import { ReviewScreen, type ReviewScreenProps } from "./ReviewScreen";
import {
  createFixtureChangeRequestSource,
  createFixtureReviewEvidenceSource,
  type ChangeRequestSource,
  type ReviewEvidenceSource,
} from "./sources";
import { DIFF, approval, evidenceFor, evidenceSource, fail, makeClient, taskRecord, type TestClient } from "./testing";
import { APPROVER_REQUIRED_NOTICE } from "../daemon/errors";
import type { Approval } from "../daemon/types";

interface Setup {
  client: TestClient;
  approvals: Approval[];
  onBack: ReturnType<typeof vi.fn>;
  changeSource: ChangeRequestSource;
  evidence: ReviewEvidenceSource;
}

function setup(opts: { approvals?: Approval[]; scope?: { taskId?: string }; fixtureEvidence?: boolean; changeSource?: ChangeRequestSource; record?: ReviewScreenProps["record"]; workstreamOf?: ReviewScreenProps["workstreamOf"] } = {}) {
  const approvals = opts.approvals ?? [approval()];
  const state = { list: [...approvals] };
  const client = makeClient({
    "approvals.list": () => state.list.filter((a) => a.state.state === "requested"),
    "tasks.list": () => [taskRecord("task-1", "project-1"), taskRecord("task-2", "project-2")],
    "tasks.get": (p) => taskRecord(String(p.task_id), p.task_id === "task-2" ? "project-2" : "project-1"),
    "workspaces.diff": (p) => ({ task_id: p.task_id, workspace_id: "ws-1", diff: p.path ? `diff for ${String(p.path)}\n+x` : DIFF }),
    "approvals.resolve": (p) => {
      const target = state.list.find((a) => a.id === p.approval_id)!;
      const next: Approval =
        p.decision === "approve" ? { ...target, state: { state: "approved", by: { kind: "user", id: "u" } } } : { ...target, state: { state: "rejected" } };
      state.list = state.list.map((a) => (a.id === next.id ? next : a));
      return next;
    },
    "delivery.request": (p) => ({ task_id: p.task_id, step_id: "step-1", outcome: "recorded" }),
  });
  const evidence = evidenceSource(
    opts.fixtureEvidence ? "fixture" : "daemon",
    (a) => ({ state: "available", evidence: evidenceFor(a, opts.fixtureEvidence ? { provenance: "fixture" } : {}) }),
    approvals,
  );
  const changeSource = opts.changeSource ?? createFixtureChangeRequestSource();
  const onBack = vi.fn();
  let n = 0;
  const utils = render(
    <ReviewScreen
      client={client}
      evidenceSource={evidence}
      changeRequestSource={changeSource}
      scope={opts.scope ?? { taskId: "task-1" }}
      onBack={onBack}
      newKey={() => `key-${++n}`}
      record={opts.record}
      workstreamOf={opts.workstreamOf}
    />,
  );
  return { client, state, approvals, onBack, changeSource, evidence, ...utils } satisfies Setup & Record<string, unknown>;
}

const accept = () => screen.getByRole("button", { name: "Accept" }) as HTMLButtonElement;
const reject = () => screen.getByRole("button", { name: "Reject" }) as HTMLButtonElement;
const deliver = () => screen.getByRole("button", { name: "Request delivery" }) as HTMLButtonElement;

async function ready() {
  await waitFor(() => expect(accept().disabled).toBe(false));
}

describe("S6 identity, diff and navigation", () => {
  it("shows task, approval, candidate, action, files and the unified diff", async () => {
    const { client } = setup();
    await ready();
    const identity = screen.getByRole("region", { name: "Candidate identity" });
    expect(identity.textContent).toContain("task-1");
    expect(identity.textContent).toContain("approval-1");
    expect(identity.textContent).toContain("a".repeat(40));
    expect(identity.textContent).toContain("LocalReviewMerge");
    const files = screen.getByRole("complementary", { name: "Files" });
    expect(within(files).getByRole("button", { name: "src/a.rs" })).toBeTruthy();
    expect(within(files).getByRole("button", { name: "old.txt" })).toBeTruthy();
    expect(screen.getByLabelText("Diff content").textContent).toContain("+added line");
    expect(client.callsOf("workspaces.diff")[0]).toEqual({ task_id: "task-1" });
  });

  it("requests a scoped diff with path and workspace_id for the selected file", async () => {
    const { client } = setup();
    await ready();
    await userEvent.click(screen.getByRole("button", { name: "src/a.rs" }));
    await waitFor(() => expect(screen.getByLabelText("Diff content").textContent).toContain("diff for src/a.rs"));
    expect(client.callsOf("workspaces.diff").at(-1)).toEqual({ task_id: "task-1", workspace_id: "ws-1", path: "src/a.rs" });
  });

  it("falls back to the loaded full diff for a deleted file the daemon cannot scope", async () => {
    const { client } = setup();
    await ready();
    client.setHandler("workspaces.diff", (p) => {
      if (p.path) fail("invalid-params");
      return { task_id: "task-1", workspace_id: "ws-1", diff: DIFF };
    });
    await userEvent.click(screen.getByRole("button", { name: "old.txt" }));
    await waitFor(() => expect(screen.getByLabelText("Diff content").textContent).toContain("-gone"));
    expect(screen.getByText(/no longer exists in the worktree/)).toBeTruthy();
  });

  it("goes back to the review queue", async () => {
    const { onBack } = setup();
    await ready();
    await userEvent.click(screen.getByRole("button", { name: "← Review" }));
    expect(onBack).toHaveBeenCalledTimes(1);
  });

  it("names the task, project, commit and checks in the title and facts, and has one primary button", async () => {
    setup();
    await ready();
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("Review changes: Objective of task-1");
    expect(document.querySelector(".rv-sub")).toHaveTextContent("project-1 · commit aaaaaaa · checks passed");
    expect(document.querySelectorAll(".ui-btn-primary")).toHaveLength(1);
    expect(screen.getByRole("button", { name: "Accept" })).toHaveClass("ui-btn-primary");
  });

  it("keeps the identity, six facts and audit under a collapsed Technical details", async () => {
    setup({ record: () => <section aria-label="Record probe">Record probe</section> });
    await ready();
    const details = screen.getByText("Technical details").closest("details")!;
    expect(details.open).toBe(false);
    expect(within(details).getByRole("region", { name: "Candidate identity" })).toBeTruthy();
    expect(within(details).getByRole("region", { name: "Record probe" })).toBeTruthy();
  });

  it("offers Changes, Evidence, Request changes and History as tabs, with the file count on Changes", async () => {
    setup();
    await ready();
    expect(screen.getAllByRole("tab").map((tab) => tab.textContent)).toEqual(["Changes 2", "Evidence", "Request changes", "History"]);
  });

  it("opens the evidence in its own tab", async () => {
    setup();
    await ready();
    await userEvent.click(screen.getByRole("tab", { name: "Evidence" }));
    const panel = screen.getByRole("region", { name: "Action parameters and verification evidence" });
    expect(panel).toBeVisible();
    expect(panel).toHaveTextContent("run-1");
  });
});

describe("S6 states", () => {
  it("shows the empty state", async () => {
    setup({ approvals: [] });
    expect(await screen.findByText(/no longer waiting for a decision/)).toBeTruthy();
  });

  it("shows loading before the first response", async () => {
    render(
      <ReviewScreen
        client={makeClient({ "approvals.list": () => new Promise(() => undefined) })}
        evidenceSource={createFixtureReviewEvidenceSource()}
        changeRequestSource={createFixtureChangeRequestSource()}
        scope={{ taskId: "task-1" }}
        onBack={() => undefined}
      />,
    );
    expect(screen.getByText("Loading pending decisions…")).toBeTruthy();
  });

  it("shows unavailable with no fixture data when the live list fails (no silent fallback)", async () => {
    render(
      <ReviewScreen
        client={createReviewClient(createServices().connection)}
        evidenceSource={createFixtureReviewEvidenceSource()}
        changeRequestSource={createFixtureChangeRequestSource()}
        scope={{ taskId: "task-1" }}
        onBack={() => undefined}
      />,
    );
    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toMatch(/unavailable/);
    expect(screen.queryByText(/fixture-approval-1/)).toBeNull();
    expect(screen.queryByRole("button", { name: "Accept" })).toBeNull();
  });

  it("shows oversized diffs, disables Accept, and offers a relative-path request", async () => {
    const { client } = setup();
    client.setHandler("workspaces.diff", (p) => {
      if (p.path) return { task_id: "task-1", workspace_id: "ws-1", diff: "diff --git a/s b/s\n+one" };
      fail("result-too-large");
    });
    await userEvent.click(await screen.findByRole("button", { name: "Refresh" }));
    await screen.findByText(/too large for the daemon/);
    expect(accept().disabled).toBe(true);
    expect(reject().disabled).toBe(false);
    await userEvent.type(screen.getByLabelText("Relative path"), "../etc/passwd");
    await userEvent.click(screen.getByRole("button", { name: "Show file diff" }));
    expect(await screen.findByText(/safe repository-relative path/)).toBeTruthy();
    expect(client.callsOf("workspaces.diff").some((p) => p.path === "../etc/passwd")).toBe(false);
    await userEvent.clear(screen.getByLabelText("Relative path"));
    await userEvent.type(screen.getByLabelText("Relative path"), "s");
    await userEvent.click(screen.getByRole("button", { name: "Show file diff" }));
    await waitFor(() => expect(screen.getByLabelText("Diff content").textContent).toContain("+one"));
    expect(client.callsOf("workspaces.diff").at(-1)).toEqual({ task_id: "task-1", path: "s" });
  });

  it("shows an empty diff", async () => {
    const { client } = setup();
    await ready();
    client.setHandler("workspaces.diff", (p) => ({ task_id: p.task_id, workspace_id: "ws-1", diff: "" }));
    await userEvent.click(screen.getByRole("button", { name: "Refresh" }));
    expect(await screen.findByText(/no changes against its base/)).toBeTruthy();
  });

  it("marks the review stale on a task event and disables actions until Refresh", async () => {
    const { client } = setup();
    await ready();
    act(() => {
      client.emit({
        type: "event",
        event: { sequence: 6, task_id: "task-1", session_id: null, kind: "x", payload: {}, created_at: "now" },
      });
    });
    await screen.findByText(/Stale review/);
    expect(accept().disabled).toBe(true);
    expect(reject().disabled).toBe(true);
    await userEvent.click(screen.getByRole("button", { name: "Refresh" }));
    await waitFor(() => expect(accept().disabled).toBe(false));
    expect(screen.queryByText(/Stale review/)).toBeNull();
  });

  it("ignores events for other tasks", async () => {
    const { client } = setup();
    await ready();
    act(() => {
      client.emit({
        type: "event",
        event: { sequence: 6, task_id: "task-9", session_id: null, kind: "x", payload: {}, created_at: "now" },
      });
    });
    expect(screen.queryByText(/Stale review/)).toBeNull();
    expect(accept().disabled).toBe(false);
  });

  it("goes stale after a reconnect and disables actions while disconnected", async () => {
    const { client } = setup();
    await ready();
    act(() => client.setStatus("disconnected"));
    await waitFor(() => expect(accept().disabled).toBe(true));
    act(() => client.setStatus("connected"));
    await screen.findByText(/Stale review/);
    expect(accept().disabled).toBe(true);
  });

  it("warns when the candidate changed under the same approval id", async () => {
    const { client } = setup();
    await ready();
    client.setHandler("approvals.list", () => [approval({ commit: "e".repeat(40) })]);
    await userEvent.click(screen.getByRole("button", { name: "Refresh" }));
    expect(await screen.findByText(/candidate or action changed/)).toBeTruthy();
  });
});

describe("Accept / Reject", () => {
  it("Accept sends approvals.resolve approve with a key and never calls delivery.request", async () => {
    const { client } = setup();
    await ready();
    await userEvent.click(accept());
    await screen.findByText(/receipt from daemon: approval-1 is now approved/);
    expect(client.callsOf("approvals.resolve")).toEqual([
      { approval_id: "approval-1", decision: "approve", idempotency_key: "key-1" },
    ]);
    expect(client.callsOf("delivery.request")).toEqual([]);
    expect(screen.getByText(/Approval does not deliver anything/)).toBeTruthy();
    // Context was refreshed after the receipt.
    expect(client.callsOf("approvals.list").length).toBeGreaterThan(1);
  });

  it("Reject sends approvals.resolve reject", async () => {
    const { client } = setup();
    await ready();
    await userEvent.click(reject());
    await screen.findByText(/is now rejected/);
    expect(client.callsOf("approvals.resolve")[0]).toMatchObject({ approval_id: "approval-1", decision: "reject" });
  });

  it("keeps Accept disabled for fixture evidence but still allows Reject", async () => {
    const { client } = setup({ fixtureEvidence: true });
    await screen.findAllByText(/cannot authorize approval or delivery/);
    expect(accept().disabled).toBe(true);
    await waitFor(() => expect(reject().disabled).toBe(false));
    await userEvent.click(accept());
    expect(client.callsOf("approvals.resolve")).toEqual([]);
  });

  it("a session that is not the approver cannot Accept or Reject, and the plain-words notice says why", async () => {
    const { client } = setup();
    await ready();
    client.setApprover(false);
    await waitFor(() => expect(accept().disabled).toBe(true));
    expect(reject().disabled).toBe(true);
    // The notice shows once with the decision blockers and once in the Request changes panel.
    expect(screen.getAllByText(APPROVER_REQUIRED_NOTICE).length).toBeGreaterThan(0);
  });

  it("never lets the real D7 fixture enable Accept (default fixtures in real mode)", async () => {
    const approvals = [approval({ id: "fixture-approval-1", taskId: "fixture-task-1", commit: "1".repeat(40) })];
    const client = makeClient({
      "approvals.list": () => approvals,
      "tasks.get": () => taskRecord("fixture-task-1"),
      "workspaces.diff": () => ({ task_id: "fixture-task-1", workspace_id: "fixture-workspace-1", diff: DIFF }),
    });
    render(
      <ReviewScreen
        client={client}
        evidenceSource={createFixtureReviewEvidenceSource()}
        changeRequestSource={createFixtureChangeRequestSource()}
        scope={{ taskId: "fixture-task-1" }}
        onBack={() => undefined}
      />,
    );
    await screen.findByText(/Accept is disabled/);
    expect(accept().disabled).toBe(true);
  });

  it.each(["conflict", "forbidden", "timeout"] as const)(
    "after a %s failure, disables Accept until refresh and retries with the same key",
    async (kind) => {
      const { client } = setup();
      await ready();
      let attempts = 0;
      const original = client.callsOf;
      void original;
      client.setHandler("approvals.resolve", (p) => {
        attempts += 1;
        if (attempts === 1) fail(kind);
        return { ...approval(), state: { state: "approved", by: { kind: "user", id: "u" } }, id: String(p.approval_id) };
      });
      await userEvent.click(accept());
      await screen.findByText(/Decision failed\./);
      expect(screen.getByText(/Decision failed\./).closest("[role=alert]")).not.toBeNull();
      expect(accept().disabled).toBe(true);
      if (kind === "timeout") expect(screen.getByText(/The outcome is unknown/)).toBeTruthy();
      await userEvent.click(screen.getByRole("button", { name: "Refresh" }));
      await waitFor(() => expect(accept().disabled).toBe(false));
      await userEvent.click(accept());
      await screen.findByText(/receipt from daemon/);
      const keys = client.callsOf("approvals.resolve").map((c) => c.idempotency_key);
      expect(keys).toEqual(["key-1", "key-1"]);
    },
  );
});

describe("Busy and result feedback", () => {
  it("Accept is busy while the daemon answers, and says so beside the decision", async () => {
    const { client } = setup();
    await ready();
    client.setHandler("approvals.resolve", () => new Promise(() => undefined));
    await userEvent.click(accept());
    expect(accept()).toHaveAttribute("aria-busy", "true");
    expect(reject()).toBeDisabled();
    expect(await screen.findByText(/Decision: waiting for the daemon/)).toBeTruthy();
  });

  it("Refresh is busy while it reads the candidate again, and then says it was refreshed", async () => {
    setup();
    await ready();
    const refresh = screen.getByRole("button", { name: "Refresh" });
    await userEvent.click(refresh);
    expect(refresh).toHaveAttribute("aria-busy", "true");
    expect(await screen.findByText("Candidate refreshed from the daemon.")).toBeTruthy();
  });

  it("a Refresh the daemon refuses says why nothing was refreshed, and offers Retry", async () => {
    const { client } = setup();
    await ready();
    client.setHandler("approvals.list", () => fail("timeout"));
    await userEvent.click(screen.getByRole("button", { name: "Refresh" }));
    const alert = (await screen.findByText(/so nothing was refreshed/)).closest("[role=alert]") as HTMLElement;
    expect(within(alert).getByRole("button", { name: "Retry" })).toBeTruthy();
  });

  it("Request delivery is busy while the daemon answers", async () => {
    const { client } = setup();
    await ready();
    await userEvent.click(accept());
    await waitFor(() => expect(deliver().disabled).toBe(false));
    client.setHandler("delivery.request", () => new Promise(() => undefined));
    await userEvent.click(deliver());
    expect(deliver()).toHaveAttribute("aria-busy", "true");
    expect(await screen.findByText(/Delivery request: waiting for the daemon/)).toBeTruthy();
  });
});

describe("Delivery", () => {
  it("is a separate explicit request using daemon evidence and the bound target", async () => {
    const { client } = setup();
    await ready();
    expect(deliver().disabled).toBe(true);
    await userEvent.click(accept());
    await waitFor(() => expect(deliver().disabled).toBe(false));
    expect(client.callsOf("delivery.request")).toEqual([]);
    await userEvent.click(deliver());
    await screen.findByText(/Delivery request receipt from daemon/);
    expect(client.callsOf("delivery.request")).toEqual([
      {
        task_id: "task-1",
        workspace_id: "ws-1",
        approval_id: "approval-1",
        verification_run_id: "run-1",
        action: "local_review",
        target_ref: "refs/heads/review/task-1",
        idempotency_key: "key-2",
      },
    ]);
  });

  it("is unavailable with fixture evidence and never sends fixture ids", async () => {
    const { client } = setup({ fixtureEvidence: true });
    await waitFor(() => expect(reject().disabled).toBe(false));
    expect(deliver().disabled).toBe(true);
    await userEvent.click(reject());
    await screen.findByText(/is now rejected/);
    expect(client.callsOf("delivery.request")).toEqual([]);
  });
});

const openTab = (name: string) => userEvent.click(screen.getByRole("tab", { name }));

describe("Request changes (D8)", () => {
  it("never calls approvals.resolve or any daemon method", async () => {
    const { client } = setup();
    await ready();
    const before = client.calls.length;
    await openTab("Request changes");
    await userEvent.type(screen.getByLabelText(/Feedback/), "Please add tests");
    await userEvent.click(screen.getByRole("button", { name: /Send feedback/ }));
    await screen.findByText(/No daemon call was made/);
    expect(client.calls.length).toBe(before);
    expect(client.callsOf("approvals.resolve")).toEqual([]);
  });

  it("shows the sent request in the History tab", async () => {
    setup();
    await ready();
    await openTab("Request changes");
    await userEvent.type(screen.getByLabelText(/Feedback/), "Please add tests");
    await userEvent.click(screen.getByRole("button", { name: /Send feedback/ }));
    await screen.findByText(/No daemon call was made/);
    await openTab("History");
    expect(within(screen.getByRole("list", { name: "Change request history" })).getByText(/Please add tests/)).toBeVisible();
  });

  it.each([
    ["empty", /No changes were requested yet/],
    ["error", /Change request history error/],
  ])("says plainly that the %s change-request history has nothing to show", async (scenario, text) => {
    setup({ changeSource: createFixtureChangeRequestSource(scenario) });
    await ready();
    await openTab("History");
    await waitFor(() => expect(screen.getByText(text)).toBeVisible());
  });

  it("shows a failed submit as an error without touching the daemon", async () => {
    const { client } = setup({ changeSource: createFixtureChangeRequestSource("error") });
    await ready();
    const before = client.calls.length;
    await openTab("Request changes");
    await userEvent.type(screen.getByLabelText(/Feedback/), "x");
    await userEvent.click(screen.getByRole("button", { name: /Send feedback/ }));
    expect((await screen.findAllByRole("alert")).length).toBeGreaterThan(0);
    expect(client.calls.length).toBe(before);
  });
});

describe("D7/D8 swappability", () => {
  it("renders a daemon-provenance stub adapter, including submit, with no fixture wording", async () => {
    const stub: ChangeRequestSource = {
      provenance: "daemon",
      labels: { description: "Ask for changes.", submitAction: "Send request", receiptNote: "" },
      loadHistory: async () => ({ state: "available", items: [] }),
      submit: async (input) => ({
        provenance: "daemon",
        requestId: "real-1",
        taskId: input.taskId,
        candidateCommit: input.candidateCommit,
        feedback: input.feedback,
        resultingState: "changes_requested",
        submittedAt: "now",
      }),
    };
    setup({ changeSource: stub });
    await ready();
    await openTab("Request changes");
    const panel = screen.getByRole("region", { name: "Request changes" });
    // Daemon-provenance evidence enabled Accept; fixture evidence cannot.
    expect(accept().disabled).toBe(false);
    await userEvent.type(within(panel).getByLabelText(/Feedback/), "Please add tests");
    await userEvent.click(within(panel).getByRole("button", { name: "Send request" }));
    await within(panel).findByText(/Receipt real-1: changes_requested/);
    expect(panel.textContent).not.toMatch(/simulated|No daemon call|Preview only|PLACEHOLDER/i);
    expect(screen.queryByTestId("fixture-note")).toBeNull();
  });
});

const approvalEvent = (id: string, kind = "approval.approved", taskId = "task-1") => ({
  type: "event" as const,
  event: { sequence: 7, task_id: taskId, session_id: null, kind, payload: { approval_id: id, event: { Approved: {} } }, created_at: "now" },
});

describe("self-originated approval events (M5-02 `approval.<decision>` durable events)", () => {
  it("arriving after the receipt do not mark the review stale", async () => {
    const { client } = setup();
    await ready();
    await userEvent.click(accept());
    await screen.findByText(/receipt from daemon/);
    act(() => client.emit(approvalEvent("approval-1")));
    await waitFor(() => expect(deliver().disabled).toBe(false));
    expect(screen.queryByText(/Stale review/)).toBeNull();
  });

  it("arriving before the receipt (same transaction) do not mark the review stale", async () => {
    const { client, state } = setup();
    await ready();
    let release!: () => void;
    const gate = new Promise<void>((resolve) => (release = resolve));
    client.setHandler("approvals.resolve", async (p) => {
      // The daemon appends the event inside the same transaction, so it can arrive before the receipt.
      act(() => client.emit(approvalEvent(String(p.approval_id))));
      await gate;
      const approved: Approval = { ...approval(), state: { state: "approved", by: { kind: "user", id: "u" } } };
      state.list = [approved];
      return approved;
    });
    await userEvent.click(accept());
    release();
    await screen.findByText(/receipt from daemon/);
    await waitFor(() => expect(deliver().disabled).toBe(false));
    expect(screen.queryByText(/Stale review/)).toBeNull();
    // Only one event is expected per submitted decision; a second one is external.
    act(() => client.emit(approvalEvent("approval-1")));
    await screen.findByText(/Stale review/);
  });

  it("only suppress the one event for that approval and decision", async () => {
    const { client } = setup();
    await ready();
    await userEvent.click(accept());
    await screen.findByText(/receipt from daemon/);
    act(() => client.emit(approvalEvent("other-approval")));
    await screen.findByText(/Stale review/);
  });

  it("still mark the review stale for other kinds of task events after approve", async () => {
    const { client } = setup();
    await ready();
    await userEvent.click(accept());
    await screen.findByText(/receipt from daemon/);
    act(() => client.emit(approvalEvent("approval-1", "task.transition")));
    await screen.findByText(/Stale review/);
    expect(deliver().disabled).toBe(true);
  });

  it("are not suppressed when the decision was not ours (no submitted request)", async () => {
    const { client } = setup();
    await ready();
    act(() => client.emit(approvalEvent("approval-1", "approval.rejected")));
    await screen.findByText(/Stale review/);
  });

  it("are not suppressed when our request failed", async () => {
    const { client } = setup();
    await ready();
    client.setHandler("approvals.resolve", () => fail("timeout"));
    await userEvent.click(accept());
    await screen.findByText(/Decision failed \(timeout\)/);
    act(() => client.emit(approvalEvent("approval-1")));
    await screen.findByText(/Stale review/);
  });
});

describe("self-originated delivery events (`delivery.<outcome>` durable events)", () => {
  const deliveryEvent = (taskId = "task-1", kind = "delivery.review_ref_created") => ({
    type: "event" as const,
    event: { sequence: 9, task_id: taskId, session_id: null, kind, payload: { step_id: "step-1", outcome: "review_ref_created" }, created_at: "now" },
  });

  async function delivered() {
    const ctx = setup();
    await ready();
    await userEvent.click(accept());
    await waitFor(() => expect(deliver().disabled).toBe(false));
    await userEvent.click(deliver());
    await screen.findByText(/Delivery request receipt from daemon/);
    return ctx;
  }

  it("arriving after the receipt do not mark the review stale, but a second one does", async () => {
    const { client } = await delivered();
    act(() => client.emit(deliveryEvent()));
    expect(screen.queryByText(/Stale review/)).toBeNull();
    act(() => client.emit(deliveryEvent()));
    await screen.findByText(/Stale review/);
  });

  it("still mark the review stale when the delivery event is for another task", async () => {
    const { client } = await delivered();
    act(() => client.emit(deliveryEvent("task-other")));
    expect(screen.queryByText(/Stale review/)).toBeNull();
    act(() => client.emit({ ...deliveryEvent(), event: { ...deliveryEvent().event, task_id: "task-1", kind: "task.transition" } }));
    await screen.findByText(/Stale review/);
  });

  it("are not suppressed when our delivery request failed", async () => {
    const { client } = setup();
    await ready();
    client.setHandler("delivery.request", () => fail("timeout"));
    await userEvent.click(accept());
    await waitFor(() => expect(deliver().disabled).toBe(false));
    await userEvent.click(deliver());
    await screen.findByText(/Delivery request failed \(timeout\)/);
    act(() => client.emit(deliveryEvent()));
    await screen.findByText(/Stale review/);
  });
});

describe("request integrity", () => {
  it("rejects a diff the daemon returns for a different task", async () => {
    const { client } = setup();
    client.setHandler("workspaces.diff", () => ({ task_id: "task-other", workspace_id: "ws-1", diff: DIFF }));
    await userEvent.click(await screen.findByRole("button", { name: "Refresh" }));
    await screen.findByText(/different task/);
    expect(accept().disabled).toBe(true);
    expect(screen.queryByLabelText("Diff content")).toBeNull();
  });

  it("rejects a scoped diff for a different workspace and marks the review stale", async () => {
    const { client } = setup();
    await ready();
    client.setHandler("workspaces.diff", (p) => ({ task_id: p.task_id, workspace_id: "ws-other", diff: "x" }));
    await userEvent.click(screen.getByRole("button", { name: "src/a.rs" }));
    await screen.findByText(/different task or workspace/);
    expect(accept().disabled).toBe(true);
  });

  it("sends Accept once when double-submitted while the request is pending", async () => {
    const { client } = setup();
    await ready();
    let release!: () => void;
    const gate = new Promise<void>((resolve) => (release = resolve));
    client.setHandler("approvals.resolve", async () => {
      await gate;
      return { ...approval(), state: { state: "approved", by: { kind: "user", id: "u" } } };
    });
    const button = accept();
    // Two clicks in one act() share a render, so only the handler-level guard can stop the second.
    act(() => {
      button.click();
      button.click();
    });
    await waitFor(() => expect(accept().disabled).toBe(true));
    await userEvent.click(reject());
    release();
    await screen.findByText(/receipt from daemon/);
    expect(client.callsOf("approvals.resolve")).toHaveLength(1);
  });

  it("sends delivery once when double-submitted", async () => {
    const { client } = setup();
    await ready();
    await userEvent.click(accept());
    await waitFor(() => expect(deliver().disabled).toBe(false));
    let release!: () => void;
    const gate = new Promise<void>((resolve) => (release = resolve));
    client.setHandler("delivery.request", async (p) => {
      await gate;
      return { task_id: p.task_id, step_id: "s", outcome: "recorded" };
    });
    const button = deliver();
    act(() => {
      button.click();
      button.click();
    });
    release();
    await screen.findByText(/Delivery request receipt from daemon/);
    expect(client.callsOf("delivery.request")).toHaveLength(1);
  });
});

describe("fixture daemon demo mode", () => {
  it("notes the fixture data under Technical details, and its fixture evidence cannot authorize anything", async () => {
    const client = createFixtureDaemonClient();
    render(
      <ReviewScreen
        client={client}
        evidenceSource={createFixtureReviewEvidenceSource()}
        changeRequestSource={createFixtureChangeRequestSource()}
        scope={{ taskId: "fixture-task-1" }}
        onBack={() => undefined}
      />,
    );
    await waitFor(() => expect(reject().disabled).toBe(false));
    expect(accept().disabled).toBe(true);
    expect(deliver().disabled).toBe(true);
    await userEvent.click(reject());
    await screen.findByText(/receipt from daemon/);
    // The provenance note lives under Technical details.
    expect(within(screen.getByText("Technical details").closest("details")!).getByTestId("fixture-note")).toHaveTextContent(/placeholder data/i);
  });
});

describe("six-fact record placement", () => {
  it("renders the record under Technical details, after the diff and the delivery step", async () => {
    setup({ record: () => <section aria-label="Record probe">Record probe</section> });
    await ready();
    const probe = screen.getByRole("region", { name: "Record probe" });
    const diff = screen.getByRole("region", { name: "Diff" });
    const delivery = screen.getByRole("region", { name: "Delivery" });
    expect(diff.compareDocumentPosition(probe) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(delivery.compareDocumentPosition(probe) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });
});

describe("candidate identity and actions", () => {
  it("names the project and workstream in the candidate identity", async () => {
    setup({ workstreamOf: () => "Authentication" });
    await ready();
    const identity = screen.getByRole("region", { name: "Candidate identity" });
    await waitFor(() => expect(identity).toHaveTextContent("project-1"));
    expect(identity).toHaveTextContent("Workstream");
    expect(identity).toHaveTextContent("Authentication");
  });

  it("hides Request additional checks: the daemon has no method for it", async () => {
    setup();
    await ready();
    expect(screen.queryByRole("button", { name: "Request additional checks" })).toBeNull();
  });
});
