import { screen, waitFor, within } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { DaemonError } from "../daemon/errors";
import { ALL_CAPABILITIES } from "../daemon/types";
import { createFixtureSources } from "../sources/fixtures";
import { FakeDaemon, mkApproval, mkEvent, mkTask, until } from "../test/fakeDaemon";
import { REPO, mount } from "../test/render";
import { TASK_PREFERENCE_KEY } from "./views/workspace/permissionLevel";

/** An incomplete history keeps the connection strip on screen over a healthy connection, so its notes can be read. */
const stripOpen = (fake: FakeDaemon) => {
  fake.pushStreamError(-32015, "slow");
  return screen.findByTestId("history-incomplete");
};
const statusConnected = () => waitFor(() => expect(screen.getByTestId("connection-status")).toHaveTextContent("Connected"));

describe("C1 connection strip", () => {
  it("shows Offline and the fixture demo banner when no connector is supplied", async () => {
    await mount(new FakeDaemon(), { bridge: false });
    expect(screen.getByTestId("connection-status")).toHaveTextContent("Offline");
    expect(screen.getByTestId("fixture-banner")).toHaveTextContent("FIXTURE DEMO MODE");
    expect(screen.getByRole("button", { name: "Refresh / reconnect" })).toBeDisabled();
    expect(screen.getByText("Offline: no cached projects. This is not an empty list.")).toBeInTheDocument();
  });

  it("shows the last durable sequence and labels journal continuity as placeholder, never proof", async () => {
    const fake = new FakeDaemon();
    fake.events = [mkEvent(1, "t", null), mkEvent(2, "t", null)];
    await mount(fake);
    await stripOpen(fake);
    await statusConnected();
    await waitFor(() => expect(screen.getByTestId("last-sequence")).toHaveTextContent("Last durable sequence: 2"));
    const continuity = screen.getByTestId("journal-continuity");
    await waitFor(() => expect(continuity).toHaveTextContent("never proof that history is complete"));
    expect(continuity).toHaveTextContent(/: unknown \(never proof that history is complete\)/);
    expect(continuity).not.toHaveTextContent("same");
    expect(within(continuity).getByTestId("badge-D6")).toHaveTextContent("PLACEHOLDER DATA");
  });

  it("says continuity is unknown when D6 is unavailable", async () => {
    const fake = new FakeDaemon();
    await mount(fake, { sources: createFixtureSources({ scenario: "unavailable" }) });
    await stripOpen(fake);
    await waitFor(() =>
      expect(screen.getByTestId("journal-continuity")).toHaveTextContent(
        "unknown (Journal identity is unavailable in this fixture scenario.)",
      ),
    );
  });

  it("reports authentication failure with a safe next step and no retry", async () => {
    const fake = new FakeDaemon();
    fake.openError = new DaemonError("auth", "bad", -32001);
    await mount(fake, { waitConnected: false });
    await waitFor(() => expect(screen.getByTestId("connection-status")).toHaveTextContent("Authentication failed"));
    expect(screen.getByTestId("connection-error")).toHaveTextContent(/rejected authentication/);
    expect(fake.channels).toHaveLength(0);
  });

  it("reports an incompatible daemon", async () => {
    const fake = new FakeDaemon();
    fake.openError = new DaemonError("version", "no shared version", -32003);
    await mount(fake, { waitConnected: false });
    await waitFor(() =>
      expect(screen.getByTestId("connection-status")).toHaveTextContent("Incompatible version or capability"),
    );
  });

  it("marks cached data stale and disables actions while the connection is lost", async () => {
    const fake = new FakeDaemon();
    fake.projects = [{ id: REPO }];
    await mount(fake);
    await screen.findByText("repo");
    fake.openError = new DaemonError("disconnected", "down");
    fake.drop();
    await waitFor(() => {
      const status = screen.getByTestId("connection-status");
      expect(status).toHaveTextContent("Reconnecting...");
      expect(status.querySelector(".conn-glyph")).toHaveTextContent("↻");
    });
    await waitFor(() => expect(screen.getByTestId("offline-note")).toHaveTextContent("Showing cached data (stale)"));
    expect(screen.getByTestId("stale-projects")).toBeInTheDocument();
    expect(screen.getByText("repo")).toBeInTheDocument();
  });

  it("flags incomplete history after a slow-consumer end and clears it only with a fresh reconnect", async () => {
    const fake = new FakeDaemon();
    const { user } = await mount(fake);
    fake.pushStreamError(-32015, "slow");
    expect(await screen.findByTestId("history-incomplete")).toHaveTextContent(/History incomplete \(slow-consumer\)/);
    await statusConnected();
    expect(screen.getByTestId("history-incomplete")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Refresh / reconnect" }));
    await waitFor(() => expect(screen.queryByTestId("history-incomplete")).not.toBeInTheDocument());
  });
});

describe("S1 project list", () => {
  it("shows an empty state that is distinct from offline", async () => {
    await mount(new FakeDaemon());
    expect(await screen.findByText("No repositories registered.")).toBeInTheDocument();
  });

  it("lists registered repositories with the full path and opens one", async () => {
    const fake = new FakeDaemon();
    fake.projects = [{ id: REPO }];
    const { user } = await mount(fake);
    const list = await screen.findByRole("list", { name: "Projects" });
    expect(within(list).getByText(REPO)).toBeInTheDocument();
    await user.click(within(list).getByRole("button", { name: "Open" }));
    expect(await screen.findByRole("heading", { name: "repo" })).toBeInTheDocument();
  });

  it("registers a repository and navigates to its detail", async () => {
    const fake = new FakeDaemon();
    const { user } = await mount(fake);
    await screen.findByText("No repositories registered.");
    await user.type(screen.getByLabelText(/Register repository/), "/work/new-repo");
    await user.click(screen.getByRole("button", { name: "Register" }));
    expect(await screen.findByRole("heading", { name: "new-repo" })).toBeInTheDocument();
    expect(fake.callsTo("projects.register")[0]!.params).toEqual({ path: "/work/new-repo" });
  });

  it("rejects a relative path locally without calling the daemon", async () => {
    const fake = new FakeDaemon();
    const { user } = await mount(fake);
    await user.type(screen.getByLabelText(/Register repository/), "relative/path");
    await user.click(screen.getByRole("button", { name: "Register" }));
    expect(await screen.findByText("Enter an absolute repository path.")).toBeInTheDocument();
    expect(fake.callsTo("projects.register")).toHaveLength(0);
  });

  it("shows the daemon's reason and a next step when registration fails, keeping the draft", async () => {
    const fake = new FakeDaemon();
    const { user } = await mount(fake);
    fake.failNext("projects.register", { code: -32602, message: "not a git repository" });
    fake.delayNext("projects.register", 150);
    await user.type(screen.getByLabelText(/Register repository/), "/work/not-a-repo");
    await user.click(screen.getByRole("button", { name: "Register" }));
    expect(screen.getByRole("button", { name: "Register" })).toHaveAttribute("aria-busy", "true");
    const alert = await screen.findByText(/Registering the repository failed/);
    expect(alert.parentElement).toHaveTextContent(/rejected the request as invalid/);
    expect(alert.parentElement).toHaveTextContent("not a git repository");
    expect(screen.getByLabelText(/Register repository/)).toHaveValue("/work/not-a-repo");
  });

  it("shows a load error with retry that recovers", async () => {
    const fake = new FakeDaemon();
    fake.projects = [{ id: REPO }];
    fake.failNext("projects.list", { code: -32603, message: "boom" });
    const { user } = await mount(fake);
    expect(await screen.findByText(/Loading projects failed/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByText("repo")).toBeInTheDocument();
  });

  it("keeps the registration draft and disables Register while offline", async () => {
    const fake = new FakeDaemon();
    const { user } = await mount(fake);
    await user.type(screen.getByLabelText(/Register repository/), "/work/later");
    fake.openError = new DaemonError("disconnected", "down");
    fake.drop();
    await waitFor(() => expect(screen.getByTestId("connection-status")).not.toHaveTextContent("Connected"));
    expect(screen.getByRole("button", { name: "Register" })).toBeDisabled();
    expect(screen.getByText(/unavailable while offline; the draft is kept/)).toBeInTheDocument();
    expect(screen.getByLabelText(/Register repository/)).toHaveValue("/work/later");
  });
});

describe("S2 project page and new task", () => {
  const route = { name: "project", projectId: REPO } as const;

  function seeded() {
    const fake = new FakeDaemon();
    fake.projects = [{ id: REPO }];
    fake.tasks = [
      mkTask("t-q", REPO),
      mkTask("t-r", REPO, { state: "Running" }),
      mkTask("t-d", REPO, { state: "Succeeded" }),
      mkTask("t-other", "/other", { state: "Running" }),
    ];
    fake.approvals = [mkApproval("a1", "t-r"), mkApproval("a2", "t-other")];
    return fake;
  }

  const openDialog = async (user: Awaited<ReturnType<typeof mount>>["user"]) => {
    await user.click(await screen.findByRole("button", { name: "New task" }));
    return screen.findByRole("dialog", { name: "New task" });
  };

  it("opens on the task list with one primary New task button and sub-navigation tabs", async () => {
    await mount(seeded(), { route });
    expect(await screen.findByRole("table", { name: "Tasks" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 1, name: "repo" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Global tasks" })).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: "New task" })).toHaveLength(1);
    const tabs = within(screen.getByRole("tablist", { name: "Project pages" })).getAllByRole("tab");
    // Defaults appears only when the daemon can read project defaults (see ProjectDetailDefaults.test).
    expect(tabs.map((tab) => tab.textContent)).toEqual(["Tasks", "Coordinator"]);
    expect(screen.getByRole("tab", { name: "Tasks" })).toHaveAttribute("aria-selected", "true");
    // No always-open form and none of the removed panels.
    expect(screen.queryByLabelText("Objective")).not.toBeInTheDocument();
    expect(screen.queryByText("Task counts")).not.toBeInTheDocument();
    expect(screen.queryByText(/Agent detection/)).not.toBeInTheDocument();
  });

  it("summarizes this project's tasks and awaiting review in the header", async () => {
    await mount(seeded(), { route });
    expect(await screen.findByTestId("task-total")).toHaveTextContent("3 tasks · 1 awaiting review");
  });

  it("offers Pending review in the actions menu, counted for this project only", async () => {
    const { user } = await mount(seeded(), { route });
    await screen.findByRole("table", { name: "Tasks" });
    await user.click(screen.getByRole("button", { name: "Project actions" }));
    expect(screen.getByRole("menuitem", { name: "Pending review (1)" })).toBeInTheDocument();
  });

  it("does not read pending review as a count while the daemon is unreachable", async () => {
    const fake = seeded();
    fake.openError = new DaemonError("disconnected", "down");
    const { user } = await mount(fake, { route, waitConnected: false });
    await user.click(await screen.findByRole("button", { name: "Project actions" }));
    expect(screen.getByRole("menuitem", { name: "Pending review" })).toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: /\(0\)/ })).not.toBeInTheDocument();
  });

  it("does not read pending review as 0 when the task list fails but approvals loaded", async () => {
    const fake = seeded();
    fake.failNext("tasks.list", { code: -32603, message: "boom" }, 5);
    const { services, user } = await mount(fake, { route });
    await waitFor(() => expect(services.data.getSnapshot().tasks.status).toBe("error"));
    await waitFor(() => expect(services.data.getSnapshot().approvals.status).toBe("ready"));
    await user.click(screen.getByRole("button", { name: "Project actions" }));
    expect(screen.queryByRole("menuitem", { name: /\(0\)/ })).not.toBeInTheDocument();
  });

  it("opens the New task form in a dialog with kind, agent and delivery up front and More options collapsed", async () => {
    const { user } = await mount(seeded(), { route });
    const dialog = await openDialog(user);
    expect(within(dialog).getByLabelText("Objective")).toBeInTheDocument();
    expect(within(dialog).getByLabelText("Kind")).toBeInTheDocument();
    expect(within(dialog).getByLabelText("Agent")).toHaveValue("");
    expect(within(dialog).getByLabelText("Delivery")).toBeInTheDocument();
    const more = within(dialog).getByText("More options").closest("details")!;
    expect(more).not.toHaveAttribute("open");
    for (const label of ["Autonomy", "Verification", "Max retries", "Max repairs"]) {
      expect(more).toContainElement(within(dialog).getByLabelText(label));
    }
    // The upfront fields are not in the collapsed part.
    for (const label of ["Objective", "Kind", "Agent", "Delivery"]) {
      expect(more).not.toContainElement(within(dialog).getByLabelText(label));
    }
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("submits a task, navigates to its detail and states that no agent was launched", async () => {
    const fake = seeded();
    const { user } = await mount(fake, { route });
    await openDialog(user);
    await user.type(screen.getByLabelText("Objective"), "Add a login page");
    await user.type(screen.getByLabelText("Agent"), "codex");
    await user.click(screen.getByRole("button", { name: "Create task" }));
    expect(await screen.findByTestId("task-state")).toHaveTextContent("Queued");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.getByText(/no agent has been launched and no session is recorded/)).toBeInTheDocument();
    const params = fake.callsTo("tasks.submit")[0]!.params as { contract: Record<string, unknown>; idempotency_key: string };
    expect(params.contract).toMatchObject({
      project_id: REPO,
      objective: "Add a login page",
      agent: "codex",
      autonomy: "ApprovalRequired",
      delivery: "LocalReview",
      max_retries: 1,
      max_repairs: 1,
    });
    expect(params.contract).not.toHaveProperty("id");
    expect(params.idempotency_key).toBeTruthy();
  });

  describe("permission level", () => {
    beforeEach(() => window.localStorage.clear());

    it("starts at Auto and sends it with the task", async () => {
      const fake = seeded();
      const { user } = await mount(fake, { route });
      await openDialog(user);
      expect(screen.getByRole("combobox", { name: "Permission level" })).toHaveValue("auto");
      await user.type(screen.getByLabelText("Objective"), "Add a login page");
      await user.click(screen.getByRole("button", { name: "Create task" }));
      await screen.findByTestId("task-state");
      expect(fake.callsTo("tasks.submit")[0]!.params).toMatchObject({ contract: { permission: "auto" } });
    });

    it("saves a Restricted task choice under the task's own key only", async () => {
      const fake = seeded();
      const { user } = await mount(fake, { route });
      await openDialog(user);
      await user.selectOptions(screen.getByRole("combobox", { name: "Permission level" }), "restricted");
      expect(window.localStorage.getItem(TASK_PREFERENCE_KEY)).toBe("restricted");
      await user.type(screen.getByLabelText("Objective"), "Add a login page");
      await user.click(screen.getByRole("button", { name: "Create task" }));
      await screen.findByTestId("task-state");
      expect(fake.callsTo("tasks.submit")[0]!.params).toMatchObject({ contract: { permission: "restricted" } });
    });

    it("asks before a Full task, and sends Full only after the user confirms", async () => {
      const fake = seeded();
      const { user } = await mount(fake, { route });
      await openDialog(user);
      await user.selectOptions(screen.getByRole("combobox", { name: "Permission level" }), "full");
      const confirm = await screen.findByRole("dialog", { name: "Run this task at Full?" });
      await user.click(within(confirm).getByRole("button", { name: "Use Full" }));
      expect(screen.getByRole("combobox", { name: "Permission level" })).toHaveValue("full");
      expect(window.localStorage.getItem(TASK_PREFERENCE_KEY)).toBeNull();
      await user.type(screen.getByLabelText("Objective"), "Add a login page");
      await user.click(screen.getByRole("button", { name: "Create task" }));
      await screen.findByTestId("task-state");
      expect(fake.callsTo("tasks.submit")[0]!.params).toMatchObject({ contract: { permission: "full" } });
    });

    it("sends no level and reads Restricted on an older daemon", async () => {
      const fake = seeded();
      fake.capabilitiesOverride = ALL_CAPABILITIES.filter((capability) => capability !== "permission_levels");
      const { user } = await mount(fake, { route });
      await openDialog(user);
      expect(screen.getByRole("combobox", { name: "Permission level" })).toHaveValue("restricted");
      await user.type(screen.getByLabelText("Objective"), "Add a login page");
      await user.click(screen.getByRole("button", { name: "Create task" }));
      await screen.findByTestId("task-state");
      expect(fake.callsTo("tasks.submit")[0]!.params).not.toHaveProperty("contract.permission");
    });
  });

  it("submits a task for the home directory with ReportOnly delivery and does not offer another", async () => {
    const fake = seeded();
    fake.projects = [{ id: REPO, in_place: true }];
    const { user } = await mount(fake, { route });
    await openDialog(user);
    await user.type(screen.getByLabelText("Objective"), "List my notes");
    await user.type(screen.getByLabelText("Agent"), "codex");
    expect(screen.queryByLabelText("Delivery")).not.toBeInTheDocument();
    expect(screen.getByText(/This folder is the home directory/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Create task" }));
    await until(() => fake.callsTo("tasks.submit").length === 1, "submitted");
    const params = fake.callsTo("tasks.submit")[0]!.params as { contract: Record<string, unknown> };
    expect(params.contract).toMatchObject({ project_id: REPO, delivery: "ReportOnly" });
  });

  it("does not offer ReadOnly autonomy, which the daemon rejects", async () => {
    const { user } = await mount(seeded(), { route });
    await openDialog(user);
    const offered = within(screen.getByLabelText("Autonomy")).getAllByRole("option").map((option) => option.textContent);
    expect(offered).toEqual(["WorkspaceWrites", "ApprovalRequired"]);
  });

  it("explains local validation failures and does not call the daemon", async () => {
    const fake = seeded();
    const { user } = await mount(fake, { route });
    await openDialog(user);
    await user.click(screen.getByRole("button", { name: "Create task" }));
    expect(await screen.findByText("Objective is required.")).toBeInTheDocument();
    expect(screen.queryByText(/Agent is required/)).not.toBeInTheDocument();
    expect(fake.callsTo("tasks.submit")).toHaveLength(0);
  });

  it("shows the daemon's reason for a rejected submission and keeps the draft", async () => {
    const fake = seeded();
    const { user } = await mount(fake, { route });
    await openDialog(user);
    fake.failNext("tasks.submit", { code: -32010, message: "project is not registered" });
    fake.delayNext("tasks.submit", 150);
    await user.type(screen.getByLabelText("Objective"), "x");
    await user.type(screen.getByLabelText("Agent"), "codex");
    await user.click(screen.getByRole("button", { name: "Create task" }));
    expect(screen.getByRole("button", { name: "Create task" })).toHaveAttribute("aria-busy", "true");
    expect(await screen.findByText(/Submitting the task failed/)).toBeInTheDocument();
    expect(screen.getByLabelText("Objective")).toHaveValue("x");
  });

  it("reuses the request key when the previous outcome is unknown", async () => {
    const fake = seeded();
    const { user } = await mount(fake, { route, connection: { requestTimeoutMs: 40 } });
    await openDialog(user);
    fake.failNext("tasks.submit", "hang");
    await user.type(screen.getByLabelText("Objective"), "x");
    await user.type(screen.getByLabelText("Agent"), "codex");
    await user.click(screen.getByRole("button", { name: "Create task" }));
    expect(await screen.findByText(/The outcome is unknown/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Create task" }));
    await screen.findByTestId("task-state");
    const keys = fake.callsTo("tasks.submit").map((c) => (c.params as { idempotency_key: string }).idempotency_key);
    expect(keys).toHaveLength(2);
    expect(keys[0]).toBe(keys[1]);
  });

  it("keeps the form draft after closing the dialog and moving between tabs", async () => {
    const { user } = await mount(seeded(), { route });
    await openDialog(user);
    await user.type(screen.getByLabelText("Objective"), "keep me");
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    await user.click(screen.getByRole("tab", { name: "Coordinator" }));
    await screen.findByRole("tabpanel", { name: "Coordinator" });
    await user.click(screen.getByRole("tab", { name: "Tasks" }));
    await openDialog(user);
    expect(await screen.findByLabelText("Objective")).toHaveValue("keep me");
  });

  it("disables creating while offline and says the draft is kept", async () => {
    const fake = seeded();
    const { user } = await mount(fake, { route });
    await openDialog(user);
    await user.type(screen.getByLabelText("Objective"), "offline work");
    fake.openError = new DaemonError("disconnected", "down");
    fake.drop();
    await waitFor(() => expect(screen.getByRole("button", { name: "Create task" })).toBeDisabled());
    expect(screen.getByText(/Offline: the draft is kept/)).toBeInTheDocument();
    expect(screen.getByLabelText("Objective")).toHaveValue("offline work");
  });

  it("shows Commands verification inputs only when selected", async () => {
    const { user } = await mount(seeded(), { route });
    await openDialog(user);
    expect(screen.queryByLabelText(/Verification commands/)).not.toBeInTheDocument();
    await user.selectOptions(screen.getByLabelText("Verification"), "Commands");
    expect(screen.getByLabelText(/Verification commands/)).toBeInTheDocument();
  });
});
