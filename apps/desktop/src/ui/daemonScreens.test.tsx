import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { FakeDaemon, mkTask } from "../test/fakeDaemon";
import { REPO, mount } from "../test/render";

function seeded() {
  const fake = new FakeDaemon();
  fake.projects = [{ id: REPO }];
  fake.tasks = [mkTask("t-failed", REPO, { state: "Failed" }), mkTask("t-q", REPO)];
  fake.stub("agents.capabilities", [{ agent: "codex", installed: true, launchable: true, capabilities: null }]);
  fake.stub("tasks.retry", (params: Record<string, unknown>) => {
    const taskId = String(params.task_id);
    const queued = mkTask(taskId, REPO, {
      state: "Queued",
      attempts: [{ id: "attempt-1", task_id: taskId, attempt_number: 1, state: "Pending" }],
    });
    const index = fake.tasks.findIndex((task) => task.contract.id === taskId);
    if (index >= 0) fake.tasks[index] = queued;
    return queued;
  });
  return fake;
}

describe("daemon-backed D1-D6 screens", () => {
  it("shows project detection without placeholder or simulated wording", async () => {
    await mount(seeded(), { route: { name: "project", projectId: REPO }, daemonSources: true });
    expect(await screen.findByText(/Codex: installed/)).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "Agent detection" })).toBeInTheDocument();
    const screens = within(screen.getByRole("main"));
    expect(screens.queryByText(/placeholder/i)).not.toBeInTheDocument();
    expect(screens.queryByText(/simulated/i)).not.toBeInTheDocument();
  });

  it("hides the fixture-only execution preview", async () => {
    await mount(seeded(), { route: { name: "tasks", projectId: REPO, filter: "all" }, daemonSources: true });
    await screen.findByRole("table", { name: "Tasks" });
    expect(screen.queryByRole("region", { name: /Execution preview/ })).not.toBeInTheDocument();
  });

  it("retries a failed task through tasks.retry and shows a plain status", async () => {
    const fake = seeded();
    const { user } = await mount(fake, {
      route: { name: "task", projectId: REPO, taskId: "t-failed" },
      daemonSources: true,
    });
    const button = await screen.findByRole("button", { name: "Retry task" });
    await waitFor(() => expect(button).toBeEnabled());
    expect(screen.queryByRole("button", { name: "Preview retry" })).not.toBeInTheDocument();
    await user.click(button);
    expect(await screen.findByText("The retry request was accepted by the daemon.")).toBeInTheDocument();
    await waitFor(() => expect(screen.getByTestId("task-state")).toHaveTextContent("Queued"));
    const attempts = within(screen.getByRole("region", { name: "Attempts" }));
    expect(attempts.getAllByRole("listitem")[0]).toHaveTextContent("#1 attempt-1: Pending");
    expect(fake.callsTo("tasks.retry")).toHaveLength(1);
    expect(screen.queryByText(/simulated/i)).not.toBeInTheDocument();
  });

  it("keeps retry acknowledgement historical when the latest task state is failed", async () => {
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
    const { user } = await mount(fake, {
      route: { name: "task", projectId: REPO, taskId: "t-failed" },
      daemonSources: true,
    });
    const button = await screen.findByRole("button", { name: "Retry task" });
    await waitFor(() => expect(button).toBeEnabled());
    await user.click(button);

    expect(await screen.findByText("The retry request was accepted by the daemon.")).toBeInTheDocument();
    await waitFor(() => expect(screen.getByTestId("task-state")).toHaveTextContent("Failed"));
    const attempts = within(screen.getByRole("region", { name: "Attempts" }));
    expect(attempts.getAllByRole("listitem")).toHaveLength(2);
    expect(attempts.getAllByRole("listitem")[0]).toHaveTextContent("#1 attempt-1: Failed");
    expect(attempts.getAllByRole("listitem")[1]).toHaveTextContent("#2 attempt-2: Failed");
    expect(screen.queryByText(/daemon now records the task as Queued with 1 attempt/)).not.toBeInTheDocument();
  });

  it("disables retry for a task the daemon would refuse", async () => {
    await mount(seeded(), { route: { name: "task", projectId: REPO, taskId: "t-q" }, daemonSources: true });
    expect(await screen.findByText(/only Failed or VerificationFailed tasks can be retried/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Retry task" })).toBeDisabled();
  });

  it("sends a coordinator message and validates a proposal without any simulated receipt", async () => {
    const fake = seeded();
    const entries: unknown[] = [];
    fake.stub("coordinator.get", () => ({ project_id: REPO, entries, more: false, next_after_sequence: entries.length }));
    fake.stub("coordinator.send_message", (params: Record<string, unknown>) => {
      const entry = {
        sequence: 1,
        id: "e1",
        project_id: REPO,
        kind: "message",
        author: "user",
        body: params.text,
        validation: null,
        created_at: "now",
      };
      entries.push(entry);
      return entry;
    });
    const { user } = await mount(fake, { route: { name: "coordinator", projectId: REPO }, daemonSources: true });
    expect(screen.queryByText(/conversation service is unavailable/)).not.toBeInTheDocument();
    await user.type(await screen.findByLabelText("Message or proposal JSON"), "plan the work");
    await user.click(screen.getByRole("button", { name: "Send message" }));
    expect(await screen.findByText("Message recorded. Nothing acts on it automatically.")).toBeInTheDocument();
    expect(await screen.findByText(/plan the work/, { selector: "li" })).toBeInTheDocument();
    expect(fake.callsTo("coordinator.send_message")).toHaveLength(1);
    expect(screen.queryByText(/simulated/i)).not.toBeInTheDocument();
  });

  it("shows the real error when the coordinator rejects a proposal", async () => {
    const fake = seeded();
    fake.stub("coordinator.get", { project_id: REPO, entries: [], more: false, next_after_sequence: 0 });
    const { user } = await mount(fake, { route: { name: "coordinator", projectId: REPO }, daemonSources: true });
    await user.type(await screen.findByLabelText("Message or proposal JSON"), "{{}");
    await user.click(screen.getByRole("button", { name: "Validate proposal" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(/Contacting the coordinator failed/);
  });

  it("shows the journal identity from the real Welcome epoch", async () => {
    const fake = seeded();
    await mount(fake, { route: { name: "project", projectId: REPO }, daemonSources: true });
    // A healthy connection hides the strip; an incomplete history brings it back with the continuity note.
    fake.pushStreamError(-32015, "slow");
    const continuity = await screen.findByTestId("journal-continuity");
    await waitFor(() => expect(continuity).toHaveTextContent("never proof that history is complete"));
    expect(continuity).not.toHaveTextContent(/placeholder/i);
  });
});
