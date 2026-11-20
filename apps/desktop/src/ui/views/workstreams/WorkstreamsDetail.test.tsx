import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import App from "../../../App";
import { createFixtureSources } from "../../../sources/fixtures";
import { createFixtureSurfaces } from "../../../sources/surfaces/fixtures";
import { FakeDaemon, mkSession, mkTask } from "../../../test/fakeDaemon";
import { installFakeWorkstreams } from "../../../test/fakeWorkstreams";
import { connected } from "../../../test/render";
import { openMenu, openTab, openWorkstream } from "../../../test/workstreamsUi";
import { createServices } from "../../createServices";
import type { Route } from "../../router";
import { workstreamsGlobalViews, workstreamsShellSlots } from "./register";

const A = "/work/alpha";
const VIEW: Route = { name: "global", view: "workstreams" };

/** Alpha stream: a1 Succeeded, a2 Running with one codex session. */
function daemon() {
  const fake = new FakeDaemon();
  fake.projects = [{ id: A }];
  fake.tasks = [
    mkTask("a1", A, { state: "Succeeded", contract: { objective: "Ship the parser" } }),
    mkTask("a2", A, {
      state: "Running",
      contract: { objective: "Wire the CLI", dependencies: ["a1"] },
      sessions: [mkSession("s1", { adapter: "codex", state: "Running" })],
      attempts: [{ attempt_number: 1, state: "Running" }] as never,
    }),
  ];
  installFakeWorkstreams(fake, {
    workstreams: [{ id: "ws-a", title: "Alpha stream", summary: "", state: "active", revision: 2 }],
    objectives: [],
    links: [
      { task_id: "a1", workstream_id: "ws-a", objective_id: null },
      { task_id: "a2", workstream_id: "ws-a", objective_id: null },
    ],
  });
  return fake;
}

async function mountLive() {
  const fake = daemon();
  const services = createServices({
    connector: fake,
    initialRoute: VIEW,
    connection: { backoffMs: [5], requestTimeoutMs: 250 },
    data: { coalesceMs: 10 },
    sources: createFixtureSources(),
  });
  const user = userEvent.setup();
  render(<App services={services} shellSlots={workstreamsShellSlots} globalViews={workstreamsGlobalViews} />);
  await connected();
  await openWorkstream(user, "Alpha stream");
  await screen.findAllByTestId("ws-overview-task");
  return { user };
}

async function mountFixture() {
  const services = createServices({
    initialRoute: VIEW,
    sources: createFixtureSources(),
    surfaces: createFixtureSurfaces(),
  });
  const user = userEvent.setup();
  render(<App services={services} shellSlots={workstreamsShellSlots} globalViews={workstreamsGlobalViews} />);
  await openWorkstream(user, "Auth hardening");
  return { user };
}

describe("workstream detail tabs (OX-W3)", () => {
  it("exposes Overview, Tasks, Objectives, Dependencies and Activity as one tablist with Overview selected", async () => {
    await mountLive();
    const tablist = screen.getByRole("tablist", { name: /Views of Alpha stream/ });
    const tabs = within(tablist).getAllByRole("tab");
    expect(tabs.map((tab) => tab.textContent)).toEqual(["Overview", "Tasks 2", "Objectives 0", "Dependencies", "Activity"]);
    expect(screen.getByRole("tab", { name: "Overview" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tab", { name: "Tasks 2" })).toHaveAttribute("aria-selected", "false");
  });

  it("moves between tabs with the arrow, Home and End keys and shows the selected panel only", async () => {
    const { user } = await mountLive();
    screen.getByRole("tab", { name: "Overview" }).focus();
    await user.keyboard("{ArrowRight}");
    expect(screen.getByRole("tab", { name: "Tasks 2" })).toHaveFocus();
    expect(screen.getByRole("tab", { name: "Tasks 2" })).toHaveAttribute("aria-selected", "true");
    const panelFor = (tab: string) => document.getElementById(screen.getByRole("tab", { name: tab }).getAttribute("aria-controls") ?? "");
    expect(panelFor("Tasks 2")).toBeVisible();
    expect(panelFor("Overview")).not.toBeVisible();
    await user.keyboard("{End}");
    expect(screen.getByRole("tab", { name: "Activity" })).toHaveFocus();
    await user.keyboard("{Home}");
    expect(screen.getByRole("tab", { name: "Overview" })).toHaveFocus();
    await user.keyboard("{ArrowLeft}");
    expect(screen.getByRole("tab", { name: "Activity" })).toHaveFocus();
  });

  it("lists each task's state and assignment in the Activity tab, derived from recorded data", async () => {
    const { user } = await mountLive();
    await openTab(user, "Activity");
    const rows = await screen.findAllByTestId("ws-activity-row");
    expect(rows).toHaveLength(2);
    expect(rows[1]).toHaveTextContent("Wire the CLI");
    expect(rows[1]).toHaveTextContent("Running");
    expect(rows[1]).toHaveTextContent("codex · Running · attempt 1");
    expect(screen.getByText(/event stream is on the task detail/)).toBeVisible();
  });

  it("shows the live plan revision as not available, never a number, inside Technical details", async () => {
    await mountLive();
    expect(screen.getByTestId("ws-technical")).toContainElement(screen.getByTestId("ws-plan-revision"));
    expect(screen.getByTestId("ws-plan-revision")).toHaveTextContent("Plan revision not available");
  });

  it("labels the fixture plan revision as PLACEHOLDER", async () => {
    await mountFixture();
    expect(screen.getByTestId("ws-plan-revision")).toHaveTextContent("Plan revision: PLACEHOLDER");
  });

  it("shows the read-only dependency detail with predecessors, and disables every edit action with a reason", async () => {
    const { user } = await mountLive();
    await openTab(user, "Dependencies");
    const detail = screen.getByTestId("ws-dependency-detail");
    expect(within(detail).getByText("Dependency and resource detail")).toBeInTheDocument();
    expect(detail).toHaveTextContent("Wire the CLI waits on Ship the parser (Succeeded)");
    for (const name of ["Edit predecessors", "Ask coordinator to regenerate subplan", "Validate draft before apply"]) {
      const button = within(detail).getByRole("button", { name });
      expect(button).toBeDisabled();
      const reason = document.getElementById(button.getAttribute("aria-describedby") ?? "");
      expect(reason?.textContent).toMatch(/read-only/);
    }
  });

  it("merges objective and project into one task table column", async () => {
    const { user } = await mountLive();
    await openTab(user, "Tasks");
    const headers = screen.getAllByRole("columnheader").map((header) => header.textContent);
    expect(headers).toContain("Objective / project");
    expect(headers).not.toContain("Project");
    expect(screen.getByRole("row", { name: /Wire the CLI/ })).toHaveTextContent(/Workstream only \/ /);
  });
});

describe("workstream Actions menu (OX-W2)", () => {
  it("offers Suspend, Prioritize and Archive for the open workstream, with Prioritize disabled for the only one", async () => {
    const { user } = await mountLive();
    const menu = await openMenu(user, "Workstream actions");
    expect(within(menu).getAllByRole("menuitem").map((item) => item.textContent)).toEqual([
      "Suspend workstream",
      "Prioritize",
      "Archive workstream",
    ]);
    // The only workstream is already first, so there is nothing to move.
    const prioritize = within(menu).getByRole("menuitem", { name: "Prioritize" });
    expect(prioritize).toHaveAttribute("aria-disabled", "true");
    expect(prioritize).toHaveAccessibleDescription('"Alpha stream" is already first.');
    expect(within(menu).getByRole("menuitem", { name: "Archive workstream" })).not.toHaveAttribute("aria-disabled");
  });
});
