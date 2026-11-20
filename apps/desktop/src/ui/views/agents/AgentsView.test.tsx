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
  fake.stub("agents.capabilities", [
    {
      agent: "codex",
      installed: true,
      launchable: true,
      capabilities: {
        streaming: true,
        interactive_input: false,
        structured_output: true,
        cancellation: true,
        session_resumption: true,
        approvals: false,
        model_selection: false,
      },
    },
    { agent: "pi", installed: true, launchable: false, capabilities: null },
  ]);
  fake.stub("tasks.retry", (params: Record<string, unknown>) => ({
    ...mkTask(String(params.task_id), REPO),
    state: "Queued",
    attempts: [{ id: "a1" }, { id: "a2" }],
  }));
  return fake;
}

async function liveView(fake = seeded()) {
  const mounted = await mountAgents(fake, { live: true });
  await screen.findByRole("tab", { name: "Active (1)" });
  return { fake, ...mounted };
}

describe("Agents & Sessions, live mode", () => {
  it("renders landmarks inside the shell's single main region", async () => {
    await liveView();
    expect(screen.getAllByRole("main")).toHaveLength(1);
    expect(within(screen.getByRole("main")).getByRole("heading", { level: 1, name: "Agents & Sessions" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Execution status" })).toBeInTheDocument();
    expect(screen.getByRole("complementary", { name: "Selected task / session detail" })).toBeInTheDocument();
    expect(screen.getByRole("tablist", { name: "Task and session lists" })).toBeInTheDocument();
  });

  it("starts with the first running task selected and the configured inventory collapsed", async () => {
    await liveView();
    const detail = screen.getByRole("complementary", { name: "Selected task / session detail" });
    expect(within(detail).getByText("t-run")).toBeInTheDocument();
    expect(within(detail).queryByText("No task selected")).not.toBeInTheDocument();
    expect(screen.getByTestId("agents-inventory")).not.toHaveAttribute("open");
  });

  it("keeps tab counts equal to the lists, counts failed sessions and marks disconnected not available", async () => {
    const { user } = await liveView();
    expect(screen.getByRole("tab", { name: "Active (1)" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Queued (1)" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Failed sessions (0)" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Available / idle" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Disconnected (not available)" })).toBeInTheDocument();
    expect(screen.getByText("1 running task(s), 1 running session(s)")).toBeInTheDocument();
    expect(screen.getByText("1 tasks queued (1 ready)")).toBeInTheDocument();
    expect(screen.getByText("0 tasks blocked")).toBeInTheDocument();
    expect(within(screen.getByRole("list", { name: "active tasks" })).getAllByRole("button")).toHaveLength(1);

    await user.click(screen.getByRole("tab", { name: "Queued (1)" }));
    const queued = within(screen.getByRole("list", { name: "queued tasks" }));
    expect(queued.getAllByRole("button")).toHaveLength(1);
    expect(queued.getByText("t-queued")).toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "Failed sessions (0)" }));
    const orphans = within(screen.getByRole("list", { name: "failed tasks without a session" }));
    expect(orphans.getAllByRole("button")).toHaveLength(1);
    expect(orphans.getByText("t-failed")).toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "Available / idle" }));
    expect(screen.getByText("Available / idle workers are not available")).toBeInTheDocument();
    expect(screen.queryByRole("list", { name: "Available adapters" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "Disconnected (not available)" }));
    expect(screen.getByText("Disconnected sessions are not available")).toBeInTheDocument();
    expect(screen.getByText("Disconnected sessions not available")).toBeInTheDocument();
    expect(screen.queryByText(/0 disconnected sessions/)).not.toBeInTheDocument();
  });

  it("filters assigned work locally and shows the attempt number on each row", async () => {
    const fake = seeded();
    fake.tasks = [
      mkTask("t-run", REPO, {
        state: "Running",
        sessions: [mkSession("s-1")],
        attempts: [{ id: "a-1", task_id: "t-run", attempt_number: 3, state: "Running" }],
      }),
      mkTask("t-other", REPO, { state: "Running", contract: { objective: "Repair desktop build" } }),
    ];
    const { user } = await mountAgents(fake, { live: true });
    await screen.findByRole("tab", { name: "Active (2)" });
    expect(screen.getByRole("button", { name: /t-run/ })).toHaveTextContent("Attempt 3");
    await user.type(screen.getByRole("searchbox", { name: "Filter by task, objective or agent" }), "repair");
    expect(screen.getByText("Showing 1 of 2.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /t-run/ })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /t-other/ })).toBeInTheDocument();
    expect(fake.callsTo("tasks.list").length).toBeGreaterThan(0);
  });

  it("lists a failed session as one row and counts sessions, not tasks", async () => {
    const fake = seeded();
    fake.tasks = [
      mkTask("t-f", REPO, {
        state: "Failed",
        attempts: [{ id: "a-f", task_id: "t-f", attempt_number: 1, state: "Failed" }],
        sessions: [mkSession("s-f", { state: "Aborted", attempt_id: "a-f" })],
      }),
    ];
    const { user } = await mountAgents(fake, { live: true });
    await user.click(await screen.findByRole("tab", { name: "Failed sessions (1)" }));
    const failed = within(screen.getByRole("list", { name: "failed sessions" }));
    expect(failed.getAllByRole("button")).toHaveLength(1);
    expect(failed.getByRole("button", { name: /t-f/ })).toHaveTextContent("Attempt 1");
    expect(failed.getByRole("button", { name: /t-f/ })).toHaveTextContent("s-f");
    expect(screen.queryByRole("list", { name: "failed tasks without a session" })).not.toBeInTheDocument();
  });

  it("lists a Waiting task in the Waiting tab and counts it in the header and the total", async () => {
    const fake = seeded();
    fake.tasks.push(mkTask("t-wait", REPO, { state: "Waiting", contract: { objective: "Find the root cause" } }));
    const { user } = await mountAgents(fake, { live: true });
    await user.click(await screen.findByRole("tab", { name: "Waiting (1)" }));
    const list = within(screen.getByRole("list", { name: "waiting tasks" }));
    expect(list.getByRole("button", { name: /t-wait/ })).toBeInTheDocument();
    expect(screen.getByText("1 tasks waiting for a reply")).toBeInTheDocument();
    expect(screen.getByText("1 running task(s), 1 running session(s)")).toBeInTheDocument();
    expect(screen.getByText(/Counts cover 5 task\(s\)/)).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Active (1)" })).toBeInTheDocument();
  });

  it("lists succeeded and canceled tasks in the Finished tab", async () => {
    const fake = seeded();
    fake.tasks.push(mkTask("t-cancel", REPO, { state: "Cancelled" }));
    const { user } = await mountAgents(fake, { live: true });
    await user.click(await screen.findByRole("tab", { name: "Finished (2)" }));
    const list = within(screen.getByRole("list", { name: "finished tasks" }));
    expect(list.getByRole("button", { name: /t-done/ })).toBeInTheDocument();
    expect(list.getByRole("button", { name: /t-cancel/ })).toBeInTheDocument();
  });

  it("moves between tabs with the arrow keys, Home and End", async () => {
    const { user } = await liveView();
    screen.getByRole("tab", { name: "Active (1)" }).focus();
    await user.keyboard("{ArrowRight}");
    expect(screen.getByRole("tab", { name: "Waiting (0)" })).toHaveFocus();
    expect(screen.getByRole("tab", { name: "Waiting (0)" })).toHaveAttribute("aria-selected", "true");
    await user.keyboard("{End}");
    expect(screen.getByRole("tab", { name: "Disconnected (not available)" })).toHaveFocus();
    await user.keyboard("{Home}");
    expect(screen.getByRole("tab", { name: "Active (1)" })).toHaveFocus();
    await user.keyboard("{ArrowLeft}");
    expect(screen.getByRole("tab", { name: "Disconnected (not available)" })).toHaveFocus();
  });

  it("reports the daemon's exact installed and launchable flags and unknown usage", async () => {
    await liveView();
    const table = await screen.findByRole("table", { name: /inventory/i });
    const codex = within(table).getByRole("row", { name: /Codex/ });
    expect(within(codex).getByText("Installed and launchable")).toBeInTheDocument();
    expect(within(codex).getByText("installed: yes; launchable: yes")).toBeInTheDocument();
    const capabilities = within(codex).getByRole("list", { name: "Codex capabilities" });
    for (const flag of [
      "Streaming: Yes",
      "Interactive input: No",
      "Structured output: Yes",
      "Cancellation: Yes",
      "Session resumption: Yes",
      "Approvals: No",
      "Model selection: No",
    ]) {
      expect(within(capabilities).getByText(flag)).toBeInTheDocument();
    }
    const pi = within(table).getByRole("row", { name: /Pi/ });
    expect(within(pi).getByText("Installed, not launchable")).toBeInTheDocument();
    expect(within(pi).getByText("installed: yes; launchable: no")).toBeInTheDocument();
    expect(within(pi).getAllByText("Unknown")).toHaveLength(2);
    expect(within(pi).getByText("Not reported")).toBeInTheDocument();
    expect(within(table).getAllByText("Unknown (the daemon does not report usage)")).toHaveLength(2);
    const configure = screen.getByRole("button", { name: "Configure providers" });
    expect(configure).toBeDisabled();
    expect(configure).toHaveAccessibleDescription(/not available/i);
  });

  it("shows reported, estimated and unknown usage apart, with the price table version", async () => {
    const fake = seeded();
    const figures = (costs: Array<{ currency: string; amount: string }>) => ({
      input_tokens: 10,
      cache_read_tokens: null,
      cache_write_tokens: null,
      output_tokens: 10,
      reasoning_tokens: null,
      costs,
    });
    fake.stub("usage.summary", {
      window: { start_unix_ms: 1_767_250_800_000, end_unix_ms: 1_767_337_200_000, timezone: "America/Denver", utc_offset_seconds: -25_200 },
      totals: {
        sessions: 4,
        sessions_with_tokens: 4,
        sessions_with_cost: 1,
        token_status: "complete",
        cost_status: "partial",
        sessions_needing_estimate: 3,
        sessions_with_estimated_cost: 1,
        sessions_with_known_cost: 2,
        estimated_cost_status: "partial",
        known_cost_status: "partial",
        reported: figures([{ currency: "USD", amount: "2" }]),
        estimated: figures([{ currency: "USD", amount: "0.5" }]),
      },
      providers: [],
      price_table: { version: "2026-11-11.1", basis: "list_price_standard_tier", source: "LiteLLM, 2026-11-11" },
    });
    await liveView(fake);
    const usage = within(await screen.findByTestId("agents-usage"));
    await waitFor(() => expect(usage.getByRole("table", { name: "Usage provenance", hidden: true })).toBeInTheDocument());
    const table = usage.getByRole("table", { name: "Usage provenance", hidden: true });
    expect(within(table).getByText(/USD 2\.00 \(reported\)/)).toBeInTheDocument();
    expect(within(table).getByText(/USD 0\.50 \(estimated, table 2026-11-11\.1\)/)).toBeInTheDocument();
    expect(screen.getByTestId("agents-usage-estimate-coverage")).toHaveTextContent("partial: 1 of 3 sessions");
    expect(screen.getByTestId("agents-usage-unknown")).toHaveTextContent("2 of 4 sessions have neither a reported nor an estimated cost");
    expect(screen.getByTestId("agents-usage-price-table")).toHaveTextContent("2026-11-11.1 · basis list_price_standard_tier");
  });

  it("shows no placeholder badge and no wireframe numbers", async () => {
    await liveView();
    const main = within(screen.getByRole("main"));
    expect(main.queryByText(/PLACEHOLDER/i)).not.toBeInTheDocument();
    expect(main.queryByText(/20\/24/)).not.toBeInTheDocument();
    expect(main.queryByText(/simulated/i)).not.toBeInTheDocument();
  });

  it("requests cancel once and says it is not proof of a stop", async () => {
    const { fake, user } = await liveView();
    await user.click(screen.getByRole("button", { name: /t-run/ }));
    const cancel = await screen.findByRole("button", { name: "Cancel attempt" });
    expect(cancel).toHaveAccessibleDescription(/cancels the whole task, not only this attempt or session/i);
    await user.click(cancel);
    expect(await screen.findByText(/Cancel request acknowledged; the daemon now records the task as Canceled/)).toBeInTheDocument();
    expect(screen.getByText(/applied to the whole task, not only this attempt or session/)).toBeInTheDocument();
    expect(screen.getByText(/not proof that any process has stopped/)).toBeInTheDocument();
    expect(fake.callsTo("tasks.cancel")).toHaveLength(1);
    await waitFor(() => expect(screen.getByRole("button", { name: "Cancel attempt" })).toBeDisabled());
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
    await screen.findByRole("tab", { name: "Active (2)" });
    await user.click(screen.getByRole("button", { name: /t-verify/ }));
    const cancel = await screen.findByRole("button", { name: "Cancel attempt" });
    expect(cancel).toBeEnabled();
    await user.click(cancel);
    expect(await screen.findByText(/Cancel request acknowledged; the daemon now records the task as Canceled/)).toBeInTheDocument();
    expect(fake.callsTo("tasks.cancel")).toHaveLength(1);
    expect(fake.callsTo("tasks.cancel")[0]!.params).toMatchObject({ task_id: "t-verify" });
    expect(fake.tasks.find((task) => task.contract.id === "t-verify")!.attempts[0]!.state).toBe("Cancelled");
    await waitFor(() => expect(screen.getByRole("button", { name: "Cancel attempt" })).toBeDisabled());
  });

  it("shows the selected session's recent output read-only in the detail", async () => {
    const fake = seeded();
    fake.outputs.set("s-1", {
      lines: [{ sequence: 1, stream: "stderr", text: "compiling", line_truncated: false }],
    });
    const { user } = await liveView(fake);
    await user.click(screen.getByRole("button", { name: /t-run/ }));
    const panel = within(await screen.findByRole("region", { name: "Recent output" }));
    expect(await panel.findByText("compiling")).toBeInTheDocument();
    expect(panel.getByText("stderr")).toBeInTheDocument();
    expect(panel.queryByRole("button")).not.toBeInTheDocument();
    expect(fake.callsTo("sessions.output_tail")[0]!.params).toMatchObject({ task_id: "t-run", session_id: "s-1" });
  });

  it("shows the selected session's thinking in a Thinking disclosure, empty by default", async () => {
    const { user } = await liveView(seeded());
    await user.click(screen.getByRole("button", { name: /t-run/ }));
    expect(await screen.findByText("No thinking recorded")).toBeInTheDocument();
  });

  it("shows the selected session's recorded thinking and the size-limit note", async () => {
    const fake = seeded();
    fake.stub("sessions.thinking", { chunks: [{ sequence: 1, body: "Plan the **marker-zz**." }], next_cursor: 1, truncated: true });
    const { user } = await liveView(fake);
    await user.click(screen.getByRole("button", { name: /t-run/ }));
    expect(await screen.findByText("marker-zz")).toBeInTheDocument();
    expect(screen.getByText("Thinking log hit its size limit.")).toBeInTheDocument();
    expect(fake.callsTo("sessions.thinking")[0]!.params).toMatchObject({ session_id: "s-1" });
  });

  it("places cancel under Session controls and the reassignment under Task controls", async () => {
    const { user } = await liveView();
    await user.click(screen.getByRole("button", { name: /t-run/ }));
    const session = within(await screen.findByRole("region", { name: "Session controls" }));
    expect(session.getByRole("button", { name: "Cancel attempt" })).toBeEnabled();
    expect(session.getByRole("button", { name: "Resume unavailable until capability verified" })).toBeDisabled();
    const task = within(screen.getByRole("region", { name: "Task controls" }));
    expect(task.queryByRole("button", { name: "Cancel attempt" })).not.toBeInTheDocument();
    expect(task.getByRole("button", { name: "Reassign via new attempt" })).toBeInTheDocument();
  });

  it("reports an accepted reassign request as a past event", async () => {
    const { fake, user } = await liveView();
    await user.click(screen.getByRole("tab", { name: "Failed sessions (0)" }));
    await user.click(screen.getByRole("button", { name: /t-failed/ }));
    const reassign = await screen.findByRole("button", { name: "Reassign via new attempt" });
    await waitFor(() => expect(reassign).toBeEnabled());
    await user.click(reassign);
    expect(await screen.findByText("The request for a new attempt was accepted by the daemon.")).toBeInTheDocument();
    expect(fake.callsTo("tasks.retry")).toHaveLength(1);
    expect(screen.queryByText(/simulated/i)).not.toBeInTheDocument();
  });

  it("keeps the new-attempt acknowledgement historical when the later task state is failed", async () => {
    const fake = seeded();
    fake.stub("tasks.retry", (params: Record<string, unknown>) => {
      const taskId = String(params.task_id);
      const queuedAcknowledgement = mkTask(taskId, REPO, {
        state: "Queued",
        attempts: [{ id: "attempt-1", task_id: taskId, attempt_number: 1, state: "Pending" }],
      });
      const latest = mkTask(taskId, REPO, {
        state: "Failed",
        attempts: [1, 2].map((attempt_number) => ({
          id: `attempt-${attempt_number}`,
          task_id: taskId,
          attempt_number,
          state: "Failed" as const,
        })),
      });
      const index = fake.tasks.findIndex((task) => task.contract.id === taskId);
      if (index >= 0) fake.tasks[index] = latest;
      return queuedAcknowledgement;
    });
    const { user } = await liveView(fake);
    await user.click(screen.getByRole("tab", { name: "Failed sessions (0)" }));
    await user.click(screen.getByRole("button", { name: /t-failed/ }));
    const reassign = await screen.findByRole("button", { name: "Reassign via new attempt" });
    await waitFor(() => expect(reassign).toBeEnabled());
    await user.click(reassign);
    expect(await screen.findByText("The request for a new attempt was accepted by the daemon.")).toBeInTheDocument();
    const detail = within(screen.getByRole("complementary", { name: "Selected task / session detail" }));
    await waitFor(() => expect(detail.getByText("t-failed").parentElement).toHaveTextContent("Failed"));
    const history = within(detail.getByRole("region", { name: "Attempt history" }));
    expect(history.getAllByRole("listitem")).toHaveLength(2);
    expect(history.getAllByRole("listitem")[0]).toHaveTextContent(/Attempt 1 attempt-1.*Failed/);
    expect(history.getAllByRole("listitem")[1]).toHaveTextContent(/Attempt 2 attempt-2.*Failed/);
    expect(screen.queryByText(/daemon now records the task as Queued with 1 attempt/)).not.toBeInTheDocument();
    expect(fake.callsTo("tasks.retry")).toHaveLength(1);
    expect(screen.queryByText(/simulated/i)).not.toBeInTheDocument();
  });
  it("shows daemon failures for Cancel and Retry without claiming either succeeded", async () => {
    const fake = seeded();
    fake.failNext("tasks.cancel", { code: -32011, message: "task already terminal" });
    fake.failNext("tasks.retry", { code: -32011, message: "retry rejected" });
    const { user } = await liveView(fake);
    await user.click(screen.getByRole("button", { name: /t-run/ }));
    await user.click(screen.getByRole("button", { name: "Cancel attempt" }));
    expect(await screen.findByText(/Canceling the task failed/)).toBeInTheDocument();
    expect(screen.queryByText(/Cancel request acknowledged/)).not.toBeInTheDocument();
    expect(fake.callsTo("tasks.cancel")).toHaveLength(1);
    expect(fake.callsTo("tasks.cancel")[0]!.params).toMatchObject({ idempotency_key: expect.any(String) });

    await user.click(screen.getByRole("tab", { name: "Failed sessions (0)" }));
    await user.click(screen.getByRole("button", { name: /t-failed/ }));
    const retry = await screen.findByRole("button", { name: "Reassign via new attempt" });
    await waitFor(() => expect(retry).toBeEnabled());
    await user.click(retry);
    expect(await screen.findByText(/Reassigning the task failed/)).toBeInTheDocument();
    expect(screen.queryByText(/Reassignment accepted/)).not.toBeInTheDocument();
    expect(fake.callsTo("tasks.retry")).toHaveLength(1);
  });

  it("shows attempt history and the selected session's daemon event feed", async () => {
    const fake = seeded();
    fake.tasks[0] = mkTask("t-run", REPO, {
      state: "Running",
      attempts: [{ id: "attempt-run", task_id: "t-run", attempt_number: 1, state: "Running" }],
      sessions: [mkSession("s-1")],
    });
    fake.events = [mkEvent(1, "t-run", "s-1")];
    const { user } = await liveView(fake);
    await user.click(screen.getByRole("button", { name: /t-run/ }));
    const detail = within(screen.getByRole("complementary", { name: "Selected task / session detail" }));
    expect(detail.getByText("Attempt 1")).toBeInTheDocument();
    expect(detail.getByText("attempt-run")).toBeInTheDocument();
    await user.click(detail.getByText("Execution feed, side effects and supplied context"));
    const feed = within(await detail.findByRole("table", { name: "Execution feed" }));
    expect(feed.getByText("SessionOutput")).toBeInTheDocument();
    expect(feed.getByText('{"n":1}')).toBeInTheDocument();
    expect(fake.callsTo("events.subscribe").length).toBeGreaterThan(0);
    expect(fake.callsTo("sessions.events").map((call) => call.params)).toContainEqual(
      expect.objectContaining({ task_id: "t-run", session_id: "s-1" }),
    );
  });

  it("disables unsupported controls with reasons and applies Cancel only to nonterminal tasks", async () => {
    const fake = seeded();
    fake.tasks.push(mkTask("t-verify", REPO, { state: "Verifying" }));
    const { user } = await mountAgents(fake, { live: true });
    await screen.findByRole("tab", { name: "Active (2)" });
    await user.click(screen.getByRole("button", { name: /t-run/ }));
    const retry = await screen.findByRole("button", { name: "Reassign via new attempt" });
    await waitFor(() => expect(retry).toBeDisabled());
    expect(retry).toHaveAccessibleDescription(/\S/);
    for (const name of [
      "Configure providers",
      "Amend future TaskContract",
      "Request new decomposition",
      "Resume unavailable until capability verified",
      "Inspect retained checkpoint",
    ]) {
      const button = screen.getByRole("button", { name });
      expect(button).toBeDisabled();
      expect(button).toHaveAccessibleDescription(/not available|unavailable|not/i);
    }
    for (const summary of [
      "Scheduling, routing and resource policy · Later-scope preview",
      "Usage provenance and budget accounting · Later-scope preview",
    ]) {
      await user.click(screen.getByText(summary));
    }
    for (const name of ["Review routing proposal", "Choose manual assignment"]) {
      const button = screen.getByRole("button", { name });
      expect(button).toBeDisabled();
      expect(button).toHaveAccessibleDescription(/not available/i);
    }
    await user.click(screen.getByRole("button", { name: /t-verify/ }));
    expect(await screen.findByRole("button", { name: "Cancel attempt" })).toBeEnabled();
    await user.click(screen.getByRole("tab", { name: "Queued (1)" }));
    await user.click(screen.getByRole("button", { name: /t-queued/ }));
    expect(await screen.findByRole("button", { name: "Cancel attempt" })).toBeEnabled();
    await user.click(screen.getByRole("tab", { name: "Failed sessions (0)" }));
    await user.click(screen.getByRole("button", { name: /t-failed/ }));
    const cancel = await screen.findByRole("button", { name: "Cancel attempt" });
    expect(cancel).toBeDisabled();
    expect(cancel).toHaveAccessibleDescription(/The task is Failed/);
  });

  it("force-starts only a queued task, sending tasks.force_start for it", async () => {
    const { fake, user } = await liveView();
    const running = await screen.findByRole("button", { name: "Force start" });
    expect(running).toBeDisabled();
    expect(running).toHaveAccessibleDescription(/only a queued task can be force-started/);

    await user.click(screen.getByRole("tab", { name: "Queued (1)" }));
    await user.click(screen.getByRole("button", { name: /t-queued/ }));
    const force = await screen.findByRole("button", { name: "Force start" });
    await waitFor(() => expect(force).toBeEnabled());
    await user.click(force);

    expect(await screen.findByText(/The daemon accepted the force start/)).toBeInTheDocument();
    expect(fake.callsTo("tasks.force_start")).toHaveLength(1);
    expect(fake.callsTo("tasks.force_start")[0]!.params).toEqual({ task_id: "t-queued", idempotency_key: expect.any(String) });
  });

  it("offers no force start to a session that cannot send user-only methods", async () => {
    const fake = seeded();
    fake.approver = false;
    const { user } = await mountAgents(fake, { live: true });
    await user.click(await screen.findByRole("tab", { name: "Queued (1)" }));
    await user.click(screen.getByRole("button", { name: /t-queued/ }));
    const force = await screen.findByRole("button", { name: "Force start" });
    expect(force).toBeDisabled();
    expect(force).toHaveAccessibleDescription(/can force a start/);
    expect(fake.callsTo("tasks.force_start")).toHaveLength(0);
  });

  it("shows an empty state when there are no tasks", async () => {
    const fake = seeded();
    fake.tasks = [];
    await mountAgents(fake, { live: true });
    expect(await screen.findByText("No active tasks.")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Active (0)" })).toBeInTheDocument();
  });

  it("shows an empty state on the Queued and Failed tabs", async () => {
    const fake = seeded();
    fake.tasks = [mkTask("t-run", REPO, { state: "Running", sessions: [mkSession("s-1")] })];
    const { user } = await mountAgents(fake, { live: true });
    await user.click(await screen.findByRole("tab", { name: "Queued (0)" }));
    expect(screen.getByText("No queued tasks.")).toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "Failed sessions (0)" }));
    expect(screen.getByText(/^No failed sessions\./)).toBeInTheDocument();
  });

  it("never shows disconnected sessions as zero, even when no task is active", async () => {
    const fake = seeded();
    fake.tasks = [mkTask("t-queued", REPO, { state: "Queued" })];
    const { user } = await mountAgents(fake, { live: true });
    await user.click(await screen.findByRole("tab", { name: "Disconnected (not available)" }));
    expect(screen.getByText("Disconnected sessions are not available")).toBeInTheDocument();
    expect(screen.getByText("Disconnected sessions not available")).toBeInTheDocument();
    expect(screen.queryByText(/0 disconnected sessions/)).not.toBeInTheDocument();
  });

  it("agrees between the headline and the Queued tab when a queued task is blocked", async () => {
    const fake = seeded();
    fake.tasks = [
      mkTask("t-a", REPO, { state: "Queued" }),
      mkTask("t-b", REPO, { state: "Queued", contract: { dependencies: ["t-a"] } }),
    ];
    const { user } = await mountAgents(fake, { live: true });
    await user.click(await screen.findByRole("tab", { name: "Queued (2)" }));
    expect(screen.getByText("2 tasks queued (1 ready)")).toBeInTheDocument();
    expect(screen.getByText(/1 tasks blocked/)).toBeInTheDocument();
    expect(within(screen.getByRole("list", { name: "queued tasks" })).getAllByRole("button")).toHaveLength(2);
    expect(screen.getByText(/Queued tab lists every queued task, blocked ones included/)).toBeInTheDocument();
  });

  it("shows why each queued task has not started in the Queued tab", async () => {
    const fake = seeded();
    fake.tasks = [
      mkTask("t-a", REPO, { state: "Queued" }),
      mkTask("t-b", REPO, { state: "Queued", contract: { dependencies: ["t-a"] } }),
    ];
    fake.queue.dispatch_blocked = true;
    const { user } = await mountAgents(fake, { live: true });
    await user.click(await screen.findByRole("tab", { name: "Queued (2)" }));
    const list = screen.getByRole("list", { name: "queued tasks" });
    expect(await within(list).findByRole("button", { name: /t-a.*Restart gate blocking launches/ })).toBeInTheDocument();
    expect(within(list).getByRole("button", { name: /t-b.*Waiting for t-a/ })).toBeInTheDocument();
  });

  it("shows a retryable error when the task list cannot load", async () => {
    const fake = seeded();
    fake.failNext("tasks.list", { code: -32603, message: "list failed" }, 50);
    const { user } = await mountAgents(fake, { live: true });
    expect(await screen.findByText("Tasks could not be loaded")).toBeInTheDocument();
    expect(screen.getByText(/Execution status is unknown because the task list is not available/)).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Queued (unknown)" })).toBeInTheDocument();
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
    expect(within(screen.getByRole("tabpanel")).getByRole("button", { name: "Retry" })).toBeInTheDocument();
  });

  it("keeps the last rows and warns when a refresh fails", async () => {
    const fake = seeded();
    const { services } = await liveView(fake);
    fake.failNext("tasks.list", { code: -32603, message: "refresh failed" }, 50);
    await services.data.refreshAll();
    expect(await screen.findByText("The last refresh failed; these rows may be out of date.")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Active (1)" })).toBeInTheDocument();
  });

  it("shows sessions as unknown, not zero or none, while the session list cannot load", async () => {
    const fake = seeded();
    fake.failNext("sessions.list", { code: -32603, message: "sessions failed" }, 2);
    const { user } = await mountAgents(fake, { live: true });
    const detail = within(screen.getByRole("complementary", { name: "Selected task / session detail" }));
    expect(await detail.findByText(/Sessions unknown/)).toBeInTheDocument();
    expect(detail.queryByText("No session recorded.")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /t-run/ })).toHaveTextContent("sessions unknown");
    expect(screen.getByText("1 running task(s), unknown running session(s)")).toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "Failed sessions (unknown)" }));
    expect(screen.getByText("Failed sessions are unknown.")).toBeInTheDocument();
    expect(screen.queryByText(/No session recorded/)).not.toBeInTheDocument();
    expect(screen.queryByRole("tab", { name: "Failed sessions (0)" })).not.toBeInTheDocument();
    expect(screen.queryByRole("list", { name: "failed tasks without a session" })).not.toBeInTheDocument();
  });

  it("keeps the Unknown flag pill on one line", async () => {
    await liveView();
    const table = await screen.findByRole("table", { name: /inventory/i });
    const pills = within(table).getAllByText("Unknown");
    expect(pills.length).toBeGreaterThan(0);
    for (const pill of pills) expect(pill).toHaveClass("agents-flag");
  });

  it("shows the inventory error on the collapsed summary, so it is visible without opening", async () => {
    const fake = seeded();
    fake.failNext("agents.capabilities", { code: -32603, message: "probe failed" }, 5);
    await mountAgents(fake, { live: true });
    const inventory = screen.getByTestId("agents-inventory");
    expect(await within(inventory).findByText("error")).toBeInTheDocument();
    expect(inventory).not.toHaveAttribute("open");
  });

  it("shows a retryable error when the inventory cannot load", async () => {
    const fake = seeded();
    fake.failNext("agents.capabilities", { code: -32603, message: "probe failed" }, 5);
    const { user } = await mountAgents(fake, { live: true });
    const inventory = within(screen.getByTestId("agents-inventory"));
    await user.click(inventory.getByText("Configured inventory and discovered capabilities"));
    expect(await inventory.findByRole("alert")).toHaveTextContent("Adapter inventory could not be loaded");
    expect(inventory.getByRole("button", { name: "Retry" })).toBeInTheDocument();
  });
  it("never counts Available / idle workers, even when the daemon reports no adapters", async () => {
    const fake = seeded();
    fake.stub("agents.capabilities", []);
    const { user } = await mountAgents(fake, { live: true });
    const tab = await screen.findByRole("tab", { name: "Available / idle" });
    expect(tab.textContent).not.toMatch(/\d/);
    await user.click(tab);
    expect(screen.getByText("Available / idle workers are not available")).toBeInTheDocument();
  });

});

describe("Agents & Sessions, fixture mode", () => {
  it("labels fixture-backed values with PLACEHOLDER badges", async () => {
    await mountAgents(new FakeDaemon(), { bridge: false });
    expect((await screen.findAllByTestId("badge-D2")).length).toBeGreaterThan(0);
    expect((await screen.findAllByTestId("badge-D1")).length).toBeGreaterThan(0);
    expect(screen.getAllByText(/PLACEHOLDER/).length).toBeGreaterThan(0);
  });

  it("simulates cancel and retry without any daemon mutation", async () => {
    const fake = new FakeDaemon();
    const { user } = await mountAgents(fake, { bridge: false });
    await user.click(await screen.findByRole("button", { name: /fixture:task-running/ }));
    await user.click(await screen.findByRole("button", { name: "Simulate cancel attempt" }));
    expect(await screen.findAllByTestId("simulated-receipt")).toHaveLength(1);
    await user.click(screen.getByRole("tab", { name: /^Failed/ }));
    await user.click(screen.getByRole("button", { name: /fixture:task-failed/ }));
    const preview = await screen.findByRole("button", { name: "Preview reassign via new attempt" });
    await waitFor(() => expect(preview).toBeEnabled());
    await user.click(preview);
    expect(await screen.findAllByText(/new attempt/)).not.toHaveLength(0);
    expect(fake.callsTo("tasks.cancel")).toHaveLength(0);
    expect(fake.callsTo("tasks.retry")).toHaveLength(0);
  });

  it("offers a local trace filter and disables diagnostic and branch actions in the feed", async () => {
    const { user } = await mountAgents(new FakeDaemon(), { bridge: false });
    await user.click(await screen.findByRole("button", { name: /fixture:task-running/ }));
    const detail = within(screen.getByRole("complementary", { name: "Selected task / session detail" }));
    await user.click(detail.getByText("Execution feed, side effects and supplied context"));
    const filter = detail.getByRole("combobox", { name: "Filter raw trace" });
    await user.selectOptions(filter, "ExecutionStarted");
    expect(within(detail.getByRole("table", { name: "Execution feed" })).queryByText("agent.message")).not.toBeInTheDocument();
    expect(detail.getByRole("button", { name: "Request diagnostic" })).toBeDisabled();
    expect(detail.getByRole("button", { name: "Branch from retained evidence" })).toBeDisabled();
  });
});
