import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { FakeDaemon, mkSession, mkTask } from "../../../test/fakeDaemon";
import { mountAgents } from "./harness";

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

  it("keeps tab counts equal to the lists and shows disconnected as unknown", async () => {
    const { user } = await liveView();
    expect(screen.getByRole("tab", { name: "Queued (1)" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Failed (1)" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Available (1)" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Disconnected (unknown)" })).toBeInTheDocument();
    expect(within(screen.getByRole("list", { name: "active tasks" })).getAllByRole("button")).toHaveLength(1);

    await user.click(screen.getByRole("tab", { name: "Queued (1)" }));
    expect(within(screen.getByRole("list", { name: "queued tasks" })).getByText("t-queued")).toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "Failed (1)" }));
    expect(within(screen.getByRole("list", { name: "failed tasks" })).getByText("t-failed")).toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "Available (1)" }));
    expect(within(screen.getByRole("list", { name: "Available adapters" })).getByText("codex")).toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "Disconnected (unknown)" }));
    expect(screen.getByText("Disconnected sessions are unknown")).toBeInTheDocument();
    expect(screen.getByText(/disconnected sessions/)).toHaveTextContent(/unknown disconnected sessions/);
  });

  it("moves between tabs with the arrow keys, Home and End", async () => {
    const { user } = await liveView();
    screen.getByRole("tab", { name: "Active (1)" }).focus();
    await user.keyboard("{ArrowRight}");
    expect(screen.getByRole("tab", { name: "Available (1)" })).toHaveFocus();
    expect(screen.getByRole("tab", { name: "Available (1)" })).toHaveAttribute("aria-selected", "true");
    await user.keyboard("{End}");
    expect(screen.getByRole("tab", { name: "Disconnected (unknown)" })).toHaveFocus();
    await user.keyboard("{Home}");
    expect(screen.getByRole("tab", { name: "Active (1)" })).toHaveFocus();
    await user.keyboard("{ArrowLeft}");
    expect(screen.getByRole("tab", { name: "Disconnected (unknown)" })).toHaveFocus();
  });

  it("reports the daemon's exact installed and launchable flags and unknown usage", async () => {
    await liveView();
    const table = await screen.findByRole("table", { name: /inventory/i });
    expect(within(table).getByText("Installed and launchable")).toBeInTheDocument();
    expect(within(table).getByText("Installed, not launchable")).toBeInTheDocument();
    expect(within(table).getAllByText(/Unknown/).length).toBeGreaterThanOrEqual(2);
    expect(screen.getByRole("button", { name: "Configure providers" })).toBeDisabled();
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
    await user.click(await screen.findByRole("button", { name: "Request cancel" }));
    expect(await screen.findByText(/Cancel request acknowledged; the daemon now records the task as Cancelled/)).toBeInTheDocument();
    expect(screen.getByText(/not proof that any process has stopped/)).toBeInTheDocument();
    expect(fake.callsTo("tasks.cancel")).toHaveLength(1);
  });

  it("retries a failed task through tasks.retry", async () => {
    const { fake, user } = await liveView();
    await user.click(screen.getByRole("tab", { name: "Failed (1)" }));
    await user.click(screen.getByRole("button", { name: /t-failed/ }));
    const retry = await screen.findByRole("button", { name: "Retry task" });
    await waitFor(() => expect(retry).toBeEnabled());
    await user.click(retry);
    expect(await screen.findByText(/Retry accepted; the daemon now records the task as Queued/)).toBeInTheDocument();
    expect(fake.callsTo("tasks.retry")).toHaveLength(1);
  });

  it("disables retry and cancel with a reason when they cannot apply", async () => {
    const { user } = await liveView();
    await user.click(screen.getByRole("button", { name: /t-run/ }));
    const retry = await screen.findByRole("button", { name: "Retry task" });
    await waitFor(() => expect(retry).toBeDisabled());
    expect(retry).toHaveAccessibleDescription(/\S/);
    for (const name of ["Amend TaskContract", "Request new decomposition", "Resume session", "Inspect retained checkpoint"]) {
      const button = screen.getByRole("button", { name });
      expect(button).toBeDisabled();
      expect(button).toHaveAccessibleDescription(/Not available|unavailable|not/i);
    }
    await user.click(screen.getByRole("tab", { name: "Queued (1)" }));
    await user.click(screen.getByRole("button", { name: /t-queued/ }));
    expect(await screen.findByRole("button", { name: "Request cancel" })).toBeEnabled();
  });

  it("shows an empty state when there are no tasks", async () => {
    const fake = seeded();
    fake.tasks = [];
    await mountAgents(fake, { live: true });
    expect(await screen.findByText("No active tasks.")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Active (0)" })).toBeInTheDocument();
  });

  it("shows a retryable error when the inventory cannot load", async () => {
    const fake = seeded();
    fake.failNext("agents.capabilities", { code: -32603, message: "probe failed" }, 5);
    const { user } = await mountAgents(fake, { live: true });
    await user.click(await screen.findByRole("tab", { name: "Available (unknown)" }));
    expect(await screen.findAllByRole("button", { name: /Retry/ })).not.toHaveLength(0);
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
    await user.click(await screen.findByRole("button", { name: "Simulate cancel" }));
    expect(await screen.findAllByTestId("simulated-receipt")).toHaveLength(1);
    await user.click(screen.getByRole("tab", { name: /^Failed/ }));
    await user.click(screen.getByRole("button", { name: /fixture:task-failed/ }));
    const preview = await screen.findByRole("button", { name: "Preview retry" });
    await waitFor(() => expect(preview).toBeEnabled());
    await user.click(preview);
    expect(await screen.findAllByText(/new attempt/)).not.toHaveLength(0);
    expect(fake.callsTo("tasks.cancel")).toHaveLength(0);
    expect(fake.callsTo("tasks.retry")).toHaveLength(0);
  });
});
