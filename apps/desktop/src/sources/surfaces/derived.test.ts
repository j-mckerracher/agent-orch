import { afterEach, describe, expect, it } from "vitest";
import { DaemonError } from "../../daemon/errors";
import { ALL_CAPABILITIES, type ActionProposalWire } from "../../daemon/types";
import { FakeDaemon, mkApproval, mkTask } from "../../test/fakeDaemon";
import { stubConversation } from "../../test/m724Stubs";
import { ALPHA, BETA, WORKSTREAM_STATE, connect, populate } from "./testkit";

const stops: Array<() => void> = [];
afterEach(() => {
  while (stops.length) stops.pop()!();
});

async function live(fake: FakeDaemon, start = true) {
  const connected = await connect(fake, start);
  stops.push(connected.stop);
  return connected.surfaces;
}

const asKind = (kind: string) => expect.objectContaining({ kind });

describe("derived D9 workstreams", () => {
  it("maps daemon workstreams, objectives, gates and links, joined with task records", async () => {
    const fake = populate(new FakeDaemon());
    const s = await live(fake);
    const result = await s.workstreams.workstreams();
    expect(result).toMatchObject({
      status: "ok",
      provenance: { kind: "daemon", id: "D9", methods: ["workstreams.list", "workstreams.get", "tasks.list"] },
      ageMs: 0,
    });
    if (result.status !== "ok") return;
    expect(result.data.map((w) => [w.id, w.title, w.state, w.revision])).toEqual([
      ["ws-1", "Session hardening", "active", 2],
      ["ws-2", "Docs refresh", "suspended", 1],
    ]);
    const first = result.data[0]!;
    expect(first.summary).toBe("Make sessions durable");
    expect(first.projectIds).toEqual([ALPHA, BETA]);
    expect(first.tasks.map((t) => [t.id, t.projectId, t.objectiveId, t.state])).toEqual([
      ["t-run", ALPHA, "obj-1", "Running"],
      ["t-wait", ALPHA, "obj-1", "Queued"],
      ["t-done", BETA, null, "Succeeded"],
    ]);
    expect(first.tasks[1]).toMatchObject({ objective: "Document the session model", dependencies: ["t-run"] });
    expect(first.counts).toMatchObject({ status: "ok", value: { Running: 1, Queued: 1, Succeeded: 1, Failed: 0 } });
    expect(first.objectives).toMatchObject({
      status: "ok",
      value: [
        {
          id: "obj-1",
          title: "Durable sessions",
          description: "Sessions survive restarts",
          revision: 3,
          gates: [
            { id: "gate-1", label: "Restart test passes", state: "verified", evidenceTaskId: "t-done" },
            { id: "gate-2", label: "Docs updated", state: "pending", evidenceTaskId: null },
          ],
        },
      ],
    });
    // The empty workstream is a real, known-empty answer, not an unsupported one.
    expect(result.data[1]).toMatchObject({ tasks: [], projectIds: [], objectives: { status: "ok", value: [] } });
    expect(result.data[1]!.counts).toMatchObject({ status: "ok", value: { Running: 0 } });
  });

  it("keeps the daemon's task state and marks a linked task that tasks.list no longer returns", async () => {
    const fake = populate(new FakeDaemon());
    fake.tasks = fake.tasks.filter((task) => task.contract.id !== "t-wait");
    WORKSTREAM_STATE.get(fake)!.links.push({ task_id: "t-ghost", workstream_id: "ws-2", objective_id: null });
    const s = await live(fake);
    const result = await s.workstreams.workstreams();
    const ws1 = result.status === "ok" ? result.data[0]! : null;
    expect(ws1?.tasks.map((t) => [t.id, t.objective, t.agent, t.state])).toEqual([
      ["t-run", "objective of t-run", "fake-agent", "Running"],
      ["t-wait", "", "unknown", "Created"],
      ["t-done", "objective of t-done", "fake-agent", "Succeeded"],
    ]);
    const ws2 = result.status === "ok" ? result.data[1]! : null;
    expect(ws2?.tasks).toEqual([
      { id: "t-ghost", projectId: "", objectiveId: null, objective: "", state: "Created", agent: "unknown", dependencies: [] },
    ]);
  });

  it("reports a failed read as unavailable with daemon provenance, never as fixture data", async () => {
    const fake = populate(new FakeDaemon());
    // Every detail read fails: nothing is known, so the read fails instead of listing unknown rows.
    fake.failNext("workstreams.get", { code: -32603, message: "boom" }, 2);
    const result = await (await live(fake)).workstreams.workstreams();
    expect(result).toMatchObject({ status: "unavailable", provenance: { kind: "daemon", id: "D9" } });
    expect(result).not.toHaveProperty("data");
  });

  it("keeps the readable workstreams when one detail read fails, and marks that row unknown", async () => {
    const fake = populate(new FakeDaemon());
    fake.failNext("workstreams.get", { code: -32603, message: "boom" });
    const result = await (await live(fake)).workstreams.workstreams();
    if (result.status !== "ok") throw new Error(`expected ok, got ${result.status}`);
    expect(result.data.map((item) => item.unreadReason !== undefined)).toEqual([true, false]);
    expect(result.data[0]!.objectives.status).toBe("unknown");
  });
});

describe("derived D10 conversation", () => {
  it("preserves legacy notes alongside a live project conversation", async () => {
    const fake = populate(new FakeDaemon());
    stubConversation(fake, "completed");
    const s = await live(fake);
    await s.conversation.send({ kind: "project", projectId: ALPHA }, "new question");
    const result = await s.conversation.transcript({ kind: "project", projectId: ALPHA });
    expect(result).toMatchObject({
      status: "ok",
      data: { repliesExpected: true, truncated: false, entries: [
        { id: "c-1", role: "user", text: "hello", kind: "message" },
        { role: "user", text: "new question" },
        { role: "coordinator", text: "Hello from the runtime." },
      ] },
    });
  });

  it("flags a transcript that hit the paging limit as truncated", async () => {
    const fake = new FakeDaemon();
    stubConversation(fake);
    let sequence = 0;
    fake.stub("coordinator.get", (params: Record<string, unknown>) => {
      sequence += 1;
      return {
        project_id: params.project_id,
        entries: [{ sequence, id: `c-${sequence}`, project_id: ALPHA, kind: "message", author: "user", body: "x", validation: null, created_at: "2026-01-01T00:00:00Z" }],
        more: true,
        next_after_sequence: sequence,
      };
    });
    const result = await (await live(fake)).conversation.transcript({ kind: "project", projectId: ALPHA });
    expect(result.status === "ok" && result.data.truncated).toBe(true);
    expect(fake.callsTo("coordinator.get")).toHaveLength(50);
  });

  it("answers a global transcript from conversations.get and never touches the project coordinator", async () => {
    const fake = populate(new FakeDaemon());
    stubConversation(fake);
    const result = await (await live(fake)).conversation.transcript({ kind: "global" });
    expect(result).toMatchObject({ status: "ok", data: { entries: [], turn: null }, provenance: { methods: ["conversations.get", "conversations.send", "conversations.cancel", "conversations.clear", "actions.resolve"] } });
    expect(fake.callsTo("conversations.get")).toHaveLength(1);
    expect(fake.callsTo("coordinator.get")).toHaveLength(0);
  });

  it("says a daemon without conversations.get does not support a global transcript", async () => {
    const result = await (await live(populate(new FakeDaemon()))).conversation.transcript({ kind: "global" });
    expect(result.status).toBe("unsupported");
  });

  it("sends a project message and reuses the key after an indeterminate failure", async () => {
    const fake = populate(new FakeDaemon());
    stubConversation(fake, "completed");
    fake.failNext("conversations.send", "hang");
    const s = await live(fake);
    await expect(s.conversation.send({ kind: "project", projectId: ALPHA }, "ship it")).rejects.toMatchObject({ kind: "timeout" });
    const receipt = await s.conversation.send({ kind: "project", projectId: ALPHA }, "ship it");
    expect(receipt).toMatchObject({ simulated: false, turn: { state: "completed" }, provenance: { kind: "daemon", id: "D10" } });
    const keys = fake.callsTo("conversations.send").map((call) => (call.params as { idempotency_key: string }).idempotency_key);
    expect(keys).toHaveLength(2);
    expect(keys[0]).toBe(keys[1]);
  });

  it("refuses project sends on old daemons rather than silently saving notes", async () => {
    const fake = populate(new FakeDaemon());
    fake.capabilitiesOverride = ALL_CAPABILITIES.filter((capability) => capability !== "project_conversations");
    stubConversation(fake);
    const s = await live(fake);
    const choice = { agent: "codex", model: "gpt-a" };
    await expect(s.conversation.send({ kind: "project", projectId: ALPHA }, "note", choice)).rejects.toMatchObject({
      kind: "capability-unavailable",
    });
    expect(fake.callsTo("coordinator.send_message")).toHaveLength(0);
    expect(fake.callsTo("conversations.send")).toHaveLength(0);
  });

  it("sends a global message through conversations.send, not coordinator.send_message", async () => {
    const fake = populate(new FakeDaemon());
    stubConversation(fake);
    const s = await live(fake);
    const receipt = await s.conversation.send({ kind: "global" }, "hi");
    expect(receipt).toMatchObject({ simulated: false, turn: { state: "failed", errorCode: "runtime_unavailable" } });
    expect(receipt.notice).toBe("Recorded by the daemon.");
    expect(fake.callsTo("coordinator.send_message")).toHaveLength(0);
    expect(fake.callsTo("conversations.send")).toHaveLength(1);
  });
});

describe("derived D11 activity", () => {
  it("splits running and queued, explains waiting honestly, and lists only requested approvals", async () => {
    const result = await (await live(populate(new FakeDaemon()))).activity.activity();
    expect(result).toMatchObject({ status: "ok", provenance: { methods: ["queue.snapshot", "tasks.list", "approvals.list"] } });
    if (result.status !== "ok") return;
    expect(result.data.running.map((e) => [e.taskId, e.waiting])).toEqual([["t-run", null]]);
    const queued = Object.fromEntries(result.data.queued.map((e) => [e.taskId, e.waiting]));
    expect(queued["t-wait"]).toMatchObject({ reason: "dependency" });
    expect(queued["t-free"]).toMatchObject({ reason: "unknown" });
    expect(result.data.attention).toEqual([
      { approvalId: "a-1", taskId: "t-run", projectId: ALPHA, kind: "LocalReviewMerge", priority: "unknown" },
    ]);
    expect(result.data.slots).toMatchObject({ status: "ok", value: { used: 1, limit: 2 } });
    expect(result.data.upNext).toMatchObject({ status: "ok", value: "t-free" });
    expect(result.data.dispatchPaused.status).toBe("unsupported");
  });

  it("treats a dependency on an unlisted task as unmet", async () => {
    const fake = new FakeDaemon();
    fake.tasks = [mkTask("t1", ALPHA, { state: "Queued", contract: { dependencies: ["ghost"] } })];
    const result = await (await live(fake)).activity.activity();
    expect(result.status === "ok" && result.data.queued[0]!.waiting).toMatchObject({ reason: "dependency" });
  });
});

describe("derived D14 models", () => {
  it("maps agents.capabilities and never claims a model list", async () => {
    const result = await (await live(populate(new FakeDaemon()))).models.catalog();
    expect(result.status === "ok" && result.data.providers).toMatchObject([
      { id: "codex", installed: true, launchable: true, modelSelection: "supported", models: { status: "unsupported" }, efforts: { status: "unsupported" } },
      { id: "pi", installed: false, modelSelection: "unknown", models: { status: "unsupported" } },
    ]);
  });
});

describe("derived D16 decisions", () => {
  it("joins approvals to projects and offers no extra actions", async () => {
    const result = await (await live(populate(new FakeDaemon()))).decisions.queue();
    expect(result.status === "ok" && result.data.items).toMatchObject([
      { id: "a-1", projectId: ALPHA, state: "requested", priority: "unknown", latestResponseAt: null },
      { id: "a-2", projectId: BETA, state: "approved" },
    ]);
    if (result.status !== "ok") return;
    for (const action of Object.values(result.data.actions)) expect(action.available).toBe(false);
  });

  it("keeps an approval whose task is unknown with a null project", async () => {
    const fake = new FakeDaemon();
    fake.approvals = [mkApproval("a-x", "ghost-task")];
    const result = await (await live(fake)).decisions.queue();
    expect(result.status === "ok" && result.data.items[0]!.projectId).toBeNull();
  });

  const pendingWire = (id: string, over: Partial<ActionProposalWire> = {}): ActionProposalWire => ({
    id,
    turn_id: "turn-1",
    index: 0,
    kind: "task.propose",
    params: { kind: "task.propose", project: "p-1", objective: "Add unit tests", agent: "codex" },
    digest: "d".repeat(64),
    depends_on: null,
    state: "pending",
    result_ref: null,
    error_code: null,
    denial: null,
    target: { project_name: "alpha" },
    created_at: "2026-11-10T21:00:00Z",
    updated_at: "2026-11-10T21:00:00Z",
    ...over,
  });

  it("queue_lists_each_pending_proposal_with_the_conversation_that_holds_its_card", async () => {
    const fake = populate(new FakeDaemon());
    fake.stub("actions.pending", {
      proposals: [
        { ...pendingWire("proposal-1"), scope: { kind: "global" } },
        {
          ...pendingWire("proposal-2", {
            kind: "task.cancel",
            params: { kind: "task.cancel", task_id: "t-run" },
            target: { task_title: "objective of t-run", task_state: "Running" },
          }),
          scope: { kind: "workstream", workstream_id: "ws-1" },
        },
      ],
      more: false,
    });
    const result = await (await live(fake)).decisions.queue();
    expect(result).toMatchObject({
      status: "ok",
      provenance: { kind: "daemon", id: "D16", methods: ["approvals.list", "tasks.list", "actions.pending"] },
    });
    if (result.status !== "ok") return;
    expect(result.data.proposals.map((item) => [item.proposal.id, item.proposal.kind, item.scope])).toEqual([
      ["proposal-1", "task.propose", { kind: "global" }],
      ["proposal-2", "task.cancel", { kind: "workstream", workstreamId: "ws-1" }],
    ]);
    // The wire's `kind` inside `params` is dropped, as for every card, and the display facts come through.
    expect(result.data.proposals[0]!.proposal.params).not.toHaveProperty("kind");
    expect(result.data.proposals[1]!.proposal.target).toEqual({ projectName: undefined, taskTitle: "objective of t-run", taskState: "Running" });
    expect(fake.callsTo("actions.pending")).toHaveLength(1);
  });

  it("queue_names_a_coordinator_card_by_its_project_conversation_and_reads_its_display_facts", async () => {
    const fake = populate(new FakeDaemon());
    fake.stub("actions.pending", {
      proposals: [
        {
          ...pendingWire("proposal-3", {
            kind: "coordinator.report",
            params: { kind: "coordinator.report", verb: "answer", note: "Done.", request: "r-0123456789abcdef" },
            target: { request_work: "Write the page.", request_answered: false },
          }),
          scope: { kind: "project", project_id: ALPHA },
        },
      ],
      more: false,
    });
    const result = await (await live(fake)).decisions.queue();
    if (result.status !== "ok") throw new Error("queue not ok");
    expect(result.data.proposals[0]!.scope).toEqual({ kind: "project", projectId: ALPHA });
    expect(result.data.proposals[0]!.proposal.target).toMatchObject({ requestWork: "Write the page.", requestAnswered: false });
  });

  it("coordinator_state_is_read_from_the_project_conversation_and_each_settled_scope_save_gets_a_new_key", async () => {
    const fake = populate(new FakeDaemon());
    const conversation = stubConversation(fake, "completed");
    conversation.coordinator = {
      scope_sentence: "Owns docs.",
      open_tasks: 2,
      open_requests: 1,
      routed_requests: [{ id: "r-0123456789abcdef", work: "Write the page.", answered: false, created_at: "2026-11-16T21:04:11Z" }],
      reports: [
        { id: "rep-1", author: "daemon", verb: "task_done", request_id: null, task_id: "t-run", key: null, note: null, created_at: "2026-11-16T21:04:11Z" },
      ],
    };
    fake.stub("coordinator.get", { project_id: ALPHA, entries: [], more: false, next_after_sequence: 0, clear_generation: 0 });
    const s = await live(fake);
    const result = await s.conversation.transcript({ kind: "project", projectId: ALPHA });
    if (result.status !== "ok") throw new Error("transcript not ok");
    expect(result.data.coordinator).toEqual({
      scopeSentence: "Owns docs.",
      openTasks: 2,
      openRequests: 1,
      routedRequests: [{ id: "r-0123456789abcdef", work: "Write the page.", answered: false, createdAt: "2026-11-16T21:04:11Z" }],
      reports: [
        { id: "rep-1", author: "daemon", verb: "task_done", requestId: null, taskId: "t-run", key: null, note: null, createdAt: "2026-11-16T21:04:11Z" },
      ],
    });

    fake.failNext("coordinator.set_scope", { code: -32010, message: "not found" });
    await expect(s.conversation.setCoordinatorScope!(ALPHA, "Owns docs and notes.")).rejects.toMatchObject({ kind: "not-found" });
    fake.stub("coordinator.set_scope", (params: Record<string, unknown>) => ({ project_id: params.project_id, scope_sentence: params.scope_sentence }));
    const saved = await s.conversation.setCoordinatorScope!(ALPHA, "Owns docs and notes.");
    expect(saved).toMatchObject({ simulated: false, scopeSentence: "Owns docs and notes." });
    const cleared = await s.conversation.setCoordinatorScope!(ALPHA, null);
    expect(cleared.scopeSentence).toBeNull();
    const calls = fake.callsTo("coordinator.set_scope").map((call) => call.params as Record<string, unknown>);
    expect(calls.map((call) => call.scope_sentence)).toEqual(["Owns docs and notes.", "Owns docs and notes.", null]);
    expect(calls[0]!.idempotency_key).not.toBe(calls[1]!.idempotency_key);
    expect(calls[2]!.idempotency_key).not.toBe(calls[1]!.idempotency_key);
  });

  it("queue_has_no_proposals_and_no_failure_when_the_daemon_has_no_actions_pending", async () => {
    const result = await (await live(populate(new FakeDaemon()))).decisions.queue();
    expect(result.status).toBe("ok");
    expect(result.status === "ok" && result.data.proposals).toEqual([]);
  });

  it("queue_is_unavailable_not_empty_when_actions_pending_fails_for_another_reason", async () => {
    const fake = populate(new FakeDaemon());
    fake.failNext("actions.pending", { code: -32603, message: "store busy" }, 10);
    const result = await (await live(fake)).decisions.queue();
    expect(result.status).toBe("unavailable");
    expect("data" in result).toBe(false);
  });
});

describe("fixture ids cannot reach a real mutation", () => {
  it("refuses a fixture: project id before any request, connected or not", async () => {
    for (const start of [true, false]) {
      const fake = populate(new FakeDaemon());
      const s = await live(fake, start);
      await expect(s.conversation.send({ kind: "project", projectId: "fixture:project-alpha" }, "x")).rejects.toMatchObject({
        kind: "invalid-params",
      });
      expect(fake.callsTo("coordinator.send_message")).toHaveLength(0);
    }
  });

  it("answers a fixture: transcript as unsupported without calling coordinator.get", async () => {
    const fake = populate(new FakeDaemon());
    const result = await (await live(fake)).conversation.transcript({ kind: "project", projectId: "fixture:project-alpha" });
    expect(result.status).toBe("unsupported");
    expect(fake.callsTo("coordinator.get")).toHaveLength(0);
  });

  it("rejects every simulated action on derived adapters and sends nothing", async () => {
    const fake = populate(new FakeDaemon());
    const s = await live(fake);
    const before = fake.calls.length;
    const attempts = [
      s.activity.simulatePauseDispatch(),
      s.activity.simulateStop("fixture:task-auth-2"),
      s.decisions.simulatePostpone("fixture:approval-1"),
      s.decisions.simulateDismiss("fixture:approval-1"),
    ];
    for (const attempt of attempts) {
      await expect(attempt).rejects.toBeInstanceOf(DaemonError);
      await expect(attempt).rejects.toMatchObject({ kind: "method-unavailable" });
    }
    expect(fake.calls.length).toBe(before);
  });
});

describe("a failed live call never falls back to fixtures", () => {
  it("returns non-ok daemon results with no data when the daemon errors", async () => {
    const fake = populate(new FakeDaemon());
    for (const method of ["workstreams.list", "tasks.list", "approvals.list", "queue.snapshot", "agents.capabilities", "coordinator.get"]) {
      fake.failNext(method, { code: -32603, message: "boom" }, 10);
    }
    const s = await live(fake);
    const results = [
      await s.workstreams.workstreams(),
      await s.conversation.transcript({ kind: "project", projectId: ALPHA }),
      await s.activity.activity(),
      await s.models.catalog(),
      await s.search.search("task"),
      await s.decisions.queue(),
    ];
    for (const result of results) {
      expect(result.status).toBe("unavailable");
      expect(result.provenance.kind).toBe("daemon");
      expect(result).not.toHaveProperty("data");
      expect(JSON.stringify(result)).not.toMatch(/fixture/i);
    }
  });

  it("says unavailable when disconnected", async () => {
    const s = await live(populate(new FakeDaemon()), false);
    const result = await s.workstreams.workstreams();
    expect(result).toMatchObject({ status: "unavailable", provenance: { kind: "daemon", id: "D9" } });
  });

  it("recovers to real data after a failure, still without fixtures", async () => {
    const fake = populate(new FakeDaemon());
    fake.failNext("tasks.list", { code: -32603, message: "boom" });
    const s = await live(fake);
    expect((await s.workstreams.workstreams()).status).toBe("unavailable");
    const second = await s.workstreams.workstreams();
    expect(second.status).toBe("ok");
    expect(JSON.stringify(second)).not.toMatch(/fixture/i);
  });

  it("maps a missing method to unsupported, not to fixtures or empty", async () => {
    const fake = new FakeDaemon();
    const result = await (await live(fake)).models.catalog();
    expect(result).toMatchObject({ status: "unsupported", provenance: asKind("daemon") });
  });
});
