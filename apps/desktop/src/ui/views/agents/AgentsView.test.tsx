import { render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { DaemonError } from "../../../daemon/errors";
import { FakeDaemon, mkEvent, mkSession, mkTask } from "../../../test/fakeDaemon";
import App from "../../../App";
import { createServices } from "../../createServices";
import { mountAgents } from "./harness";
import { agentsGlobalViews } from "./register";

const REPO = "/work/repo";

function seeded() {
  const fake = new FakeDaemon();
  fake.projects = [{ id: REPO }];
  fake.tasks = [
    mkTask("t-run", REPO, { state: "Running", sessions: [mkSession("s-1")] }),
    mkTask("t-queued", REPO, { state: "Queued" }),
    mkTask("t-failed", REPO, { state: "Failed" }),
    mkTask("t-done", REPO, { state: "Succeeded" }),
  ];
  fake.stub("tasks.retry", (params: Record<string, unknown>) => ({
    ...mkTask(String(params.task_id), REPO),
    state: "Queued",
    attempts: [{ id: "a1" }, { id: "a2" }],
  }));
  return fake;
}

async function liveView(fake = seeded()) {
  const mounted = await mountAgents(fake, { live: true });
  await screen.findByRole("tab", { name: "Running 1" });
  return { fake, ...mounted };
}

type ActivityRow = { sequence: number; kind: string; body: string };
const toolStarted = (sequence: number, id: string, name: string, summary = ""): ActivityRow => ({ sequence, kind: "tool_started", body: JSON.stringify({ id, name, summary }) });
const toolFinished = (sequence: number, id: string, ok: boolean, summary = ""): ActivityRow => ({ sequence, kind: "tool_finished", body: JSON.stringify({ id, ok, summary }) });

/** Answers `sessions.activity` like the daemon: the session's own rows after the cursor. */
function stubActivity(fake: FakeDaemon, bySession: Record<string, ActivityRow[]>) {
  fake.stub("sessions.activity", (params: Record<string, unknown>) => {
    const after = typeof params.after === "number" ? params.after : 0;
    const entries = (bySession[String(params.session_id)] ?? []).filter((row) => row.sequence > after);
    return { entries, next_cursor: entries.length > 0 ? entries[entries.length - 1]!.sequence : after, truncated: false, live: null };
  });
}

async function openTask(user: Awaited<ReturnType<typeof liveView>>["user"], id: string) {
  await user.click(await screen.findByRole("button", { name: new RegExp(`^objective of ${id}`) }));
  return screen.findByTestId("agents-detail");
}

describe("Agents, the task list", () => {
  it("renders inside the shell's single main region with a plain header and no status pills", async () => {
    await liveView();
    expect(screen.getAllByRole("main")).toHaveLength(1);
    const main = within(screen.getByRole("main"));
    expect(main.getByRole("heading", { level: 1, name: "Agents" })).toBeInTheDocument();
    expect(main.getByText("1 running · 1 queued")).toBeInTheDocument();
    expect(main.queryByText(/Execution inventory/)).not.toBeInTheDocument();
    expect(main.queryByText(/running session\(s\)/)).not.toBeInTheDocument();
    expect(main.queryByText(/PLACEHOLDER/i)).not.toBeInTheDocument();
    expect(main.queryByRole("button", { name: "Configure providers" })).not.toBeInTheDocument();
  });

  it("has five tabs with counts, and hides Disconnected, Available and the old scope previews", async () => {
    await liveView();
    expect(screen.getAllByRole("tab").map((tab) => tab.textContent)).toEqual([
      "Running 1",
      "Waiting 0",
      "Queued 1",
      "Failed 1",
      "Finished 1",
    ]);
    const main = within(screen.getByRole("main"));
    expect(main.queryByText(/Disconnected/)).not.toBeInTheDocument();
    expect(main.queryByText(/Available/)).not.toBeInTheDocument();
    expect(main.queryByText(/Later-scope/)).not.toBeInTheDocument();
    expect(main.queryByTestId("agents-inventory")).not.toBeInTheDocument();
    expect(main.queryByRole("complementary")).not.toBeInTheDocument();
  });

  it("opens on Running, with rows showing title, then project · agent · attempt N", async () => {
    const fake = seeded();
    fake.tasks = [
      mkTask("t-run", REPO, {
        state: "Running",
        sessions: [mkSession("s-1")],
        attempts: [{ id: "a-1", task_id: "t-run", attempt_number: 3, state: "Running" }],
      }),
    ];
    await mountAgents(fake, { live: true });
    const row = await screen.findByRole("button", { name: /objective of t-run/ });
    expect(screen.getByRole("tab", { name: "Running 1" })).toHaveAttribute("aria-selected", "true");
    expect(row).toHaveTextContent("objective of t-run");
    expect(row).toHaveTextContent("repo · fake-agent · attempt 3");
    expect(within(screen.getByRole("list", { name: "running tasks" })).getAllByRole("button")).toHaveLength(1);
  });

  it("lists queued, failed and finished tasks under their own tabs", async () => {
    const fake = seeded();
    fake.tasks.push(mkTask("t-cancel", REPO, { state: "Cancelled" }));
    const { user } = await mountAgents(fake, { live: true });
    await user.click(await screen.findByRole("tab", { name: "Queued 1" }));
    expect(within(screen.getByRole("list", { name: "queued tasks" })).getByText("objective of t-queued")).toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "Failed 1" }));
    expect(within(screen.getByRole("list", { name: "failed tasks" })).getByText("objective of t-failed")).toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "Finished 2" }));
    const finished = within(screen.getByRole("list", { name: "finished tasks" }));
    expect(finished.getByText("objective of t-done")).toBeInTheDocument();
    expect(finished.getByText("objective of t-cancel")).toBeInTheDocument();
  });

  it("keeps a Waiting task visible in its own tab and the header, not under Running", async () => {
    const fake = seeded();
    fake.tasks.push(mkTask("t-wait", REPO, { state: "Waiting", contract: { objective: "Find the root cause" } }));
    const { user } = await mountAgents(fake, { live: true });
    await user.click(await screen.findByRole("tab", { name: "Waiting 1" }));
    expect(within(screen.getByRole("list", { name: "waiting tasks" })).getByText("Find the root cause")).toBeInTheDocument();
    expect(screen.getByText("1 running · 1 queued · 1 waiting")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Running 1" })).toBeInTheDocument();
  });

  it("filters locally by title, project or agent", async () => {
    const fake = seeded();
    fake.tasks = [
      mkTask("t-run", REPO, { state: "Running" }),
      mkTask("t-other", REPO, { state: "Running", contract: { objective: "Repair desktop build" } }),
    ];
    const { user } = await mountAgents(fake, { live: true });
    await screen.findByRole("tab", { name: "Running 2" });
    await user.type(screen.getByRole("searchbox", { name: "Filter tasks or agents" }), "repair");
    expect(screen.queryByText("objective of t-run")).not.toBeInTheDocument();
    expect(screen.getByText("Repair desktop build")).toBeInTheDocument();
    await user.clear(screen.getByRole("searchbox", { name: "Filter tasks or agents" }));
    await user.type(screen.getByRole("searchbox", { name: "Filter tasks or agents" }), "nothing like it");
    expect(screen.getByText("No tasks match the filter.")).toBeInTheDocument();
  });

  it("moves between tabs with the arrow keys, Home and End", async () => {
    const { user } = await liveView();
    screen.getByRole("tab", { name: "Running 1" }).focus();
    await user.keyboard("{ArrowRight}");
    expect(screen.getByRole("tab", { name: "Waiting 0" })).toHaveFocus();
    await user.keyboard("{End}");
    expect(screen.getByRole("tab", { name: "Finished 1" })).toHaveFocus();
    await user.keyboard("{Home}");
    expect(screen.getByRole("tab", { name: "Running 1" })).toHaveFocus();
  });

  it("says each empty tab in plain words", async () => {
    const fake = seeded();
    fake.tasks = [mkTask("t-run", REPO, { state: "Running" })];
    const { user } = await mountAgents(fake, { live: true });
    await user.click(await screen.findByRole("tab", { name: "Queued 0" }));
    expect(screen.getByText("No tasks are queued.")).toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "Failed 0" }));
    expect(screen.getByText("No tasks have failed.")).toBeInTheDocument();
  });

  it("shows an empty state when there are no tasks", async () => {
    const fake = seeded();
    fake.tasks = [];
    await mountAgents(fake, { live: true });
    expect(await screen.findByText("No tasks")).toBeInTheDocument();
    expect(screen.getByText("0 running · 0 queued")).toBeInTheDocument();
    expect(screen.queryByRole("tab")).not.toBeInTheDocument();
  });

  it("shows why each queued task has not started and marks a blocked one", async () => {
    const fake = seeded();
    fake.tasks = [
      mkTask("t-a", REPO, { state: "Queued" }),
      mkTask("t-b", REPO, { state: "Queued", contract: { dependencies: ["t-a"] } }),
    ];
    fake.queue.dispatch_blocked = true;
    const { user } = await mountAgents(fake, { live: true });
    await user.click(await screen.findByRole("tab", { name: "Queued 2" }));
    const list = screen.getByRole("list", { name: "queued tasks" });
    expect(await within(list).findByRole("button", { name: /t-a.*Restart gate blocking launches/ })).toBeInTheDocument();
    expect(within(list).getByRole("button", { name: /t-b.*Waiting for t-a/ })).toBeInTheDocument();
    expect(within(list).getByText("Blocked")).toBeInTheDocument();
  });

  it("shows each task's level on its card and page, and the tools its worker lacked beside the outcome", async () => {
    const fake = seeded();
    fake.tasks = [
      mkTask("t-ok", REPO, {
        state: "Succeeded",
        permission_level: "auto",
        tools_notice: { kind: "lacked_tools", tools: ["Bash", "Edit"], reason: "denied 3×" },
      }),
      mkTask("t-old", REPO, { state: "Succeeded", contract: { permission: "full" } }),
      mkTask("t-none", REPO, { state: "Succeeded" }),
    ];
    const { user } = await mountAgents(fake, { live: true });
    await user.click(await screen.findByRole("tab", { name: "Finished 3" }));
    const list = screen.getByRole("list", { name: "finished tasks" });
    const card = within(list).getByRole("button", { name: /objective of t-ok/ });
    expect(within(card).getByText("Auto")).toBeInTheDocument();
    expect(within(card).getByText("Worker lacked tools: Bash, Edit (denied 3×)")).toBeInTheDocument();
    const older = within(list).getByRole("button", { name: /objective of t-old/ });
    expect(within(older).getByText("Full")).toBeInTheDocument();
    expect(within(older).queryByText(/Worker lacked tools/)).not.toBeInTheDocument();
    expect(within(within(list).getByRole("button", { name: /objective of t-none/ })).getByText("Restricted")).toBeInTheDocument();

    const detail = within(await openTask(user, "t-ok"));
    expect(detail.getByText("Auto")).toBeInTheDocument();
    expect(detail.getByText("Worker lacked tools: Bash, Edit (denied 3×)")).toBeInTheDocument();
  });

  it("shows a retryable error when the task list cannot load", async () => {
    const fake = seeded();
    fake.failNext("tasks.list", { code: -32603, message: "list failed" }, 50);
    const { user } = await mountAgents(fake, { live: true });
    expect(await screen.findByText("Tasks could not be loaded")).toBeInTheDocument();
    expect(screen.queryByRole("tab")).not.toBeInTheDocument();
    const calls = fake.callsTo("tasks.list").length;
    await user.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(fake.callsTo("tasks.list").length).toBeGreaterThan(calls));
  });

  it("says tasks are unavailable when the daemon is unreachable before any task list arrived", async () => {
    const fake = seeded();
    fake.failNext("tasks.list", { code: -32603, message: "list failed" }, 50);
    fake.openError = new DaemonError("disconnected", "daemon unreachable");
    const services = createServices({
      connector: fake,
      initialRoute: { name: "global", view: "agents" },
      connection: { backoffMs: [5], requestTimeoutMs: 250 },
      data: { coalesceMs: 10 },
    });
    render(<App services={services} globalViews={agentsGlobalViews} />);
    expect(await screen.findByText("Tasks unavailable")).toBeInTheDocument();
    expect(screen.getByText(/Not connected to the daemon, so tasks cannot be read/)).toBeInTheDocument();
  });

  it("keeps the last rows and warns when a refresh fails", async () => {
    const fake = seeded();
    const { services } = await liveView(fake);
    fake.failNext("tasks.list", { code: -32603, message: "refresh failed" }, 50);
    await services.data.refreshAll();
    expect(await screen.findByText("The last refresh failed; these rows may be out of date.")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Running 1" })).toBeInTheDocument();
  });
});

describe("Agents, the task view", () => {
  it("opens a row into its own view with a back link, title, state and four tabs, and the list goes away", async () => {
    const { user, services } = await liveView();
    const detail = within(await openTask(user, "t-run"));
    expect(detail.getByRole("heading", { level: 1, name: "objective of t-run" })).toBeInTheDocument();
    expect(within(detail.getByRole("heading", { level: 1 }).parentElement!).getByText("Running")).toBeInTheDocument();
    expect(detail.getByText("repo · fake-agent")).toBeInTheDocument();
    expect(detail.getAllByRole("tab").map((tab) => tab.textContent)).toEqual(["Chat", "Output", "Attempts 0", "Event feed"]);
    expect(detail.getByRole("tab", { name: "Chat" })).toHaveAttribute("aria-selected", "true");
    expect(screen.queryByRole("tablist", { name: "Task status" })).not.toBeInTheDocument();
    expect(services.router.getSnapshot()).toEqual({ name: "global", view: "agents", taskId: "t-run" });

    await user.click(detail.getByRole("button", { name: /Agents/ }));
    expect(await screen.findByRole("tab", { name: "Running 1" })).toBeInTheDocument();
    expect(services.router.getSnapshot()).toEqual({ name: "global", view: "agents" });
  });

  it("opens on Chat every time, also after another tab was chosen", async () => {
    const { user } = await liveView();
    let detail = within(await openTask(user, "t-run"));
    await user.click(detail.getByRole("tab", { name: "Event feed" }));
    expect(detail.getByRole("tab", { name: "Event feed" })).toHaveAttribute("aria-selected", "true");
    await user.click(detail.getByRole("button", { name: /Agents/ }));
    detail = within(await openTask(user, "t-run"));
    expect(detail.getByRole("tab", { name: "Chat" })).toHaveAttribute("aria-selected", "true");
    expect(detail.getByRole("tab", { name: "Event feed" })).toHaveAttribute("aria-selected", "false");
  });

  it("opens straight into a task from a #/agents?task= link, and says so for an unknown task", async () => {
    const fake = seeded();
    const services = createServices({
      connector: fake,
      initialRoute: { name: "global", view: "agents", taskId: "t-done" },
      connection: { backoffMs: [5], requestTimeoutMs: 250 },
      data: { coalesceMs: 10 },
    });
    render(<App services={services} globalViews={agentsGlobalViews} />);
    expect(await screen.findByRole("heading", { level: 1, name: "objective of t-done" })).toBeInTheDocument();
    services.router.navigate({ name: "global", view: "agents", taskId: "nope" });
    expect(await screen.findByText("Task not found")).toBeInTheDocument();
  });

  it("shows the session's recent output on the Output tab, read-only", async () => {
    const fake = seeded();
    fake.outputs.set("s-1", { lines: [{ sequence: 1, stream: "stderr", text: "compiling", line_truncated: false }] });
    const { user } = await liveView(fake);
    const detail = within(await openTask(user, "t-run"));
    await user.click(detail.getByRole("tab", { name: "Output" }));
    const panel = within(await screen.findByRole("region", { name: "Recent output" }));
    expect(await panel.findByText("compiling")).toBeInTheDocument();
    expect(panel.getByText("stderr")).toBeInTheDocument();
    expect(panel.queryByRole("button")).not.toBeInTheDocument();
    expect(fake.callsTo("sessions.output_tail")[0]!.params).toMatchObject({ task_id: "t-run", session_id: "s-1" });
  });

  it("interleaves the agent's thinking and tool calls in the Chat thread, then its output", async () => {
    const fake = seeded();
    fake.outputs.set("s-1", { lines: [{ sequence: 1, stream: "stdout", text: "compiling", line_truncated: false }] });
    stubActivity(fake, {
      "s-1": [
        { sequence: 1, kind: "thinking", body: "Plan the work first." },
        toolStarted(2, "a", "Read", "src/lib.rs"),
        toolFinished(3, "a", true, "40 lines"),
        toolStarted(4, "b", "Bash", "make"),
      ],
    });
    const { user } = await liveView(fake);
    await openTask(user, "t-run");
    const log = await screen.findByRole("log", { name: /^Conversation with/ });
    const thread = within(log);
    expect(await thread.findByRole("button", { name: /Bash/ })).toHaveTextContent("Running: Bash");
    expect(thread.getByRole("button", { name: /Read/ })).toHaveTextContent("Finished: Read");
    const text = log.textContent ?? "";
    expect(text.indexOf("Plan the work first.")).toBeGreaterThanOrEqual(0);
    expect(text.indexOf("Plan the work first.")).toBeLessThan(text.indexOf("Read"));
    expect(text.indexOf("Read")).toBeLessThan(text.indexOf("Bash"));
    expect(text.indexOf("Bash")).toBeLessThan(text.indexOf("compiling"));
  });

  it("shows recorded thinking and the size-limit note in Chat from a daemon without sessions.activity", async () => {
    const fake = seeded();
    fake.stub("sessions.thinking", { chunks: [{ sequence: 1, body: "Plan the **marker-zz**." }], next_cursor: 1, truncated: true });
    const { user } = await liveView(fake);
    const detail = within(await openTask(user, "t-run"));
    expect(await detail.findByText("marker-zz")).toBeInTheDocument();
    expect(detail.getByText("Thinking log hit its size limit.")).toBeInTheDocument();
    expect(fake.callsTo("sessions.thinking")[0]!.params).toMatchObject({ session_id: "s-1" });
  });

  it("switches the Chat thread with the session picker", async () => {
    const fake = seeded();
    fake.tasks.push(
      mkTask("t-multi", REPO, { state: "Failed", sessions: [mkSession("s-a", { state: "Exited" }), mkSession("s-b", { state: "Exited" })] }),
    );
    stubActivity(fake, {
      "s-a": [{ sequence: 1, kind: "thinking", body: "Thought in the first session." }],
      "s-b": [{ sequence: 1, kind: "thinking", body: "Thought in the second session." }],
    });
    const { user, services } = await liveView(fake);
    services.router.navigate({ name: "global", view: "agents", taskId: "t-multi" });
    const detail = within(await screen.findByTestId("agents-detail"));
    expect(await detail.findByText("Thought in the second session.")).toBeInTheDocument();
    await user.selectOptions(detail.getByRole("combobox", { name: "Session" }), "s-a");
    expect(await detail.findByText("Thought in the first session.")).toBeInTheDocument();
    expect(detail.queryByText("Thought in the second session.")).not.toBeInTheDocument();
  });

  describe("the Chat composer", () => {
    const waiting = () => {
      const fake = seeded();
      fake.tasks.push(mkTask("t-wait", REPO, { state: "Waiting", sessions: [mkSession("s-w", { state: "Exited" })] }));
      fake.stub("decisions.list", { questions: [] });
      return fake;
    };
    const openWaiting = async (fake: FakeDaemon) => {
      const live = await liveView(fake);
      live.services.router.navigate({ name: "global", view: "agents", taskId: "t-wait" });
      await screen.findByTestId("agents-detail");
      const box = await screen.findByRole("textbox", { name: "Message to the agent" });
      await waitFor(() => expect(box).toBeEnabled());
      return { ...live, box };
    };

    it("sends a message to a waiting task with the existing API and adds its bubble to the thread", async () => {
      const fake = waiting();
      const { user, box } = await openWaiting(fake);
      await user.type(box, "Use the staging database{Enter}");
      const thread = within(await screen.findByRole("log", { name: /^Conversation with/ }));
      expect(await thread.findByText("Use the staging database")).toBeInTheDocument();
      expect(thread.getByText("You")).toBeInTheDocument();
      expect(fake.messages).toEqual([{ task_id: "t-wait", text: "Use the staging database" }]);
      expect(box).toHaveValue("");
    });

    it("answers an open question from the thread with decisions.answer", async () => {
      const fake = seeded();
      fake.tasks.push(mkTask("t-wait", REPO, { state: "Waiting" }));
      const question = {
        id: "q1",
        task_id: "t-wait",
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
      fake.stub("decisions.list", { questions: [question] });
      fake.stub("decisions.answer", { question: { ...question, state: "answered" } });
      const { user, services } = await liveView(fake);
      services.router.navigate({ name: "global", view: "agents", taskId: "t-wait" });
      const thread = within(await screen.findByRole("log", { name: /^Conversation with/ }));
      expect(await thread.findByText("Migrate sessions now?")).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: "Migrate them now" }));
      await user.click(screen.getByRole("button", { name: "Send answer" }));
      await waitFor(() => expect(fake.callsTo("decisions.answer")).toHaveLength(1));
      expect(fake.callsTo("decisions.answer")[0]!.params).toMatchObject({ question_id: "q1", answer: "Migrate them now" });
      expect(await thread.findByText("You")).toBeInTheDocument();
      expect(fake.callsTo("tasks.message")).toHaveLength(0);
    });

    it("keeps the unsent draft of a task when its window is closed and opened again", async () => {
      const { user, box, services } = await openWaiting(waiting());
      await user.type(box, "Half a thought");
      await user.click(within(screen.getByTestId("agents-detail")).getByRole("button", { name: /Agents/ }));
      services.router.navigate({ name: "global", view: "agents", taskId: "t-wait" });
      expect(await screen.findByRole("textbox", { name: "Message to the agent" })).toHaveValue("Half a thought");
    });

    it.each([
      ["t-queued", "Sent. The agent reads it when it starts."],
      ["t-done", "Sent. The agent picks the task back up shortly."],
    ])("sends a message to %s and says what happens next", async (taskId, notice) => {
      const fake = seeded();
      fake.stub("decisions.list", { questions: [] });
      const { user, services } = await liveView(fake);
      services.router.navigate({ name: "global", view: "agents", taskId });
      const detail = within(await screen.findByTestId("agents-detail"));
      const box = await detail.findByRole("textbox", { name: "Message to the agent" });
      await waitFor(() => expect(box).toBeEnabled());
      await user.type(box, "Check it{Enter}");
      expect(await detail.findByText(notice)).toBeInTheDocument();
      expect(fake.messages).toEqual([{ task_id: taskId, text: "Check it" }]);
    });

    it("disables the composer of a running task and says why, while the thread stays readable", async () => {
      const { services } = await liveView();
      services.router.navigate({ name: "global", view: "agents", taskId: "t-run" });
      const detail = within(await screen.findByTestId("agents-detail"));
      const box = await detail.findByRole("textbox", { name: "Message to the agent" });
      expect(box).toBeDisabled();
      expect(detail.getByRole("button", { name: "Send message" })).toBeDisabled();
      expect(detail.getByText(/Live steering is not available yet/)).toBeInTheDocument();
      expect(box).toHaveAccessibleDescription(/Live steering is not available yet/);
      expect(detail.getByRole("log", { name: /^Conversation with/ })).toBeInTheDocument();
    });

    it("disables the composer of a verifying task and says why", async () => {
      const fake = seeded();
      fake.tasks.push(mkTask("t-verify", REPO, { state: "Verifying" }));
      // Verifying counts as running, so the tab reads "Running 2" here, not "Running 1".
      const { services } = await mountAgents(fake, { live: true });
      await screen.findByRole("tab", { name: "Running 2" });
      services.router.navigate({ name: "global", view: "agents", taskId: "t-verify" });
      const detail = within(await screen.findByTestId("agents-detail"));
      const box = await detail.findByRole("textbox", { name: "Message to the agent" });
      expect(box).toBeDisabled();
      expect(detail.getByRole("button", { name: "Send message" })).toBeDisabled();
      expect(detail.getByText(/The agent's work is being verified\. You can reply when verification finishes\./)).toBeInTheDocument();
      expect(fake.callsTo("tasks.message")).toHaveLength(0);
    });
  });

  it("lists attempts with the sessions each ran, and counts them in the tab", async () => {
    const fake = seeded();
    fake.tasks[0] = mkTask("t-run", REPO, {
      state: "Running",
      attempts: [
        { id: "attempt-old", task_id: "t-run", attempt_number: 1, state: "Failed" },
        { id: "attempt-run", task_id: "t-run", attempt_number: 2, state: "Running" },
      ],
      sessions: [mkSession("s-old", { state: "Aborted", attempt_id: "attempt-old" }), mkSession("s-1", { attempt_id: "attempt-run" })],
    });
    const { user } = await liveView(fake);
    const detail = within(await openTask(user, "t-run"));
    await user.click(detail.getByRole("tab", { name: "Attempts 2" }));
    const items = within(detail.getByRole("tabpanel", { name: "Attempts 2" })).getAllByRole("listitem");
    const [first, second] = items.filter((item) => /^Attempt/.test(item.textContent ?? ""));
    expect(first).toHaveTextContent(/Attempt 1.*Failed.*session s-old.*Aborted/);
    expect(second).toHaveTextContent(/Attempt 2.*Running.*session s-1.*Running/);
    expect(detail.getByText("repo · fake-agent · attempt 2 of 2")).toBeInTheDocument();
  });

  it("shows the daemon event feed on the Event feed tab, with the chosen session", async () => {
    const fake = seeded();
    fake.events = [mkEvent(1, "t-run", "s-1")];
    const { user } = await liveView(fake);
    const detail = within(await openTask(user, "t-run"));
    await user.click(detail.getByRole("tab", { name: "Event feed" }));
    const feed = within(await detail.findByRole("table", { name: "Execution feed" }));
    expect(feed.getByText("SessionOutput")).toBeInTheDocument();
    expect(feed.getByText('{"n":1}')).toBeInTheDocument();
    expect(fake.callsTo("sessions.events").map((call) => call.params)).toContainEqual(
      expect.objectContaining({ task_id: "t-run", session_id: "s-1" }),
    );
    expect(detail.queryByRole("button", { name: /diagnostic|retained evidence/i })).not.toBeInTheDocument();
  });

  it("offers a session picker only when the task has more than one session", async () => {
    const fake = seeded();
    fake.tasks[0] = mkTask("t-run", REPO, { state: "Running", sessions: [mkSession("s-1"), mkSession("s-2")] });
    const { user } = await liveView(fake);
    const detail = within(await openTask(user, "t-run"));
    expect(detail.getByRole("combobox", { name: "Session" })).toHaveValue("s-2");
  });

  it("keeps ids, agent and scope notes in a collapsed Technical details section", async () => {
    const { user } = await liveView();
    const detail = within(await openTask(user, "t-run"));
    const technical = detail.getByTestId("agents-technical");
    expect(technical).not.toHaveAttribute("open");
    expect(within(technical).getByText("t-run")).toBeInTheDocument();
    expect(within(technical).getByText("fake-agent")).toBeInTheDocument();
    expect(within(technical).getByText(/no attempt-level cancel/)).toBeInTheDocument();
  });

  it("says sessions are unknown, not none, while the session list cannot load", async () => {
    const fake = seeded();
    fake.failNext("sessions.list", { code: -32603, message: "sessions failed" }, 2);
    const { user } = await mountAgents(fake, { live: true });
    const detail = within(await openTask(user, "t-run"));
    expect(await detail.findAllByText(/Sessions unknown/)).not.toHaveLength(0);
    expect(detail.queryByText("No session")).not.toBeInTheDocument();
  });
});

describe("Agents, the Actions menu", () => {
  const menu = (detail: ReturnType<typeof within>) => detail.getByRole("button", { name: "Actions" });

  it("offers Cancel attempt on a running task, requests it once, and says it is not proof of a stop", async () => {
    const { fake, user } = await liveView();
    const detail = within(await openTask(user, "t-run"));
    await user.click(menu(detail));
    expect(within(detail.getByRole("menu", { name: "Actions" })).getAllByRole("menuitem").map((item) => item.textContent)).toEqual([
      "Cancel attempt",
    ]);
    await user.click(detail.getByRole("menuitem", { name: "Cancel attempt" }));
    expect(await screen.findByText(/Cancel request acknowledged; the daemon now records the task as Canceled/)).toBeInTheDocument();
    expect(screen.getByText(/applied to the whole task, not only this attempt/)).toBeInTheDocument();
    expect(screen.getByText(/not proof that any process has stopped/)).toBeInTheDocument();
    expect(fake.callsTo("tasks.cancel")).toHaveLength(1);
    expect(fake.callsTo("tasks.cancel")[0]!.params).toMatchObject({ task_id: "t-run" });
    await waitFor(() => expect(detail.queryByRole("button", { name: "Actions" })).not.toBeInTheDocument());
  });

  it("cancels a Verifying task and marks its candidate attempt Cancelled", async () => {
    const fake = seeded();
    fake.tasks.push(
      mkTask("t-verify", REPO, {
        state: "Verifying",
        attempts: [{ id: "attempt-verify", task_id: "t-verify", attempt_number: 1, state: "CandidateReady" }],
      }),
    );
    const { user } = await mountAgents(fake, { live: true });
    await screen.findByRole("tab", { name: "Running 2" });
    const detail = within(await openTask(user, "t-verify"));
    await user.click(menu(detail));
    await user.click(detail.getByRole("menuitem", { name: "Cancel attempt" }));
    expect(await screen.findByText(/Cancel request acknowledged/)).toBeInTheDocument();
    expect(fake.tasks.find((task) => task.contract.id === "t-verify")!.attempts[0]!.state).toBe("Cancelled");
  });

  it("offers Reassign, and only that, on a failed task; the accepted request is reported as a past event", async () => {
    const { fake, user } = await liveView();
    await user.click(screen.getByRole("tab", { name: "Failed 1" }));
    const detail = within(await openTask(user, "t-failed"));
    await user.click(await detail.findByRole("button", { name: "Actions" }));
    expect(detail.getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Reassign"]);
    await user.click(detail.getByRole("menuitem", { name: "Reassign" }));
    expect(await screen.findByText("The request for a new attempt was accepted by the daemon.")).toBeInTheDocument();
    expect(fake.callsTo("tasks.retry")).toHaveLength(1);
  });

  it("hides the whole menu, not a disabled copy, when no action is possible", async () => {
    const { user } = await liveView();
    await user.click(screen.getByRole("tab", { name: "Finished 1" }));
    const detail = within(await openTask(user, "t-done"));
    expect(detail.queryByRole("button", { name: "Actions" })).not.toBeInTheDocument();
    expect(detail.queryByRole("button", { name: /Cancel attempt|Reassign|Resume|Amend|decomposition|checkpoint/ })).not.toBeInTheDocument();
    // The Chat composer's Send stays, disabled with its reason shown; no action is left disabled.
    const disabled = detail.getAllByRole("button").filter((button) => button.hasAttribute("disabled"));
    expect(disabled.map((button) => button.getAttribute("aria-label"))).toEqual(["Send message"]);
  });

  it("offers no Reassign on a running task and Cancel attempt and Force start on a queued one", async () => {
    const { user } = await liveView();
    await user.click(screen.getByRole("tab", { name: "Queued 1" }));
    const detail = within(await openTask(user, "t-queued"));
    await user.click(menu(detail));
    expect(detail.getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Cancel attempt", "Force start"]);
  });

  it("force-starts a queued task, sending tasks.force_start for it", async () => {
    const { fake, user } = await liveView();
    await user.click(screen.getByRole("tab", { name: "Queued 1" }));
    const detail = within(await openTask(user, "t-queued"));
    await user.click(menu(detail));
    await user.click(detail.getByRole("menuitem", { name: "Force start" }));
    expect(await screen.findByText(/The daemon accepted the force start/)).toBeInTheDocument();
    expect(fake.callsTo("tasks.force_start")).toHaveLength(1);
    expect(fake.callsTo("tasks.force_start")[0]!.params).toEqual({ task_id: "t-queued", idempotency_key: expect.any(String) });
  });

  it("answers a refused force start with an alert that says why and offers Retry", async () => {
    const { fake, user } = await liveView();
    fake.failNext("tasks.force_start", { code: -32011, message: "task is not queued" });
    await user.click(screen.getByRole("tab", { name: "Queued 1" }));
    const detail = within(await openTask(user, "t-queued"));
    await user.click(menu(detail));
    await user.click(detail.getByRole("menuitem", { name: "Force start" }));
    const alert = await detail.findByRole("alert");
    expect(alert).toHaveTextContent(/Forcing the task to start failed/);
    expect(within(alert).getByRole("button", { name: "Retry" })).toBeInTheDocument();
  });

  it("offers no Force start to a session that cannot send user-only methods", async () => {
    const fake = seeded();
    fake.approver = false;
    const { user } = await liveView(fake);
    await user.click(screen.getByRole("tab", { name: "Queued 1" }));
    const queued = within(await openTask(user, "t-queued"));
    await user.click(menu(queued));
    expect(queued.getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Cancel attempt"]);
    expect(fake.callsTo("tasks.force_start")).toHaveLength(0);
  });

  it("shows daemon failures for Cancel and Reassign without claiming either succeeded", async () => {
    const fake = seeded();
    fake.failNext("tasks.cancel", { code: -32011, message: "task already terminal" });
    fake.failNext("tasks.retry", { code: -32011, message: "retry rejected" });
    const { user } = await liveView(fake);
    let detail = within(await openTask(user, "t-run"));
    await user.click(menu(detail));
    await user.click(detail.getByRole("menuitem", { name: "Cancel attempt" }));
    expect(await screen.findByText(/Canceling the task failed/)).toBeInTheDocument();
    expect(screen.queryByText(/Cancel request acknowledged/)).not.toBeInTheDocument();
    expect(fake.callsTo("tasks.cancel")[0]!.params).toMatchObject({ idempotency_key: expect.any(String) });

    await user.click(detail.getByRole("button", { name: /Agents/ }));
    await user.click(await screen.findByRole("tab", { name: "Failed 1" }));
    detail = within(await openTask(user, "t-failed"));
    await user.click(await detail.findByRole("button", { name: "Actions" }));
    await user.click(detail.getByRole("menuitem", { name: "Reassign" }));
    expect(await screen.findByText(/Reassigning the task failed/)).toBeInTheDocument();
    expect(fake.callsTo("tasks.retry")).toHaveLength(1);
  });
});

describe("Agents, fixture mode", () => {
  it("labels fixture-backed values with a PLACEHOLDER badge and offers no daemon action", async () => {
    const fake = new FakeDaemon();
    const { user } = await mountAgents(fake, { bridge: false });
    expect((await screen.findAllByTestId("badge-D2")).length).toBeGreaterThan(0);
    await user.click(await screen.findByRole("button", { name: /Preview: running task/ }));
    const detail = within(await screen.findByTestId("agents-detail"));
    expect(detail.getByTestId("badge-D2")).toBeInTheDocument();
    expect(detail.queryByRole("button", { name: "Actions" })).not.toBeInTheDocument();
    expect(fake.callsTo("tasks.cancel")).toHaveLength(0);
    expect(fake.callsTo("tasks.retry")).toHaveLength(0);
  });

  it("offers a local trace filter on the Event feed tab", async () => {
    const { user } = await mountAgents(new FakeDaemon(), { bridge: false });
    await user.click(await screen.findByRole("button", { name: /Preview: running task/ }));
    const detail = within(await screen.findByTestId("agents-detail"));
    await user.click(detail.getByRole("tab", { name: "Event feed" }));
    await user.selectOptions(detail.getByRole("combobox", { name: "Filter raw trace" }), "ExecutionStarted");
    expect(within(detail.getByRole("table", { name: "Execution feed" })).queryByText("agent.message")).not.toBeInTheDocument();
  });
});
