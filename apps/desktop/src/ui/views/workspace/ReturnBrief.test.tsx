import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { createFixtureSurfaces } from "../../../sources/surfaces/fixtures";
import type { SurfaceSources } from "../../../sources/surfaces/types";
import { FakeDaemon, until } from "../../../test/fakeDaemon";
import { createServices } from "../../createServices";
import { ServicesProvider } from "../../services";
import { ReturnBrief } from "./ReturnBrief";

const COUNTS = {
  tasks_succeeded: 3,
  tasks_failed: 1,
  tasks_stalled: 0,
  tasks_running: 2,
  proposals_approved_by_user: 0,
  proposals_approved_by_grant: 0,
  executed_by_grant: 0,
  failed_under_grant: 0,
  proposals_waiting: 2,
  deliveries_waiting: 0,
  decisions_held: 0,
  grant_uses: 0,
  grants_expired: 0,
  grants_revoked: 0,
  quiet_updates: 0,
  replies_paused_by_limit: 0,
};
const BRIEF = {
  from_ms: Date.parse("2026-11-16T10:00:00Z"),
  to_ms: Date.parse("2026-11-16T14:00:00Z"),
  open: false,
  counts: COUNTS,
  grants: [{ id: "grant-9f2c1d7e3a", scope_kind: "project", scope_label: "p-0a1b2c3d", uses_in_window: 2, uses_used: 3, max_uses: 20, status: "active" }],
  waiting: [{ kind: "proposal_waiting", id: "proposal-77aa88bb", title: "workstream.update", state: "pending", project: "p-0a1b2c3d", at_ms: 5 }],
  attention: [{ kind: "task_failed", id: "task-55cc66dd", title: "Fix the login form", state: "Failed", project: "p-0a1b2c3d", at_ms: 6 }],
};

const stops: Array<() => void> = [];
afterEach(() => {
  cleanup();
  while (stops.length) stops.pop()!();
});

interface Options {
  brief?: unknown;
  surfaces?: SurfaceSources;
  configure?: (fake: FakeDaemon) => void;
}

async function mount({ brief = BRIEF, surfaces, configure }: Options = {}) {
  const fake = new FakeDaemon();
  let current: unknown = brief;
  fake.stub("monitoring.brief", () => ({ authority: "none", brief: current }));
  fake.stub("monitoring.brief_ack", () => {
    current = null;
    return { authority: "none", brief_pending: false };
  });
  configure?.(fake);
  const services = createServices({ connector: fake, surfaces, connection: { backoffMs: [5], requestTimeoutMs: 250 }, data: { coalesceMs: 10 } });
  stops.push(() => services.connection.stop());
  services.connection.start();
  await until(() => services.connection.getSnapshot().status === "connected", "connected");
  render(
    <ServicesProvider services={services}>
      <ReturnBrief />
    </ServicesProvider>,
  );
  return { fake, user: userEvent.setup() };
}

const panel = () => screen.findByRole("region", { name: /While you were away/ });

describe("return brief panel", () => {
  it("brief_panel_shows_the_lines_and_titles_of_a_closed_brief", async () => {
    await mount();
    const region = await panel();
    expect(region).toHaveTextContent("3 tasks finished, 1 failed, 2 still running");
    expect(region).toHaveTextContent("2 decisions are waiting for you");
    expect(region).toHaveTextContent("Task failed: Fix the login form");
    expect(region).toHaveTextContent("Decision waiting: workstream.update");
    expect(region).toHaveTextContent(/From .+ to .+/);
    expect(screen.getByRole("heading", { name: "While you were away" })).toBeInTheDocument();
  });

  it("brief_panel_open_window_says_so_far_and_dismisses_without_asking_the_daemon", async () => {
    const { fake, user } = await mount({ brief: { ...BRIEF, open: true } });
    const region = await screen.findByRole("region", { name: "While you were away, so far" });
    expect(region).toHaveTextContent(/so far/);
    await user.click(screen.getByRole("button", { name: "Dismiss" }));
    expect(screen.queryByRole("region", { name: /While you were away/ })).not.toBeInTheDocument();
    expect(fake.callsTo("monitoring.brief_ack")).toEqual([]);
  });

  it("brief_panel_is_one_line_until_Show_opens_the_detail", async () => {
    const { user } = await mount();
    const region = await panel();
    const show = within(region).getByRole("button", { name: "Show" });
    expect(show).toHaveAttribute("aria-expanded", "false");
    expect(region.querySelector(".ws-brief-detail")).not.toBeVisible();
    expect(region).toHaveTextContent("3 tasks finished, 1 failed, 2 still running · 2 decisions are waiting for you");
    await user.click(show);
    expect(within(region).getByRole("button", { name: "Hide" })).toHaveAttribute("aria-expanded", "true");
    expect(region.querySelector(".ws-brief-detail")).toBeVisible();
    expect(within(region).getByText("Task failed: Fix the login form")).toBeVisible();
    expect(within(region).getByText(/From .+ to .+/)).toBeVisible();
  });

  it("brief_panel_acknowledge_calls_the_method_once_and_hides_the_panel", async () => {
    const { fake, user } = await mount();
    await panel();
    const button = screen.getByRole("button", { name: "Dismiss" });
    await user.dblClick(button);
    await waitFor(() => expect(screen.queryByRole("region", { name: /While you were away/ })).not.toBeInTheDocument());
    expect(fake.callsTo("monitoring.brief_ack")).toHaveLength(1);
    expect(screen.getByRole("status")).toHaveTextContent("Return brief acknowledged");
  });

  it("brief_panel_failed_acknowledge_is_an_alert_with_a_retry_that_hides_the_panel", async () => {
    const { fake, user } = await mount({ configure: (daemon) => daemon.failNext("monitoring.brief_ack", { code: -32000, message: "boom" }) });
    await panel();
    await user.click(screen.getByRole("button", { name: "Dismiss" }));
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("The return brief could not be acknowledged.");
    expect(screen.getByRole("region", { name: /While you were away/ })).toBeInTheDocument();
    await user.click(within(alert).getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(screen.queryByRole("region", { name: /While you were away/ })).not.toBeInTheDocument());
    expect(screen.queryByRole("alert")).toBeNull();
    expect(fake.callsTo("monitoring.brief_ack")).toHaveLength(2);
  });

  it("brief_panel_announces_that_a_brief_is_ready_in_one_polite_status_region", async () => {
    await mount();
    await panel();
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Return brief ready"));
    expect(screen.getAllByRole("status")).toHaveLength(1);
  });

  it("brief_panel_fixture_acknowledge_says_nothing_was_saved", async () => {
    const { fake, user } = await mount({ surfaces: createFixtureSurfaces() });
    await panel();
    await user.click(screen.getByRole("button", { name: "Dismiss" }));
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("Fixture data: nothing was saved"));
    expect(screen.queryByRole("region", { name: /While you were away/ })).not.toBeInTheDocument();
    expect(fake.callsTo("monitoring.brief_ack")).toEqual([]);
  });

  it("brief_panel_hides_quietly_without_the_capability", async () => {
    const { fake } = await mount({ configure: (daemon) => (daemon.unknownCapabilities = ["standing_approvals"]) });
    await new Promise((resolve) => setTimeout(resolve, 60));
    expect(screen.queryByRole("region", { name: /While you were away/ })).not.toBeInTheDocument();
    expect(screen.queryByText(/could not/)).not.toBeInTheDocument();
    expect(fake.callsTo("monitoring.brief")).toEqual([]);
  });

  it("brief_panel_renders_titles_only_never_ids_or_project_codes", async () => {
    await mount();
    const region = await panel();
    const text = region.textContent ?? "";
    for (const hidden of ["proposal-77aa88bb", "task-55cc66dd", "grant-9f2c1d7e3a"]) expect(text).not.toContain(hidden);
    expect(text).not.toMatch(/[\\/]Users[\\/]|[A-Za-z]:\\|\/work\//);
    expect(text).not.toContain("proposal_waiting");
  });
});
