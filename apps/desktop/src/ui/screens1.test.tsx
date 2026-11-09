import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { DaemonError } from "../daemon/errors";
import { createFixtureSources } from "../sources/fixtures";
import type { AgentCapabilitiesSource, Sources } from "../sources/types";
import { FakeDaemon, mkApproval, mkEvent, mkTask, until } from "../test/fakeDaemon";
import { REPO, connected, mount } from "../test/render";

const fixture = createFixtureSources();

describe("C1 connection strip", () => {
  it("shows Offline and the unbound bridge when the host supplies no connector", async () => {
    await mount(new FakeDaemon(), { bridge: false });
    expect(screen.getByTestId("connection-status")).toHaveTextContent("Offline");
    expect(screen.getByText(/Native bridge not bound/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Refresh / reconnect" })).toBeDisabled();
    expect(screen.getByText("Offline: no cached projects. This is not an empty list.")).toBeInTheDocument();
  });

  it("shows the last durable sequence and labels journal continuity as placeholder, never proof", async () => {
    const fake = new FakeDaemon();
    fake.events = [mkEvent(1, "t", null), mkEvent(2, "t", null)];
    await mount(fake);
    await waitFor(() => expect(screen.getByTestId("last-sequence")).toHaveTextContent("Last durable sequence: 2"));
    const continuity = screen.getByTestId("journal-continuity");
    await waitFor(() => expect(continuity).toHaveTextContent("never proof that history is complete"));
    expect(continuity).toHaveTextContent(/: unknown \(never proof that history is complete\)/);
    expect(continuity).not.toHaveTextContent("same");
    expect(within(continuity).getByTestId("badge-D6")).toHaveTextContent("PLACEHOLDER DATA");
  });

  it("says continuity is unknown when D6 is unavailable", async () => {
    await mount(new FakeDaemon(), { sources: createFixtureSources({ scenario: "unavailable" }) });
    await waitFor(() =>
      expect(screen.getByTestId("journal-continuity")).toHaveTextContent("unknown (no store identity in M5-02)"),
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
    await waitFor(() => expect(screen.getByTestId("offline-note")).toHaveTextContent("Showing cached data (stale)"));
    expect(screen.getByTestId("stale-projects")).toBeInTheDocument();
    expect(screen.getByText("repo")).toBeInTheDocument();
  });

  it("flags incomplete history after a slow-consumer end and clears it only with a fresh reconnect", async () => {
    const fake = new FakeDaemon();
    const { user } = await mount(fake);
    fake.pushStreamError(-32015, "slow");
    expect(await screen.findByTestId("history-incomplete")).toHaveTextContent(/History incomplete \(slow-consumer\)/);
    await connected();
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
    await user.type(screen.getByLabelText(/Register repository/), "/work/not-a-repo");
    await user.click(screen.getByRole("button", { name: "Register" }));
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

describe("S2 project detail and submission", () => {
  const route = { name: "project", projectId: REPO } as const;

  function seeded() {
    const fake = new FakeDaemon();
    fake.projects = [{ id: REPO }];
    fake.tasks = [
      mkTask("t-q", REPO),
      mkTask("t-r", REPO, { state: "Running" }),
      mkTask("t-other", "/other", { state: "Running" }),
    ];
    fake.approvals = [mkApproval("a1", "t-r"), mkApproval("a2", "t-other")];
    return fake;
  }

  it("derives per-state counts for this project only and links to a filtered list", async () => {
    const { user } = await mount(seeded(), { route });
    expect(await screen.findByTestId("task-total")).toHaveTextContent("2 tasks in this project");
    expect(screen.getByRole("button", { name: "Queued: 1" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Failed: 0" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Pending review (1)" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Running: 1" }));
    const table = await screen.findByRole("table", { name: "Tasks" });
    expect(within(table).getByText("t-r")).toBeInTheDocument();
    expect(within(table).queryByText("t-q")).not.toBeInTheDocument();
  });

  it("submits a task, navigates to its detail and states that no agent was launched", async () => {
    const fake = seeded();
    const { user } = await mount(fake, { route });
    await user.type(screen.getByLabelText("Objective"), "Add a login page");
    await user.type(screen.getByLabelText("Agent"), "codex");
    await user.click(screen.getByRole("button", { name: "Submit" }));
    expect(await screen.findByTestId("task-state")).toHaveTextContent("Queued");
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

  it("explains local validation failures and does not call the daemon", async () => {
    const fake = seeded();
    const { user } = await mount(fake, { route });
    await user.click(screen.getByRole("button", { name: "Submit" }));
    expect(await screen.findByText("Objective is required.")).toBeInTheDocument();
    expect(screen.getByText(/Agent is required; no agent is chosen automatically/)).toBeInTheDocument();
    expect(fake.callsTo("tasks.submit")).toHaveLength(0);
  });

  it("shows the daemon's reason for a rejected submission and keeps the draft", async () => {
    const fake = seeded();
    const { user } = await mount(fake, { route });
    fake.failNext("tasks.submit", { code: -32010, message: "project is not registered" });
    await user.type(screen.getByLabelText("Objective"), "x");
    await user.type(screen.getByLabelText("Agent"), "a");
    await user.click(screen.getByRole("button", { name: "Submit" }));
    expect(await screen.findByText(/Submitting the task failed/)).toBeInTheDocument();
    expect(screen.getByLabelText("Objective")).toHaveValue("x");
  });

  it("reuses the request key when the previous outcome is unknown", async () => {
    const fake = seeded();
    const { user } = await mount(fake, { route, connection: { requestTimeoutMs: 40 } });
    fake.failNext("tasks.submit", "hang");
    await user.type(screen.getByLabelText("Objective"), "x");
    await user.type(screen.getByLabelText("Agent"), "a");
    await user.click(screen.getByRole("button", { name: "Submit" }));
    expect(await screen.findByText(/The outcome is unknown/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Submit" }));
    await screen.findByTestId("task-state");
    const keys = fake.callsTo("tasks.submit").map((c) => (c.params as { idempotency_key: string }).idempotency_key);
    expect(keys).toHaveLength(2);
    expect(keys[0]).toBe(keys[1]);
  });

  it("keeps the form draft across navigation", async () => {
    const { user } = await mount(seeded(), { route });
    await user.type(screen.getByLabelText("Objective"), "keep me");
    await user.click(screen.getByRole("button", { name: "Coordinator" }));
    await screen.findByRole("heading", { name: "Coordinator" });
    await user.click(screen.getByRole("link", { name: "repo" }));
    expect(await screen.findByLabelText("Objective")).toHaveValue("keep me");
  });

  it("disables submission while offline and says the draft is kept", async () => {
    const fake = seeded();
    const { user } = await mount(fake, { route });
    await user.type(screen.getByLabelText("Objective"), "offline work");
    fake.openError = new DaemonError("disconnected", "down");
    fake.drop();
    await waitFor(() => expect(screen.getByRole("button", { name: "Submit" })).toBeDisabled());
    expect(screen.getByText(/Submission is unavailable while offline; the draft is kept/)).toBeInTheDocument();
    expect(screen.getByLabelText("Objective")).toHaveValue("offline work");
  });

  it("shows Commands verification inputs only when selected", async () => {
    const { user } = await mount(seeded(), { route });
    expect(screen.queryByLabelText(/Verification commands/)).not.toBeInTheDocument();
    await user.selectOptions(screen.getByLabelText("Verification"), "Commands");
    expect(screen.getByLabelText(/Verification commands/)).toBeInTheDocument();
  });

  it("labels D1 and D2 as placeholder data and never auto-fills the agent from D1", async () => {
    await mount(seeded(), { route });
    expect(await screen.findByTestId("badge-D1")).toHaveTextContent("PLACEHOLDER DATA");
    expect(screen.getByTestId("badge-D2")).toHaveTextContent("PLACEHOLDER DATA");
    expect(screen.getByText(/Launch available: no\./)).toBeInTheDocument();
    expect(screen.getByText(/Codex \(preview\): installed/)).toBeInTheDocument();
    expect(screen.getByLabelText("Agent")).toHaveValue("");
  });

  it.each([
    ["loading", "Loading placeholder data…"],
    ["empty", "No agents reported."],
    ["unavailable", /Unavailable: Agent discovery is not available/],
    ["error", /Placeholder source error: Fixture discovery failed/],
  ] as const)("renders the D1 %s state", async (scenario, text) => {
    const sources: Sources = { ...fixture, agents: createFixtureSources({ scenario }).agents };
    await mount(seeded(), { route, sources });
    const section = screen.getByRole("region", { name: "Agent detection (placeholder)" });
    expect(await within(section).findByText(text)).toBeInTheDocument();
  });

  it("swaps in a daemon-provenance adapter with no component change", async () => {
    const agents: AgentCapabilitiesSource = {
      provenance: { kind: "daemon", id: "D1" },
      listAgents: () =>
        Promise.resolve({
          state: "ready",
          data: [{ id: "real-1", name: "Real Agent", availability: "installed", features: [], discoveredAt: "now" }],
        }),
    };
    await mount(seeded(), { route, sources: { ...fixture, agents } });
    const section = screen.getByRole("region", { name: "Agent detection (placeholder)" });
    expect(await within(section).findByText(/Real Agent: installed/)).toBeInTheDocument();
    expect(within(section).getByTestId("badge-D1")).toHaveTextContent("DAEMON (D1)");
    expect(within(section).getByTestId("badge-D1")).not.toHaveTextContent("PLACEHOLDER");
  });
});

describe("shell", () => {
  it("falls back to a stub for S6 when M5-04 has not supplied one, and renders an injected one", async () => {
    const fake = new FakeDaemon();
    fake.projects = [{ id: REPO }];
    await mount(fake, { route: { name: "review", projectId: REPO, taskId: null } });
    expect(await screen.findByText(/delivered by M5-04/)).toBeInTheDocument();
  });

  it("renders the injected review screen with the route identifiers", async () => {
    const fake = new FakeDaemon();
    await mount(fake, {
      route: { name: "review", projectId: REPO, taskId: "t1" },
      renderReview: ({ projectId, taskId }) => <p>review {projectId} {taskId}</p>,
    });
    expect(await screen.findByText(`review ${REPO} t1`)).toBeInTheDocument();
    await until(() => true);
  });
});
