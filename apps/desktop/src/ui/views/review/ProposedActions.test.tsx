import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { ALL_CAPABILITIES } from "../../../daemon/types";
import { createFixtureSurfaces } from "../../../sources/surfaces/fixtures";
import { notOk, observation, okResult } from "../../../sources/surfaces/result";
import type { ActionProposal, DecisionQueue, ProposalItem, Provenance } from "../../../sources/surfaces/types";
import { FakeDaemon, until } from "../../../test/fakeDaemon";
import { createServices } from "../../createServices";
import { ServicesProvider, type Services, type SurfaceState } from "../../services";
import { ProposedActions } from "./ProposedActions";

const NOW = Date.parse("2026-11-10T21:00:00Z");
const DAEMON: Provenance = { kind: "daemon", id: "D16", methods: ["approvals.list", "tasks.list", "actions.pending"] };
const NO_ACTIONS: DecisionQueue["actions"] = {
  postpone: { available: false, reason: "r" },
  dismiss: { available: false, reason: "r" },
  escalate: { available: false, reason: "r" },
};

function proposal(id: string, over: Partial<ActionProposal> = {}): ActionProposal {
  return {
    id,
    turnId: "turn-1",
    index: 0,
    kind: "task.propose",
    params: { project: "p-1", objective: "Add unit tests", agent: "codex" },
    digest: "d".repeat(64),
    dependsOn: null,
    state: "pending",
    resultRef: null,
    errorCode: null,
    denial: null,
    target: null,
    updatedAt: "2026-11-10T21:00:00Z",
    ...over,
  };
}

const globalCard: ProposalItem = { proposal: proposal("proposal-1"), scope: { kind: "global" } };
const workstreamCard: ProposalItem = {
  proposal: proposal("proposal-2", { kind: "task.cancel", params: { task_id: "t-1" }, target: { taskTitle: "Fix login", taskState: "Running" } }),
  scope: { kind: "workstream", workstreamId: "ws-1" },
};

const queue = (proposals: ProposalItem[], provenance: Provenance = DAEMON): SurfaceState<DecisionQueue> =>
  okResult(provenance, { items: [], proposals, actions: NO_ACTIONS }, observation(NOW));

const stops: Array<() => void> = [];
afterEach(() => {
  while (stops.length) stops.pop()!();
  window.location.hash = "";
});

function section(services: Services, decisions: SurfaceState<DecisionQueue>) {
  const tree = (state: SurfaceState<DecisionQueue>) => (
    <ServicesProvider services={services}>
      <ProposedActions decisions={state} />
    </ServicesProvider>
  );
  const view = render(tree(decisions));
  return { ...view, update: (state: SurfaceState<DecisionQueue>) => view.rerender(tree(state)) };
}

/** A connected daemon session that negotiated `capabilities` (every capability when none is given). */
async function connectedServices(capabilities: typeof ALL_CAPABILITIES = ALL_CAPABILITIES): Promise<Services> {
  const fake = new FakeDaemon();
  fake.capabilitiesOverride = [...capabilities];
  const services = createServices({ connector: fake, connection: { backoffMs: [5], requestTimeoutMs: 250 }, data: { coalesceMs: 10 } });
  stops.push(() => {
    services.connection.stop();
    services.data.dispose();
  });
  services.connection.start();
  await until(() => services.connection.getSnapshot().status === "connected", "connected");
  return services;
}

describe("ProposedActions", () => {
  it("proposed_actions_lists_each_pending_card_in_a_labelled_region_with_named_links", () => {
    section(createServices(), queue([globalCard, workstreamCard]));
    const region = screen.getByRole("region", { name: "Proposed actions" });
    const list = within(region).getByRole("list", { name: "Proposed actions waiting for a decision" });
    expect(within(list).getAllByRole("listitem")).toHaveLength(2);
    expect(within(region).getByRole("link", { name: "Open the card: create task Add unit tests" })).toHaveAttribute("href", "#/workspace");
    expect(within(region).getByRole("link", { name: "Open the card: cancel task Fix login" })).toBeInTheDocument();
    expect(within(region).getByText("Global orchestrator")).toBeInTheDocument();
    expect(within(region).getByText("Workstream ws-1")).toBeInTheDocument();
    expect(within(region).getAllByText("Pending approval")).toHaveLength(2);
    expect(within(region).getByText("Proposed: Create task")).toBeInTheDocument();
    expect(within(region).getByText("Proposed: Cancel task")).toBeInTheDocument();
  });

  it("proposed_actions_offers_a_link_to_the_card_and_no_approve_or_reject_button", () => {
    section(createServices(), queue([globalCard, workstreamCard]));
    const region = screen.getByRole("region", { name: "Proposed actions" });
    expect(within(region).queryAllByRole("button")).toEqual([]);
    expect(within(region).queryByText(/approve|reject/i)).toBeNull();
  });

  it("proposed_actions_link_selects_the_global_conversation_and_opens_the_workspace", async () => {
    const services = createServices();
    section(services, queue([globalCard]));
    await userEvent.click(screen.getByRole("link", { name: /^Open the card: create task Add unit tests/ }));
    expect(services.drafts.workspace().target).toBe("global");
    expect(services.router.getSnapshot()).toEqual({ name: "global", view: "workspace" });
  });

  it("proposed_actions_link_selects_the_workstream_conversation_for_a_workstream_card", async () => {
    const services = createServices();
    section(services, queue([globalCard, workstreamCard]));
    await userEvent.click(screen.getByRole("link", { name: /^Open the card: cancel task Fix login/ }));
    expect(services.drafts.workspace().target).toBe("workstream:ws-1");
    expect(services.router.getSnapshot()).toEqual({ name: "global", view: "workspace" });
  });

  it("proposed_actions_says_nothing_is_waiting_when_the_list_is_empty", () => {
    section(createServices(), queue([]));
    const region = screen.getByRole("region", { name: "Proposed actions" });
    expect(within(region).getByRole("status")).toHaveTextContent("No proposed actions are waiting.");
    expect(within(region).queryByRole("list")).toBeNull();
    expect(within(region).queryByRole("link")).toBeNull();
  });

  it("proposed_actions_status_is_a_polite_live_region_that_follows_the_count", () => {
    const view = section(createServices(), queue([globalCard]));
    expect(screen.getByRole("status")).toHaveTextContent("1 proposed action is waiting for a decision.");
    view.update(queue([globalCard, workstreamCard]));
    // The same node is kept, so a screen reader reads the change instead of a new region.
    expect(screen.getByRole("status")).toHaveTextContent("2 proposed actions are waiting for a decision.");
    expect(screen.getAllByRole("status")).toHaveLength(1);
  });

  it("proposed_actions_states_that_it_is_loading_or_not_available_instead_of_an_empty_list", () => {
    const view = section(createServices(), { status: "loading" });
    expect(screen.getByRole("status")).toHaveTextContent("Loading proposed actions…");
    expect(screen.queryByText("No proposed actions are waiting.")).toBeNull();
    view.update(notOk<DecisionQueue>(DAEMON, "unavailable", "store busy"));
    expect(screen.getByRole("status")).toHaveTextContent("Proposed actions are not available: store busy");
    expect(screen.queryByText("No proposed actions are waiting.")).toBeNull();
    expect(screen.queryByRole("list")).toBeNull();
  });

  it("proposed_actions_labels_fixture_cards_as_fixture_data", async () => {
    const fixtures = await createFixtureSurfaces({ now: () => NOW }).decisions.queue();
    expect(fixtures.status).toBe("ok");
    if (fixtures.status !== "ok") return;
    expect(fixtures.data.proposals.length).toBeGreaterThan(0);
    section(createServices(), fixtures);
    const region = screen.getByRole("region", { name: "Proposed actions" });
    const rows = within(region).getAllByRole("listitem");
    expect(rows).toHaveLength(fixtures.data.proposals.length);
    for (const row of rows) expect(within(row).getByText("Fixture data")).toBeInTheDocument();
    expect(within(region).getByRole("heading", { name: /Proposed actions/ })).toHaveTextContent("Proposed actions");
    expect(within(region).getAllByText("Fixture data")).toHaveLength(rows.length + 1);
  });

  it("proposed_actions_are_left_out_when_the_daemon_did_not_negotiate_orchestrator_actions", async () => {
    const services = await connectedServices(ALL_CAPABILITIES.filter((capability) => capability !== "orchestrator_actions"));
    section(services, queue([globalCard]));
    expect(screen.queryByRole("region", { name: "Proposed actions" })).toBeNull();
    expect(screen.queryByTestId("proposed-actions")).toBeNull();
  });

  it("proposed_actions_are_shown_when_the_daemon_negotiated_orchestrator_actions", async () => {
    const services = await connectedServices();
    section(services, queue([globalCard]));
    expect(screen.getByRole("region", { name: "Proposed actions" })).toBeInTheDocument();
    expect(screen.queryByText("Fixture data")).toBeNull();
  });
});
