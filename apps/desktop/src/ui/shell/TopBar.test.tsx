import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import App from "../../App";
import { createFixtureSources } from "../../sources/fixtures";
import { DaemonError } from "../../daemon/errors";
import { FakeDaemon, mkApproval, mkEvent, mkSession, mkTask } from "../../test/fakeDaemon";
import { installFakeWorkstreams } from "../../test/fakeWorkstreams";
import { REPO, connected } from "../../test/render";
import { createServices } from "../createServices";
import type { Route } from "../router";
import { topBarSlots } from "./TopBar";

const WORKSPACE: Route = { name: "global", view: "workspace" };

async function mount(fake: FakeDaemon | null, options: { waitConnected?: boolean } = {}) {
  const services = createServices({
    connector: fake ?? undefined,
    initialRoute: WORKSPACE,
    connection: { backoffMs: [5], requestTimeoutMs: 250 },
    data: { coalesceMs: 10 },
    sources: createFixtureSources(),
  });
  const user = userEvent.setup();
  render(<App services={services} shellSlots={topBarSlots} />);
  if (fake && options.waitConnected !== false) await connected();
  return { services, user };
}

const streams = new WeakMap<FakeDaemon, ReturnType<typeof installFakeWorkstreams>>();

function daemon() {
  const fake = new FakeDaemon();
  fake.projects = [{ id: REPO }, { id: "/work/other" }];
  fake.tasks = [
    mkTask("t1", REPO, { state: "Running", sessions: [mkSession("sess-alpha", { state: "Running" })] }),
    mkTask("t2", REPO, { state: "Queued", contract: { objective: "Write the release notes" } }),
    mkTask("t3", "/work/other", { state: "Verifying" }),
  ];
  fake.approvals = [mkApproval("a1", "t1"), mkApproval("a2", "t2", "approved")];
  const state = installFakeWorkstreams(fake, {
    workstreams: [
      { id: "ws-a", title: "Alpha stream", summary: "", state: "active", revision: 1 },
      { id: "ws-b", title: "Other stream", summary: "", state: "active", revision: 1 },
    ],
    links: [
      { task_id: "t1", workstream_id: "ws-a", objective_id: null },
      { task_id: "t2", workstream_id: "ws-a", objective_id: null },
      { task_id: "t3", workstream_id: "ws-b", objective_id: null },
    ],
  });
  streams.set(fake, state);
  return fake;
}

const originalMatchMedia = window.matchMedia;

afterEach(() => {
  window.location.hash = "";
  window.matchMedia = originalMatchMedia;
});

function setViewport(width: number) {
  window.matchMedia = (query) => {
    const maxWidth = /\(max-width:\s*(\d+)px\)/.exec(query);
    return {
      matches: maxWidth !== null && width <= Number(maxWidth[1]),
      media: query,
      onchange: null,
      addListener() {},
      removeListener() {},
      addEventListener() {},
      removeEventListener() {},
      dispatchEvent() {
        return true;
      },
    };
  };
}

describe("top bar need-you count", () => {
  it("fixture mode shows a dash, no placeholder badge and no four-count line", async () => {
    await mount(null);
    expect(screen.getByTestId("top-need-you")).toHaveTextContent("— need you");
    expect(screen.queryByTestId("top-counts")).toBeNull();
    expect(screen.queryByTestId("top-counts-placeholder")).toBeNull();
  });

  it("live mode shows one count that equals the Review nav count, and no four-count line", async () => {
    await mount(daemon());
    await waitFor(() => expect(screen.getByTestId("top-need-you")).toHaveTextContent("1 need you"));
    expect(screen.getByTestId("nav-review-count")).toHaveTextContent("1");
    expect(screen.queryByTestId("top-counts")).toBeNull();
    for (const word of ["workstreams", "tasks", "running", "require attention"]) {
      expect(screen.getByRole("region", { name: "Workspace summary" })).not.toHaveTextContent(new RegExp(`\\b${word}\\b`));
    }
  });

  it("links to Review", async () => {
    const { user, services } = await mount(daemon());
    await waitFor(() => expect(screen.getByTestId("top-need-you")).toHaveTextContent("1 need you"));
    const link = within(screen.getByTestId("top-need-you")).getByRole("link");
    expect(link).toHaveAttribute("href", "#/review");
    await user.click(link);
    expect(services.router.getSnapshot()).toEqual({ name: "global", view: "review" });
  });

  it("updates when events change the lists", async () => {
    const fake = daemon();
    await mount(fake);
    await waitFor(() => expect(screen.getByTestId("top-need-you")).toHaveTextContent("1 need you"));
    fake.tasks.push(mkTask("t4", "/work/third", { state: "Running" }));
    fake.approvals.push(mkApproval("a3", "t4"));
    await act(async () => fake.emit(mkEvent(1, "t4", null, "TaskAdmitted")));
    await waitFor(() => expect(screen.getByTestId("top-need-you")).toHaveTextContent("2 need you"));
  });

  it("shows a dash, never 0, when the daemon is unreachable", async () => {
    const fake = new FakeDaemon();
    fake.openError = new DaemonError("disconnected", "down");
    await mount(fake, { waitConnected: false });
    const count = screen.getByTestId("top-need-you");
    // First connection attempt is still loading; once it fails, the count must read unavailable, not loading.
    await waitFor(() => expect(count).toHaveTextContent("need you: not available"));
    expect(count).toHaveTextContent("— need you");
    expect(count.textContent).not.toMatch(/\b0\b/);
    expect(count.textContent).not.toMatch(/loading/);
  });
});

describe("usage pill", () => {
  it("fixture mode shows a PLACEHOLDER amount", async () => {
    await mount(null);
    const pill = screen.getByTestId("top-usage");
    await waitFor(() => expect(within(pill).getByTestId("top-usage-value")).toHaveTextContent("$26.00"));
    expect(within(pill).getByTestId("badge-D12")).toBeInTheDocument();
  });

  it("live mode never shows a number or currency", async () => {
    await mount(daemon());
    const pill = screen.getByTestId("top-usage");
    await waitFor(() => expect(within(pill).getByTestId("top-usage-value")).toHaveTextContent("not available"));
    expect(pill.textContent).not.toMatch(/\$|\d/);
    expect(screen.getByTestId("top-usage-info")).toHaveTextContent("A missing figure is not zero spend");
  });

  it("opens the info popover from the keyboard and closes it with Escape", async () => {
    const { user } = await mount(null);
    const button = screen.getByRole("group", { name: "Daily usage" }).querySelector("summary") as HTMLElement;
    expect(button).toHaveAccessibleName("More information about usage");
    button.focus();
    await user.click(button);
    const details = button.closest("details") as HTMLDetailsElement;
    expect(details.open).toBe(true);
    await waitFor(() => expect(screen.getByTestId("top-usage-info")).toHaveTextContent("estimated"));
    expect(screen.getByTestId("top-usage-info")).toHaveTextContent("America/Denver");
    expect(screen.getByTestId("top-usage-info")).toHaveTextContent("Codex");
    await user.keyboard("{Escape}");
    expect(details.open).toBe(false);
    expect(button).toHaveFocus();
  });

  it("shows a readable window start and no doubled periods", async () => {
    await mount(null);
    const button = screen.getByRole("group", { name: "Daily usage" }).querySelector("summary") as HTMLElement;
    await userEvent.setup().click(button);
    const info = await screen.findByTestId("top-usage-info");
    await waitFor(() => expect(info).toHaveTextContent("reported since"));
    expect(info).toHaveTextContent(/since [A-Z][a-z]{2} \d{1,2}, \d{4}, 12:00 AM \(America\/Denver\)/);
    expect(info.textContent).not.toMatch(/\d{4}-\d{2}-\d{2}T/);
    expect(info.textContent).not.toContain("..");
  });

  it("live mode popover has no doubled period", async () => {
    await mount(daemon());
    const info = screen.getByTestId("top-usage-info");
    expect(info.textContent).not.toContain("..");
  });

  it("closes the popover on an outside pointer press", async () => {
    const { user } = await mount(null);
    const button = screen.getByRole("group", { name: "Daily usage" }).querySelector("summary") as HTMLElement;
    await user.click(button);
    const details = button.closest("details") as HTMLDetailsElement;
    expect(details.open).toBe(true);
    await user.click(document.body);
    expect(details.open).toBe(false);
  });
});

describe("narrow top bar keyboard order", () => {
  it("keeps search, the need-you count and usage in DOM order at 760 px", async () => {
    setViewport(760);
    const { user } = await mount(daemon());
    await waitFor(() => expect(screen.getByTestId("top-need-you")).toHaveTextContent("1 need you"));
    const usage = screen.getByRole("group", { name: "Daily usage" }).querySelector<HTMLElement>("summary");
    if (!usage) throw new Error("usage disclosure is missing");
    const needYou = within(screen.getByTestId("top-need-you")).getByRole("link");

    screen.getByRole("combobox", { name: "Search" }).focus();
    await user.tab();
    expect(needYou).toHaveFocus();
    await user.tab();
    expect(usage).toHaveFocus();
  });
});

describe("local search", () => {
  const search = () => screen.getByRole("combobox", { name: "Search" });
  const status = () => screen.getByTestId("top-search-status");

  it("is a labeled search landmark with a visible loaded-records note", async () => {
    const { user } = await mount(daemon());
    expect(screen.getByRole("search", { name: "Search loaded records" })).toBeInTheDocument();
    expect(search()).toHaveAttribute("placeholder", "Search projects, tasks, sessions, approvals");
    await user.click(search());
    expect(screen.getByTestId("top-search-note")).toHaveTextContent("Searches loaded records only");
  });

  it("groups results, announces the count and navigates with the keyboard", async () => {
    const { user, services } = await mount(daemon());
    await waitFor(() => expect(services.data.getSnapshot().tasks.status).toBe("ready"));
    await user.click(search());
    await user.type(search(), "release");
    await waitFor(() => expect(status()).toHaveTextContent("1 result for “release”."));
    expect(screen.getByRole("group", { name: /Tasks \(1\)/ })).toBeInTheDocument();
    await user.keyboard("{ArrowDown}");
    const option = screen.getByRole("option", { name: /Write the release notes/ });
    expect(option).toHaveAttribute("aria-selected", "true");
    expect(search()).toHaveAttribute("aria-activedescendant", option.id);
    await user.keyboard("{Enter}");
    expect(services.router.getSnapshot()).toEqual({ name: "task", projectId: REPO, taskId: "t2" });
    expect(search()).toHaveValue("");
    expect(status()).toHaveTextContent("Opened Write the release notes.");
  });

  it("opens the first result on Enter without an active option", async () => {
    const { user, services } = await mount(daemon());
    await waitFor(() => expect(services.data.getSnapshot().tasks.status).toBe("ready"));
    await user.type(search(), "t3");
    await waitFor(() => expect(screen.getAllByRole("option").length).toBeGreaterThan(0));
    await user.keyboard("{Enter}");
    expect(services.router.getSnapshot()).toMatchObject({ name: "task", taskId: "t3", projectId: "/work/other" });
  });

  it("finds projects, sessions and approvals and routes each to its record", async () => {
    const { user, services } = await mount(daemon());
    await waitFor(() => expect(services.data.getSnapshot().tasks.status).toBe("ready"));
    await user.type(search(), "sess-alpha");
    const session = await screen.findByRole("option", { name: /sess-alpha/ });
    expect(screen.getByRole("group", { name: /Sessions \(1\)/ })).toBeInTheDocument();
    await user.click(session);
    expect(services.router.getSnapshot()).toEqual({ name: "task", projectId: REPO, taskId: "t1" });

    await user.type(search(), "a1");
    const approval = await screen.findByRole("option", { name: /LocalReviewMerge for t1/ });
    await user.click(approval);
    expect(services.router.getSnapshot()).toEqual({ name: "global", view: "review", taskId: "t1" });

    await user.type(search(), "/work/other");
    await user.click(await screen.findByRole("option", { name: /other/ }));
    expect(services.router.getSnapshot()).toEqual({ name: "project", projectId: "/work/other" });
  });

  it("announces empty and unavailable results, then closes with Escape and clears", async () => {
    const { user, services } = await mount(daemon());
    await waitFor(() => expect(services.data.getSnapshot().tasks.status).toBe("ready"));
    await user.type(search(), "zzzz");
    await waitFor(() => expect(status()).toHaveTextContent("No loaded records match “zzzz”."));
    await user.keyboard("{Escape}");
    expect(screen.queryByTestId("top-search-panel")).toBeNull();
    expect(search()).toHaveValue("zzzz");
    await user.keyboard("{Escape}");
    expect(search()).toHaveValue("");
    services.surfaces.search.search = async () => ({
      status: "unavailable",
      reason: "local index failed",
      provenance: services.surfaces.search.provenance,
      observedAt: new Date().toISOString(),
      ageMs: 0,
    });
    await user.type(search(), "failure");
    await waitFor(() => expect(status()).toHaveTextContent(/Search is unavailable: local index failed · observed \d+ s ago/));
  });

  it("fixture hits carry PLACEHOLDER badges and never route to a daemon record", async () => {
    const { user, services } = await mount(null);
    await user.type(search(), "refresh-token");
    const option = await screen.findByRole("option", { name: /refresh-token/ });
    expect(within(option).getByText("PLACEHOLDER DATA")).toBeInTheDocument();
    expect(screen.getByTestId("top-search-note")).toHaveTextContent("PLACEHOLDER");
    await user.click(option);
    expect(services.router.getSnapshot()).toEqual({ name: "global", view: "workstreams" });
    await user.type(search(), "approval");
    const decision = await screen.findAllByRole("option");
    await user.click(decision[0]);
    expect(services.router.getSnapshot()).toMatchObject({ name: "global" });
  });

  it("gives options whitespace-free ids and keeps aria-expanded true while the panel is open", async () => {
    const { user, services } = await mount(daemon());
    await waitFor(() => expect(services.data.getSnapshot().tasks.status).toBe("ready"));
    await user.type(search(), "zzzz");
    await waitFor(() => expect(status()).toHaveTextContent("No loaded records match"));
    expect(search()).toHaveAttribute("aria-expanded", "true");
    await user.clear(search());
    await user.type(search(), "/work");
    const options = await screen.findAllByRole("option");
    for (const option of options) expect(option.id).toMatch(/^\S+$/);
    expect(new Set(options.map((option) => option.id)).size).toBe(options.length);
  });

  it("searching and navigating issue only read RPCs", async () => {
    const fake = daemon();
    const { user, services } = await mount(fake);
    await waitFor(() => expect(services.data.getSnapshot().tasks.status).toBe("ready"));
    const before = fake.calls.length;
    await user.type(search(), "release");
    await screen.findByRole("option", { name: /release/ });
    await user.keyboard("{Enter}");
    const methods = fake.calls.slice(before).map((call) => call.method);
    expect(methods.filter((method) => method !== "queue.snapshot" && !/\.(list|get|pending)$|subscribe|capabilities/.test(method))).toEqual([]);
  });

  it("reports skipped inputs instead of implying absence", async () => {
    const fake = daemon();
    const { user, services } = await mount(fake);
    await waitFor(() => expect(services.data.getSnapshot().tasks.status).toBe("ready"));
    fake.failNext("approvals.list", { code: -32000, message: "boom" }, 5);
    await user.type(search(), "release");
    await screen.findByRole("option", { name: /release/ });
    const skipped = await screen.findByTestId("top-search-skipped");
    expect(skipped).toHaveTextContent("Not searched: D16");
    expect(skipped).toHaveTextContent("not evidence of absence");
  });
});

describe("top bar pending decisions", () => {
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

  it("top_bar_counts_pending_proposed_actions_with_approvals_and_matches_the_nav", async () => {
    const fake = daemon();
    fake.stub("actions.pending", { proposals: [pendingWire("proposal-1"), pendingWire("proposal-2")], more: false });
    const services = createServices({
      connector: fake,
      initialRoute: WORKSPACE,
      connection: { backoffMs: [5], requestTimeoutMs: 250 },
      data: { coalesceMs: 10 },
      sources: createFixtureSources(),
    });
    render(<App services={services} shellSlots={{ ...topBarSlots }} />);
    await connected();
    await waitFor(() => expect(screen.getByTestId("top-need-you")).toHaveTextContent("3 need you"));
    expect(screen.getByTestId("nav-review-count")).toHaveTextContent("3");
    expect(screen.getByTestId("nav-review-count")).toHaveAccessibleName("3 pending decisions: 1 approvals, 2 proposed actions");
  });

  it("the_decision_count_shows_a_plus_when_more_proposals_are_waiting", async () => {
    const fake = daemon();
    // The daemon caps the list and says so: the two sent are not all that wait.
    fake.stub("actions.pending", { proposals: [pendingWire("proposal-1"), pendingWire("proposal-2")], more: true });
    const services = createServices({
      connector: fake,
      initialRoute: WORKSPACE,
      connection: { backoffMs: [5], requestTimeoutMs: 250 },
      data: { coalesceMs: 10 },
      sources: createFixtureSources(),
    });
    render(<App services={services} shellSlots={{ ...topBarSlots }} />);
    await connected();
    await waitFor(() => expect(screen.getByTestId("top-need-you")).toHaveTextContent("3+ need you"));
    expect(screen.getByTestId("nav-review-count")).toHaveTextContent("3+");
    expect(screen.getByTestId("nav-review-count")).toHaveAccessibleName("3+ pending decisions: 1 approvals, 2+ proposed actions");
  });

  it("top_bar_reads_a_daemon_without_actions_pending_as_the_approvals_alone_not_as_unavailable", async () => {
    await mount(daemon());
    const counts = screen.getByTestId("top-need-you");
    await waitFor(() => expect(counts).toHaveTextContent("1 need you"));
    expect(counts).not.toHaveTextContent("need you: not available");
    expect(screen.getByTestId("nav-review-count")).toHaveTextContent("1");
  });

  it("top_bar_attention_is_unavailable_not_a_partial_number_while_the_proposals_list_cannot_be_read", async () => {
    const fake = daemon();
    fake.failNext("actions.pending", { code: -32603, message: "store busy" }, 20);
    const { services } = await mount(fake);
    await waitFor(() => expect(services.data.getSnapshot().proposals.status).toBe("error"));
    const counts = screen.getByTestId("top-need-you");
    expect(counts).toHaveTextContent("— need you");
    expect(counts).toHaveTextContent("need you: not available");
    expect(counts.textContent).not.toMatch(/\b1 need you/);
  });

  it("top_bar_attention_drops_when_a_proposal_is_decided_and_the_pending_list_is_read_again", async () => {
    const fake = daemon();
    let ids = ["proposal-1", "proposal-2"];
    fake.stub("actions.pending", () => ({ proposals: ids.map(pendingWire), more: false }));
    const { services } = await mount(fake);
    await waitFor(() => expect(screen.getByTestId("top-need-you")).toHaveTextContent("3 need you"));
    ids = ["proposal-1"];
    await act(async () => services.data.refreshProposals());
    await waitFor(() => expect(screen.getByTestId("top-need-you")).toHaveTextContent("2 need you"));
    expect(screen.getByTestId("nav-review-count")).toHaveTextContent("2");
  });
});
