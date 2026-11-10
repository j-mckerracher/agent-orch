/// <reference types="node" />
import { readFileSync } from "node:fs";
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import App from "../../App";
import { createFixtureSources } from "../../sources/fixtures";
import { DaemonError } from "../../daemon/errors";
import { FakeDaemon, mkApproval, mkEvent, mkSession, mkTask } from "../../test/fakeDaemon";
import { REPO, connected } from "../../test/render";
import { createServices } from "../createServices";
import type { Route } from "../router";
import { topBarSlots } from "./TopBar";

const css = readFileSync("src/ui/shell/topbar.css", "utf8");
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

function daemon() {
  const fake = new FakeDaemon();
  fake.projects = [{ id: REPO }, { id: "/work/other" }];
  fake.tasks = [
    mkTask("t1", REPO, { state: "Running", sessions: [mkSession("sess-alpha", { state: "Running" })] }),
    mkTask("t2", REPO, { state: "Queued", contract: { objective: "Write the release notes" } }),
    mkTask("t3", "/work/other", { state: "Verifying" }),
  ];
  fake.approvals = [mkApproval("a1", "t1"), mkApproval("a2", "t2", "approved")];
  return fake;
}

afterEach(() => {
  window.location.hash = "";
});

describe("top bar counts", () => {
  it("fixture mode shows the PLACEHOLDER D9/D11 numbers with a badge", async () => {
    await mount(null);
    const counts = screen.getByTestId("top-counts");
    await waitFor(() => expect(counts).toHaveTextContent("3 workstreams"));
    expect(counts).toHaveTextContent("6 tasks");
    expect(counts).toHaveTextContent("2 running");
    expect(counts).toHaveTextContent("2 require attention");
    expect(screen.getByTestId("top-counts-placeholder")).toHaveTextContent("PLACEHOLDER DATA");
  });

  it("live mode counts equal the loaded lists and the Review nav count", async () => {
    await mount(daemon());
    const counts = screen.getByTestId("top-counts");
    await waitFor(() => expect(counts).toHaveTextContent("2 workstreams"));
    expect(counts).toHaveTextContent("3 tasks");
    expect(counts).toHaveTextContent("2 running");
    expect(counts).toHaveTextContent("1 require attention");
    expect(screen.queryByTestId("top-counts-placeholder")).toBeNull();
    expect(screen.getByTestId("nav-review-count")).toHaveTextContent("1");
  });

  it("updates when events change the lists", async () => {
    const fake = daemon();
    await mount(fake);
    const counts = screen.getByTestId("top-counts");
    await waitFor(() => expect(counts).toHaveTextContent("3 tasks"));
    fake.tasks.push(mkTask("t4", "/work/third", { state: "Running" }));
    fake.projects.push({ id: "/work/third" });
    fake.approvals.push(mkApproval("a3", "t4"));
    await act(async () => fake.emit(mkEvent(1, "t4", null, "TaskAdmitted")));
    await waitFor(() => expect(counts).toHaveTextContent("4 tasks"));
    expect(counts).toHaveTextContent("3 running");
    expect(counts).toHaveTextContent("2 require attention");
    expect(counts).toHaveTextContent("3 workstreams");
  });

  it("shows a dash, never 0, when the daemon is unreachable", async () => {
    const fake = new FakeDaemon();
    fake.openError = new DaemonError("disconnected", "down");
    await mount(fake, { waitConnected: false });
    const counts = screen.getByTestId("top-counts");
    expect(counts).toHaveTextContent("— tasks");
    expect(counts).toHaveTextContent("tasks: not available");
    expect(counts.textContent).not.toMatch(/\b0\b/);
  });

  it("is a link set to the matching global views", async () => {
    await mount(null);
    const counts = screen.getByTestId("top-counts");
    await waitFor(() => expect(counts).toHaveTextContent("3 workstreams"));
    const hrefs = within(counts)
      .getAllByRole("link")
      .map((link) => link.getAttribute("href"));
    expect(new Set(hrefs).size).toBe(3);
    expect(hrefs).toHaveLength(4);
  });

});

describe("usage pill", () => {
  it("fixture mode shows a PLACEHOLDER amount", async () => {
    await mount(null);
    const pill = screen.getByTestId("top-usage");
    await waitFor(() => expect(within(pill).getByTestId("top-usage-value")).toHaveTextContent("$20.00"));
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
    expect(screen.getByTestId("top-usage-info")).toHaveTextContent("codex");
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
    expect(info).toHaveTextContent(/since Jan 1, 2026/);
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

describe("local search", () => {
  const search = () => screen.getByRole("combobox", { name: "Search" });
  const status = () => screen.getByTestId("top-search-status");

  it("is a labelled search landmark with a visible loaded-records note", async () => {
    const { user } = await mount(daemon());
    expect(screen.getByRole("search", { name: "Search loaded records" })).toBeInTheDocument();
    await user.click(search());
    expect(screen.getByTestId("top-search-note")).toHaveTextContent("Searches loaded records only");
  });

  it("groups results, announces the count and navigates with the keyboard", async () => {
    const { user, services } = await mount(daemon());
    await waitFor(() => expect(screen.getByTestId("top-counts")).toHaveTextContent("3 tasks"));
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
    await waitFor(() => expect(screen.getByTestId("top-counts")).toHaveTextContent("3 tasks"));
    await user.type(search(), "t3");
    await waitFor(() => expect(screen.getAllByRole("option").length).toBeGreaterThan(0));
    await user.keyboard("{Enter}");
    expect(services.router.getSnapshot()).toMatchObject({ name: "task", taskId: "t3", projectId: "/work/other" });
  });

  it("finds projects, sessions and approvals and routes each to its record", async () => {
    const { user, services } = await mount(daemon());
    await waitFor(() => expect(screen.getByTestId("top-counts")).toHaveTextContent("3 tasks"));
    await user.type(search(), "sess-alpha");
    const session = await screen.findByRole("option", { name: /sess-alpha/ });
    expect(screen.getByRole("group", { name: /Sessions \(1\)/ })).toBeInTheDocument();
    await user.click(session);
    expect(services.router.getSnapshot()).toEqual({ name: "task", projectId: REPO, taskId: "t1" });

    await user.type(search(), "a1");
    const approval = await screen.findByRole("option", { name: /LocalReviewMerge for t1/ });
    await user.click(approval);
    expect(services.router.getSnapshot()).toEqual({ name: "review", projectId: REPO, taskId: "t1" });

    await user.type(search(), "/work/other");
    await user.click(await screen.findByRole("option", { name: /other/ }));
    expect(services.router.getSnapshot()).toEqual({ name: "project", projectId: "/work/other" });
  });

  it("announces when nothing matches and closes with Escape, then clears", async () => {
    const { user } = await mount(daemon());
    await waitFor(() => expect(screen.getByTestId("top-counts")).toHaveTextContent("3 tasks"));
    await user.type(search(), "zzzz");
    await waitFor(() => expect(status()).toHaveTextContent("No loaded records match “zzzz”."));
    await user.keyboard("{Escape}");
    expect(screen.queryByTestId("top-search-panel")).toBeNull();
    expect(search()).toHaveValue("zzzz");
    await user.keyboard("{Escape}");
    expect(search()).toHaveValue("");
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
    const { user } = await mount(daemon());
    await waitFor(() => expect(screen.getByTestId("top-counts")).toHaveTextContent("3 tasks"));
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
    const { user } = await mount(fake);
    await waitFor(() => expect(screen.getByTestId("top-counts")).toHaveTextContent("3 tasks"));
    const before = fake.calls.length;
    await user.type(search(), "release");
    await screen.findByRole("option", { name: /release/ });
    await user.keyboard("{Enter}");
    const methods = fake.calls.slice(before).map((call) => call.method);
    expect(methods.filter((method) => !/\.(list|get)$|subscribe|capabilities/.test(method))).toEqual([]);
  });

  it("reports skipped inputs instead of implying absence", async () => {
    const fake = daemon();
    const { user } = await mount(fake);
    await waitFor(() => expect(screen.getByTestId("top-counts")).toHaveTextContent("3 tasks"));
    fake.failNext("approvals.list", { code: -32000, message: "boom" }, 5);
    await user.type(search(), "release");
    await screen.findByRole("option", { name: /release/ });
    const skipped = await screen.findByTestId("top-search-skipped");
    expect(skipped).toHaveTextContent("Not searched: D16");
    expect(skipped).toHaveTextContent("not evidence of absence");
  });
});

describe("layout", () => {
  it("puts search on its own row below 761 px", () => {
    expect(css).toMatch(/@media \(max-width: 760px\)[\s\S]*\.shell-topbar-search\s*\{\s*order: 3/);
    expect(css).toMatch(/\.shell-topbar-usage\s*\{\s*order: 1/);
  });

  it("anchors the usage popover to the top bar below 761 px so it cannot overflow the viewport", () => {
    const narrow = css.slice(css.indexOf("@media (max-width: 760px)"));
    expect(narrow).toMatch(/\.shell-topbar\s*\{\s*position: relative/);
    expect(narrow).toMatch(/\.top-usage,\s*\.top-usage-info\s*\{\s*position: static/);
    expect(narrow).toMatch(/\.top-usage-popup\s*\{[^}]*left: 18px;[^}]*right: 18px/);
  });
});
