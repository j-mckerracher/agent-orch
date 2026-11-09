import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { DaemonError } from "../daemon/errors";
import { createFixtureSources } from "../sources/fixtures";
import { FakeDaemon, mkApproval, mkEvent, mkSession, mkTask } from "../test/fakeDaemon";
import { REPO, mount } from "../test/render";

const tasksRoute = (filter: "all" | "queued" | "running" | "blocked" | "review" | "terminal" = "all") =>
  ({ name: "tasks", projectId: REPO, filter }) as const;
const taskRoute = (taskId: string) => ({ name: "task", projectId: REPO, taskId }) as const;

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

  it("keeps the execution preview separate and labelled as placeholder data", async () => {
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
});

describe("S4 task and session detail", () => {
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
    expect(await screen.findByText(/Cancel request acknowledged; the daemon now records the task as Cancelled\. This is not proof that any process has stopped\./)).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("button", { name: "Request cancel" })).toBeDisabled());
    expect(fake.callsTo("tasks.cancel")).toHaveLength(1);
  });

  it("shows the daemon's reason when cancel fails", async () => {
    const fake = seeded();
    const { user } = await mount(fake, { route: taskRoute("t-q") });
    await screen.findByTestId("task-state");
    fake.failNext("tasks.cancel", { code: -32011, message: "task already terminal" });
    await user.click(screen.getByRole("button", { name: "Request cancel" }));
    expect(await screen.findByText(/Cancelling the task failed/)).toBeInTheDocument();
    expect(screen.queryByText(/Cancel request acknowledged/)).not.toBeInTheDocument();
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

  it("labels retry, outcome and recovery panels as placeholder data and previews retry as simulated", async () => {
    const { user } = await mount(seeded(), { route: taskRoute("t-b") });
    await screen.findByTestId("task-state");
    for (const id of ["D2", "D3", "D4"]) {
      await waitFor(() => expect(screen.getAllByTestId(`badge-${id}`).length).toBeGreaterThan(0));
    }
    expect(screen.getByRole("region", { name: "Recovery classification (placeholder)" })).toBeInTheDocument();
    const retry = await screen.findByRole("button", { name: "Preview retry" });
    if (!retry.hasAttribute("disabled")) {
      await user.click(retry);
      expect(await screen.findByTestId("simulated-receipt")).toHaveTextContent("SIMULATED");
    }
  });

  it("renders a D2 unavailable state without hiding the real task", async () => {
    await mount(seeded(), { route: taskRoute("t-q"), sources: createFixtureSources({ scenario: "unavailable" }) });
    expect(await screen.findByTestId("task-state")).toHaveTextContent("Queued");
    const section = await screen.findByRole("region", { name: "Execution outcome (placeholder)" });
    expect(await within(section).findByText(/Unavailable:/)).toBeInTheDocument();
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
    for (const receipt of screen.getAllByTestId("simulated-receipt")) expect(receipt).toHaveTextContent("SIMULATED");
    expect(fake.callsTo("tasks.submit")).toHaveLength(0);
  });

  it("lists related real tasks and pending decisions and navigates to them", async () => {
    const { user } = await mount(seeded(), { route, renderReview: ({ taskId }) => <p>review {taskId}</p> });
    const related = await screen.findByRole("list", { name: "Related tasks" });
    expect(within(related).getAllByRole("listitem")).toHaveLength(4);
    expect(within(related).queryByText("t-other")).not.toBeInTheDocument();
    await user.click(within(screen.getByRole("list", { name: "Pending decisions" })).getByRole("button", { name: "Open review" }));
    expect(await screen.findByText("review t-r")).toBeInTheDocument();
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
