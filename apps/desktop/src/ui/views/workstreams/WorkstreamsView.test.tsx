import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import App from "../../../App";
import { DaemonError } from "../../../daemon/errors";
import { createFixtureSources } from "../../../sources/fixtures";
import { createFixtureSurfaces, type SurfaceFixtureScenario } from "../../../sources/surfaces/fixtures";
import { okResult } from "../../../sources/surfaces/result";
import type { SurfaceSources, Workstream } from "../../../sources/surfaces/types";
import type { Provenance } from "../../../sources/types";
import { FakeDaemon, mkApproval, mkSession, mkTask } from "../../../test/fakeDaemon";
import { installFakeWorkstreams, type FakeWorkstreams } from "../../../test/fakeWorkstreams";
import { connected } from "../../../test/render";
import { backToList, chooseAction, openMenu, openTab, openWorkstream, type User } from "../../../test/workstreamsUi";
import { createServices } from "../../createServices";
import type { Route } from "../../router";
import { workstreamsGlobalViews } from "./register";

const A = "/work/alpha";
const B = "/work/beta";
const VIEW: Route = { name: "global", view: "workstreams" };
const D9_OBSERVATION = { observedAt: "2026-01-01T11:59:55.000Z", ageMs: 5_000 };
const D9_PROVENANCE: Provenance = {
  kind: "daemon",
  id: "D9",
  methods: ["workstreams.list", "workstreams.get", "tasks.list"],
};

function d9CountSurfaces(): SurfaceSources {
  const base = createFixtureSurfaces({ now: () => Date.parse("2026-01-01T12:00:00.000Z") });
  const workstream: Workstream = {
    id: "ws-a",
    title: "Alpha stream",
    summary: "Parser and CLI",
    state: "active",
    revision: 2,
    projectIds: [A],
    tasks: [
      {
        id: "a2",
        projectId: A,
        objectiveId: null,
        objective: "Wire the CLI",
        state: "Running",
        agent: "codex",
        dependencies: [],
      },
    ],
    counts: {
      status: "ok",
      value: {
        Created: 0,
        Queued: 0,
        Running: 1,
        Waiting: 0,
        Verifying: 0,
        Succeeded: 0,
        VerificationFailed: 0,
        Failed: 0,
        Cancelled: 0,
      },
      ...D9_OBSERVATION,
    },
    objectives: { status: "ok", value: [], ...D9_OBSERVATION },
  };
  return {
    ...base,
    workstreams: {
      ...base.workstreams,
      provenance: D9_PROVENANCE,
      workstreams: () => Promise.resolve(okResult(D9_PROVENANCE, [workstream], D9_OBSERVATION)),
    },
  };
}

async function mount(
  fake: FakeDaemon | null,
  options: { scenario?: SurfaceFixtureScenario; fixtureOverBridge?: boolean; surfaces?: SurfaceSources; route?: Route } = {},
) {
  const services = createServices({
    connector: fake ?? undefined,
    initialRoute: options.route ?? VIEW,
    connection: { backoffMs: [5], requestTimeoutMs: 250 },
    data: { coalesceMs: 10 },
    sources: createFixtureSources(),
    surfaces: options.surfaces ?? (fake && !options.fixtureOverBridge ? undefined : createFixtureSurfaces({ scenario: options.scenario })),
  });
  const user = userEvent.setup();
  render(<App services={services} globalViews={workstreamsGlobalViews} />);
  if (fake) await connected();
  return { services, user };
}

/** Mounts the list screen, then opens one workstream (Alpha stream unless named). */
async function mountOpen(fake: FakeDaemon, title = "Alpha stream", options: Parameters<typeof mount>[1] = {}) {
  const mounted = await mount(fake, options);
  await openWorkstream(mounted.user, title);
  return mounted;
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

const groupsReady = () => screen.findAllByTestId("ws-row");
const titleOf = (row: HTMLElement) => within(row).getByText(/ stream$/).textContent;
const original = window.matchMedia;

afterEach(() => {
  window.matchMedia = original;
  window.location.hash = "";
});

describe("live data", () => {
  it("renders the daemon's workstreams, not projects, with no placeholder content", async () => {
    const { user } = await mount(daemon());
    await groupsReady();
    expect((await screen.findAllByTestId("ws-row")).map(titleOf)).toEqual(["Alpha stream", "Beta stream"]);
    expect(screen.getByTestId("workstreams-view").textContent).not.toMatch(/PLACEHOLDER|Auth hardening|Docs refresh/);
    await openWorkstream(user, "Alpha stream");
    expect(screen.getByTestId("ws-source")).toHaveTextContent("daemon records");
  });
  it("shows D9 count observation separately from AppData task state", async () => {
    const fake = daemon();
    fake.tasks = fake.tasks.map((task) => (task.contract.id === "a2" ? { ...task, state: "Failed" } : task));
    const { services, user } = await mount(fake, { surfaces: d9CountSurfaces() });
    await openWorkstream(user, "Alpha stream");
    await openTab(user, "Tasks");
    await screen.findByTestId("task-row");
    await waitFor(() => expect(services.data.getSnapshot().tasks.status).toBe("ready"));

    expect(screen.getByTestId("task-row")).toHaveTextContent("Failed");
    expect(screen.getByTestId("count-Running")).toHaveTextContent("1 Running");
    expect(screen.getByTestId("ws-count-observation")).toHaveTextContent(/D9 counts observed \d+ (s|min|h|d) ago/);
    expect(screen.getByTestId("ws-count-observation")).not.toHaveTextContent(D9_OBSERVATION.observedAt);
    expect(screen.getByTestId("ws-task-list-status")).toHaveTextContent(/AppData task list is ready as a separate snapshot/i);
  });

  it("exposes a failed AppData refresh when task rows use retained data", async () => {
    const fake = daemon();
    const { services, user } = await mountOpen(fake);
    await openTab(user, "Tasks");
    await screen.findAllByTestId("task-row");
    await waitFor(() => expect(services.data.getSnapshot().tasks.status).toBe("ready"));
    const retained = services.data.getSnapshot().tasks.data;
    expect(retained).not.toBeNull();

    fake.failNext("tasks.list", { code: -32603, message: "test refresh failure" });
    await act(async () => services.data.refreshAll());

    const taskStatus = screen.getByTestId("ws-task-list-status");
    expect(services.data.getSnapshot().tasks).toMatchObject({ status: "error", data: retained });
    expect(taskStatus).toHaveAttribute("role", "alert");
    expect(taskStatus).toHaveTextContent(/refresh failed/i);
    expect(taskStatus).toHaveTextContent(/task rows use retained data/i);
    expect(taskStatus).toHaveTextContent("The daemon reported an internal error.");
    expect(screen.getByTestId("ws-count-observation")).toBeInTheDocument();
    // The same error also surfaces as a top-level alert, outside the closed Technical details.
    expect(within(screen.getByTestId("workstreams-view")).getAllByRole("alert")[0]).toHaveTextContent(/refresh failed/i);
  });

  it("keeps the header count equal to the list", async () => {
    await mount(daemon());
    await groupsReady();
    expect(within(screen.getByTestId("workstreams-view")).getByText("2 active")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Active 2" })).toBeInTheDocument();
    expect(await screen.findAllByTestId("ws-row")).toHaveLength(2);
  });

  it("lists archived workstreams in a collapsed Archived group, not among the active rows", async () => {
    const { user } = await mount(
      daemon((_, state) => {
        state.workstreams[1]!.state = "archived";
      }),
    );
    await groupsReady();
    expect(within(screen.getByTestId("workstreams-view")).getByText("1 active")).toBeInTheDocument();

    const archived = await screen.findByTestId("ws-archived");
    expect(archived).not.toHaveAttribute("open");
    const activeRows = within(screen.getByRole("tabpanel"))
      .getAllByTestId("ws-row")
      .filter((row) => !archived.contains(row));
    expect(activeRows.map(titleOf)).toEqual(["Alpha stream"]);
    expect(within(archived).getAllByTestId("ws-row").map(titleOf)).toEqual(["Beta stream"]);

    await user.click(within(archived).getByText("Archived", { selector: ".ui-disclosure-title" }));
    expect(archived).toHaveAttribute("open");
    await openTab(user, "Archived");
    expect(within(screen.getByRole("tabpanel")).getAllByTestId("ws-row").map(titleOf)).toEqual(["Beta stream"]);
  });

  it("moves a workstream to the Archived group when it is archived", async () => {
    const { user } = await mount(daemon());
    await groupsReady();
    expect(screen.queryByTestId("ws-archived")).toBeNull();
    await openWorkstream(user, "Alpha stream");
    await chooseAction(user, "Workstream actions", "Archive workstream");
    await user.click(screen.getByRole("button", { name: /Confirm archive/ }));
    await waitFor(() => expect(screen.getByTestId("ws-status")).toHaveTextContent("Archived"));
    await backToList(user);
    expect(screen.getByRole("tab", { name: "Active 1" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Archived 1" })).toBeInTheDocument();
    expect(within(screen.getByTestId("ws-archived")).getAllByTestId("ws-row").map(titleOf)).toEqual(["Alpha stream"]);
  });

  it("shows every objective with its gate states and evidence task", async () => {
    const { user } = await mountOpen(daemon());
    await openTab(user, "Objectives");
    const objectives = await screen.findAllByTestId("ws-objective");
    expect(objectives.map((node) => node.getAttribute("aria-label"))).toEqual(["Objective Parser lands", "Objective Docs"]);
    expect(objectives[0]).toHaveTextContent("Revision 3 · 1 of 2 gates verified");
    expect(objectives[1]).toHaveTextContent("no acceptance gates");
    const gates = screen.getAllByTestId("ws-gate");
    expect(gates).toHaveLength(2);
    expect(gates[0]).toHaveTextContent("verified");
    expect(gates[0]).toHaveTextContent("evidence: a1");
    expect(gates[1]).toHaveTextContent("pending");
    expect(screen.getByTestId("ws-status")).toHaveTextContent("On track");
    expect(screen.getByTestId("ws-summary")).toHaveTextContent("Parser and CLI");
  });

  it("opens the workstream a deep link names, and a task link in its table still opens the task", async () => {
    const { services, user } = await mount(daemon(), { route: { name: "global", view: "workstreams", workstreamId: "ws-b" } });
    expect(await screen.findByRole("heading", { level: 1, name: "Beta stream" })).toBeVisible();
    await act(async () => services.router.navigate({ name: "global", view: "workstreams", workstreamId: "ws-a" }));
    expect(await screen.findByRole("heading", { level: 1, name: "Alpha stream" })).toBeVisible();
    await openTab(user, "Tasks");
    const row = (await screen.findAllByTestId("task-row"))[0]!;
    await user.click(within(row).getByRole("link", { name: "Ship the parser" }));
    expect(services.router.getSnapshot()).toEqual({ name: "task", projectId: A, taskId: "a1" });
  });

  it("derives the table: objective link, state, assignment and required evidence", async () => {
    const { user } = await mountOpen(daemon());
    await openTab(user, "Tasks");
    const rows = await screen.findAllByTestId("task-row");
    expect(rows).toHaveLength(2);
    expect(rows[0]).toHaveTextContent("Parser lands");
    expect(rows[1]).toHaveTextContent("Workstream only");
    expect(rows[1]).toHaveTextContent("Running");
    expect(rows[1]).toHaveTextContent("codex · Running · attempt 1");
    expect(rows[1]).toHaveTextContent("Candidate from the running attempt");
    expect(rows[0]).toHaveTextContent("None: task succeeded");
  });

  it("selects the workstream named in the route", async () => {
    const { user } = await mount(daemon(), { route: { name: "global", view: "workstreams", workstreamId: "ws-b" } });
    expect(await screen.findByRole("heading", { level: 1, name: "Beta stream" })).toBeVisible();
    await openTab(user, "Tasks");
    await waitFor(() => expect(screen.getAllByTestId("task-row")).toHaveLength(1));
    expect(screen.getByTestId("task-row")).toHaveTextContent("None until dispatched");
  });

  it("shows why a queued task has not started in the task table and the activity list", async () => {
    const fake = daemon();
    fake.queue.dispatch_blocked = true;
    const { user } = await mount(fake, { route: { name: "global", view: "workstreams", workstreamId: "ws-b" } });
    await openTab(user, "Tasks");
    const row = await screen.findByTestId("task-row");
    expect(await within(row).findByTestId("queue-wait")).toHaveTextContent("Restart gate blocking launches");
    await openTab(user, "Activity");
    expect(within(screen.getByTestId("ws-activity-row")).getByTestId("queue-wait")).toHaveTextContent(
      "Restart gate blocking launches",
    );
  });

  it("selecting another workstream swaps the table and the objectives", async () => {
    const { user } = await mountOpen(daemon());
    await openTab(user, "Tasks");
    expect(await screen.findAllByTestId("task-row")).toHaveLength(2);
    await backToList(user);
    await openWorkstream(user, "Beta stream");
    await openTab(user, "Tasks");
    const rows = screen.getAllByTestId("task-row");
    expect(rows).toHaveLength(1);
    expect(rows[0]).toHaveTextContent("None until dispatched");
    await openTab(user, "Objectives");
    expect(screen.getByTestId("ws-objective")).toHaveTextContent("No objectives yet.");
  });

  it("shows no decision banner without a pending approval, and keeps the advisory scheduler note in Technical details", async () => {
    const { user } = await mountOpen(daemon());
    await screen.findAllByTestId("ws-overview-task");
    expect(screen.queryByTestId("ws-banner")).toBeNull();
    expect(screen.queryByRole("link", { name: "Resolve decision" })).toBeNull();
    const technical = screen.getByTestId("ws-technical");
    expect(technical).not.toHaveAttribute("open");
    expect(within(technical).getByText(/Suspend is advisory/)).toBeInTheDocument();
    await user.click(within(technical).getByText("Technical details"));
    expect(technical).toHaveAttribute("open");
    expect(within(technical).getByText(/Suspend is advisory/)).toBeVisible();
  });

  it("pluralises task counts in the list rows and the detail tabs", async () => {
    const { user } = await mount(daemon());
    const rows = await screen.findAllByTestId("ws-row");
    expect(rows[0]).toHaveTextContent("1 of 2 tasks done");
    expect(rows[1]).toHaveTextContent("0 of 1 task done");
    expect(rows[1]).not.toHaveTextContent("1 tasks");
    await openWorkstream(user, "Beta stream");
    expect(screen.getByRole("tab", { name: "Tasks 1" })).toBeInTheDocument();
  });

  it("notes tasks that belong to no workstream instead of inventing a group", async () => {
    await mount(daemon((fake) => fake.tasks.push(mkTask("c1", A, { contract: { objective: "Orphan" } }))));
    expect(await screen.findByTestId("ws-unlinked")).toHaveTextContent("1 task not in any workstream.");
    expect(screen.getAllByTestId("ws-row")).toHaveLength(2);
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
    const { user } = await mountOpen(fake);
    await openTab(user, "Tasks");
    const rows = await screen.findAllByTestId("task-row");
    await waitFor(() => expect(rows[1]).toHaveTextContent("Approval request for candidate 12345678"));
    expect(rows[0]).toHaveTextContent("None: task succeeded");
    expect(fake.callsTo("review.evidence").map((call) => (call.params as { task_id: string }).task_id)).toEqual(["a2"]);
  });

  it("turns the Resolve decision banner into a review link when an approval is pending", async () => {
    await mountOpen(daemon((fake) => (fake.approvals = [mkApproval("ap1", "a2")])));
    const link = await screen.findByRole("link", { name: "Resolve decision" });
    expect(link).toHaveAttribute("href", "#/project/%2Fwork%2Falpha/review/a2");
    expect(screen.getByTestId("ws-banner")).toContainElement(link);
  });

  it("filters the rows by title or project as the operator types, leaving the tab counts alone", async () => {
    const { user } = await mount(
      daemon((_, state) => {
        state.workstreams[0]!.title = "Parser stream";
      }),
    );
    await screen.findAllByTestId("ws-row");
    const search = screen.getByRole("searchbox", { name: "Search workstreams" });
    await user.type(search, "bet");
    expect(within(screen.getByRole("tabpanel")).getAllByTestId("ws-row").map(titleOf)).toEqual(["Beta stream"]);
    expect(screen.getByRole("tab", { name: "Active 2" })).toBeInTheDocument();
    await user.clear(search);
    await user.type(search, "ALPHA");
    expect(within(screen.getByRole("tabpanel")).getAllByTestId("ws-row").map(titleOf)).toEqual(["Parser stream"]);
    await user.clear(search);
    await user.type(search, "zzz");
    expect(within(screen.getByRole("tabpanel")).queryAllByTestId("ws-row")).toHaveLength(0);
    expect(within(screen.getByRole("tabpanel")).getByRole("status")).toHaveTextContent("No workstream matches the search.");
  });
});

describe("list screen", () => {
  const rowTitles = (scope: HTMLElement) => within(scope).getAllByTestId("ws-row").map((row) => row.querySelector(".ws-row-name")?.textContent);

  it("shows one row per workstream with its status chip, progress and age, and none of the detail controls", async () => {
    await mount(daemon());
    const rows = await screen.findAllByTestId("ws-row");
    expect(rows[0]).toHaveTextContent("Alpha stream");
    expect(rows[0]).toHaveTextContent("alpha");
    expect(rows[0]).toHaveTextContent("On track");
    expect(rows[0]).toHaveTextContent("1 of 2 tasks done");
    expect(rows[1]).toHaveTextContent("0 of 1 task done");
    expect(screen.getByRole("heading", { level: 1, name: "Workstreams" })).toBeVisible();
    expect(screen.getByRole("link", { name: "+ New workstream" })).toHaveAttribute("href", "#/workspace");
    expect(screen.getByRole("tablist", { name: "Workstream status" })).toBeVisible();
    expect(screen.getByTestId("ws-updated")).toHaveTextContent(/Updated \d+ s ago/);
    expect(screen.queryByTestId("task-row")).not.toBeInTheDocument();
    expect(screen.queryByTestId("ws-objective")).not.toBeInTheDocument();
    expect(screen.queryByRole("form", { name: "Add objective" })).not.toBeInTheDocument();
    for (const name of [/Suspend workstream/, /Resume workstream/, "Prioritize", /Archive workstream/, /Add objective/]) {
      expect(screen.queryByRole("button", { name })).not.toBeInTheDocument();
    }
  });

  it("marks a workstream whose task has a pending approval as Needs attention, matching the top bar count", async () => {
    await mount(daemon((fake) => (fake.approvals = [mkApproval("ap1", "a2")])));
    const rows = await screen.findAllByTestId("ws-row");
    await waitFor(() => expect(rows[0]).toHaveTextContent("Needs attention"));
    expect(rows[1]).toHaveTextContent("On track");
    expect(screen.getByTestId("top-need-you")).toHaveTextContent("1 need you");
  });

  it("lists the Global tasks workstream first, in the order the daemon returns", async () => {
    await mount(
      daemon((_, state) => {
        state.workstreams.unshift({ id: "ws-global", title: "Global tasks", summary: "", state: "active", revision: 1 });
      }),
    );
    await screen.findAllByTestId("ws-row");
    expect(rowTitles(screen.getByRole("tabpanel"))).toEqual(["Global tasks", "Alpha stream", "Beta stream"]);
  });

  it("counts the three status tabs and holds archived workstreams under the closed Archived group", async () => {
    const { user } = await mount(
      daemon((_, state) => {
        state.workstreams[0]!.state = "suspended";
        state.workstreams.push({ id: "ws-c", title: "Gamma stream", summary: "", state: "archived", revision: 1 });
      }),
    );
    await screen.findAllByTestId("ws-row");
    expect(screen.getAllByRole("tab").map((tab) => tab.textContent)).toEqual(["Active 1", "Suspended 1", "Archived 1"]);
    const archived = screen.getByTestId("ws-archived");
    expect(archived).not.toHaveAttribute("open");
    await user.click(within(archived).getByText("Archived", { selector: ".ui-disclosure-title" }));
    expect(archived).toHaveAttribute("open");
    expect(rowTitles(archived)).toEqual(["Gamma stream"]);
    await openTab(user, "Archived");
    expect(rowTitles(screen.getByRole("tabpanel"))).toEqual(["Gamma stream"]);
    await openTab(user, "Suspended");
    expect(rowTitles(screen.getByRole("tabpanel"))).toEqual(["Alpha stream"]);
    expect(within(screen.getByRole("tabpanel")).getByTestId("ws-row")).toHaveTextContent("Suspended");
  });

  it("holds Manage projects, Archive finished and Refresh in the More actions menu", async () => {
    const { user } = await mount(daemon());
    await screen.findAllByTestId("ws-row");
    const menu = await openMenu(user, "More actions");
    expect(within(menu).getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Manage projects", "Archive finished (1)", "Refresh"]);
    await user.click(within(menu).getByRole("menuitem", { name: "Archive finished (1)" }));
    expect(await screen.findByRole("dialog", { name: "Archive 1 finished task?" })).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Keep" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
});

describe("detail screen", () => {
  it("shows the banner only while an approval is pending, above three stat tiles", async () => {
    await mountOpen(daemon());
    expect(screen.queryByTestId("ws-banner")).not.toBeInTheDocument();
    const stats = screen.getByRole("tabpanel").querySelectorAll(".ws-stat");
    expect(Array.from(stats).map((stat) => stat.textContent)).toEqual(["Tasks2", "Succeeded1", "Running1"]);
  });

  it("reveals the objective form from + Add objective and keeps Technical details collapsed", async () => {
    const { user } = await mountOpen(daemon());
    expect(screen.queryByRole("form", { name: "Add objective" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "+ Add objective" }));
    expect(await screen.findByRole("form", { name: "Add objective" })).toBeVisible();
    const technical = screen.getByTestId("ws-technical");
    expect(technical).not.toHaveAttribute("open");
    expect(within(technical).getByText(/Suspend is advisory: the daemon scheduler does not consult it/)).toBeInTheDocument();
  });

  it("offers Suspend, Prioritize and Archive only in the Actions menu, and archives only after the dialog", async () => {
    const fake = daemon();
    const { user } = await mountOpen(fake);
    for (const name of ["Suspend workstream", "Prioritize", "Archive workstream"]) {
      expect(screen.queryByRole("button", { name })).not.toBeInTheDocument();
    }
    const menu = await openMenu(user, "Workstream actions");
    expect(within(menu).getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Suspend workstream", "Prioritize", "Archive workstream"]);
    await user.click(within(menu).getByRole("menuitem", { name: "Archive workstream" }));
    expect(await screen.findByRole("dialog", { name: 'Archive "Alpha stream"?' })).toBeVisible();
    expect(fake.callsTo("workstreams.update")).toHaveLength(0);
    await user.click(screen.getByRole("button", { name: /Confirm archive \(cannot be undone\)/ }));
    await waitFor(() => expect(screen.getByTestId("ws-status")).toHaveTextContent("Archived"));
    expect(fake.callsTo("workstreams.update")).toHaveLength(1);
  });
});

describe("live mutations", () => {
  const writes = (fake: FakeDaemon) => fake.calls.filter((call) => /^workstreams\.(create|update|prioritize|add_|revise_|set_|link_|unlink_)/.test(call.method));
  const paramsOf = (fake: FakeDaemon, method: string) => fake.callsTo(method).map((call) => call.params as Record<string, unknown>);

  it("offers no create control: workstreams are created through the orchestrator", async () => {
    const fake = daemon();
    await mount(fake);
    await screen.findAllByTestId("ws-row");
    expect(screen.getByRole("link", { name: "+ New workstream" })).toHaveAttribute("href", "#/workspace");
    expect(screen.queryByRole("button", { name: /Create workstream/ })).not.toBeInTheDocument();
    expect(screen.queryByLabelText("New workstream title")).not.toBeInTheDocument();
    expect(paramsOf(fake, "workstreams.create")).toHaveLength(0);
  });

  it("prioritizes the selected workstream through workstreams.prioritize and lists it first", async () => {
    const fake = daemon();
    const { user } = await mountOpen(fake);
    const alphaMenu = await openMenu(user, "Workstream actions");
    const alphaPrioritize = within(alphaMenu).getByRole("menuitem", { name: "Prioritize" });
    expect(alphaPrioritize).toHaveAttribute("aria-disabled", "true");
    expect(alphaPrioritize).toHaveAccessibleDescription('"Alpha stream" is already first.');
    await user.keyboard("{Escape}");
    await backToList(user);
    await openWorkstream(user, "Beta stream");
    await chooseAction(user, "Workstream actions", "Prioritize");
    expect(await screen.findByText(/Prioritize "Beta stream": recorded by the daemon/)).toBeInTheDocument();
    const [call] = paramsOf(fake, "workstreams.prioritize");
    expect(call).toMatchObject({ workstream_id: "ws-b" });
    expect(typeof call!.idempotency_key).toBe("string");
    const betaMenu = await openMenu(user, "Workstream actions");
    await waitFor(() => expect(within(betaMenu).getByRole("menuitem", { name: "Prioritize" })).toHaveAttribute("aria-disabled", "true"));
    await user.keyboard("{Escape}");
    await backToList(user);
    expect((await screen.findAllByTestId("ws-row")).map(titleOf)).toEqual(["Beta stream", "Alpha stream"]);
  });

  it("archives the selected workstream from its Actions menu only after confirmation", async () => {
    const fake = daemon();
    const { user } = await mountOpen(fake, "Beta stream");
    await chooseAction(user, "Workstream actions", "Archive workstream");
    expect(paramsOf(fake, "workstreams.update")).toHaveLength(0);
    const dialog = screen.getByRole("dialog", { name: 'Archive "Beta stream"?' });
    expect(dialog).toHaveTextContent("It becomes read-only, and this cannot be undone.");
    await user.click(within(dialog).getByRole("button", { name: "Keep workstream" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(paramsOf(fake, "workstreams.update")).toHaveLength(0);
    await chooseAction(user, "Workstream actions", "Archive workstream");
    await user.click(screen.getByRole("button", { name: "Confirm archive (cannot be undone)" }));
    expect(await screen.findByText(/Archive workstream: recorded by the daemon/)).toBeInTheDocument();
    expect(paramsOf(fake, "workstreams.update")).toEqual([
      expect.objectContaining({ workstream_id: "ws-b", state: "archived" }),
    ]);
    const menu = await openMenu(user, "Workstream actions");
    for (const name of ["Suspend workstream", "Prioritize", "Archive workstream"]) {
      expect(within(menu).getByRole("menuitem", { name })).toHaveAttribute("aria-disabled", "true");
    }
  });

  it("adds an objective with its gates", async () => {
    const fake = daemon();
    const { user } = await mountOpen(fake);
    await openTab(user, "Objectives");
    await user.click(screen.getByRole("button", { name: "+ Add objective" }));
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
    const { user } = await mountOpen(fake);
    await openTab(user, "Objectives");
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
    const { user } = await mountOpen(fake);
    await openTab(user, "Objectives");
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
    const { user } = await mountOpen(daemon());
    await openTab(user, "Objectives");
    const verified = await screen.findByLabelText("New state of gate Parser tests pass");
    expect(within(verified).getAllByRole("option").map((option) => option.textContent)).toEqual(["Mark pending"]);
    expect(screen.getByLabelText("Evidence task for gate Parser tests pass")).toBeDisabled();
  });

  it("links an unlinked task under an objective and unlinks a linked one", async () => {
    const fake = daemon((f) => f.tasks.push(mkTask("c1", A, { contract: { objective: "Orphan" } })));
    const { user } = await mountOpen(fake);
    await openTab(user, "Tasks");
    const form = await screen.findByRole("form", { name: "Link task" });
    await user.selectOptions(within(form).getByLabelText("Task to link"), "c1");
    await user.selectOptions(within(form).getByLabelText("Under objective"), "obj-a");
    await user.click(within(form).getByRole("button", { name: "Link task" }));
    await waitFor(() => expect(screen.getAllByTestId("task-row")).toHaveLength(3));
    expect(paramsOf(fake, "workstreams.link_task")[0]).toMatchObject({ workstream_id: "ws-a", task_id: "c1", objective_id: "obj-a" });
    await backToList(user);
    expect(screen.queryByTestId("ws-unlinked")).not.toBeInTheDocument();

    await openWorkstream(user, "Alpha stream");
    await openTab(user, "Tasks");
    await user.click(screen.getByRole("button", { name: "Unlink Orphan" }));
    await waitFor(() => expect(screen.getAllByTestId("task-row")).toHaveLength(2));
    expect(paramsOf(fake, "workstreams.unlink_task")[0]).toMatchObject({ workstream_id: "ws-a", task_id: "c1" });
    await backToList(user);
    expect(await screen.findByTestId("ws-unlinked")).toHaveTextContent("1 task not in any workstream.");
  });

  it("disables unlinking a task that a gate cites as evidence and says why", async () => {
    const fake = daemon();
    const { user } = await mountOpen(fake);
    await openTab(user, "Tasks");
    const button = await screen.findByRole("button", { name: "Unlink Ship the parser" });
    expect(button).toBeDisabled();
    expect(button).toHaveAccessibleDescription(/a gate cites this task as evidence/);
    expect(screen.getByRole("button", { name: "Unlink Wire the CLI" })).toBeEnabled();
  });

  it("archives a finished task from its table row, and the task leaves the table", async () => {
    const fake = daemon();
    const { user } = await mountOpen(fake);
    await openTab(user, "Tasks");
    await screen.findAllByTestId("task-row");
    await user.click(screen.getByRole("button", { name: "Archive Ship the parser" }));
    await waitFor(() => expect(screen.getAllByTestId("task-row")).toHaveLength(1));
    expect(paramsOf(fake, "tasks.archive")).toEqual([{ task_ids: ["a1"], idempotency_key: expect.any(String) }]);
    expect(await screen.findByText(/Archive task a1: recorded by the daemon/)).toBeInTheDocument();
  });

  it("disables archive on a running task row and says why", async () => {
    const fake = daemon();
    const { user } = await mountOpen(fake);
    await openTab(user, "Tasks");
    const button = await screen.findByRole("button", { name: "Archive Wire the CLI" });
    expect(button).toBeDisabled();
    expect(button).toHaveAccessibleDescription(/Only finished tasks can be archived/);
    expect(paramsOf(fake, "tasks.archive")).toHaveLength(0);
  });

  it("shows the daemon's conflict as a notice when a finished task is refused", async () => {
    const fake = daemon();
    const { user } = await mountOpen(fake);
    await openTab(user, "Tasks");
    await screen.findAllByTestId("task-row");
    fake.failNext("tasks.archive", { code: -32011, message: "task is not finished" });
    await user.click(screen.getByRole("button", { name: "Archive Ship the parser" }));
    expect(await screen.findByText(/Archive task a1 failed/)).toBeInTheDocument();
    expect(screen.getAllByTestId("task-row")).toHaveLength(2);
  });

  it("archives every finished task, linked or not, in one call after a confirmation", async () => {
    const fake = daemon((f) => {
      f.tasks.push(mkTask("a3", A, { state: "Failed" }), mkTask("c9", B, { state: "Cancelled" }));
    });
    const { user } = await mount(fake);
    await screen.findAllByTestId("ws-row");
    await chooseAction(user, "More actions", "Archive finished (3)");
    expect(screen.getByRole("dialog", { name: "Archive 3 finished tasks?" })).toBeInTheDocument();
    expect(paramsOf(fake, "tasks.archive")).toHaveLength(0);
    await user.click(screen.getByRole("button", { name: "Confirm archive" }));
    expect(await screen.findByText(/Archive 3 finished tasks: recorded by the daemon/)).toBeInTheDocument();
    expect(paramsOf(fake, "tasks.archive")).toEqual([{ task_ids: ["a1", "a3", "c9"], idempotency_key: expect.any(String) }]);
    const menu = await openMenu(user, "More actions");
    await waitFor(() => expect(within(menu).getByRole("menuitem", { name: "Archive finished (0)" })).toHaveAttribute("aria-disabled", "true"));
    await user.keyboard("{Escape}");
    await openWorkstream(user, "Alpha stream");
    await openTab(user, "Tasks");
    await waitFor(() => expect(screen.getAllByTestId("task-row")).toHaveLength(1));
  });

  it("sends nothing when the bulk archive is kept", async () => {
    const fake = daemon();
    const { user } = await mount(fake);
    await screen.findAllByTestId("ws-row");
    await chooseAction(user, "More actions", "Archive finished (1)");
    await user.click(screen.getByRole("button", { name: "Keep" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    const menu = await openMenu(user, "More actions");
    expect(within(menu).getByRole("menuitem", { name: "Archive finished (1)" })).not.toHaveAttribute("aria-disabled");
    expect(paramsOf(fake, "tasks.archive")).toHaveLength(0);
  });

  it("disables the bulk archive when no task is finished", async () => {
    const fake = daemon((f) => {
      f.tasks = f.tasks.filter((task) => task.contract.id !== "a1");
    });
    const { user } = await mount(fake);
    await screen.findAllByTestId("ws-row");
    const menu = await openMenu(user, "More actions");
    const item = within(menu).getByRole("menuitem", { name: "Archive finished (0)" });
    expect(item).toHaveAttribute("aria-disabled", "true");
    await waitFor(() => expect(item).toHaveAccessibleDescription("No finished task to archive."));
  });

  it("hides every archive control once the daemon answers method-unavailable", async () => {
    const fake = daemon();
    const { user } = await mountOpen(fake);
    await openTab(user, "Tasks");
    await screen.findAllByTestId("task-row");
    fake.failNext("tasks.archive", { code: -32601, message: "method not found" });
    await user.click(screen.getByRole("button", { name: "Archive Ship the parser" }));
    await waitFor(() => expect(screen.queryByRole("button", { name: "Archive Ship the parser" })).not.toBeInTheDocument());
    expect(screen.getAllByTestId("task-row")).toHaveLength(2);
    await backToList(user);
    const menu = await openMenu(user, "More actions");
    expect(within(menu).getByRole("menuitem", { name: "Manage projects" })).toBeInTheDocument();
    expect(within(menu).queryByRole("menuitem", { name: /Archive finished/ })).not.toBeInTheDocument();
  });

  describe("plan editing", () => {
    /** Adds a queued "Write docs" (under Docs) and a failed "Fix lexer" to Alpha stream: both can be replaced. */
    const planDaemon = () =>
      daemon((fake, state) => {
        fake.tasks.push(
          mkTask("a3", A, { state: "Queued", contract: { objective: "Write docs", agent: "codex" } }),
          mkTask("a4", A, { state: "Failed", contract: { objective: "Fix lexer", agent: "codex" } }),
        );
        state.links.push(
          { task_id: "a3", workstream_id: "ws-a", objective_id: "obj-a2" },
          { task_id: "a4", workstream_id: "ws-a", objective_id: null },
        );
      });
    const open = async (user: User, button: string, form: string) => {
      await user.click(await screen.findByRole("button", { name: button }));
      return screen.getByRole(button === "Change execution policy" ? "group" : "form", { name: form });
    };
    const rowOf = (label: string) => screen.getAllByTestId("task-row").find((row) => row.textContent?.includes(label));
    /** Opens Alpha stream's Tasks tab, where the plan-editing controls live. */
    const mountPlan = async (fake: FakeDaemon) => {
      const mounted = await mountOpen(fake);
      await openTab(mounted.user, "Tasks");
      return mounted;
    };

    it("disables the edits no task qualifies for and says why", async () => {
      await mountPlan(daemon());
      await screen.findAllByTestId("task-row");
      for (const name of ["Split task", "Reassign", "Change execution policy"]) {
        expect(screen.getByRole("button", { name })).toBeDisabled();
        expect(screen.getByRole("button", { name })).toHaveAccessibleDescription(/no task here can be replaced/);
      }
      expect(screen.getByRole("button", { name: "Combine tasks" })).toHaveAccessibleDescription(/two tasks that can be replaced/);
      expect(screen.getByRole("button", { name: "Remove task" })).toHaveAccessibleDescription(/running, verifying, cited as gate evidence/);
      expect(screen.getByRole("button", { name: "Add task" })).toBeEnabled();
    });

    it("adds a task under an objective", async () => {
      const fake = daemon();
      const { user } = await mountPlan(fake);
      const form = await open(user, "Add task", "Add task");
      expect(within(form).getByLabelText("Project")).toHaveValue(A);
      await user.type(within(form).getByLabelText("Task objective"), "Add a REPL");
      await user.selectOptions(within(form).getByLabelText("Agent"), "codex");
      await user.selectOptions(within(form).getByLabelText("Under objective"), "obj-a2");
      await user.click(within(form).getByRole("button", { name: "Queue task" }));
      expect(await screen.findByText(/Add task "Add a REPL": recorded by the daemon/)).toBeInTheDocument();
      await waitFor(() => expect(rowOf("Add a REPL")).toHaveTextContent("Docs"));
      expect(paramsOf(fake, "workstreams.add_task")[0]).toMatchObject({
        workstream_id: "ws-a",
        objective_id: "obj-a2",
        contract: { project_id: A, objective: "Add a REPL", kind: "Implement", agent: "codex" },
      });
      expect(screen.queryByRole("form", { name: "Add task" })).not.toBeInTheDocument();
    });

    it("splits a task into one new task per line and cancels the original", async () => {
      const fake = planDaemon();
      const { user } = await mountPlan(fake);
      const form = await open(user, "Split task", "Split task");
      expect(within(within(form).getByLabelText("Task to split")).getAllByRole("option").map((option) => option.getAttribute("value"))).toEqual(["", "a3", "a4"]);
      await user.selectOptions(within(form).getByLabelText("Task to split"), "a3");
      await user.type(within(form).getByLabelText("New task objectives, one per line"), "Draft docs{Enter}Review docs");
      await user.click(within(form).getByRole("button", { name: "Split into 2 tasks" }));
      await waitFor(() => expect(rowOf("Review docs")).toBeDefined());
      expect(rowOf("Draft docs")).toHaveTextContent("Docs");
      await waitFor(() => expect(rowOf("Write docs")).toHaveTextContent("Cancelled"));
      expect(paramsOf(fake, "workstreams.split_task")[0]).toMatchObject({ workstream_id: "ws-a", task_id: "a3", objectives: ["Draft docs", "Review docs"] });
    });

    it("combines the checked tasks, prefilling the objective from theirs", async () => {
      const fake = planDaemon();
      const { user } = await mountPlan(fake);
      const form = await open(user, "Combine tasks", "Combine tasks");
      const submit = within(form).getByRole("button", { name: "Combine into one task" });
      await user.click(within(form).getByRole("checkbox", { name: /Write docs/ }));
      expect(submit).toBeDisabled();
      await user.click(within(form).getByRole("checkbox", { name: /Fix lexer/ }));
      expect(within(form).getByLabelText("Combined objective")).toHaveValue("Write docs\nFix lexer");
      await user.click(submit);
      await waitFor(() => expect(screen.getAllByTestId("task-row")).toHaveLength(5));
      expect(paramsOf(fake, "workstreams.combine_tasks")[0]).toMatchObject({ task_ids: ["a3", "a4"], objective: "Write docs\nFix lexer" });
    });

    it("removes only a task that may leave, after a confirmation that says what happens", async () => {
      const fake = planDaemon();
      const { user } = await mountPlan(fake);
      const form = await open(user, "Remove task", "Remove task");
      expect(within(within(form).getByLabelText("Task to remove")).getAllByRole("option").map((option) => option.getAttribute("value"))).toEqual(["", "a3", "a4"]);
      await user.selectOptions(within(form).getByLabelText("Task to remove"), "a3");
      expect(form).toHaveTextContent("the daemon cancels it and then unlinks it");
      await user.click(within(form).getByRole("button", { name: "Confirm remove" }));
      await waitFor(() => expect(rowOf("Write docs")).toBeUndefined());
      expect(paramsOf(fake, "workstreams.remove_task")[0]).toMatchObject({ workstream_id: "ws-a", task_id: "a3" });
    });

    it("reassigns a task to another agent, never to the one it has", async () => {
      const fake = planDaemon();
      const { user } = await mountPlan(fake);
      const form = await open(user, "Reassign", "Reassign task");
      await user.selectOptions(within(form).getByLabelText("Task to reassign"), "a4");
      expect(within(form).getByRole("option", { name: "Codex (current)" })).toBeDisabled();
      await user.selectOptions(within(form).getByLabelText("Agent"), "claude-code");
      await user.click(within(form).getByRole("button", { name: "Reassign task" }));
      expect(await screen.findByText(/Reassign task a4 to Claude Code: recorded by the daemon/)).toBeInTheDocument();
      const [call] = paramsOf(fake, "workstreams.reassign_task");
      expect(call).toMatchObject({ workstream_id: "ws-a", task_id: "a4", agent: "claude-code" });
      expect(call).not.toHaveProperty("model_selection");
    });

    it("changes the execution policy from the task's contract and sends only what changed", async () => {
      const fake = planDaemon();
      const { user } = await mountPlan(fake);
      const group = await open(user, "Change execution policy", "Change execution policy");
      await user.selectOptions(within(group).getByLabelText("Task to change"), "a4");
      const form = within(group).getByRole("form", { name: "Execution policy of Fix lexer" });
      expect(within(form).getByLabelText("Permission")).toHaveValue("restricted");
      expect(within(form).getByLabelText("Delivery")).toHaveValue("LocalReview");
      const apply = within(form).getByRole("button", { name: "Apply policy" });
      expect(apply).toBeDisabled();
      await user.selectOptions(within(form).getByLabelText("Delivery"), "PullRequest");
      await user.clear(within(form).getByLabelText("Max retries"));
      await user.type(within(form).getByLabelText("Max retries"), "3");
      await user.click(apply);
      expect(await screen.findByText(/Change execution policy of task a4: recorded by the daemon/)).toBeInTheDocument();
      const strip = ({ idempotency_key: _key, ...rest }: Record<string, unknown>) => rest;
      expect(paramsOf(fake, "workstreams.set_task_policy").map(strip)).toEqual([
        { workstream_id: "ws-a", task_id: "a4", delivery: "PullRequest", max_retries: 3 },
      ]);
    });
  });

  it("suspends and resumes with an advisory note, and updates at the revision it read", async () => {
    const fake = daemon();
    const { user } = await mountOpen(fake);
    await chooseAction(user, "Workstream actions", "Suspend workstream");
    await waitFor(() => expect(screen.getByTestId("ws-status")).toHaveTextContent("Suspended"));
    expect(paramsOf(fake, "workstreams.update")[0]).toMatchObject({ workstream_id: "ws-a", expected_revision: 2, state: "suspended" });
    expect(within(screen.getByTestId("ws-technical")).getByText(/Suspend is advisory/)).toBeInTheDocument();
    await backToList(user);
    expect(screen.getByText(/Suspend workstream: recorded by the daemon/)).toBeInTheDocument();
    await openTab(user, "Suspended 1");
    expect(within(screen.getByRole("tabpanel")).getAllByTestId("ws-row").map(titleOf)).toEqual(["Alpha stream"]);
    expect(within(screen.getByRole("tabpanel")).getByTestId("ws-row")).toHaveTextContent("Suspended");
    await openWorkstream(user, "Alpha stream");
    await chooseAction(user, "Workstream actions", "Resume workstream");
    await waitFor(() => expect(screen.getByTestId("ws-status")).toHaveTextContent("On track"));
    expect(paramsOf(fake, "workstreams.update")[1]).toMatchObject({ expected_revision: 3, state: "active" });
  });

  it("archives only after a confirmation, then locks every control with a reason", async () => {
    const fake = daemon();
    const { user } = await mountOpen(fake);
    await chooseAction(user, "Workstream actions", "Archive workstream");
    expect(writes(fake)).toHaveLength(0);
    await user.click(screen.getByRole("button", { name: "Keep workstream" }));
    expect(writes(fake)).toHaveLength(0);
    await chooseAction(user, "Workstream actions", "Archive workstream");
    await user.click(screen.getByRole("button", { name: /Confirm archive/ }));
    await waitFor(() => expect(screen.getByTestId("ws-status")).toHaveTextContent("Archived"));
    const menu = await openMenu(user, "Workstream actions");
    const suspend = within(menu).getByRole("menuitem", { name: "Suspend workstream" });
    expect(suspend).toHaveAttribute("aria-disabled", "true");
    expect(suspend).toHaveAccessibleDescription("Disabled: an archived workstream is read-only.");
    await user.keyboard("{Escape}");

    const overviewAdd = screen.getByRole("button", { name: "+ Add objective" });
    expect(overviewAdd).toBeDisabled();
    expect(overviewAdd).toHaveAccessibleDescription(/archived workstream is read-only/);
    await openTab(user, "Objectives");
    expect(screen.getByRole("button", { name: "+ Add objective" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Apply to CLI wired" })).toBeDisabled();
    await openTab(user, "Tasks");
    expect(screen.getByRole("button", { name: "Add task" })).toHaveAccessibleDescription(/archived workstream is read-only/);
    for (const note of within(screen.getByRole("tabpanel")).getAllByText(/archived workstream is read-only/)) expect(note).toBeVisible();
  });

  it("answers a stale-revision conflict by reloading, never by overwriting", async () => {
    const fake = daemon();
    const { user } = await mountOpen(fake);
    await openTab(user, "Objectives");
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
    const { user } = await mountOpen(fake);
    await openTab(user, "Tasks");
    await screen.findAllByTestId("task-row");
    fake.openError = new DaemonError("disconnected", "down");
    fake.drop();
    await waitFor(() => expect(screen.getByTestId("ws-source")).toHaveTextContent("Stale"));
    expect(screen.getAllByTestId("task-row")).toHaveLength(2);
    const menu = await openMenu(user, "Workstream actions");
    for (const name of ["Suspend workstream", "Prioritize", "Archive workstream"]) {
      expect(within(menu).getByRole("menuitem", { name })).toHaveAttribute("aria-disabled", "true");
    }
    expect(
      within(menu).getByText("Disabled: the connection was lost. Changes resume when the daemon is reachable again."),
    ).toBeInTheDocument();
  });

  it("has a Refresh control that re-reads the daemon, so another client's edit shows without an own action", async () => {
    const fake = daemon();
    const { user } = await mount(fake);
    await screen.findAllByTestId("ws-row");
    streams.get(fake)!.workstreams[0]!.title = "Alpha renamed elsewhere";
    const before = fake.callsTo("workstreams.list").length;
    await chooseAction(user, "More actions", "Refresh");
    expect(await within(screen.getByTestId("workstreams-view")).findByText("Alpha renamed elsewhere")).toBeInTheDocument();
    expect(fake.callsTo("workstreams.list").length).toBeGreaterThan(before);
  });

  it("clears a failure banner when the operator refreshes, and its advice names a control that exists", async () => {
    const fake = daemon();
    const { user } = await mountOpen(fake);
    await openTab(user, "Objectives");
    await screen.findAllByTestId("ws-gate");
    streams.get(fake)!.objectives[0]!.revision = 9;
    await user.click(screen.getByRole("button", { name: "Apply to CLI wired" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(/Refresh to see the current state/);
    // Refresh lives in the list screen's "More actions" menu; the notice follows the operator there.
    await backToList(user);
    expect(screen.getByRole("alert")).toHaveTextContent(/Refresh to see the current state/);
    await chooseAction(user, "More actions", "Refresh");
    await waitFor(() => expect(screen.queryByRole("alert")).not.toBeInTheDocument());
    expect(screen.queryByText("The workstream data was reloaded from the daemon.")).not.toBeInTheDocument();
  });

  it("clears a failure banner once the connection recovers", async () => {
    const fake = daemon();
    const { services, user } = await mountOpen(fake);
    await openTab(user, "Objectives");
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
    const rows = await screen.findAllByTestId("ws-row");
    expect(rows.map(titleOf)).toEqual(["Alpha stream", "Beta stream", "Ghost stream"]);
    expect(rows[2]).toHaveTextContent("Tasks unknown");
    expect(screen.getByTestId("ws-unlinked")).toHaveTextContent("Tasks not in any workstream: unknown.");
    const ghostRow = (await screen.findAllByTestId("ws-row")).find((row) => row.textContent?.includes("Ghost stream"));
    expect(ghostRow).toHaveTextContent("Tasks unknown");
    expect(ghostRow).not.toHaveTextContent("0 of");
    await openWorkstream(user, "Ghost stream");
    expect(screen.getByTestId("ws-status")).toHaveTextContent("Tasks unknown");
    expect(screen.getByTestId("ws-unread-reason")).toHaveTextContent(/could not be read/);
    const menu = await openMenu(user, "Workstream actions");
    const suspend = within(menu).getByRole("menuitem", { name: "Suspend workstream" });
    expect(suspend).toHaveAttribute("aria-disabled", "true");
    expect(suspend).toHaveAccessibleDescription(/could not be read/);
    await user.keyboard("{Escape}");
    expect(screen.getByRole("button", { name: "+ Add objective" })).toBeDisabled();
    await openTab(user, "Tasks");
    expect(screen.queryAllByTestId("task-row")).toHaveLength(0);
    expect(screen.queryByText("No tasks")).not.toBeInTheDocument();
    expect(screen.getByTestId("ws-unlinked-unknown")).toBeInTheDocument();
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
    const { user } = await mountOpen(big());
    await openTab(user, "Tasks");
    expect(await screen.findAllByTestId("task-row")).toHaveLength(150);
    expect(screen.getByRole("region", { name: "Tasks of Alpha stream" })).toHaveAttribute("tabindex", "0");
    await openTab(user, "Dependencies");
    expect(screen.getAllByTestId("graph-node").length).toBeLessThanOrEqual(16);
    expect(screen.getByText(/more connected tasks are not drawn/)).toBeInTheDocument();
  });
});

describe("dependency graph", () => {
  it("draws the SVG with a text equivalent on wide layouts", async () => {
    const { user } = await mountOpen(daemon());
    await openTab(user, "Dependencies");
    const svg = await screen.findByRole("img", { name: /Task dependency graph/ });
    expect(within(svg as unknown as HTMLElement).getAllByTestId("graph-node")).toHaveLength(2);
    expect(screen.getByText("Dependencies as text").closest("details")).not.toHaveAttribute("open");
  });

  it("falls back to the text list, without the SVG, on narrow layouts", async () => {
    window.matchMedia = ((query: string) => {
      const max = /max-width:\s*(\d+)px/.exec(query);
      return { matches: max ? 500 <= Number(max[1]) : false, media: query, addEventListener: () => {}, removeEventListener: () => {} };
    }) as unknown as typeof window.matchMedia;
    const { user } = await mountOpen(daemon());
    await openTab(user, "Dependencies");
    expect(screen.queryByRole("img", { name: /Task dependency graph/ })).not.toBeInTheDocument();
    expect(screen.getByText("Dependencies as text").closest("details")).toHaveAttribute("open");
    expect(screen.getByText(/waits on:/)).toHaveTextContent("Ship the parser (Succeeded)");
  });
});

describe("states without data", () => {
  it("is empty only when the daemon says no workstreams exist, and never shows fixtures", async () => {
    await mount(daemon((fake, state) => ((fake.projects = []), (fake.tasks = []), (state.workstreams = []), (state.objectives = []), (state.links = []))));
    expect(await screen.findByText("No workstreams")).toBeInTheDocument();
    expect(within(screen.getByTestId("workstreams-view")).getByText(/Ask the global orchestrator to create one/)).toBeInTheDocument();
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
    render(<App services={services} globalViews={workstreamsGlobalViews} />);
    expect(screen.getAllByText("Loading workstreams…").length).toBeGreaterThan(0);
    expect(screen.queryByText("No workstreams")).not.toBeInTheDocument();
  });

  it("reads workstreams as offline, not loading, once the first connection has failed", async () => {
    const fake = daemon();
    fake.openError = new DaemonError("disconnected", "down");
    const services = createServices({
      connector: fake,
      initialRoute: VIEW,
      connection: { backoffMs: [5], requestTimeoutMs: 250 },
      data: { coalesceMs: 10 },
      sources: createFixtureSources(),
    });
    render(<App services={services} globalViews={workstreamsGlobalViews} />);
    await waitFor(() => expect(services.connection.getSnapshot().failedAttempts).toBeGreaterThan(0));
    expect(within(screen.getByTestId("workstreams-view")).getByText(/Offline: no cached workstreams/)).toBeInTheDocument();
    expect(screen.queryByText("Loading workstreams…")).not.toBeInTheDocument();
  });
});

describe("fixture mode", () => {
  it("labels every fixture section PLACEHOLDER, shows its objectives read-only and never links fixture tasks", async () => {
    const { user } = await mount(null);
    await screen.findAllByTestId("ws-row");
    const view = screen.getByTestId("workstreams-view");
    expect(within(view).getAllByTestId("badge-D9").length).toBeGreaterThanOrEqual(1);
    await openWorkstream(user, "Auth hardening");
    expect(within(view).getAllByTestId("badge-D9")).toHaveLength(1);
    expect(screen.getByRole("button", { name: "+ Add objective" })).toBeDisabled();
    await openTab(user, "Objectives");
    expect(screen.getAllByTestId("ws-objective").length).toBeGreaterThanOrEqual(2);
    expect(screen.getByRole("button", { name: "+ Add objective" })).toBeDisabled();
    await openTab(user, "Tasks");
    expect(within(view).queryAllByRole("link", { name: /Rotate signing keys/ })).toHaveLength(0);
    expect(document.querySelector('a[href*="fixture"]')).toBeNull();
    for (const name of ["Add task", "Remove task"]) {
      expect(screen.getByRole("button", { name })).toBeDisabled();
    }
    expect(screen.queryByRole("button", { name: /Apply to/ })).not.toBeInTheDocument();
  });

  it("shows no wait reasons beside fixture tasks and reads no queue diagnostics", async () => {
    const fake = daemon();
    const { user } = await mount(fake, { fixtureOverBridge: true });
    await openWorkstream(user, "Auth hardening");
    await openTab(user, "Tasks");
    const rows = await screen.findAllByTestId("task-row");
    expect(rows.some((row) => row.textContent?.includes("Queued"))).toBe(true);
    await act(() => new Promise((resolve) => setTimeout(resolve, 50)));
    expect(screen.queryAllByTestId("queue-wait")).toHaveLength(0);
    expect(fake.callsTo("queue.snapshot")).toHaveLength(0);
  });

  it("disables the actions on placeholder data and sends nothing to the daemon, even when one is connected", async () => {
    const fake = daemon();
    const { user } = await mount(fake, { fixtureOverBridge: true });
    await screen.findAllByTestId("ws-row");
    const listMenu = await openMenu(user, "More actions");
    expect(within(listMenu).getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Manage projects"]);
    await user.keyboard("{Escape}");
    await openWorkstream(user, "Auth hardening");
    const menu = await openMenu(user, "Workstream actions");
    for (const name of ["Suspend workstream", "Prioritize", "Archive workstream"]) {
      const item = within(menu).getByRole("menuitem", { name });
      expect(item).toHaveAttribute("aria-disabled", "true");
      expect(item).toHaveAccessibleDescription(/not wired to anything/);
    }
    expect(screen.queryByRole("button", { name: /Create workstream/ })).not.toBeInTheDocument();
    expect(fake.calls.filter((call) => /^workstreams\.(create|update|prioritize|add_|revise_|set_|link_|unlink_)/.test(call.method))).toEqual([]);
  });

  it.each<[SurfaceFixtureScenario, RegExp]>([
    ["unavailable", /Unavailable: Fixture scenario/],
    ["unsupported", /Not supported: Fixture scenario/],
    ["unknown", /Unknown: Fixture scenario/],
  ])("reports the %s scenario with its reason", async (scenario, text) => {
    await mount(null, { scenario });
    await waitFor(() => {
      const message = within(screen.getByTestId("workstreams-view")).getByText(text);
      expect(message).toHaveTextContent(/observed \d+ s ago/);
    });
    expect(screen.queryAllByTestId("task-row")).toHaveLength(0);
  });

  it("stays on loading while the source has not answered", async () => {
    await mount(null, { scenario: "loading" });
    expect(screen.getAllByText("Loading workstreams…").length).toBeGreaterThan(0);
  });
});
