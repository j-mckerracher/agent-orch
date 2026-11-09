import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { createServices } from "../ui/createServices";
import { createReviewClient } from "./client";
import { createFixtureDaemonClient } from "./fixtureDaemon";
import { ReviewScreen } from "./ReviewScreen";
import {
  createFixtureChangeRequestSource,
  createFixtureReviewEvidenceSource,
  type ChangeRequestSource,
  type ReviewEvidenceSource,
} from "./sources";
import { DIFF, approval, evidenceFor, evidenceSource, fail, makeClient, taskRecord, type TestClient } from "./testing";
import type { Approval } from "../daemon/types";

interface Setup {
  client: TestClient;
  approvals: Approval[];
  onBack: ReturnType<typeof vi.fn>;
  changeSource: ChangeRequestSource;
  evidence: ReviewEvidenceSource;
}

function setup(opts: { approvals?: Approval[]; scope?: { projectId?: string; taskId?: string }; fixtureEvidence?: boolean; changeSource?: ChangeRequestSource } = {}) {
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

  it("goes back to the originating task and, for project scope, the project", async () => {
    const task = setup();
    await ready();
    await userEvent.click(screen.getByRole("button", { name: /Back to task task-1/ }));
    expect(task.onBack).toHaveBeenCalledWith({ taskId: "task-1" });
    task.unmount();

    const project = setup({ scope: { projectId: "project-1" } });
    await ready();
    await userEvent.click(screen.getByRole("button", { name: /Back to project project-1/ }));
    expect(project.onBack).toHaveBeenCalledWith({ projectId: "project-1", taskId: undefined });
  });

  it("scopes pending decisions to the project via tasks.list", async () => {
    setup({ approvals: [approval({ id: "a1", taskId: "task-1" }), approval({ id: "a2", taskId: "task-2" })], scope: { projectId: "project-2" } });
    const select = (await screen.findByLabelText("Pending decision")) as HTMLSelectElement;
    expect([...select.options].map((o) => o.value)).toEqual(["a2"]);
  });

  it("hides non-delivery decisions and says how many", async () => {
    const policy: Approval = { ...approval({ id: "p1" }), action: { ...approval().action, kind: { type: "policy_change" } } };
    setup({ approvals: [policy] });
    expect(await screen.findByText(/1 non-delivery decision/)).toBeTruthy();
  });
});

describe("S6 states", () => {
  it("shows the empty state", async () => {
    setup({ approvals: [] });
    expect(await screen.findByText(/No pending decisions for this task/)).toBeTruthy();
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
    expect(screen.getAllByText("PLACEHOLDER DATA").length).toBeGreaterThan(0);
    await waitFor(() => expect(reject().disabled).toBe(false));
    await userEvent.click(accept());
    expect(client.callsOf("approvals.resolve")).toEqual([]);
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
      await screen.findByText(new RegExp(`Decision failed \\(${kind}\\)`));
      expect(accept().disabled).toBe(true);
      if (kind === "timeout") expect(screen.getByText(/may or may not have applied/)).toBeTruthy();
      await userEvent.click(screen.getByRole("button", { name: "Refresh" }));
      await waitFor(() => expect(accept().disabled).toBe(false));
      await userEvent.click(accept());
      await screen.findByText(/receipt from daemon/);
      const keys = client.callsOf("approvals.resolve").map((c) => c.idempotency_key);
      expect(keys).toEqual(["key-1", "key-1"]);
    },
  );
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
    await screen.findByText(/Delivery is unavailable/);
    expect(deliver().disabled).toBe(true);
    await userEvent.click(reject());
    await screen.findByText(/is now rejected/);
    expect(client.callsOf("delivery.request")).toEqual([]);
  });
});

describe("Request changes (D8)", () => {
  it("never calls approvals.resolve or any daemon method and shows PLACEHOLDER DATA", async () => {
    const { client } = setup();
    await ready();
    const before = client.calls.length;
    await userEvent.click(screen.getByRole("button", { name: "Request changes" }));
    await userEvent.type(screen.getByLabelText(/Feedback/), "Please add tests");
    await userEvent.click(screen.getByRole("button", { name: /Send feedback/ }));
    await screen.findByText(/No daemon call was made/);
    expect(client.calls.length).toBe(before);
    expect(client.callsOf("approvals.resolve")).toEqual([]);
    const panel = screen.getByRole("region", { name: "Request changes" });
    expect(within(panel).getAllByText("PLACEHOLDER DATA").length).toBeGreaterThan(1);
  });

  it.each(["empty", "error", "unavailable"])("labels the %s change-request state as PLACEHOLDER DATA", async (scenario) => {
    setup({ changeSource: createFixtureChangeRequestSource(scenario) });
    await ready();
    const panel = screen.getByRole("region", { name: "Request changes" });
    await waitFor(() => expect(within(panel).getByText(/Change request history/)).toBeTruthy());
    expect(within(panel).getAllByText("PLACEHOLDER DATA").length).toBeGreaterThan(0);
  });

  it("shows a failed submit as an error without touching the daemon", async () => {
    const { client } = setup({ changeSource: createFixtureChangeRequestSource("error") });
    await ready();
    const before = client.calls.length;
    await userEvent.click(screen.getByRole("button", { name: "Request changes" }));
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
    const panel = screen.getByRole("region", { name: "Request changes" });
    expect(within(panel).queryByText("PLACEHOLDER DATA")).toBeNull();
    expect(within(panel).getByText("Ask for changes.")).toBeTruthy();
    // Daemon-provenance evidence enabled Accept; fixture evidence cannot.
    expect(accept().disabled).toBe(false);
    await userEvent.click(within(panel).getByRole("button", { name: "Request changes" }));
    await userEvent.type(within(panel).getByLabelText(/Feedback/), "Please add tests");
    await userEvent.click(within(panel).getByRole("button", { name: "Send request" }));
    await within(panel).findByText(/Receipt real-1: changes_requested/);
    expect(panel.textContent).not.toMatch(/simulated|No daemon call|Preview only|PLACEHOLDER/i);
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
  it("is labelled PLACEHOLDER DATA on receipts, and its fixture evidence cannot authorize anything", async () => {
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
    await screen.findByLabelText("Pending decision");
    await waitFor(() => expect(reject().disabled).toBe(false));
    expect(accept().disabled).toBe(true);
    expect(deliver().disabled).toBe(true);
    await userEvent.click(reject());
    const receipt = await screen.findByText(/receipt from daemon/);
    expect(receipt.textContent).toContain("PLACEHOLDER DATA");
  });
});
