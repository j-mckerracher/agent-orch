import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import App from "../../App";
import { DaemonError } from "../../daemon/errors";
import { createFixtureSources } from "../../sources/fixtures";
import { createFixtureSurfaces } from "../../sources/surfaces/fixtures";
import { FakeDaemon, mkApproval, mkEvent, mkTask } from "../../test/fakeDaemon";
import { REPO, connected } from "../../test/render";
import { createServices } from "../createServices";
import { DEFAULT_ROUTE } from "../router";
import { ActivityPanel } from "./ActivityPanel";

function servicesFor(fake: FakeDaemon | null) {
  return createServices({
    connector: fake ?? undefined,
    initialRoute: DEFAULT_ROUTE,
    connection: { backoffMs: [5], requestTimeoutMs: 250 },
    data: { coalesceMs: 10 },
    sources: createFixtureSources(),
  });
}

type Tester = ReturnType<typeof userEvent.setup>;

/** Opens the popover from its trigger, as a user does. */
async function openActivity(user: Tester, panel: HTMLElement) {
  const trigger = within(panel).getByRole("button", { name: /^Workspace activity/ });
  if (trigger.getAttribute("aria-expanded") !== "true") await user.click(trigger);
}

/** The <details> of a top-level section or queue sub-group, found by its visible title. */
function detailsFor(panel: HTMLElement, title: string): HTMLDetailsElement {
  const summary = within(panel).getByText(title, { selector: "summary > .ui-disclosure-title" }).closest("summary");
  return summary?.parentElement as HTMLDetailsElement;
}

async function expandSection(user: Tester, panel: HTMLElement, title: string) {
  const details = detailsFor(panel, title);
  if (!details.open) await user.click(details.querySelector("summary") as HTMLElement);
}

async function mountPanel(fake: FakeDaemon | null, options: { open?: boolean } = {}) {
  const { open = true } = options;
  const services = servicesFor(fake);
  const user = userEvent.setup();
  render(<App services={services} shellSlots={{ activity: <ActivityPanel /> }} />);
  if (fake) await connected();
  const panel = await screen.findByTestId("workspace-activity");
  if (open) await openActivity(user, panel);
  return { services, user, panel };
}

function liveDaemon() {
  const fake = new FakeDaemon();
  fake.projects = [{ id: REPO }];
  fake.tasks = [
    mkTask("t1", REPO, { state: "Running" }),
    mkTask("t2", REPO, { state: "Queued" }),
    mkTask("t3", REPO, { state: "Queued", contract: { dependencies: ["t1"] } }),
  ];
  fake.approvals = [mkApproval("a1", "t1"), mkApproval("a2", "t2", "approved")];
  return fake;
}

afterEach(() => {
  window.location.hash = "";
});

describe("fixture mode", () => {
  it("sends no mutation to a connected daemon when the fixture surfaces are active (Stop, Pause)", async () => {
    const fake = liveDaemon();
    const services = createServices({
      connector: fake,
      initialRoute: DEFAULT_ROUTE,
      connection: { backoffMs: [5], requestTimeoutMs: 250 },
      data: { coalesceMs: 10 },
      sources: createFixtureSources(),
      surfaces: createFixtureSurfaces(),
    });
    const user = userEvent.setup();
    render(<App services={services} shellSlots={{ activity: <ActivityPanel /> }} />);
    await connected();
    const panel = await screen.findByTestId("workspace-activity");
    await openActivity(user, panel);
    await expandSection(user, panel, "Execution queue");
    await expandSection(user, panel, "Agent slots");
    const before = fake.calls.length;
    await user.click(await within(panel).findByRole("button", { name: /Stop Add refresh-token expiry/ }));
    await user.click(within(panel).getByRole("button", { name: "Confirm stop" }));
    await waitFor(() => expect(within(panel).getAllByTestId("simulated-receipt")).toHaveLength(1));
    await user.click(within(panel).getByRole("button", { name: "Pause new dispatch" }));
    await waitFor(() => expect(within(panel).getAllByTestId("simulated-receipt")).toHaveLength(2));
    expect(fake.calls.slice(before).filter((call) => /^(tasks\.(cancel|retry|submit)|approvals\.resolve|queue\.(pause|resume))$/.test(call.method))).toEqual([]);
    expect(fake.calls.slice(before).filter((call) => /fixture:/.test(JSON.stringify(call.params)))).toEqual([]);
  });

  it("labels every placeholder group and shows the sample numbers", async () => {
    const { panel } = await mountPanel(null);
    await waitFor(() => expect(within(panel).getByTestId("activity-slots-value")).toHaveTextContent("20 of 24"));
    expect(within(panel).getAllByTestId("badge-D11").length).toBeGreaterThanOrEqual(4);
    expect(within(panel).getByTestId("activity-badge")).toHaveTextContent("—");
    expect(within(panel).getByTestId("activity-badge")).toHaveTextContent(/sample data is not counted/);
    expect(screen.getByTestId("nav-review-count")).toHaveTextContent("—");
    expect(screen.getByTestId("top-counts")).toHaveTextContent("— require attention");
    const queue = within(panel).getByTestId("activity-queue");
    expect(within(queue).getByText("Add refresh-token expiry")).toBeInTheDocument();
    expect(within(queue).getByText("First in queue")).toBeInTheDocument();
  });

  it("simulates Stop and Pause without calling any daemon", async () => {
    const { panel, user } = await mountPanel(null);
    await expandSection(user, panel, "Execution queue");
    await expandSection(user, panel, "Agent slots");
    await user.click(await within(panel).findByRole("button", { name: /Stop Add refresh-token expiry/ }));
    await user.click(within(panel).getByRole("button", { name: "Confirm stop" }));
    await waitFor(() => expect(within(panel).getAllByTestId("simulated-receipt")).toHaveLength(1));
    await user.click(within(panel).getByRole("button", { name: "Pause new dispatch" }));
    await waitFor(() => expect(within(panel).getAllByTestId("simulated-receipt")).toHaveLength(2));
  });

  it("disables the stop menu and queue settings with a visible reason", async () => {
    const { panel, user } = await mountPanel(null);
    await expandSection(user, panel, "Agent slots");
    for (const name of ["Stop menu", "Queue settings"]) {
      const button = within(panel).getByRole("button", { name });
      expect(button).toBeDisabled();
      expect(button).toHaveAccessibleDescription(/Not available:/);
    }
  });
});

describe("live mode", () => {
  it("never shows wireframe slot or budget numbers and disables unsupported controls", async () => {
    const { panel, user } = await mountPanel(liveDaemon());
    await waitFor(() => expect(within(panel).getByTestId("activity-status")).toHaveTextContent("1 running, 2 queued"));
    expect(panel).not.toHaveTextContent("20 / 24");
    expect(panel).not.toHaveTextContent("20 of 24");
    expect(panel).not.toHaveTextContent("$26");
    expect(within(panel).queryAllByTestId("badge-D11")).toHaveLength(0);
    await expandSection(user, panel, "Agent slots");
    expect(within(panel).getByTestId("activity-slots-value")).toHaveTextContent("1 of 2 slots in use · 1 available");
    for (const name of ["Pause new dispatch", "Stop menu", "Queue settings"]) {
      const button = within(panel).getByRole("button", { name });
      expect(button).toBeDisabled();
      expect(button).toHaveAccessibleDescription(/Not available:/);
    }
  });

  it("keeps the badge, Review nav count and top-bar count equal", async () => {
    const { panel } = await mountPanel(liveDaemon());
    await waitFor(() => expect(within(panel).getByTestId("activity-badge")).toHaveTextContent("1"));
    expect(screen.getByTestId("nav-review-count")).toHaveTextContent("1");
    expect(screen.getByTestId("top-counts")).toHaveTextContent("1 require attention");
    expect(within(panel).getByTestId("activity-attention-a1")).toBeInTheDocument();
    expect(within(panel).getByText("Priority not reported")).toBeInTheDocument();
  });

  it("splits the queue into running, up next and waiting", async () => {
    const { panel } = await mountPanel(liveDaemon());
    const waiting = await within(panel).findByTestId("activity-queue-waiting");
    expect(within(waiting).getByTestId("activity-task-t3")).toBeInTheDocument();
    expect(within(waiting).getByText("Waiting on a dependency")).toBeInTheDocument();
    expect(within(within(panel).getByTestId("activity-queue-running")).getByTestId("activity-task-t1")).toBeInTheDocument();
    const upNext = within(panel).getByTestId("activity-queue-up-next");
    expect(within(upNext).getByTestId("activity-task-t2")).toBeInTheDocument();
    expect(within(upNext).getByText("First in queue")).toBeInTheDocument();
  });

  it("shows real slot limits and capacity reasons when more tasks are queued than slots", async () => {
    const fake = new FakeDaemon();
    fake.projects = [{ id: REPO }];
    fake.tasks = [
      mkTask("r1", REPO, { state: "Running" }),
      mkTask("r2", REPO, { state: "Running" }),
      mkTask("q1", REPO, { state: "Queued" }),
      mkTask("q2", REPO, { state: "Queued" }),
      mkTask("q3", REPO, { state: "Queued", contract: { dependencies: ["r1"] } }),
    ];
    const { panel } = await mountPanel(fake);
    await waitFor(() => expect(within(panel).getByTestId("activity-slots-value")).toHaveTextContent("2 of 2 slots in use · 0 available"));
    const upNext = within(panel).getByTestId("activity-queue-up-next");
    expect(within(upNext).getByTestId("activity-task-q1")).toBeInTheDocument();
    expect(within(upNext).getByTestId("activity-task-q2")).toBeInTheDocument();
    expect(within(upNext).getAllByText("Waiting for capacity")).toHaveLength(2);
    expect(within(upNext).getAllByText("All 2 worker slots are in use.")).toHaveLength(2);
    expect(within(within(panel).getByTestId("activity-queue-waiting")).getByTestId("activity-task-q3")).toBeInTheDocument();
    expect(within(panel).getByTestId("activity-status")).toHaveTextContent("2 running, 3 queued");
  });

  it("refreshes the live queue after the daemon reconnects", async () => {
    const fake = liveDaemon();
    const { panel, services } = await mountPanel(fake);
    await waitFor(() =>
      expect(within(panel).getByTestId("activity-slots-value")).toHaveTextContent("1 of 2 slots in use"),
    );

    const generation = services.connection.getSnapshot().generation;
    fake.tasks = [
      mkTask("t1", REPO, { state: "Running" }),
      mkTask("t2", REPO, { state: "Running" }),
      mkTask("q1", REPO, { state: "Queued" }),
      mkTask("q2", REPO, { state: "Queued" }),
      mkTask("q3", REPO, { state: "Queued", contract: { dependencies: ["t1"] } }),
    ];
    act(() => fake.drop());

    await waitFor(() => {
      expect(services.connection.getSnapshot().status).toBe("connected");
      expect(services.connection.getSnapshot().generation).toBeGreaterThan(generation);
    });
    await waitFor(() => {
      expect(within(panel).getByTestId("activity-slots-value")).toHaveTextContent("2 of 2 slots in use · 0 available");
      expect(within(panel).getByTestId("activity-status")).toHaveTextContent("2 running, 3 queued");
    });
    const upNext = within(panel).getByTestId("activity-queue-up-next");
    expect(within(upNext).getAllByText("All 2 worker slots are in use.")).toHaveLength(2);
    const waiting = within(panel).getByTestId("activity-queue-waiting");
    expect(within(waiting).getByTestId("activity-task-q3")).toBeInTheDocument();
    expect(within(waiting).getByText("Waiting for t1.")).toBeInTheDocument();
  });

  it("shows slots as unavailable, never zero or sample numbers, when queue.snapshot is malformed", async () => {
    const fake = liveDaemon();
    fake.stub("queue.snapshot", {});
    const { panel } = await mountPanel(fake);
    await waitFor(() => expect(within(panel).getByTestId("activity-slots-value")).toHaveTextContent("not available"));
    expect(within(panel).getByTestId("activity-slots-value")).not.toHaveTextContent(/\b0 of\b/);
    expect(panel).not.toHaveTextContent("20 of 24");
  });

  it("shows an unavailable attention list, not an empty one, when only approvals.list fails", async () => {
    const fake = liveDaemon();
    fake.failNext("approvals.list", { code: -32603, message: "approvals boom" }, 20);
    const { panel } = await mountPanel(fake);
    await waitFor(() => expect(within(panel).getByTestId("activity-attention-degraded")).toHaveTextContent(/approvals boom/));
    expect(within(panel).getByTestId("activity-attention")).not.toHaveTextContent("Nothing needs attention.");
    expect(within(panel).getByTestId("activity-status")).toHaveTextContent("attention not available");
    expect(within(panel).getByTestId("activity-slots-value")).toHaveTextContent("of");
    expect(within(panel).getByTestId("activity-task-t1")).toBeInTheDocument();
  });

  it("keeps queue and slots and says task details are missing when only tasks.list fails", async () => {
    const fake = liveDaemon();
    fake.failNext("tasks.list", { code: -32603, message: "tasks boom" }, 20);
    const { panel } = await mountPanel(fake);
    await waitFor(() => expect(within(panel).getByTestId("activity-tasks-degraded")).toHaveTextContent(/tasks boom/));
    expect(within(panel).getByTestId("activity-task-t1")).toBeInTheDocument();
    expect(within(panel).getByTestId("activity-slots-value")).toHaveTextContent("of");
  });

  it("updates queue and attention when a live event arrives", async () => {
    const fake = liveDaemon();
    const { panel } = await mountPanel(fake);
    await waitFor(() => expect(within(panel).getByTestId("activity-badge")).toHaveTextContent("1"));
    await waitFor(() => expect(fake.callsTo("events.subscribe")).toHaveLength(1));
    fake.tasks = [...fake.tasks, mkTask("t4", REPO, { state: "Running" })];
    fake.approvals = [...fake.approvals, mkApproval("a3", "t4")];
    act(() => fake.emit(mkEvent(1, "t4", null, "TaskAdmitted")));
    await waitFor(() => {
      expect(within(panel).getByTestId("activity-badge")).toHaveTextContent("2");
      expect(within(panel).getByTestId("activity-task-t4")).toBeInTheDocument();
      expect(within(panel).getByTestId("activity-attention-a3")).toBeInTheDocument();
      expect(screen.getByTestId("nav-review-count")).toHaveTextContent("2");
      expect(screen.getByTestId("top-counts")).toHaveTextContent("2 require attention");
    });
  });

  it("stops a task through tasks.cancel only after confirmation", async () => {
    const fake = liveDaemon();
    const { panel, user } = await mountPanel(fake);
    const stop = await within(panel).findByRole("button", { name: /Stop objective of t1/ });
    await user.click(stop);
    expect(fake.callsTo("tasks.cancel")).toHaveLength(0);
    await user.click(within(panel).getByRole("button", { name: "Keep" }));
    expect(fake.callsTo("tasks.cancel")).toHaveLength(0);
    expect(within(panel).getByRole("button", { name: /Stop objective of t1/ })).toHaveFocus();
    await user.click(within(panel).getByRole("button", { name: /Stop objective of t1/ }));
    await user.click(within(panel).getByRole("button", { name: "Confirm stop" }));
    await waitFor(() => expect(fake.callsTo("tasks.cancel")).toHaveLength(1));
    expect(fake.callsTo("tasks.cancel")[0].params).toMatchObject({ task_id: "t1" });
    expect(await within(panel).findByTestId("activity-notice")).toHaveTextContent("not proof");
    await waitFor(() => expect(within(panel).getByTestId("activity-notice")).toHaveFocus());
  });

  it("disables Stop with a reason when the daemon is not connected", async () => {
    const fake = new FakeDaemon();
    fake.openError = new DaemonError("disconnected", "down");
    const services = servicesFor(fake);
    render(<App services={services} shellSlots={{ activity: <ActivityPanel /> }} />);
    const panel = await screen.findByTestId("workspace-activity");
    await openActivity(userEvent.setup(), panel);
    await waitFor(() => expect(within(panel).getByTestId("activity-badge")).toHaveTextContent("—"));
    expect(within(panel).queryByRole("button", { name: /^Stop .*\(/ })).not.toBeInTheDocument();
    expect(within(panel).getByText("Queue not available.")).toBeInTheDocument();
    expect(within(panel).getByTestId("activity-badge")).toHaveTextContent(/not available/);
    expect(within(panel).getByTestId("activity-offline")).toHaveTextContent(/observed \d+ s ago/);
  });
});

describe("accessibility", () => {
  it("adds no landmark of its own: the trigger and card live in the shell, not an aside", async () => {
    const { panel } = await mountPanel(null);
    expect(screen.queryByRole("complementary", { name: "Workspace activity" })).not.toBeInTheDocument();
    expect(panel.querySelector("aside,nav,main,[role=complementary],[role=region]")).toBeNull();
  });

  it("toggles a focusable group header", async () => {
    const { panel, user } = await mountPanel(null);
    await expandSection(user, panel, "Execution queue");
    const summary = within(panel).getByText("Agent slots", { selector: "summary > .ui-disclosure-title" }).closest("summary") as HTMLElement;
    const details = summary.parentElement as HTMLDetailsElement;
    expect(details.open).toBe(false);
    summary.focus();
    expect(summary).toHaveFocus();
    await user.click(summary);
    expect(details.open).toBe(true);
    await user.click(summary);
    expect(details.open).toBe(false);
  });

  it("links Review all decisions to the review view", async () => {
    const { panel, user, services } = await mountPanel(null);
    await expandSection(user, panel, "Attention");
    await user.click(await within(panel).findByRole("link", { name: "Review all decisions" }));
    expect(services.router.getSnapshot()).toEqual({ name: "global", view: "review" });
  });
});

describe("workspace activity popover", () => {
  it("starts collapsed: a labeled trigger with aria-expanded=false and a hidden card it controls", async () => {
    const { panel } = await mountPanel(null, { open: false });
    const trigger = within(panel).getByRole("button", { name: /^Workspace activity/ });
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    const card = document.getElementById(trigger.getAttribute("aria-controls") ?? "");
    expect(card).not.toBeNull();
    expect(card).not.toBeVisible();
  });

  it("expands from the trigger, which then exposes its card", async () => {
    const { panel, user } = await mountPanel(null, { open: false });
    const trigger = within(panel).getByRole("button", { name: /^Workspace activity/ });
    await user.click(trigger);
    expect(trigger).toHaveAttribute("aria-expanded", "true");
    const card = document.getElementById(trigger.getAttribute("aria-controls") ?? "");
    expect(card).toBeVisible();
    expect(card).toHaveTextContent("Execution queue");
  });

  it("shows the attention count on the trigger, matching the pending approvals", async () => {
    const { panel } = await mountPanel(liveDaemon(), { open: false });
    const trigger = within(panel).getByRole("button", { name: /^Workspace activity/ });
    await waitFor(() => expect(within(trigger).getByTestId("activity-badge")).toHaveTextContent("1"));
    expect(trigger).toHaveAccessibleName(/1 require attention/);
  });

  it("the_count_is_unchanged_while_the_holds_have_not_loaded", async () => {
    const fake = liveDaemon();
    // The holds read never answers, so nothing is known to be held.
    fake.stub("holds.list", () => new Promise(() => undefined));
    const { panel } = await mountPanel(fake, { open: false });
    const trigger = within(panel).getByRole("button", { name: /^Workspace activity/ });
    await waitFor(() => expect(within(trigger).getByTestId("activity-badge")).toHaveTextContent("1"));
    expect(screen.getByTestId("nav-review-count")).toHaveTextContent("1");
    expect(screen.getByTestId("top-counts")).toHaveTextContent("1 require attention");
  });

  it("the_nav_the_top_bar_and_the_badge_agree_when_something_is_held", async () => {
    const fake = liveDaemon();
    fake.approvals = [mkApproval("a1", "t1"), mkApproval("a3", "t2")];
    fake.stub("holds.list", () => ({
      holds: [{ id: "h1", subject_kind: "approval", subject_id: "t1", until_date: "2099-01-01", state: "active" }],
    }));
    const { panel } = await mountPanel(fake, { open: false });
    const trigger = within(panel).getByRole("button", { name: /^Workspace activity/ });
    await waitFor(() => expect(within(trigger).getByTestId("activity-badge")).toHaveTextContent("1"));
    expect(screen.getByTestId("nav-review-count")).toHaveTextContent("1");
    expect(screen.getByTestId("nav-review-count")).toHaveAccessibleName("1 pending decisions: 1 approvals, 0 proposed actions");
    expect(screen.getByTestId("top-counts")).toHaveTextContent("1 require attention");
  });

  it("closes on Escape and returns focus to the trigger", async () => {
    const { panel, user } = await mountPanel(null, { open: false });
    const trigger = within(panel).getByRole("button", { name: /^Workspace activity/ });
    await user.click(trigger);
    const summary = detailsFor(panel, "Agent slots").querySelector("summary") as HTMLElement;
    summary.focus();
    await user.keyboard("{Escape}");
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(trigger).toHaveFocus();
  });

  it("closes on a pointer press outside the card and stays open for presses inside it", async () => {
    const { panel, user } = await mountPanel(null, { open: false });
    const trigger = within(panel).getByRole("button", { name: /^Workspace activity/ });
    await user.click(trigger);
    await user.click(detailsFor(panel, "Agent slots").querySelector("summary") as HTMLElement);
    expect(trigger).toHaveAttribute("aria-expanded", "true");
    await user.click(document.body);
    expect(trigger).toHaveAttribute("aria-expanded", "false");
  });

  it("starts every top-level section and every queue row collapsed, Running included", async () => {
    const { panel, user } = await mountPanel(null);
    for (const title of ["Execution queue", "Attention", "Agent slots"]) expect(detailsFor(panel, title).open).toBe(false);
    await expandSection(user, panel, "Execution queue");
    // Running is a labeled group, not a disclosure; its rows (and their Stop control) stay collapsed.
    expect(within(panel).getByRole("group", { name: "Running" })).toBeInTheDocument();
    const runningRow = within(panel).getByText("Add refresh-token expiry", { selector: ".activity-item-title" }).closest("details") as HTMLDetailsElement;
    expect(runningRow.open).toBe(false);
    await user.click(runningRow.querySelector("summary") as HTMLElement);
    expect(runningRow.open).toBe(true);
    expect(within(runningRow).getByRole("button", { name: /^Stop Add refresh-token expiry/ })).toBeInTheDocument();
  });

  it("places System Usage as the last section, open by default, inside the popover", async () => {
    const { panel } = await mountPanel(null);
    const sections = panel.querySelector(".activity-sections") as HTMLElement;
    // The shell's resources slot is reserved as the last child; here it holds the default placeholder.
    const last = sections.lastElementChild as HTMLElement;
    expect(last).toBe(within(panel).getByTestId("slot-resources"));
    expect(panel.querySelector('[data-testid="activity-popover"]')).toContainElement(last);
  });

  it("keeps the honest empty states inside the expanded sections", async () => {
    const fake = liveDaemon();
    fake.approvals = [];
    const { panel, user } = await mountPanel(fake);
    await waitFor(() => expect(within(panel).getByTestId("activity-status")).toHaveTextContent("0 require attention"));
    await expandSection(user, panel, "Execution queue");
    expect(within(panel).getByRole("group", { name: "Unclassified" })).toHaveTextContent("Every queued task has a reported wait reason.");
    await expandSection(user, panel, "Attention");
    expect(within(panel).getByText("Nothing needs attention.")).toBeInTheDocument();
    expect(within(panel).getByRole("link", { name: "Review all decisions" })).toBeInTheDocument();
  });
});

describe("pending decision count", () => {
  const pendingWire = (id: string) => ({
    id,
    turn_id: "turn-1",
    index: 0,
    kind: "task.propose",
    params: { kind: "task.propose", project: "p-1", objective: "Add unit tests", agent: "codex" },
    digest: "d".repeat(64),
    depends_on: null,
    state: "pending",
    result_ref: null,
    error_code: null,
    denial: null,
    target: { project_name: "alpha" },
    created_at: "2026-11-10T21:00:00Z",
    updated_at: "2026-11-10T21:00:00Z",
    scope: { kind: "global" },
  });

  /** One pending approval (a1) plus `count` pending proposed actions the test can shorten later. */
  function daemonWithProposals(count: number, more = false) {
    const fake = liveDaemon();
    let list = Array.from({ length: count }, (_, index) => pendingWire(`proposal-${index + 1}`));
    fake.stub("actions.pending", () => ({ proposals: list, more }));
    return { fake, keep: (kept: number) => (list = list.slice(0, kept)) };
  }

  it("nav_top_bar_and_activity_badge_show_pending_approvals_plus_proposed_actions_as_one_number", async () => {
    const { fake } = daemonWithProposals(2);
    const { panel } = await mountPanel(fake, { open: false });
    await waitFor(() => expect(screen.getByTestId("nav-review-count")).toHaveTextContent("3"));
    expect(screen.getByTestId("nav-review-count")).toHaveAccessibleName("3 pending decisions: 1 approvals, 2 proposed actions");
    expect(screen.getByTestId("top-counts")).toHaveTextContent("3 require attention");
    expect(within(panel).getByTestId("activity-badge")).toHaveTextContent("3");
    expect(within(panel).getByRole("button", { name: /^Workspace activity/ })).toHaveAccessibleName(/3 require attention/);
    await waitFor(() => expect(within(panel).getByTestId("activity-status")).toHaveTextContent("3 require attention"));
  });

  it("the_count_drops_in_all_three_places_when_a_proposal_is_decided_and_the_list_is_read_again", async () => {
    const { fake, keep } = daemonWithProposals(2);
    const { services, panel } = await mountPanel(fake, { open: false });
    await waitFor(() => expect(screen.getByTestId("nav-review-count")).toHaveTextContent("3"));
    keep(1);
    await act(async () => services.data.refreshProposals());
    await waitFor(() => {
      expect(screen.getByTestId("nav-review-count")).toHaveTextContent("2");
      expect(screen.getByTestId("top-counts")).toHaveTextContent("2 require attention");
      expect(within(panel).getByTestId("activity-badge")).toHaveTextContent("2");
    });
  });

  it("the_activity_panel_and_the_default_slots_read_n_plus_when_more_proposals_are_waiting", async () => {
    const { fake } = daemonWithProposals(2, true);
    fake.approvals = [];
    const { panel, user } = await mountPanel(fake);
    await expandSection(user, panel, "Attention");
    await waitFor(() =>
      expect(within(panel).getByTestId("activity-attention-proposals")).toHaveTextContent(
        "2+ proposed actions wait for a decision on their cards.",
      ),
    );
    expect(within(panel).getByTestId("activity-badge")).toHaveTextContent("2+");
    expect(within(panel).getByTestId("activity-status")).toHaveTextContent("2+ require attention");
    expect(within(panel).getByRole("button", { name: /^Workspace activity/ })).toHaveAccessibleName(/2\+ require attention/);
    expect(screen.getByTestId("nav-review-count")).toHaveTextContent("2+");
    expect(screen.getByTestId("nav-review-count")).toHaveAccessibleName("2+ pending decisions: 0 approvals, 2+ proposed actions");
    expect(screen.getByTestId("top-counts")).toHaveTextContent("2+ require attention");
  });

  it("all_three_places_show_a_dash_while_the_proposals_list_has_not_loaded_though_approvals_have", async () => {
    const { fake } = daemonWithProposals(1);
    fake.failNext("actions.pending", { code: -32603, message: "store busy" }, 20);
    const { services, panel } = await mountPanel(fake, { open: false });
    await waitFor(() => {
      expect(services.data.getSnapshot().approvals.status).toBe("ready");
      expect(services.data.getSnapshot().proposals.status).toBe("error");
    });
    expect(screen.getByTestId("nav-review-count")).toHaveTextContent("—");
    expect(screen.getByTestId("nav-review-count")).toHaveAccessibleName("pending decisions: not available");
    expect(screen.getByTestId("top-counts")).toHaveTextContent("— require attention");
    expect(within(panel).getByTestId("activity-badge")).toHaveTextContent("—");
  });

  it("the_attention_section_names_pending_proposed_actions_and_does_not_claim_that_nothing_needs_attention", async () => {
    const { fake } = daemonWithProposals(1);
    fake.approvals = [];
    const { panel, user } = await mountPanel(fake);
    await expandSection(user, panel, "Attention");
    await waitFor(() =>
      expect(within(panel).getByTestId("activity-attention-proposals")).toHaveTextContent("1 proposed action waits for a decision on its card."),
    );
    expect(within(panel).getByTestId("activity-attention")).not.toHaveTextContent("Nothing needs attention.");
    expect(within(panel).getByTestId("activity-badge")).toHaveTextContent("1");
  });
});
