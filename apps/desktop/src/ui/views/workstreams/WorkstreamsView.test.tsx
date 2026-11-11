import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import App from "../../../App";
import { DaemonError } from "../../../daemon/errors";
import { createFixtureSources } from "../../../sources/fixtures";
import { createFixtureSurfaces, type SurfaceFixtureScenario } from "../../../sources/surfaces/fixtures";
import { FakeDaemon, mkApproval, mkSession, mkTask } from "../../../test/fakeDaemon";
import { installFakeWorkstreams, type FakeWorkstreams } from "../../../test/fakeWorkstreams";
import { connected } from "../../../test/render";
import { createServices } from "../../createServices";
import type { Route } from "../../router";
import { workstreamsGlobalViews, workstreamsShellSlots } from "./register";

const A = "/work/alpha";
const B = "/work/beta";
const VIEW: Route = { name: "global", view: "workstreams" };

async function mount(fake: FakeDaemon | null, options: { scenario?: SurfaceFixtureScenario; fixtureOverBridge?: boolean } = {}) {
  const services = createServices({
    connector: fake ?? undefined,
    initialRoute: VIEW,
    connection: { backoffMs: [5], requestTimeoutMs: 250 },
    data: { coalesceMs: 10 },
    sources: createFixtureSources(),
    surfaces: fake && !options.fixtureOverBridge ? undefined : createFixtureSurfaces({ scenario: options.scenario }),
  });
  const user = userEvent.setup();
  render(<App services={services} shellSlots={workstreamsShellSlots} globalViews={workstreamsGlobalViews} />);
  if (fake) await connected();
  return { services, user };
}

const streams = new WeakMap<FakeDaemon, FakeWorkstreams>();

/** Two projects; "Alpha stream" has an objective (one verified, one pending gate), "Beta stream" has one task. */
function daemon(extra: (fake: FakeDaemon, state: FakeWorkstreams) => void = () => {}) {
  const fake = new FakeDaemon();
  fake.projects = [{ id: A }, { id: B }];
  fake.tasks = [
    mkTask("a1", A, { state: "Succeeded", contract: { objective: "Ship the parser" } }),
    mkTask("a2", A, {
      state: "Running",
      contract: { objective: "Wire the CLI", dependencies: ["a1"] },
      sessions: [mkSession("s1", { adapter: "codex", state: "Running" })],
      attempts: [{ attempt_number: 1, state: "Running" }] as never,
    }),
    mkTask("b1", B, { state: "Queued", contract: { objective: "Cache downloads" } }),
  ];
  const state = installFakeWorkstreams(fake, {
    workstreams: [
      { id: "ws-a", title: "Alpha stream", summary: "Parser and CLI", state: "active", revision: 2 },
      { id: "ws-b", title: "Beta stream", summary: "", state: "active", revision: 1 },
    ],
    objectives: [
      {
        id: "obj-a",
        workstream_id: "ws-a",
        title: "Parser lands",
        description: "A parser that ships",
        revision: 3,
        gates: [
          { id: "gate-1", description: "Parser tests pass", state: "verified", evidence_task_id: "a1" },
          { id: "gate-2", description: "CLI wired", state: "pending", evidence_task_id: null },
        ],
      },
      { id: "obj-a2", workstream_id: "ws-a", title: "Docs", description: "", revision: 1, gates: [] },
    ],
    links: [
      { task_id: "a1", workstream_id: "ws-a", objective_id: "obj-a" },
      { task_id: "a2", workstream_id: "ws-a", objective_id: null },
      { task_id: "b1", workstream_id: "ws-b", objective_id: null },
    ],
  });
  streams.set(fake, state);
  extra(fake, state);
  return fake;
}

const tree = () => screen.getByRole("tree", { name: "Workstreams" });
const level = (n: number) => Array.from(tree().querySelectorAll<HTMLElement>(`[role="treeitem"][aria-level="${n}"]`));
const groupsReady = () => waitFor(() => expect(level(1).length).toBeGreaterThan(0));
const original = window.matchMedia;

afterEach(() => {
  window.matchMedia = original;
  window.location.hash = "";
});

describe("live data", () => {
  it("renders the daemon's workstreams, not projects, with no placeholder content", async () => {
    await mount(daemon());
    await groupsReady();
    expect(level(1).map((group) => group.getAttribute("aria-label"))).toEqual([
      "Workstream 1 of 2: Alpha stream, 2 tasks",
      "Workstream 2 of 2: Beta stream, 1 task",
    ]);
    for (const region of [screen.getByTestId("workstream-tree"), screen.getByTestId("workstreams-view")]) {
      expect(region.textContent).not.toMatch(/PLACEHOLDER|Auth hardening|Docs refresh/);
    }
    expect(screen.getByTestId("ws-source")).toHaveTextContent("daemon records");
  });

  it("keeps the header count equal to the tree groups and the view list", async () => {
    await mount(daemon());
    await groupsReady();
    const count = screen.getByTestId("workstreams-count");
    expect(count).toHaveTextContent("2");
    expect(level(1)).toHaveLength(Number(count.textContent));
    expect(screen.getByTestId("ws-list-count")).toHaveTextContent("2");
    expect(within(screen.getByRole("list", { name: "Workstreams" })).getAllByRole("listitem")).toHaveLength(2);
  });

  it("shows every objective with its gate states and evidence task", async () => {
    await mount(daemon());
    const objectives = await screen.findAllByTestId("ws-objective");
    expect(objectives.map((node) => node.getAttribute("aria-label"))).toEqual(["Objective Parser lands", "Objective Docs"]);
    expect(objectives[0]).toHaveTextContent("Revision 3 · 1 of 2 gates verified");
    expect(objectives[1]).toHaveTextContent("no acceptance gates");
    const gates = screen.getAllByTestId("ws-gate");
    expect(gates).toHaveLength(2);
    expect(gates[0]).toHaveTextContent("verified");
    expect(gates[0]).toHaveTextContent("evidence: a1");
    expect(gates[1]).toHaveTextContent("pending");
    expect(screen.getByTestId("ws-lifecycle")).toHaveTextContent("active");
    expect(screen.getByTestId("ws-summary")).toHaveTextContent("Parser and CLI");
  });

  it("shows worker rows as adapter · state under the open group only", async () => {
    await mount(daemon());
    await groupsReady();
    expect(within(tree()).getAllByTestId("tree-worker").map((worker) => worker.textContent)).toEqual(["codex · Running"]);
    expect(within(tree()).queryByText("Cache downloads")).not.toBeInTheDocument();
  });

  it("opens the existing task detail route from a tree row and a table row", async () => {
    const { services, user } = await mount(daemon());
    const link = await within(tree()).findByRole("treeitem", { name: /Wire the CLI/ });
    expect(link).toHaveAttribute("href", "#/project/%2Fwork%2Falpha/task/a2");
    await user.click(link);
    expect(services.router.getSnapshot()).toEqual({ name: "task", projectId: A, taskId: "a2" });

    await act(async () => services.router.navigate(VIEW));
    const row = (await screen.findAllByTestId("task-row"))[0]!;
    await user.click(within(row).getByRole("link", { name: "Ship the parser" }));
    expect(services.router.getSnapshot()).toEqual({ name: "task", projectId: A, taskId: "a1" });
  });

  it("derives the table: objective link, state, assignment and required evidence", async () => {
    await mount(daemon());
    const rows = await screen.findAllByTestId("task-row");
    expect(rows).toHaveLength(2);
    expect(rows[0]).toHaveTextContent("Parser lands");
    expect(rows[1]).toHaveTextContent("Workstream only");
    expect(rows[1]).toHaveTextContent("Running");
    expect(rows[1]).toHaveTextContent("codex · Running · attempt 1");
    expect(rows[1]).toHaveTextContent("Candidate from the running attempt");
    expect(rows[0]).toHaveTextContent("None: task succeeded");
  });

  it("selecting another workstream swaps the table and the objectives", async () => {
    const { user } = await mount(daemon());
    await screen.findAllByTestId("task-row");
    await user.click(screen.getByRole("button", { name: /2\. Beta stream/ }));
    const rows = screen.getAllByTestId("task-row");
    expect(rows).toHaveLength(1);
    expect(rows[0]).toHaveTextContent("None until dispatched");
    expect(screen.getByTestId("ws-objective")).toHaveTextContent("No objectives yet.");
  });

  it("disables what the daemon cannot do and says why", async () => {
    await mount(daemon());
    await screen.findAllByTestId("task-row");
    for (const name of ["Prioritize", "Split task", "Reassign", "Resolve contract decision"]) {
      const button = screen.getByRole("button", { name });
      expect(button).toBeDisabled();
      const reason = button.getAttribute("aria-describedby");
      expect(reason && document.getElementById(reason)?.textContent).toMatch(/Disabled|becomes a link/);
    }
    expect(screen.getByRole("button", { name: "Create workstream" })).toBeDisabled();
    expect(screen.getByText(/Suspend is advisory/)).toBeVisible();
  });

  it("pluralises task counts in the list and the detail badge", async () => {
    const { user } = await mount(daemon());
    await screen.findAllByTestId("task-row");
    expect(screen.getByRole("button", { name: /2\. Beta stream/ })).toHaveTextContent("1 task");
    expect(screen.getByRole("button", { name: /2\. Beta stream/ })).not.toHaveTextContent("1 tasks");
    expect(screen.getByRole("button", { name: /1\. Alpha stream/ })).toHaveTextContent("2 tasks");
    await user.click(screen.getByRole("button", { name: /2\. Beta stream/ }));
    expect(within(screen.getByTestId("ws-detail")).getByText("1 task")).toBeInTheDocument();
  });

  it("notes tasks that belong to no workstream instead of inventing a group", async () => {
    await mount(daemon((fake) => fake.tasks.push(mkTask("c1", A, { contract: { objective: "Orphan" } }))));
    expect(await screen.findByTestId("ws-unlinked")).toHaveTextContent("1 task not in any workstream.");
    expect(level(1)).toHaveLength(2);
  });

  it("reads required evidence from review.evidence for tasks that may hold a candidate", async () => {
    const fake = daemon();
    fake.stub("review.evidence", (params: Record<string, unknown>) => ({
      task_id: params.task_id,
      project_id: A,
      state: "current",
      reasons: [],
      review_state: "not_requested",
      decision_ready: false,
      approval: null,
      action: null,
      digest_relationship: "unavailable",
      candidate: { commit: "1234567890abcdef", source: "workspace", workspace_head: null },
      workspace: null,
      verification: [{ id: "v1", status: "passed", commit: "1234567890abcdef", started_at_ms: null, finished_at_ms: null, matches_candidate: true, details: null, details_truncated: false }],
      artifacts: [],
      snapshot: null,
      change_request: null,
    }));
    await mount(fake);
    const rows = await screen.findAllByTestId("task-row");
    await waitFor(() => expect(rows[1]).toHaveTextContent("Approval request for candidate 12345678"));
    expect(rows[0]).toHaveTextContent("None: task succeeded");
    expect(fake.callsTo("review.evidence").map((call) => (call.params as { task_id: string }).task_id)).toEqual(["a2"]);
  });

  it("turns Resolve contract decision into a review link when an approval is pending", async () => {
    await mount(daemon((fake) => (fake.approvals = [mkApproval("ap1", "a2")])));
    const link = await screen.findByRole("link", { name: "Resolve contract decision" });
    expect(link).toHaveAttribute("href", "#/project/%2Fwork%2Falpha/review/a2");
  });

  it("filters the list and announces the shown count", async () => {
    const { user } = await mount(daemon());
    await screen.findAllByTestId("task-row");
    await user.type(screen.getByRole("searchbox", { name: "Filter workstreams" }), "bet");
    expect(screen.getByTestId("ws-showing")).toHaveTextContent("Showing 1 of 2");
    expect(screen.getByTestId("ws-list-count")).toHaveTextContent("2");
  });

  it("follows live task changes without losing the operator's collapsed groups", async () => {
    const fake = daemon();
    const { services, user } = await mount(fake);
    const first = await within(tree()).findByRole("treeitem", { name: /Workstream 1 of 2/ });
    await user.click(first);
    expect(first).toHaveAttribute("aria-expanded", "false");
    await user.click(within(tree()).getByRole("treeitem", { name: /Workstream 2 of 2/ }));
    expect(within(tree()).getByText("Cache downloads")).toBeInTheDocument();

    fake.tasks = [...fake.tasks, mkTask("b2", B, { state: "Running", contract: { objective: "Pin toolchain" } })];
    streams.get(fake)!.links.push({ task_id: "b2", workstream_id: "ws-b", objective_id: null });
    await act(async () => services.data.refreshAll());
    await within(tree()).findByText("Pin toolchain");
    expect(within(tree()).getByRole("treeitem", { name: /Workstream 1 of 2/ })).toHaveAttribute("aria-expanded", "false");
    expect(within(tree()).getByRole("treeitem", { name: /Workstream 2 of 2/ })).toHaveAttribute("aria-expanded", "true");
  });
});

describe("live mutations", () => {
  const writes = (fake: FakeDaemon) => fake.calls.filter((call) => /^workstreams\.(create|update|add_|revise_|set_|link_|unlink_)/.test(call.method));
  const paramsOf = (fake: FakeDaemon, method: string) => fake.callsTo(method).map((call) => call.params as Record<string, unknown>);

  it("creates a workstream through workstreams.create with an idempotency key and selects it", async () => {
    const fake = daemon();
    const { user } = await mount(fake);
    await screen.findAllByTestId("task-row");
    await user.type(screen.getByLabelText("New workstream title"), "Gamma stream");
    await user.click(screen.getByRole("button", { name: "Create workstream" }));
    expect(await screen.findByText(/Create workstream "Gamma stream": recorded by the daemon/)).toBeInTheDocument();
    const [call] = paramsOf(fake, "workstreams.create");
    expect(call).toMatchObject({ title: "Gamma stream" });
    expect(typeof call!.idempotency_key).toBe("string");
    expect(await screen.findByTestId("ws-detail")).toHaveAccessibleName("Workstream Gamma stream");
    expect(screen.getByTestId("ws-list-count")).toHaveTextContent("3");
  });

  it("adds an objective with its gates", async () => {
    const fake = daemon();
    const { user } = await mount(fake);
    const form = await screen.findByRole("form", { name: "Add objective" });
    await user.type(within(form).getByLabelText("New objective title"), "Harden");
    await user.type(within(form).getByLabelText("Acceptance gates, one per line"), "First{Enter}Second");
    await user.click(within(form).getByRole("button", { name: "Add objective" }));
    await waitFor(() => expect(screen.getAllByTestId("ws-objective")).toHaveLength(3));
    expect(paramsOf(fake, "workstreams.add_objective")[0]).toMatchObject({
      workstream_id: "ws-a",
      title: "Harden",
      gates: [{ description: "First" }, { description: "Second" }],
    });
  });

  it("revises an objective at the revision it last read and sends only what changed", async () => {
    const fake = daemon();
    const { user } = await mount(fake);
    const card = (await screen.findAllByTestId("ws-objective"))[0]!;
    await user.click(within(card).getByText("Revise this objective"));
    const title = within(card).getByLabelText("Objective title");
    await user.clear(title);
    await user.type(title, "Parser ships");
    await user.type(within(card).getByLabelText("Add acceptance gate"), "Perf budget");
    await user.click(within(card).getByRole("button", { name: "Revise objective" }));
    await waitFor(() => expect(screen.getAllByTestId("ws-objective")[0]).toHaveTextContent("Revision 4"));
    expect(paramsOf(fake, "workstreams.revise_objective")[0]).toMatchObject({
      objective_id: "obj-a",
      expected_revision: 3,
      title: "Parser ships",
      add_gates: [{ description: "Perf budget" }],
    });
    expect(paramsOf(fake, "workstreams.revise_objective")[0]).not.toHaveProperty("description");
  });

  it("sets a gate with an evidence task linked to the workstream", async () => {
    const fake = daemon();
    const { user } = await mount(fake);
    await screen.findAllByTestId("ws-gate");
    await user.selectOptions(screen.getByLabelText("Evidence task for gate CLI wired"), "a2");
    await user.click(screen.getByRole("button", { name: "Apply to CLI wired" }));
    await waitFor(() => expect(screen.getAllByTestId("ws-gate")[1]).toHaveTextContent("evidence: a2"));
    expect(paramsOf(fake, "workstreams.set_gate")[0]).toMatchObject({
      objective_id: "obj-a",
      expected_revision: 3,
      gate_id: "gate-2",
      state: "verified",
      evidence_task_id: "a2",
    });
    expect(screen.getAllByTestId("ws-gate")[1]).toHaveTextContent("verified");
  });

  it("only offers the gate moves the daemon allows", async () => {
    await mount(daemon());
    const verified = await screen.findByLabelText("New state of gate Parser tests pass");
    expect(within(verified).getAllByRole("option").map((option) => option.textContent)).toEqual(["Mark pending"]);
    expect(screen.getByLabelText("Evidence task for gate Parser tests pass")).toBeDisabled();
  });

  it("links an unlinked task under an objective and unlinks a linked one", async () => {
    const fake = daemon((f) => f.tasks.push(mkTask("c1", A, { contract: { objective: "Orphan" } })));
    const { user } = await mount(fake);
    const form = await screen.findByRole("form", { name: "Link task" });
    await user.selectOptions(within(form).getByLabelText("Task to link"), "c1");
    await user.selectOptions(within(form).getByLabelText("Under objective"), "obj-a");
    await user.click(within(form).getByRole("button", { name: "Link task" }));
    await waitFor(() => expect(screen.getAllByTestId("task-row")).toHaveLength(3));
    expect(paramsOf(fake, "workstreams.link_task")[0]).toMatchObject({ workstream_id: "ws-a", task_id: "c1", objective_id: "obj-a" });
    expect(screen.queryByTestId("ws-unlinked")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Unlink Orphan" }));
    await waitFor(() => expect(screen.getAllByTestId("task-row")).toHaveLength(2));
    expect(paramsOf(fake, "workstreams.unlink_task")[0]).toMatchObject({ workstream_id: "ws-a", task_id: "c1" });
    expect(await screen.findByTestId("ws-unlinked")).toBeInTheDocument();
  });

  it("disables unlinking a task that a gate cites as evidence and says why", async () => {
    const fake = daemon();
    await mount(fake);
    const button = await screen.findByRole("button", { name: "Unlink Ship the parser" });
    expect(button).toBeDisabled();
    expect(button).toHaveAccessibleDescription(/a gate cites this task as evidence/);
    expect(screen.getByRole("button", { name: "Unlink Wire the CLI" })).toBeEnabled();
  });

  it("suspends and resumes with an advisory note, and updates at the revision it read", async () => {
    const fake = daemon();
    const { user } = await mount(fake);
    await user.click(await screen.findByRole("button", { name: "Suspend workstream" }));
    await waitFor(() => expect(screen.getByTestId("ws-lifecycle")).toHaveTextContent("suspended"));
    expect(paramsOf(fake, "workstreams.update")[0]).toMatchObject({ workstream_id: "ws-a", expected_revision: 2, state: "suspended" });
    await user.click(await screen.findByRole("button", { name: "Resume workstream" }));
    await waitFor(() => expect(screen.getByTestId("ws-lifecycle")).toHaveTextContent("active"));
    expect(paramsOf(fake, "workstreams.update")[1]).toMatchObject({ expected_revision: 3, state: "active" });
  });

  it("archives only after a confirmation, then locks every control with a reason", async () => {
    const fake = daemon();
    const { user } = await mount(fake);
    await user.click(await screen.findByRole("button", { name: "Archive workstream" }));
    expect(writes(fake)).toHaveLength(0);
    await user.click(screen.getByRole("button", { name: "Keep workstream" }));
    expect(writes(fake)).toHaveLength(0);
    await user.click(screen.getByRole("button", { name: "Archive workstream" }));
    await user.click(screen.getByRole("button", { name: /Confirm archive/ }));
    await waitFor(() => expect(screen.getByTestId("ws-lifecycle")).toHaveTextContent("archived"));
    expect(screen.getByRole("button", { name: "Suspend workstream" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Add objective" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Apply to CLI wired" })).toBeDisabled();
    expect(screen.getByText(/archived workstream is read-only/)).toBeVisible();
  });

  it("answers a stale-revision conflict by reloading, never by overwriting", async () => {
    const fake = daemon();
    const { user } = await mount(fake);
    await screen.findAllByTestId("ws-gate");
    const state = streams.get(fake)!;
    state.objectives[0]!.revision = 9;
    await user.click(screen.getByRole("button", { name: "Apply to CLI wired" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(/stale revision/i);
    expect(screen.getByText("The workstream data was reloaded from the daemon.")).toBeInTheDocument();
    await waitFor(() => expect(screen.getAllByTestId("ws-objective")[0]).toHaveTextContent("Revision 9"));
    expect(state.applied).toEqual([]);
  });

  it("locks every write while the data is stale because the connection is down", async () => {
    const fake = daemon();
    await mount(fake);
    await screen.findAllByTestId("task-row");
    fake.openError = new DaemonError("disconnected", "down");
    fake.drop();
    await waitFor(() => expect(screen.getByTestId("ws-source")).toHaveTextContent("Stale"));
    expect(screen.getAllByTestId("task-row")).toHaveLength(2);
    expect(screen.getByRole("button", { name: "Suspend workstream" })).toBeDisabled();
    expect(screen.getByLabelText("New workstream title")).toBeDisabled();
  });

  it("has a Refresh control that re-reads the daemon, so another client's edit shows without an own action", async () => {
    const fake = daemon();
    const { user } = await mount(fake);
    await screen.findAllByTestId("task-row");
    expect(screen.getByRole("heading", { level: 2, name: "Alpha stream" })).toBeInTheDocument();
    streams.get(fake)!.workstreams[0]!.title = "Alpha renamed elsewhere";
    const before = fake.callsTo("workstreams.list").length;
    await user.click(screen.getByRole("button", { name: "Refresh" }));
    expect(await screen.findByRole("heading", { level: 2, name: "Alpha renamed elsewhere" })).toBeInTheDocument();
    expect(fake.callsTo("workstreams.list").length).toBeGreaterThan(before);
    expect(within(tree()).getByText("Alpha renamed elsewhere")).toBeInTheDocument();
  });

  it("clears a failure banner when the operator refreshes, and its advice names a control that exists", async () => {
    const fake = daemon();
    const { user } = await mount(fake);
    await screen.findAllByTestId("ws-gate");
    streams.get(fake)!.objectives[0]!.revision = 9;
    await user.click(screen.getByRole("button", { name: "Apply to CLI wired" }));
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(/Refresh to see the current state/);
    await user.click(screen.getByRole("button", { name: "Refresh" }));
    await waitFor(() => expect(screen.queryByRole("alert")).not.toBeInTheDocument());
    expect(screen.queryByText("The workstream data was reloaded from the daemon.")).not.toBeInTheDocument();
  });

  it("clears a failure banner once the connection recovers", async () => {
    const fake = daemon();
    const { services, user } = await mount(fake);
    await screen.findAllByTestId("ws-gate");
    streams.get(fake)!.objectives[0]!.revision = 9;
    await user.click(screen.getByRole("button", { name: "Apply to CLI wired" }));
    const banner = () => within(screen.getByTestId("workstreams-view")).queryByRole("alert");
    await waitFor(() => expect(banner()).not.toBeNull());
    fake.openError = new DaemonError("disconnected", "down");
    fake.drop();
    await waitFor(() => expect(screen.getByTestId("ws-source")).toHaveTextContent("Stale"));
    expect(banner()).not.toBeNull();
    fake.openError = null;
    act(() => services.connection.reconnect());
    await waitFor(() => expect(screen.getByTestId("ws-source")).not.toHaveTextContent("Stale"));
    expect(banner()).toBeNull();
  });

  it("marks a workstream whose details could not be read as unknown, never as empty or zero", async () => {
    const fake = daemon((f, state) => {
      const ghost = { id: "ws-ghost", title: "Ghost stream", summary: "", state: "active" as const, revision: 4 };
      f.stub("workstreams.list", () => [...state.workstreams, ghost]);
    });
    const { user } = await mount(fake);
    await screen.findAllByTestId("task-row");
    expect(level(1).map((group) => group.getAttribute("aria-label"))).toEqual([
      "Workstream 1 of 3: Alpha stream, 2 tasks",
      "Workstream 2 of 3: Beta stream, 1 task",
      "Workstream 3 of 3: Ghost stream, tasks unknown",
    ]);
    expect(screen.getByTestId("ws-unlinked")).toHaveTextContent("Tasks not in any workstream: unknown.");
    await user.click(screen.getByRole("button", { name: /Ghost stream/ }));
    expect(await screen.findByTestId("ws-unread")).toHaveTextContent("Tasks unknown");
    expect(screen.getByTestId("ws-unread-reason")).toHaveTextContent(/could not be read/);
    expect(screen.queryAllByTestId("task-row")).toHaveLength(0);
    expect(screen.queryByText("No tasks")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Suspend workstream" })).toBeDisabled();
    expect(screen.queryByRole("button", { name: "Add objective" })).not.toBeInTheDocument();
    expect(screen.getByTestId("ws-unlinked-unknown")).toBeInTheDocument();
  });
});

describe("tree keyboard navigation", () => {
  it("uses a single tab stop and the ARIA tree keys", async () => {
    const { user } = await mount(daemon());
    const items = await within(tree()).findAllByRole("treeitem");
    expect(items.filter((item) => item.tabIndex === 0)).toHaveLength(1);
    items[0]!.focus();
    await user.keyboard("{ArrowDown}");
    expect(items[1]).toHaveFocus();
    await user.keyboard("{ArrowDown}{ArrowDown}");
    expect(within(tree()).getAllByRole("treeitem")[3]).toHaveFocus();
    await user.keyboard("{ArrowUp}");
    expect(within(tree()).getAllByRole("treeitem")[2]).toHaveFocus();
    await user.keyboard("{Home}");
    expect(items[0]).toHaveFocus();
    await user.keyboard("{End}");
    expect(within(tree()).getAllByRole("treeitem").at(-1)).toHaveFocus();
  });

  it("expands, collapses and moves between levels with Right, Left and Enter", async () => {
    const { user } = await mount(daemon());
    const first = await within(tree()).findByRole("treeitem", { name: /Workstream 1 of 2/ });
    first.focus();
    await user.keyboard("{ArrowRight}");
    expect(within(tree()).getAllByRole("treeitem")[1]).toHaveFocus();
    await user.keyboard("{ArrowLeft}");
    expect(first).toHaveFocus();
    await user.keyboard("{ArrowLeft}");
    expect(first).toHaveAttribute("aria-expanded", "false");
    await user.keyboard("{Enter}");
    expect(first).toHaveAttribute("aria-expanded", "true");
    await user.keyboard(" ");
    expect(first).toHaveAttribute("aria-expanded", "false");
    await user.keyboard("{ArrowRight}");
    expect(first).toHaveAttribute("aria-expanded", "true");
  });

  it("Enter on a task row follows its link", async () => {
    const { services, user } = await mount(daemon());
    const task = await within(tree()).findByRole("treeitem", { name: /Ship the parser/ });
    task.focus();
    await user.keyboard("{Enter}");
    expect(services.router.getSnapshot()).toMatchObject({ name: "task", taskId: "a1" });
  });
});

describe("100+ tasks", () => {
  const big = () =>
    daemon((fake, state) => {
      fake.tasks = Array.from({ length: 150 }, (_, index) =>
        mkTask(`t${index}`, A, { contract: { objective: `Task number ${index}`, dependencies: index > 0 ? [`t${index - 1}`] : [] } }),
      );
      state.links = fake.tasks.map((task) => ({ task_id: task.contract.id, workstream_id: "ws-a", objective_id: null }));
    });

  it("renders every task in independently scrollable containers and keeps the counts honest", async () => {
    await mount(big());
    expect(await screen.findAllByTestId("task-row")).toHaveLength(150);
    expect(level(2)).toHaveLength(150);
    expect(level(1)[0]).toHaveAttribute("aria-label", "Workstream 1 of 2: Alpha stream, 150 tasks");
    expect(screen.getByRole("region", { name: "Tasks of Alpha stream" })).toHaveAttribute("tabindex", "0");
    expect(tree().closest(".shell-tree-body")).not.toBeNull();
    expect(screen.getAllByTestId("graph-node").length).toBeLessThanOrEqual(16);
    expect(screen.getByText(/more connected tasks are not drawn/)).toBeInTheDocument();
  });
});

describe("dependency graph", () => {
  it("draws the SVG with a text equivalent on wide layouts", async () => {
    await mount(daemon());
    const svg = await screen.findByRole("img", { name: /Task dependency graph/ });
    expect(within(svg as unknown as HTMLElement).getAllByTestId("graph-node")).toHaveLength(2);
    expect(screen.getByText("Dependencies as text").closest("details")).not.toHaveAttribute("open");
  });

  it("falls back to the text list, without the SVG, on narrow layouts", async () => {
    window.matchMedia = ((query: string) => {
      const max = /max-width:\s*(\d+)px/.exec(query);
      return { matches: max ? 500 <= Number(max[1]) : false, media: query, addEventListener: () => {}, removeEventListener: () => {} };
    }) as unknown as typeof window.matchMedia;
    await mount(daemon());
    await screen.findAllByTestId("task-row");
    expect(screen.queryByRole("img", { name: /Task dependency graph/ })).not.toBeInTheDocument();
    expect(screen.getByText("Dependencies as text").closest("details")).toHaveAttribute("open");
    expect(screen.getByText(/waits on:/)).toHaveTextContent("Ship the parser (Succeeded)");
  });
});

describe("states without data", () => {
  it("is empty only when the daemon says no workstreams exist, and never shows fixtures", async () => {
    await mount(daemon((fake, state) => ((fake.projects = []), (fake.tasks = []), (state.workstreams = []), (state.objectives = []), (state.links = []))));
    expect(await screen.findByText("No workstreams")).toBeInTheDocument();
    expect(within(screen.getByRole("navigation", { name: "Workstreams" })).getByText(/No workstreams exist yet/)).toBeInTheDocument();
    expect(within(screen.getByTestId("workstreams-view")).getByText(/Create one above/)).toBeInTheDocument();
    expect(screen.getByTestId("workstreams-view").textContent).not.toMatch(/PLACEHOLDER|Harden session/);
  });

  it("reports unavailable, with no fixture data, when the daemon read fails", async () => {
    const fake = daemon();
    fake.failNext("workstreams.list", { code: -32603, message: "boom" }, 10);
    await mount(fake);
    await waitFor(() => expect(within(screen.getByTestId("workstreams-view")).getByText(/Unavailable|failed to load/)).toBeInTheDocument());
    expect(screen.getByTestId("workstreams-view").textContent).not.toMatch(/PLACEHOLDER|Harden session/);
    expect(screen.queryAllByTestId("task-row")).toHaveLength(0);
  });

  it("reports unsupported when the daemon has no workstreams methods", async () => {
    const fake = new FakeDaemon();
    fake.projects = [{ id: A }];
    fake.tasks = [mkTask("a1", A)];
    await mount(fake);
    await waitFor(() => expect(within(screen.getByTestId("workstreams-view")).getByText(/Not supported/)).toBeInTheDocument());
    expect(screen.queryAllByTestId("task-row")).toHaveLength(0);
  });

  it("shows loading, never empty, before the daemon has answered", async () => {
    const services = createServices({ connector: new FakeDaemon(), initialRoute: VIEW, sources: createFixtureSources() });
    render(<App services={services} shellSlots={workstreamsShellSlots} globalViews={workstreamsGlobalViews} />);
    expect(screen.getAllByText("Loading workstreams…").length).toBeGreaterThan(0);
    expect(screen.queryByText("No workstreams")).not.toBeInTheDocument();
  });
});

describe("fixture mode", () => {
  it("labels every fixture section PLACEHOLDER, shows its objectives read-only and never links fixture tasks", async () => {
    await mount(null);
    await screen.findAllByTestId("task-row");
    const view = screen.getByTestId("workstreams-view");
    expect(within(view).getAllByTestId("badge-D9").length).toBeGreaterThanOrEqual(4);
    expect(within(screen.getByTestId("workstream-tree")).getAllByText(/PLACEHOLDER DATA/).length).toBeGreaterThanOrEqual(1);
    expect(screen.getByTestId("workstreams-count")).toHaveTextContent("3");
    expect(screen.getAllByTestId("ws-objective").length).toBeGreaterThanOrEqual(2);
    expect(within(view).queryAllByRole("link", { name: /Rotate signing keys/ })).toHaveLength(0);
    expect(document.querySelector('a[href*="fixture"]')).toBeNull();
    for (const name of ["Add objective", "Suspend new dispatch", "Add task", "Remove task"]) {
      expect(screen.getByRole("button", { name })).toBeDisabled();
    }
    expect(screen.queryByRole("button", { name: /Apply to/ })).not.toBeInTheDocument();
  });

  it("simulates creation with a receipt and sends nothing to the daemon, even when one is connected", async () => {
    const fake = daemon();
    const { user } = await mount(fake, { fixtureOverBridge: true });
    await screen.findAllByTestId("task-row");
    const before = fake.calls.length;
    await user.type(screen.getByLabelText("New workstream title"), "Brand new");
    await user.click(screen.getByRole("button", { name: "Create workstream (simulated)" }));
    expect(await screen.findByTestId("simulated-receipt")).toHaveTextContent("SIMULATED");
    expect(fake.calls.slice(before).filter((call) => /fixture:/.test(JSON.stringify(call.params)))).toEqual([]);
    expect(fake.calls.filter((call) => /^workstreams\.(create|update|add_|revise_|set_|link_|unlink_)/.test(call.method))).toEqual([]);
    expect(fake.calls.filter((call) => /create|submit|cancel|register/.test(call.method))).toEqual([]);
  });

  it.each<[SurfaceFixtureScenario, RegExp]>([
    ["unavailable", /Unavailable: Fixture scenario/],
    ["unsupported", /Not supported: Fixture scenario/],
    ["unknown", /Unknown: Fixture scenario/],
  ])("reports the %s scenario with its reason", async (scenario, text) => {
    await mount(null, { scenario });
    await waitFor(() => expect(within(screen.getByTestId("workstreams-view")).getByText(text)).toBeInTheDocument());
    expect(screen.queryAllByTestId("task-row")).toHaveLength(0);
  });

  it("stays on loading while the source has not answered", async () => {
    await mount(null, { scenario: "loading" });
    expect(screen.getAllByText("Loading workstreams…").length).toBeGreaterThan(0);
  });
});
