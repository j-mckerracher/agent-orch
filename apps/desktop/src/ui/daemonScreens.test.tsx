import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { FakeDaemon, mkTask } from "../test/fakeDaemon";
import { REPO, mount } from "../test/render";

function seeded() {
  const fake = new FakeDaemon();
  fake.projects = [{ id: REPO }];
  fake.tasks = [mkTask("t-failed", REPO, { state: "Failed" }), mkTask("t-q", REPO)];
  fake.stub("agents.capabilities", [{ agent: "codex", installed: true, launchable: true, capabilities: null }]);
  fake.stub("tasks.retry", (params: Record<string, unknown>) => ({
    ...mkTask(String(params.task_id), REPO),
    state: "Queued",
  }));
  return fake;
}

describe("daemon-backed D1-D6 screens", () => {
  it("shows project detection without placeholder or simulated wording", async () => {
    await mount(seeded(), { route: { name: "project", projectId: REPO }, daemonSources: true });
    expect(await screen.findByText(/codex: installed/)).toBeInTheDocument();
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
    expect(await screen.findByText(/Retry accepted; the daemon now records the task as Queued/)).toBeInTheDocument();
    expect(fake.callsTo("tasks.retry")).toHaveLength(1);
    expect(screen.queryByText(/simulated/i)).not.toBeInTheDocument();
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
    await mount(seeded(), { route: { name: "project", projectId: REPO }, daemonSources: true });
    const continuity = await screen.findByTestId("journal-continuity");
    await waitFor(() => expect(continuity).toHaveTextContent("never proof that history is complete"));
    expect(continuity).not.toHaveTextContent(/placeholder/i);
  });
});
