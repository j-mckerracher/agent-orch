import { act, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { DaemonError } from "../daemon/errors";
import { ALL_CAPABILITIES } from "../daemon/types";
import { createFixtureSources } from "../sources/fixtures";
import { FakeDaemon, mkApproval, mkEvent, mkSession, mkTask } from "../test/fakeDaemon";
import { REPO, mount } from "../test/render";
import { fakeQueueSnapshot } from "../test/queueSnapshot";

const tasksRoute = (filter: "all" | "queued" | "running" | "blocked" | "review" | "terminal" = "all") =>
  ({ name: "tasks", projectId: REPO, filter }) as const;
const taskRoute = (taskId: string) => ({ name: "task", projectId: REPO, taskId }) as const;
const replyBox = (name = "Message to the agent") => screen.findByRole("textbox", { name });
// Kept out of seeded(): the task list counts (S3) would change.
const waitingTask = () => mkTask("t-w", REPO, { state: "Waiting" });
const openQuestion = {
  id: "q1",
  task_id: "t-w",
  task_title: "Fix the login bug",
  project: "",
  question: "Migrate sessions now?",
  options: ["Migrate them now", "Let them expire"],
  key: null,
  raised_by: "worker",
  state: "open",
  deferred_until: null,
  created_at: "2026-11-18T09:00:00Z",
};

function seeded() {
  const fake = new FakeDaemon();
  fake.projects = [{ id: REPO }];
  fake.tasks = [
    mkTask("t-q", REPO),
    mkTask("t-r", REPO, { state: "Running", sessions: [mkSession("s1")] }),
    mkTask("t-b", REPO, { contract: { dependencies: ["t-q"] } }),
    mkTask("t-d", REPO, { state: "Succeeded" }),
    mkTask("t-other", "/other", { state: "Running" }),
  ];
  fake.approvals = [mkApproval("a1", "t-r")];
  return fake;
}

describe("S3 task list", () => {
  it("lists only this project's tasks with filter counts including Awaiting review", async () => {
    await mount(seeded(), { route: tasksRoute() });
    const table = await screen.findByRole("table", { name: "Tasks" });
    expect(within(table).getAllByRole("row")).toHaveLength(5);
    expect(within(table).queryByText("t-other")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "All (4/4)" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Queued (2/4)" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Blocked (1/4)" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Awaiting review (1/4)" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Terminal (1/4)" })).toBeInTheDocument();
  });

  it("filters locally without another daemon call", async () => {
    const fake = seeded();
    const { user } = await mount(fake, { route: tasksRoute() });
    await screen.findByRole("table", { name: "Tasks" });
    const before = fake.callsTo("tasks.list").length;
    await user.click(screen.getByRole("button", { name: "Awaiting review (1/4)" }));
    const table = await screen.findByRole("table", { name: "Tasks" });
    expect(within(table).getByText("t-r")).toBeInTheDocument();
    expect(within(table).queryByText("t-q")).not.toBeInTheDocument();
    expect(fake.callsTo("tasks.list").length).toBe(before);
  });

  it("shows a distinct message when a filter matches nothing and when the project has no tasks", async () => {
    const fake = new FakeDaemon();
    fake.projects = [{ id: REPO }];
    await mount(fake, { route: tasksRoute() });
    expect(await screen.findByText("No tasks in this project yet.")).toBeInTheDocument();
  });

  it("opens a real task", async () => {
    const { user } = await mount(seeded(), { route: tasksRoute() });
    const table = await screen.findByRole("table", { name: "Tasks" });
    const row = within(table).getByText("t-b").closest("tr")!;
    await user.click(within(row).getByRole("button", { name: "Open" }));
    expect(await screen.findByTestId("task-state")).toHaveTextContent("Queued");
  });

  it("derives the Blocked filter from unmet dependencies", async () => {
    const { user } = await mount(seeded(), { route: tasksRoute() });
    await screen.findByRole("table", { name: "Tasks" });
    await user.click(screen.getByRole("button", { name: "Blocked (1/4)" }));
    const table = await screen.findByRole("table", { name: "Tasks" });
    expect(within(table).getAllByRole("row")).toHaveLength(2);
    expect(within(table).getByText("t-b")).toBeInTheDocument();
  });

  it("keeps the execution preview separate and labeled as placeholder data", async () => {
    await mount(seeded(), { route: tasksRoute() });
    const section = await screen.findByRole("region", { name: "Execution preview (placeholder)" });
    expect(within(section).getByTestId("badge-D2")).toHaveTextContent("PLACEHOLDER DATA");
    expect(within(section).getByRole("table", { name: "Execution preview" })).toBeInTheDocument();
    expect(within(screen.getByRole("table", { name: "Tasks" })).queryByText(/fixture:/)).not.toBeInTheDocument();
  });

  it("opens a preview task in the fixture-only view and never sends its id to the daemon", async () => {
    const fake = seeded();
    const { user } = await mount(fake, { route: tasksRoute() });
    const preview = await screen.findByRole("table", { name: "Execution preview" });
    await user.click(within(preview).getAllByRole("button", { name: "Open preview" })[0]!);
    expect(await screen.findByRole("heading", { name: "Task preview" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Simulate launch" }));
    expect(await screen.findByTestId("simulated-receipt")).toHaveTextContent("SIMULATED");
    expect(screen.getByTestId("simulated-receipt")).toHaveTextContent("PLACEHOLDER DATA");
    await user.click(screen.getByRole("button", { name: "Simulate cancel" }));
    await waitFor(() => expect(screen.getAllByTestId("simulated-receipt")).toHaveLength(2));
    const mutating = fake.calls.filter((c) => /^(tasks\.(submit|cancel)|sessions\.)/.test(c.method));
    expect(JSON.stringify(mutating.map((c) => c.params))).not.toContain("fixture:");
    expect(fake.callsTo("tasks.cancel")).toHaveLength(0);
  });

  it("does not fall back to fixtures after an IPC failure", async () => {
    const fake = seeded();
    fake.failNext("tasks.list", { code: -32603, message: "boom" }, 5);
    await mount(fake, { route: tasksRoute() });
    expect(await screen.findByText(/Loading tasks failed/)).toBeInTheDocument();
    expect(screen.queryByRole("table", { name: "Tasks" })).not.toBeInTheDocument();
  });

  it("reads filter counts as unknown, not loading, when the task list fails while connected", async () => {
    const fake = seeded();
    fake.failNext("tasks.list", { code: -32603, message: "boom" }, 5);
    const { services } = await mount(fake, { route: tasksRoute() });
    await waitFor(() => expect(services.data.getSnapshot().tasks.status).toBe("error"));
    const filters = screen.getByRole("group", { name: "Task filters" });
    expect(within(filters).getByRole("button", { name: "All (unknown)" })).toBeInTheDocument();
    expect(within(filters).getByRole("button", { name: "Awaiting review (unknown)" })).toBeInTheDocument();
    expect(filters).not.toHaveTextContent("…");
  });

  it("does not filter the rows by review, or count review as 0, when the approvals list is unavailable", async () => {
    const fake = seeded();
    fake.failNext("approvals.list", { code: -32603, message: "boom" }, 5);
    const { services } = await mount(fake, { route: tasksRoute("review") });
    await waitFor(() => expect(services.data.getSnapshot().approvals.status).toBe("error"));
    expect(screen.getByRole("button", { name: "Awaiting review (unknown)" })).toBeInTheDocument();
    expect(screen.getByText(/Awaiting review is unknown/)).toBeInTheDocument();
    const table = await screen.findByRole("table", { name: "Tasks" });
    expect(within(table).getAllByRole("row")).toHaveLength(5);
    expect(screen.queryByText("No tasks match this filter.")).not.toBeInTheDocument();
  });

  it("shows cached tasks as stale while offline", async () => {
    const fake = seeded();
    await mount(fake, { route: tasksRoute() });
    await screen.findByRole("table", { name: "Tasks" });
    fake.openError = new DaemonError("disconnected", "down");
    fake.drop();
    await waitFor(() => expect(screen.getByTestId("offline-note")).toBeInTheDocument());
    expect(screen.getByRole("table", { name: "Tasks" })).toBeInTheDocument();
    expect(screen.getByTestId("stale-tasks")).toBeInTheDocument();
  });

  it("shows why each queued row has not started beside its state", async () => {
    const fake = seeded();
    fake.stub("queue.snapshot", () => ({
      ...fakeQueueSnapshot(fake.tasks, fake.queue),
      queued: [
        {
          task_id: "t-q", project_id: REPO, position: 1,
          waiting_reason: "load", waiting_on: null, waiting_detail: null,
          load: { meter: "disk", mean_percent: 89, limit_percent: 90, reopen_percent: 80 },
        },
        { task_id: "t-b", project_id: REPO, position: null, waiting_reason: "dependency", waiting_on: "t-q", waiting_detail: null },
      ],
    }));
    await mount(fake, { route: tasksRoute() });
    const table = await screen.findByRole("table", { name: "Tasks" });
    const row = (id: string) => within(table).getByText(id).closest("tr")!;
    expect(await within(row("t-q")).findByTestId("queue-wait")).toHaveTextContent("Waiting for disk space (89% used)");
    expect(within(row("t-b")).getByTestId("queue-wait")).toHaveTextContent("Waiting for t-q");
    expect(within(row("t-r")).queryByTestId("queue-wait")).not.toBeInTheDocument();
    expect(within(row("t-d")).queryByTestId("queue-wait")).not.toBeInTheDocument();
  });

  it("reads queue diagnostics only while a queued row is shown", async () => {
    const fake = seeded();
    const { user } = await mount(fake, { route: tasksRoute("terminal") });
    expect(within(await screen.findByRole("table", { name: "Tasks" })).getByText("t-d")).toBeInTheDocument();
    expect(fake.callsTo("queue.snapshot")).toHaveLength(0);
    await user.click(screen.getByRole("button", { name: "Queued (2/4)" }));
    expect(await screen.findAllByTestId("queue-wait")).toHaveLength(2);
    expect(fake.callsTo("queue.snapshot").length).toBeGreaterThan(0);
  });

  it("removes wait reasons when a re-read fails and restores them on the next check", async () => {
    const fake = seeded();
    await mount(fake, { route: tasksRoute("queued") });
    expect(await screen.findAllByTestId("queue-wait")).toHaveLength(2);
    fake.failNext("queue.snapshot", { code: -32603, message: "boom" }, 1);
    act(() => void window.dispatchEvent(new Event("focus")));
    await waitFor(() => expect(screen.queryAllByTestId("queue-wait")).toHaveLength(0));
    act(() => void window.dispatchEvent(new Event("focus")));
    expect(await screen.findAllByTestId("queue-wait")).toHaveLength(2);
  });

  it("keeps the current wait reason on screen while re-reading after a task changes state", async () => {
    const fake = seeded();
    vi.useFakeTimers();
    try {
      await mount(fake, { route: tasksRoute("queued"), waitConnected: false });
      await act(() => vi.advanceTimersByTimeAsync(100));
      const note = () =>
        within(within(screen.getByRole("table", { name: "Tasks" })).getByText("t-q").closest("tr")!).getByTestId("queue-wait");
      expect(note()).toHaveTextContent("All 2 worker slots in use");
      expect(fake.callsTo("events.subscribe")).toHaveLength(1);
      const reads = fake.callsTo("queue.snapshot").length;
      fake.delayNext("queue.snapshot", 150);
      fake.tasks = fake.tasks.map((task) => (task.contract.id === "t-r" ? { ...task, state: "Succeeded" } : task));
      act(() => fake.emit(mkEvent(1, "t-r", "s1", "SessionExited")));
      await act(() => vi.advanceTimersByTimeAsync(100));
      expect(fake.callsTo("queue.snapshot").length).toBeGreaterThan(reads);
      expect(note()).toHaveTextContent("All 2 worker slots in use");
      await act(() => vi.advanceTimersByTimeAsync(200));
      expect(note()).toHaveTextContent("No blocker reported");
    } finally {
      vi.useRealTimers();
    }
  });

  it("says the wait reason is unavailable when the daemon has no queue diagnostics", async () => {
    const fake = seeded();
    fake.unknownCapabilities = ["queue"];
    await mount(fake, { route: tasksRoute("queued") });
    const notes = await screen.findAllByTestId("queue-wait");
    expect(notes.map((note) => note.textContent)).toEqual(["Wait reason unavailable", "Wait reason unavailable"]);
  });
});

describe("S4 task and session detail", () => {
  it("explains a queued task's launch blocker on the task page", async () => {
    const fake = seeded();
    fake.tasks = [mkTask("t-q", REPO, { contract: { kind: "Investigate", agent: "codex" } })];
    fake.queue.dispatch_blocked = true;
    await mount(fake, { route: taskRoute("t-q"), daemonSources: true });
    expect(await within(screen.getByRole("main")).findByText(/restart gate is blocking all launches/)).toBeInTheDocument();
    const status = screen.getByRole("region", { name: "Queue status" });
    expect(within(status).getByRole("status")).toHaveTextContent("restart gate");
    expect(status).toHaveTextContent("recovery_gate_blocked");
    expect(status).toHaveTextContent("do not resubmit");
    expect(status).toHaveTextContent("Selected agent: Codex");
  });

  it.each([
    ["dependency", "t-dependency", null, /Waiting for t-dependency/, /dependencies succeed/],
    ["global_cap", null, null, /All 2 worker slots/, /running task to finish/],
    ["approval", null, "project_not_registered", /project is not registered/, /Register the project/],
    ["other", "t-owner", "worktree_owned", /t-owner to release the shared worktree/, /Do not delete/],
    ["future_reason", null, null, /does not recognize: future_reason/, /check the daemon log/],
  ] as const)("shows %s queue feedback with a next step", async (reason, on, detail, message, action) => {
    const fake = seeded();
    fake.tasks = [mkTask("t-q", REPO)];
    fake.stub("queue.snapshot", () => ({
      ...fakeQueueSnapshot(fake.tasks, fake.queue),
      queued: [{
        task_id: "t-q", project_id: REPO, position: null,
        waiting_reason: reason, waiting_on: on, waiting_detail: detail,
      }],
    }));
    await mount(fake, { route: taskRoute("t-q") });
    const status = await screen.findByRole("region", { name: "Queue status" });
    expect(await within(status).findByText(message)).toBeInTheDocument();
    expect(status).toHaveTextContent(action);
  });

  it("does not promise a launch or infer exhausted quota when no scheduler blocker is reported", async () => {
    const fake = seeded();
    fake.tasks = [mkTask("t-q", REPO, { contract: { agent: "codex" } })];
    await mount(fake, { route: taskRoute("t-q") });
    const status = await screen.findByRole("region", { name: "Queue status" });
    expect(await within(status).findByText(/scheduler reports no blocker/)).toBeInTheDocument();
    expect(status).toHaveTextContent("an agent launch is not confirmed");
    expect(status).toHaveTextContent("does not establish whether quota is exhausted");
    expect(status).not.toHaveTextContent("the next scheduling pass starts this task");
  });

  it("explains the disk-space hold even below the trigger limit and names the recovery threshold", async () => {
    const fake = seeded();
    fake.tasks = [mkTask("t-q", REPO, { contract: { agent: "codex" } })];
    fake.stub("queue.snapshot", () => ({
      ...fakeQueueSnapshot(fake.tasks, fake.queue),
      queued: [{
        task_id: "t-q", project_id: REPO, position: 1,
        waiting_reason: "load", waiting_on: null, waiting_detail: null,
        load: { meter: "disk", mean_percent: 89, limit_percent: 90, reopen_percent: 80 },
      }],
    }));
    await mount(fake, { route: taskRoute("t-q") });
    const status = await screen.findByRole("region", { name: "Queue status" });
    expect(await within(status).findByText(/disk 89 percent, limit 90/)).toBeInTheDocument();
    expect(status).toHaveTextContent("rolling average of 80 percent or lower");
    expect(status).toHaveTextContent("Free disk space on the machine running the daemon");
    expect(status).toHaveTextContent("checks again automatically");
    expect(status).not.toHaveTextContent("quota is exhausted");
  });

  it("refreshes queue feedback without launching or resubmitting and removes it when the task runs", async () => {
    const fake = seeded();
    fake.tasks = [mkTask("t-q", REPO)];
    fake.queue.dispatch_blocked = true;
    const { user } = await mount(fake, { route: taskRoute("t-q") });
    const status = await screen.findByRole("region", { name: "Queue status" });
    await within(status).findByText(/restart gate/);
    fake.queue.dispatch_blocked = false;
    await user.click(within(status).getByRole("button", { name: "Refresh queue status" }));
    expect(await within(status).findByText(/scheduler reports no blocker/)).toBeInTheDocument();
    fake.tasks[0]!.state = "Running";
    fake.emit(mkEvent(1, "t-q", null, "ExecutionStarted"));
    await waitFor(() => expect(screen.queryByRole("region", { name: "Queue status" })).not.toBeInTheDocument());
    expect(fake.callsTo("tasks.submit")).toHaveLength(0);
    expect(fake.callsTo("tasks.retry")).toHaveLength(0);
    expect(fake.callsTo("tasks.cancel")).toHaveLength(0);
  });

  it("polls for queue changes even when no task event arrives", async () => {
    const fake = seeded();
    fake.tasks = [mkTask("t-q", REPO)];
    fake.queue.dispatch_blocked = true;
    vi.useFakeTimers();
    try {
      const { view: polledView } = await mount(fake, { route: taskRoute("t-q"), waitConnected: false });
      await act(() => vi.advanceTimersByTimeAsync(100));
      expect(screen.getByRole("region", { name: "Queue status" })).toHaveTextContent("restart gate");
      fake.queue.dispatch_blocked = false;
      await act(() => vi.advanceTimersByTimeAsync(15_000));
      expect(screen.getByRole("region", { name: "Queue status" })).toHaveTextContent("scheduler reports no blocker");
      polledView.unmount();
      const calls = fake.callsTo("queue.snapshot").length;
      await act(() => vi.advanceTimersByTimeAsync(15_000));
      expect(fake.callsTo("queue.snapshot")).toHaveLength(calls);
    } finally {
      vi.useRealTimers();
    }
  });

  it("shows failed reads, recovery on refresh, and lost connectivity without retaining a stale blocker", async () => {
    const fake = seeded();
    fake.tasks = [mkTask("t-q", REPO)];
    fake.queue.dispatch_blocked = true;
    const { user } = await mount(fake, { route: taskRoute("t-q") });
    const status = await screen.findByRole("region", { name: "Queue status" });
    await within(status).findByText(/restart gate/);
    fake.failNext("queue.snapshot", { code: -32603, message: "queue read failed" }, 1);
    await user.click(within(status).getByRole("button", { name: "Refresh queue status" }));
    expect(await within(status).findByText(/Queue status unavailable:/)).toHaveTextContent("queue read failed");
    expect(status).not.toHaveTextContent("restart gate");
    await user.click(within(status).getByRole("button", { name: "Refresh queue status" }));
    await within(status).findByText(/restart gate/);
    fake.openError = new DaemonError("disconnected", "down");
    fake.drop();
    expect(await within(status).findByText(/Queue status unavailable while disconnected/)).toBeInTheDocument();
    expect(status).not.toHaveTextContent("restart gate");
    expect(within(status).getByRole("button", { name: "Refresh queue status" })).toBeDisabled();
  });

  it("explains missing queue support instead of presenting it as an unblocked task", async () => {
    const fake = seeded();
    fake.unknownCapabilities = ["queue"];
    await mount(fake, { route: taskRoute("t-q") });
    const status = await screen.findByRole("region", { name: "Queue status" });
    expect(await within(status).findByText(/wait reason are unavailable without queue.snapshot/)).toBeInTheDocument();
    expect(status).toHaveTextContent("Update the daemon");
  });

  it("does not re-read queue feedback when an event changes no task state", async () => {
    const fake = seeded();
    vi.useFakeTimers();
    try {
      await mount(fake, { route: taskRoute("t-q"), waitConnected: false });
      await act(() => vi.advanceTimersByTimeAsync(100));
      const status = () => screen.getByRole("region", { name: "Queue status" });
      expect(status()).toHaveTextContent("All 2 worker slots are in use");
      expect(fake.callsTo("events.subscribe")).toHaveLength(1);
      const reads = fake.callsTo("queue.snapshot").length;
      const lists = fake.callsTo("tasks.list").length;
      act(() => fake.emit(mkEvent(1, "t-r", "s1")));
      await act(() => vi.advanceTimersByTimeAsync(100));
      expect(fake.callsTo("tasks.list").length).toBeGreaterThan(lists);
      expect(status()).toHaveTextContent("All 2 worker slots are in use");
      expect(fake.callsTo("queue.snapshot")).toHaveLength(reads);
    } finally {
      vi.useRealTimers();
    }
  });

  it("explains that a Queued task with no session has had no agent launched", async () => {
    await mount(seeded(), { route: taskRoute("t-q") });
    expect(await screen.findByTestId("task-state")).toHaveTextContent("Queued");
    expect(screen.getByText(/no agent has been launched and no session is recorded/)).toBeInTheDocument();
    expect(screen.getByText("No recorded session.")).toBeInTheDocument();
    expect(screen.getByText(/No session events: no session is recorded/)).toBeInTheDocument();
    expect(screen.getByText("No pending approval for this task.")).toBeInTheDocument();
  });

  it("shows the recorded session and its timeline with sequence numbers and reported kinds", async () => {
    const fake = seeded();
    fake.events = [mkEvent(1, "t-r", "s1", "SessionOutput"), mkEvent(2, "t-r", "s1", "SessionExited")];
    await mount(fake, { route: taskRoute("t-r") });
    const table = await screen.findByRole("table", { name: "Session events" });
    expect(within(table).getByText("SessionOutput")).toBeInTheDocument();
    expect(within(table).getByText("SessionExited")).toBeInTheDocument();
    expect(await screen.findByTestId("session-detail")).toHaveTextContent("Adapter fake-adapter");
  });

  it("pages the timeline with Load more", async () => {
    const fake = seeded();
    fake.pageSize = 2;
    fake.events = [1, 2, 3, 4, 5].map((n) => mkEvent(n, "t-r", "s1"));
    const { user } = await mount(fake, { route: taskRoute("t-r") });
    const table = await screen.findByRole("table", { name: "Session events" });
    expect(within(table).getAllByRole("row")).toHaveLength(3);
    await user.click(screen.getByRole("button", { name: "Load more session events" }));
    await waitFor(() => expect(within(screen.getByRole("table", { name: "Session events" })).getAllByRole("row")).toHaveLength(5));
    await user.click(screen.getByRole("button", { name: "Load more session events" }));
    await waitFor(() => expect(screen.queryByRole("button", { name: "Load more session events" })).not.toBeInTheDocument());
    expect(within(screen.getByRole("table", { name: "Session events" })).getAllByRole("row")).toHaveLength(6);
  });

  it("offers Review candidate only when an approval is pending for the task and routes to S6", async () => {
    const { user } = await mount(seeded(), {
      route: taskRoute("t-r"),
      renderReview: ({ projectId, taskId }) => <p>review {projectId} {taskId}</p>,
    });
    await user.click(await screen.findByRole("button", { name: "Review candidate" }));
    expect(await screen.findByText(`review ${REPO} t-r`)).toBeInTheDocument();
  });

  it("links dependencies that are known and flags those that are not", async () => {
    const fake = seeded();
    fake.tasks.push(mkTask("t-dep", REPO, { contract: { dependencies: ["t-q", "ghost"] } }));
    const { user } = await mount(fake, { route: taskRoute("t-dep") });
    await screen.findByTestId("task-state");
    expect(screen.getByText("ghost (not in the returned list)")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "t-q" }));
    await waitFor(() => expect(screen.getByText("objective of t-q")).toBeInTheDocument());
  });

  it("states that cancel is an acknowledgement, not process termination", async () => {
    const fake = seeded();
    const { user } = await mount(fake, { route: taskRoute("t-q") });
    await screen.findByTestId("task-state");
    await user.click(screen.getByRole("button", { name: "Request cancel" }));
    expect(await screen.findByText(/Cancel request acknowledged; the daemon now records the task as Canceled\. This is not proof that any process has stopped\./)).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("button", { name: "Request cancel" })).toBeDisabled());
    expect(fake.callsTo("tasks.cancel")).toHaveLength(1);
    expect((fake.callsTo("tasks.cancel")[0]!.params as { idempotency_key?: string }).idempotency_key).toEqual(
      expect.any(String),
    );
  });

  it("lets a Verifying task be canceled from its detail", async () => {
    const fake = seeded();
    fake.tasks.push(mkTask("t-v", REPO, { state: "Verifying" }));
    const { user } = await mount(fake, { route: taskRoute("t-v") });
    await screen.findByTestId("task-state");
    expect(screen.getByRole("button", { name: "Request cancel" })).toBeEnabled();
    await user.click(screen.getByRole("button", { name: "Request cancel" }));
    expect(await screen.findByText(/Cancel request acknowledged; the daemon now records the task as Canceled/)).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("button", { name: "Request cancel" })).toBeDisabled());
    expect(fake.callsTo("tasks.cancel")).toHaveLength(1);
    expect(fake.callsTo("tasks.cancel")[0]!.params).toMatchObject({ task_id: "t-v" });
  });

  it("shows the daemon's reason when cancel fails", async () => {
    const fake = seeded();
    const { user } = await mount(fake, { route: taskRoute("t-q") });
    await screen.findByTestId("task-state");
    fake.failNext("tasks.cancel", { code: -32011, message: "task already terminal" });
    await user.click(screen.getByRole("button", { name: "Request cancel" }));
    expect(await screen.findByText(/Canceling the task failed/)).toBeInTheDocument();
    expect(screen.queryByText(/Cancel request acknowledged/)).not.toBeInTheDocument();
  });

  it("archives a finished task from its detail, once, and says what archive keeps", async () => {
    const fake = seeded();
    const { user } = await mount(fake, { route: taskRoute("t-d") });
    await screen.findByTestId("task-state");
    await user.click(screen.getByRole("button", { name: "Archive task" }));
    expect(await screen.findByText(/Archived\. The task leaves the task lists; its history is kept\./)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Archive task" })).toBeDisabled();
    expect(fake.callsTo("tasks.archive")).toHaveLength(1);
    expect(fake.callsTo("tasks.archive")[0]!.params).toMatchObject({ task_ids: ["t-d"], idempotency_key: expect.any(String) });
  });

  it("disables archive on a running task and says why", async () => {
    const fake = seeded();
    await mount(fake, { route: taskRoute("t-r") });
    await screen.findByTestId("task-state");
    const button = screen.getByRole("button", { name: "Archive task" });
    expect(button).toBeDisabled();
    expect(button.parentElement).toHaveTextContent("Only finished tasks can be archived; this task is Running.");
    expect(fake.callsTo("tasks.archive")).toHaveLength(0);
  });

  it("shows the daemon's reason when archive is refused", async () => {
    const fake = seeded();
    const { user } = await mount(fake, { route: taskRoute("t-d") });
    await screen.findByTestId("task-state");
    fake.failNext("tasks.archive", { code: -32603, message: "store error" });
    await user.click(screen.getByRole("button", { name: "Archive task" }));
    expect(await screen.findByText(/Archiving the task failed/)).toBeInTheDocument();
    expect(screen.queryByText(/Archived\. The task leaves/)).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Discard changes" })).not.toBeInTheDocument();
  });

  it("offers to discard undelivered changes after a confirmation, then lets archive go through", async () => {
    const fake = seeded();
    fake.undelivered.add("t-d");
    const { user } = await mount(fake, { route: taskRoute("t-d") });
    await screen.findByTestId("task-state");
    await user.click(screen.getByRole("button", { name: "Archive task" }));
    expect(await screen.findByText("This task has changes that were not delivered. Deliver them, or discard them first.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Discard changes" }));
    expect(screen.getByText(/Discard this task's changes\? The branch stays in the repository/)).toBeInTheDocument();
    expect(fake.callsTo("tasks.discard")).toHaveLength(0);
    await user.click(screen.getByRole("button", { name: "Keep changes" }));
    expect(fake.callsTo("tasks.discard")).toHaveLength(0);
    await user.click(screen.getByRole("button", { name: "Discard changes" }));
    await user.click(screen.getByRole("button", { name: "Confirm discard" }));
    expect(await screen.findByText("Changes discarded. You can archive the task now.")).toBeInTheDocument();
    expect(fake.callsTo("tasks.discard")[0]!.params).toEqual({ task_id: "t-d", idempotency_key: expect.any(String) });
    expect(screen.queryByRole("button", { name: "Discard changes" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Archive task" }));
    expect(await screen.findByText(/Archived\. The task leaves the task lists; its history is kept\./)).toBeInTheDocument();
  });

  it("locks discard for a session that is not the approver", async () => {
    const fake = seeded();
    fake.approver = false;
    fake.undelivered.add("t-d");
    const { user } = await mount(fake, { route: taskRoute("t-d") });
    await screen.findByTestId("task-state");
    await user.click(screen.getByRole("button", { name: "Archive task" }));
    expect(await screen.findByRole("button", { name: "Discard changes" })).toBeDisabled();
    expect(fake.callsTo("tasks.discard")).toHaveLength(0);
  });

  it("disables cancel for terminal tasks", async () => {
    const fake = seeded();
    await mount(fake, { route: taskRoute("t-d") });
    await screen.findByTestId("task-state");
    expect(screen.getByRole("button", { name: "Request cancel" })).toBeDisabled();
  });

  it("disables cancel and says so when the connection is lost", async () => {
    const fake = seeded();
    await mount(fake, { route: taskRoute("t-q") });
    await screen.findByTestId("task-state");
    fake.openError = new DaemonError("disconnected", "down");
    fake.drop();
    await waitFor(() => expect(screen.getByRole("button", { name: "Request cancel" })).toBeDisabled());
    expect(screen.getByText(/Unavailable while offline/)).toBeInTheDocument();
    expect(screen.getByTestId("task-state")).toHaveTextContent("Queued");
  });

  it("reports a missing task with the daemon's not-found reason", async () => {
    await mount(seeded(), { route: taskRoute("nope") });
    expect(await screen.findByText(/Loading task failed/)).toBeInTheDocument();
  });

  it("labels retry, outcome and recovery panels as placeholder data", async () => {
    await mount(seeded(), { route: taskRoute("t-b") });
    await screen.findByTestId("task-state");
    for (const id of ["D2", "D3", "D4"]) {
      await waitFor(() => expect(screen.getAllByTestId(`badge-${id}`).length).toBeGreaterThan(0));
    }
    expect(screen.getByRole("region", { name: "Recovery classification (placeholder)" })).toBeInTheDocument();
  });

  it("previews a retry of a failed task as a simulated receipt that creates nothing", async () => {
    const fake = seeded();
    fake.tasks.push(mkTask("t-f", REPO, { state: "Failed" }));
    const { user } = await mount(fake, { route: taskRoute("t-f") });
    await screen.findByTestId("task-state");
    await user.click(await screen.findByRole("button", { name: "Preview retry" }));
    const receipt = await screen.findByTestId("simulated-receipt");
    expect(receipt).toHaveTextContent("SIMULATED");
    expect(receipt).toHaveTextContent("PLACEHOLDER DATA");
    expect(receipt).toHaveTextContent("Simulated retry of t-f. No new attempt was created.");
    expect(receipt).toHaveTextContent("(new attempt fixture:attempt-simulated)");
    expect(fake.callsTo("tasks.submit")).toHaveLength(0);
  });

  it("renders a D2 unavailable state without hiding the real task", async () => {
    await mount(seeded(), { route: taskRoute("t-q"), sources: createFixtureSources({ scenario: "unavailable" }) });
    expect(await screen.findByTestId("task-state")).toHaveTextContent("Queued");
    const section = await screen.findByRole("region", { name: "Execution outcome (placeholder)" });
    expect(await within(section).findByText(/Unavailable:/)).toBeInTheDocument();
  });

  it("shows the agent's thinking and recent output as chat entries for a running task", async () => {
    const fake = seeded();
    fake.outputs.set("s1", {
      lines: [
        { sequence: 1, stream: "stdout", text: "reading the spec", line_truncated: false },
        { sequence: 2, stream: "stderr", text: "<b>warn</b> **x**", line_truncated: false },
      ],
    });
    fake.stub("sessions.thinking", { chunks: [{ sequence: 1, body: "Checking the spec first." }], next_cursor: 1, truncated: false });
    await mount(fake, { route: taskRoute("t-r") });
    const thread = within(await screen.findByRole("log", { name: /^Conversation with/ }));
    expect(await thread.findByText("reading the spec")).toBeInTheDocument();
    // Worker output is plain text: markup stays literal and the stream is named.
    expect(thread.getByText("<b>warn</b> **x**")).toBeInTheDocument();
    expect(thread.getByText("stderr")).toBeInTheDocument();
    expect(thread.getByText(/not redacted/)).toBeInTheDocument();
    // Output lines have no times, so the bubble says when the app read them.
    expect(thread.getByText(/^as of/).closest("article")!.querySelector("time")).toHaveAttribute("datetime", expect.stringMatching(/^\d{4}-\d{2}-\d{2}T/));
    expect(await thread.findByText("Thinking")).toBeInTheDocument();
    expect(thread.getByText("Checking the spec first.")).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "Recent output" })).not.toBeInTheDocument();
  });

  it("shows an empty state in the thread when the session's output is not retained", async () => {
    const fake = seeded();
    fake.tasks.push(mkTask("t-e", REPO, { state: "Failed", sessions: [mkSession("s1", { state: "Exited" })] }));
    await mount(fake, { route: taskRoute("t-e") });
    const thread = within(await screen.findByRole("log", { name: /^Conversation with/ }));
    expect(await thread.findByText("The session ended and no output is retained.")).toBeInTheDocument();
    expect(thread.getByText("No messages yet")).toBeInTheDocument();
  });

  it("shows an empty thread and reads no output when no session is recorded", async () => {
    const fake = seeded();
    await mount(fake, { route: taskRoute("t-q") });
    const thread = within(await screen.findByRole("log", { name: /^Conversation with/ }));
    expect(thread.getByText("No messages yet")).toBeInTheDocument();
    expect(thread.getByText(/No session is recorded/)).toBeInTheDocument();
    expect(fake.callsTo("sessions.output_tail")).toHaveLength(0);
  });

  it("reads the output of the session the session selector shows", async () => {
    const fake = seeded();
    fake.tasks.push(
      mkTask("t-m", REPO, { state: "Failed", sessions: [mkSession("s1", { state: "Exited" }), mkSession("s2", { state: "Exited" })] }),
    );
    const { user } = await mount(fake, { route: taskRoute("t-m") });
    await screen.findByTestId("task-state");
    await waitFor(() =>
      expect(fake.callsTo("sessions.output_tail")).toContainEqual(expect.objectContaining({ params: expect.objectContaining({ session_id: "s2" }) })),
    );
    await user.selectOptions(within(screen.getByRole("region", { name: "Sessions" })).getByRole("combobox"), "s1");
    await waitFor(() =>
      expect(fake.callsTo("sessions.output_tail")).toContainEqual(expect.objectContaining({ params: expect.objectContaining({ session_id: "s1" }) })),
    );
  });

  it("sends a free-form message to a waiting task when no question is open", async () => {
    const fake = seeded();
    fake.tasks.push(waitingTask());
    fake.stub("decisions.list", { questions: [] });
    const { user } = await mount(fake, { route: taskRoute("t-w"), daemonSources: true });
    const box = await replyBox();
    await waitFor(() => expect(box).toBeEnabled());
    await user.type(box, "Use the staging database");
    await user.click(screen.getByRole("button", { name: "Send message" }));
    expect(await screen.findByText("Sent. The agent resumes shortly.")).toBeInTheDocument();
    expect(box).toHaveValue("");
    expect(fake.callsTo("tasks.message")).toHaveLength(1);
    expect(fake.messages).toEqual([{ task_id: "t-w", text: "Use the staging database" }]);
    expect(fake.callsTo("tasks.message")[0]!.params).toEqual({
      task_id: "t-w",
      text: "Use the staging database",
      idempotency_key: expect.any(String),
    });
    expect(fake.callsTo("decisions.answer")).toHaveLength(0);
  });

  it("sends with Enter, keeps a newline for Shift+Enter, and shows the sent message in the thread", async () => {
    const fake = seeded();
    fake.tasks.push(waitingTask());
    fake.stub("decisions.list", { questions: [] });
    const { user } = await mount(fake, { route: taskRoute("t-w"), daemonSources: true });
    const box = await replyBox();
    await waitFor(() => expect(box).toBeEnabled());
    await user.type(box, "first line{Shift>}{Enter}{/Shift}second line");
    expect(box).toHaveValue("first line\nsecond line");
    expect(fake.callsTo("tasks.message")).toHaveLength(0);
    await user.keyboard("{Enter}");
    expect(await screen.findByText("Sent. The agent resumes shortly.")).toBeInTheDocument();
    expect(fake.messages).toEqual([{ task_id: "t-w", text: "first line\nsecond line" }]);
    const thread = within(screen.getByRole("log", { name: /^Conversation with/ }));
    expect(thread.getByText("You")).toBeInTheDocument();
    expect(thread.getByText("You").closest("article")!.querySelector("time")).toHaveAttribute("datetime", expect.stringMatching(/^\d{4}-\d{2}-\d{2}T/));
    expect(thread.getByText(/second line/)).toBeInTheDocument();
  });

  it("answers an open question with decisions.answer, and an option only fills the answer box", async () => {
    const fake = seeded();
    fake.tasks.push(waitingTask());
    fake.stub("decisions.list", { questions: [openQuestion] });
    fake.stub("decisions.answer", { question: { ...openQuestion, state: "answered" } });
    const { user } = await mount(fake, { route: taskRoute("t-w"), daemonSources: true });
    const thread = within(await screen.findByRole("log", { name: /^Conversation with/ }));
    expect(await thread.findByText("Migrate sessions now?")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Migrate them now" }));
    expect(screen.getByRole("textbox", { name: "Your answer" })).toHaveValue("Migrate them now");
    expect(fake.callsTo("decisions.answer")).toHaveLength(0);
    expect(fake.callsTo("tasks.message")).toHaveLength(0);
    await user.click(screen.getByRole("button", { name: "Send answer" }));
    expect(await thread.findByText("Answered: Fix the login bug.")).toBeInTheDocument();
    // The answered question stays in the thread once, with the answer under it as the person's own bubble.
    expect(thread.getAllByText("Migrate sessions now?")).toHaveLength(1);
    expect(thread.getByText("Migrate sessions now?").closest("article")!.querySelector("time")).toHaveAttribute("datetime", "2026-11-18T09:00:00Z");
    expect(thread.getByText("You").closest("article")!.querySelector("time")).not.toBeNull();
    expect(thread.getByText("You")).toBeInTheDocument();
    expect(thread.getAllByText("Migrate them now")).toHaveLength(1);
    expect(fake.callsTo("decisions.answer")[0]!.params).toEqual({
      question_id: "q1",
      answer: "Migrate them now",
      idempotency_key: expect.any(String),
    });
    expect(fake.callsTo("tasks.message")).toHaveLength(0);
  });

  it.each([
    ["t-r", /Live steering is not available yet/],
    ["t-d", /This task is finished\. The agent cannot be messaged\./],
  ])("disables the reply box for %s and says why", async (taskId, reason) => {
    const fake = seeded();
    await mount(fake, { route: taskRoute(taskId) });
    const box = await replyBox();
    expect(box).toBeDisabled();
    expect(screen.getByRole("button", { name: "Send message" })).toBeDisabled();
    expect(screen.getByText(reason)).toBeInTheDocument();
    expect(box).toHaveAccessibleDescription(reason);
    expect(fake.callsTo("tasks.message")).toHaveLength(0);
  });

  it("disables the reply box for a non-approver without reading the question list", async () => {
    const fake = seeded();
    fake.approver = false;
    fake.tasks.push(waitingTask());
    fake.stub("decisions.list", { questions: [] });
    await mount(fake, { route: taskRoute("t-w"), daemonSources: true });
    expect(await replyBox()).toBeDisabled();
    expect(screen.getByText(/This app did not start the orchestrator service/)).toBeInTheDocument();
    expect(fake.callsTo("decisions.list")).toHaveLength(0);
    expect(fake.callsTo("tasks.message")).toHaveLength(0);
  });

  it("disables the reply box when the connection is lost", async () => {
    const fake = seeded();
    fake.tasks.push(waitingTask());
    fake.stub("decisions.list", { questions: [] });
    await mount(fake, { route: taskRoute("t-w"), daemonSources: true });
    await screen.findByTestId("task-state");
    fake.openError = new DaemonError("disconnected", "down");
    fake.drop();
    await waitFor(() => expect(screen.getByRole("textbox", { name: "Message to the agent" })).toBeDisabled());
    expect(screen.getByText(/Reconnect to the daemon to reply to the agent/)).toBeInTheDocument();
  });

  it("keeps the typed message and says why the daemon refused it", async () => {
    const fake = seeded();
    fake.tasks.push(waitingTask());
    fake.stub("decisions.list", { questions: [] });
    fake.failNext("tasks.message", { code: -32011, message: "task already holds queued messages" });
    const { user } = await mount(fake, { route: taskRoute("t-w"), daemonSources: true });
    const box = await replyBox();
    await waitFor(() => expect(box).toBeEnabled());
    await user.type(box, "One more thing");
    await user.click(screen.getByRole("button", { name: "Send message" }));
    expect(
      await screen.findByText(
        "The task is no longer waiting, is waiting on a question, or already has the most messages it can hold. Nothing was changed.",
      ),
    ).toBeInTheDocument();
    expect(box).toHaveValue("One more thing");
  });

  it("reuses the request key while the outcome is unknown, and mints a new one after a success", async () => {
    const fake = seeded();
    fake.tasks.push(waitingTask());
    fake.stub("decisions.list", { questions: [] });
    fake.failNext("tasks.message", { code: -32603, message: "store error" });
    const { user } = await mount(fake, { route: taskRoute("t-w"), daemonSources: true });
    const box = await replyBox();
    await waitFor(() => expect(box).toBeEnabled());
    await user.type(box, "Use the staging database");
    await user.click(screen.getByRole("button", { name: "Send message" }));
    expect(await screen.findByText("The outcome is unknown. Sending again reuses the same request key.")).toBeInTheDocument();
    expect(box).toBeDisabled();
    expect(screen.getByRole("button", { name: "Send message" })).toBeEnabled();
    await user.click(screen.getByRole("button", { name: "Send message" }));
    expect(await screen.findByText("Sent. The agent resumes shortly.")).toBeInTheDocument();
    const keys = () => fake.callsTo("tasks.message").map((call) => (call.params as { idempotency_key: string }).idempotency_key);
    expect(keys()).toHaveLength(2);
    expect(keys()[1]).toBe(keys()[0]);
    await user.type(box, "Use the staging database");
    await user.click(screen.getByRole("button", { name: "Send message" }));
    await waitFor(() => expect(keys()).toHaveLength(3));
    expect(keys()[2]).not.toBe(keys()[1]);
  });

  it("locks the message text while a send is in flight", async () => {
    const fake = seeded();
    fake.tasks.push(waitingTask());
    fake.stub("decisions.list", { questions: [] });
    fake.delayNext("tasks.message", 200);
    const { user } = await mount(fake, { route: taskRoute("t-w"), daemonSources: true });
    const box = await replyBox();
    await waitFor(() => expect(box).toBeEnabled());
    await user.type(box, "Use the staging database");
    await user.click(screen.getByRole("button", { name: "Send message" }));
    expect(box).toBeDisabled();
    expect(await screen.findByText("Sent. The agent resumes shortly.")).toBeInTheDocument();
    expect(box).toBeEnabled();
  });

  it("disables the reply box on a waiting task when the open questions cannot be read", async () => {
    const fake = seeded();
    fake.tasks.push(waitingTask());
    fake.failNext("decisions.list", { code: -32603, message: "boom" }, 5);
    await mount(fake, { route: taskRoute("t-w"), daemonSources: true });
    await waitFor(() => expect(screen.getByRole("textbox", { name: "Message to the agent" })).toBeDisabled());
    expect(fake.callsTo("tasks.message")).toHaveLength(0);
  });

  it("disables the reply box on a waiting task when the daemon does not report holds", async () => {
    const fake = seeded();
    fake.tasks.push(waitingTask());
    fake.capabilitiesOverride = ALL_CAPABILITIES.filter((capability) => capability !== "holds");
    await mount(fake, { route: taskRoute("t-w"), daemonSources: true });
    expect(await screen.findByText(/does not report open questions/)).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Message to the agent" })).toBeDisabled();
    expect(fake.callsTo("decisions.list")).toHaveLength(0);
  });
});

describe("S5 coordinator", () => {
  const route = { name: "coordinator", projectId: REPO } as const;

  it("states the conversation service is unavailable and labels the conversation as placeholder", async () => {
    await mount(seeded(), { route });
    expect(await screen.findByText(/coordinator conversation service is unavailable \(D5\)/)).toBeInTheDocument();
    expect(screen.getByTestId("badge-D5")).toHaveTextContent("PLACEHOLDER DATA");
    expect(await screen.findByRole("list", { name: "Messages" })).toHaveTextContent("Preview:");
    expect(screen.getByRole("list", { name: "Proposals" })).toBeInTheDocument();
  });

  it("previews a reply and proposal as SIMULATED and never submits a task", async () => {
    const fake = seeded();
    const { user } = await mount(fake, { route });
    await user.type(await screen.findByLabelText("Draft objective"), "Refactor settings");
    await user.click(screen.getByRole("button", { name: "Preview reply" }));
    await user.click(screen.getByRole("button", { name: "Preview proposed tasks" }));
    await waitFor(() => expect(screen.getAllByTestId("simulated-receipt")).toHaveLength(2));
    for (const receipt of screen.getAllByTestId("simulated-receipt")) {
      expect(receipt).toHaveTextContent("SIMULATED");
      expect(receipt).toHaveTextContent("PLACEHOLDER DATA");
    }
    expect(fake.callsTo("tasks.submit")).toHaveLength(0);
  });

  it("renders repeated previews without duplicate React keys", async () => {
    const errors = vi.spyOn(console, "error").mockImplementation(() => undefined);
    try {
      const { user } = await mount(seeded(), { route });
      await user.type(await screen.findByLabelText("Draft objective"), "Refactor settings");
      for (let i = 0; i < 2; i++) {
        await user.click(screen.getByRole("button", { name: "Preview reply" }));
        await user.click(screen.getByRole("button", { name: "Preview proposed tasks" }));
      }
      await waitFor(() => expect(screen.getAllByTestId("simulated-receipt")).toHaveLength(4));
      expect(errors.mock.calls.some((call) => /same key/.test(String(call[0])))).toBe(false);
    } finally {
      errors.mockRestore();
    }
  });

  it("lists related real tasks and pending decisions and navigates to them", async () => {
    const { user } = await mount(seeded(), { route, renderReview: ({ taskId }) => <p>review {taskId}</p> });
    const related = await screen.findByRole("list", { name: "Related tasks" });
    expect(within(related).getAllByRole("listitem")).toHaveLength(4);
    expect(within(related).queryByText("t-other")).not.toBeInTheDocument();
    await user.click(within(screen.getByRole("list", { name: "Pending decisions" })).getByRole("button", { name: "Open review" }));
    expect(await screen.findByText("review t-r")).toBeInTheDocument();
  });

  it("shows why related queued tasks have not started", async () => {
    await mount(seeded(), { route });
    const related = await screen.findByRole("list", { name: "Related tasks" });
    const item = (id: string) => within(related).getByText(id).closest("li")!;
    expect(await within(item("t-q")).findByTestId("queue-wait")).toHaveTextContent("All 2 worker slots in use");
    expect(within(item("t-b")).getByTestId("queue-wait")).toHaveTextContent("Waiting for t-q");
    expect(within(item("t-r")).queryByTestId("queue-wait")).not.toBeInTheDocument();
  });

  it("keeps the draft objective across navigation", async () => {
    const { user } = await mount(seeded(), { route });
    await user.type(await screen.findByLabelText("Draft objective"), "remember");
    await user.click(screen.getByRole("link", { name: "repo" }));
    await screen.findByLabelText("Objective");
    await user.click(screen.getByRole("button", { name: "Coordinator" }));
    expect(await screen.findByLabelText("Draft objective")).toHaveValue("remember");
  });
});
