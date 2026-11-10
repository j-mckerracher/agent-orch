import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import App from "../../../App";
import { createFixtureSources } from "../../../sources/fixtures";
import { createFixtureSurfaces, type SurfaceFixtureScenario } from "../../../sources/surfaces/fixtures";
import { FakeDaemon, mkApproval, mkSession, mkTask } from "../../../test/fakeDaemon";
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

function daemon(extra: (fake: FakeDaemon) => void = () => {}) {
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
  extra(fake);
  return fake;
}

const tree = () => screen.getByRole("tree", { name: "Workstreams grouped by project" });
const level = (n: number) => Array.from(tree().querySelectorAll<HTMLElement>(`[role="treeitem"][aria-level="${n}"]`));
const groupsReady = () => waitFor(() => expect(level(1).length).toBeGreaterThan(0));
const original = window.matchMedia;

afterEach(() => {
  window.matchMedia = original;
  window.location.hash = "";
});

describe("live data", () => {
  it("renders one workstream per project, grouped by project, with no placeholder content", async () => {
    await mount(daemon());
    await groupsReady();
    const groups = level(1);
    expect(groups.map((group) => group.getAttribute("aria-label"))).toEqual([
      "Workstream 1 of 2: alpha, 2 tasks",
      "Workstream 2 of 2: beta, 1 tasks",
    ]);
    expect(screen.getAllByText("Grouped by project", { exact: false }).length).toBeGreaterThanOrEqual(2);
    const mine = [screen.getByTestId("workstream-tree"), screen.getByTestId("workstreams-view")];
    for (const region of mine) {
      expect(region.textContent).not.toMatch(/PLACEHOLDER|Auth hardening|Docs refresh/);
    }
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

  it("shows worker rows as adapter · state under the open group only", async () => {
    await mount(daemon());
    await groupsReady();
    const workers = within(tree()).getAllByTestId("tree-worker");
    expect(workers.map((worker) => worker.textContent)).toEqual(["codex · Running"]);
    expect(within(tree()).getByText("fake-agent · no session")).toBeInTheDocument();
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

  it("derives the table: state, assignment and required evidence", async () => {
    await mount(daemon());
    const rows = await screen.findAllByTestId("task-row");
    expect(rows).toHaveLength(2);
    expect(rows[1]).toHaveTextContent("Running");
    expect(rows[1]).toHaveTextContent("codex · Running · attempt 1");
    expect(rows[1]).toHaveTextContent("Candidate from the running attempt");
    expect(rows[0]).toHaveTextContent("None: task succeeded");
  });

  it("selecting another workstream swaps the table and keeps unsupported objective honest", async () => {
    const { user } = await mount(daemon());
    await screen.findAllByTestId("task-row");
    expect(screen.getByTestId("ws-objective")).toHaveTextContent(/Objective unsupported/);
    await user.click(screen.getByRole("button", { name: /2\. beta/ }));
    const rows = screen.getAllByTestId("task-row");
    expect(rows).toHaveLength(1);
    expect(rows[0]).toHaveTextContent("None until dispatched");
  });

  it("disables unsupported actions and shows the reason", async () => {
    await mount(daemon());
    await screen.findAllByTestId("task-row");
    for (const name of ["Revise objective", "Add objective", "Suspend new dispatch", "Resolve contract decision", "Prioritize", "Archive", "Add task", "Reassign"]) {
      const button = screen.getByRole("button", { name });
      expect(button).toBeDisabled();
      const reason = button.getAttribute("aria-describedby");
      expect(reason && document.getElementById(reason)?.textContent).toMatch(/Disabled|daemon/);
    }
    expect(screen.getByRole("button", { name: "Create workstream" })).toBeDisabled();
    expect(screen.getByText(/daemon cannot create workstreams/)).toBeVisible();
  });

  it("turns Resolve contract decision into a review link when an approval is pending", async () => {
    await mount(daemon((fake) => (fake.approvals = [mkApproval("ap1", "a2")])));
    const link = await screen.findByRole("link", { name: "Resolve contract decision" });
    expect(link).toHaveAttribute("href", "#/project/%2Fwork%2Falpha/review/a2");
    expect((await screen.findAllByTestId("task-row"))[1]).toHaveTextContent("Operator decision on LocalReviewMerge approval");
  });

  it("filters the list and announces the shown count", async () => {
    const { user } = await mount(daemon());
    await screen.findAllByTestId("task-row");
    await user.type(screen.getByRole("searchbox", { name: "Filter workstreams" }), "bet");
    expect(screen.getByTestId("ws-showing")).toHaveTextContent("Showing 1 of 2");
    expect(screen.getByTestId("ws-list-count")).toHaveTextContent("2");
  });

  it("follows live changes without losing the operator's collapsed groups", async () => {
    const fake = daemon();
    const { services, user } = await mount(fake);
    const first = await within(tree()).findByRole("treeitem", { name: /Workstream 1 of 2/ });
    await user.click(first);
    expect(first).toHaveAttribute("aria-expanded", "false");
    await user.click(within(tree()).getByRole("treeitem", { name: /Workstream 2 of 2/ }));
    expect(within(tree()).getByText("Cache downloads")).toBeInTheDocument();

    fake.tasks = [...fake.tasks, mkTask("b2", B, { state: "Running", contract: { objective: "Pin toolchain" } })];
    await act(async () => services.data.refreshAll());
    await within(tree()).findByText("Pin toolchain");
    expect(within(tree()).getByRole("treeitem", { name: /Workstream 1 of 2/ })).toHaveAttribute("aria-expanded", "false");
    expect(within(tree()).getByRole("treeitem", { name: /Workstream 2 of 2/ })).toHaveAttribute("aria-expanded", "true");
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
    daemon((fake) => {
      fake.tasks = Array.from({ length: 150 }, (_, index) =>
        mkTask(`t${index}`, A, { contract: { objective: `Task number ${index}`, dependencies: index > 0 ? [`t${index - 1}`] : [] } }),
      );
    });

  it("renders every task in independently scrollable containers and keeps the counts honest", async () => {
    await mount(big());
    expect(await screen.findAllByTestId("task-row")).toHaveLength(150);
    expect(level(2)).toHaveLength(150);
    expect(level(1)[0]).toHaveAttribute("aria-label", "Workstream 1 of 2: alpha, 150 tasks");
    expect(screen.getByRole("region", { name: "Tasks of alpha" })).toHaveAttribute("tabindex", "0");
    expect(tree().closest(".shell-tree-body")).not.toBeNull();
    expect(screen.getAllByTestId("graph-node").length).toBeLessThanOrEqual(16);
    expect(screen.getByText(/more connected tasks are not drawn/)).toBeInTheDocument();
  });
});

describe("dependency graph", () => {
  it("draws the SVG with a text equivalent on wide layouts", async () => {
    await mount(daemon());
    const svg = await screen.findByRole("img", { name: /Task dependency graph/ });
    expect(svg).toBeInTheDocument();
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
  it("is empty only when the daemon says no projects exist", async () => {
    await mount(daemon((fake) => ((fake.projects = []), (fake.tasks = []))));
    expect(await screen.findByText("No workstreams")).toBeInTheDocument();
    expect(within(screen.getByRole("navigation", { name: "Workstreams" })).getByText(/No projects are registered/)).toBeInTheDocument();
  });

  it("shows loading, never empty, before the daemon has answered", async () => {
    const services = createServices({ connector: new FakeDaemon(), initialRoute: VIEW, sources: createFixtureSources() });
    render(<App services={services} shellSlots={workstreamsShellSlots} globalViews={workstreamsGlobalViews} />);
    expect(screen.getAllByText("Loading workstreams…").length).toBeGreaterThan(0);
    expect(screen.queryByText("No workstreams")).not.toBeInTheDocument();
  });
});

describe("fixture mode", () => {
  it("labels every fixture section PLACEHOLDER and never links fixture tasks", async () => {
    await mount(null);
    await screen.findAllByTestId("task-row");
    const view = screen.getByTestId("workstreams-view");
    expect(within(view).getAllByTestId("badge-D9").length).toBeGreaterThanOrEqual(4);
    expect(within(screen.getByTestId("workstream-tree")).getAllByText(/PLACEHOLDER DATA/).length).toBeGreaterThanOrEqual(1);
    expect(screen.getByTestId("workstreams-count")).toHaveTextContent("3");
    expect(screen.getByTestId("ws-objective")).toHaveTextContent("Harden session handling");
    expect(screen.getByText("Revision 3 · 1 of 2 gates passed")).toBeInTheDocument();
    expect(within(view).queryAllByRole("link", { name: /Rotate signing keys/ })).toHaveLength(0);
    expect(document.querySelector('a[href*="fixture"]')).toBeNull();
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
