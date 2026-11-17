import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import type { ActionProposalWire } from "../../../daemon/types";
import { ALL_CAPABILITIES } from "../../../daemon/types";
import { createFixtureSources } from "../../../sources/fixtures";
import { populate } from "../../../sources/surfaces/testkit";
import { FakeDaemon } from "../../../test/fakeDaemon";
import { stubActionsResolve, stubConversation } from "../../../test/m724Stubs";
import { createServices } from "../../createServices";
import { ServicesProvider, type Services } from "../../services";
import { WorkspaceView } from "./WorkspaceView";

const at = "2026-11-10T21:00:00Z";

function build(options: { fake?: FakeDaemon } = {}): Services {
  return createServices({
    connector: options.fake,
    connection: { backoffMs: [5], requestTimeoutMs: 250 },
    data: { coalesceMs: 10 },
    sources: createFixtureSources(),
  });
}

async function mountLive(services: Services) {
  const user = userEvent.setup();
  render(
    <ServicesProvider services={services}>
      <WorkspaceView />
    </ServicesProvider>,
  );
  services.connection.start();
  await waitFor(() => expect(services.connection.getSnapshot().status).toBe("connected"));
  return user;
}

const liveDaemon = () => populate(new FakeDaemon());
const created: Services[] = [];
const track = (services: Services) => (created.push(services), services);

afterEach(() => {
  for (const services of created.splice(0)) services.connection.stop();
});

async function pickTarget(user: ReturnType<typeof userEvent.setup>, name: RegExp) {
  const button = screen.getByRole("button", { name: /Choose conversation target/ });
  if (button.getAttribute("aria-expanded") !== "true") await user.click(button);
  await user.click(screen.getByRole("radio", { name }));
}

async function pickGlobal(user: ReturnType<typeof userEvent.setup>) {
  await pickTarget(user, /Global orchestrator/);
}

/** Holds every `actions.resolve` call until the returned release runs, so a test can watch the in-flight state. */
function gateResolve(services: Services): () => void {
  let release: () => void = () => undefined;
  const open = new Promise<void>((done) => {
    release = () => done();
  });
  const source = services.surfaces.conversation;
  const real = source.resolve.bind(source);
  source.resolve = async (proposalId, decision, digest) => {
    await open;
    return real(proposalId, decision, digest);
  };
  return release;
}

/** One proposed workstream creation in the Global conversation's first turn, as the daemon lists it. */
function proposal(over: Partial<ActionProposalWire> = {}): ActionProposalWire {
  return {
    id: "proposal-1",
    turn_id: "turn-1",
    index: 0,
    kind: "workstream.create",
    params: { kind: "workstream.create", title: "Post-MVP work", summary: "" },
    digest: "digest-1",
    depends_on: null,
    state: "pending",
    result_ref: null,
    error_code: null,
    created_at: at,
    updated_at: at,
    ...over,
  };
}

/** Sends one message in Global and waits for the stubbed reply, so the proposals sit under it. */
async function sendAndReply(user: ReturnType<typeof userEvent.setup>) {
  await user.type(await screen.findByRole("textbox", { name: "Message Global orchestrator" }), "hello");
  await user.click(screen.getByRole("button", { name: "Send message to Global orchestrator" }));
  return screen.findByText("Hello from the runtime.");
}

function cardNamed(name: string): HTMLElement {
  return screen.getByRole("article", { name });
}

function pillOf(card: HTMLElement): HTMLElement {
  return card.querySelector<HTMLElement>(".ui-pill") as HTMLElement;
}

describe("orchestrator action cards over the daemon stub", () => {
  it("renders one card per proposal under its reply with the exact canonical fields as a list", async () => {
    const fake = liveDaemon();
    const state = stubConversation(fake, "completed");
    state.proposals = [proposal()];
    stubActionsResolve(fake, state);
    const services = track(build({ fake }));
    const user = await mountLive(services);
    await pickGlobal(user);
    const reply = await sendAndReply(user);

    const card = cardNamed("Proposed: Create workstream");
    expect(screen.getAllByRole("article", { name: /^Proposed:/ })).toHaveLength(1);
    const terms = Array.from(card.querySelectorAll("dt")).map((term) => [term.textContent, term.nextElementSibling?.textContent]);
    expect(terms).toEqual([
      ["Title", "Post-MVP work"],
      ["Summary", "(none)"],
    ]);
    expect(reply.compareDocumentPosition(card) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("approves once through actions.resolve and disables both buttons while in flight", async () => {
    const fake = liveDaemon();
    const state = stubConversation(fake, "completed");
    state.proposals = [proposal()];
    stubActionsResolve(fake, state);
    const services = track(build({ fake }));
    const user = await mountLive(services);
    await pickGlobal(user);
    await sendAndReply(user);
    const release = gateResolve(services);

    await user.click(await screen.findByRole("button", { name: "Approve: create workstream Post-MVP work" }));
    expect(screen.getByRole("button", { name: "Approve: create workstream Post-MVP work" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Reject: create workstream Post-MVP work" })).toBeDisabled();
    expect(screen.getByText("Sending your decision…")).toBeInTheDocument();
    expect(pillOf(cardNamed("Proposed: Create workstream"))).toHaveTextContent("Pending approval");
    expect(fake.callsTo("actions.resolve")).toHaveLength(0);

    release();
    await waitFor(() => expect(pillOf(cardNamed("Proposed: Create workstream"))).toHaveTextContent("Created"));
    expect(fake.callsTo("actions.resolve")).toHaveLength(1);
    expect(fake.callsTo("actions.resolve")[0]!.params).toMatchObject({ proposal_id: "proposal-1", decision: "approve" });
  });

  it("reuses the key after a timeout and mints a new one after idempotency-in-progress", async () => {
    const fake = liveDaemon();
    const state = stubConversation(fake, "completed");
    state.proposals = [proposal()];
    stubActionsResolve(fake, state);
    // The first call never answers and times out; the second is refused as in progress; the third succeeds.
    fake.failNext("actions.resolve", "hang");
    fake.failNext("actions.resolve", { code: -32014, message: "decision in progress" });
    const services = track(build({ fake }));
    const user = await mountLive(services);
    await pickGlobal(user);
    await sendAndReply(user);
    const approve = () => screen.getByRole("button", { name: "Approve: create workstream Post-MVP work" });

    await user.click(await screen.findByRole("button", { name: "Approve: create workstream Post-MVP work" }));
    expect(
      (await screen.findAllByText(/The daemon did not answer\. Your decision may have been recorded/)).length,
    ).toBeGreaterThan(0);
    await waitFor(() => expect(approve()).toBeEnabled());

    await user.click(approve());
    expect(
      (await screen.findAllByText("Still finishing. Refresh to see the result, then try again if it is still pending.")).length,
    ).toBeGreaterThan(0);
    await waitFor(() => expect(approve()).toBeEnabled());

    await user.click(approve());
    await waitFor(() => expect(pillOf(cardNamed("Proposed: Create workstream"))).toHaveTextContent("Created"));
    const keys = fake.callsTo("actions.resolve").map((call) => (call.params as Record<string, unknown>).idempotency_key);
    expect(keys).toHaveLength(3);
    expect(keys[1]).toBe(keys[0]);
    expect(keys[2]).not.toBe(keys[0]);
  });

  it("keeps an in-flight decision when the target changes and comes back", async () => {
    const fake = liveDaemon();
    const state = stubConversation(fake, "completed");
    state.proposals = [proposal()];
    stubActionsResolve(fake, state);
    const services = track(build({ fake }));
    const user = await mountLive(services);
    await pickGlobal(user);
    await sendAndReply(user);
    const release = gateResolve(services);

    await user.click(await screen.findByRole("button", { name: "Approve: create workstream Post-MVP work" }));
    await pickTarget(user, /alpha coordinator/);
    await pickTarget(user, /Global orchestrator/);

    const approve = await screen.findByRole("button", { name: "Approve: create workstream Post-MVP work" });
    expect(approve).toBeDisabled();
    expect(screen.getByText("Sending your decision…")).toBeInTheDocument();

    release();
    await waitFor(() => expect(pillOf(cardNamed("Proposed: Create workstream"))).toHaveTextContent("Created"));
    expect(fake.callsTo("actions.resolve")).toHaveLength(1);
  });

  it("shows Created with Open workstream only after the daemon confirms execution", async () => {
    const fake = liveDaemon();
    const state = stubConversation(fake, "completed");
    state.proposals = [proposal()];
    stubActionsResolve(fake, state);
    const services = track(build({ fake }));
    const user = await mountLive(services);
    await pickGlobal(user);
    await sendAndReply(user);

    await user.click(await screen.findByRole("button", { name: "Approve: create workstream Post-MVP work" }));
    await waitFor(() => expect(pillOf(cardNamed("Proposed: Create workstream"))).toHaveTextContent("Created"));
    const link = within(cardNamed("Proposed: Create workstream")).getByRole("link", { name: "Open workstream" });
    expect(link.getAttribute("href")).toMatch(/^#\/workstreams\?workstream=/);
    expect(screen.queryByRole("button", { name: /^Approve:/ })).toBeNull();
  });

  it("shows Rejected after the daemon confirms the rejection", async () => {
    const fake = liveDaemon();
    const state = stubConversation(fake, "completed");
    state.proposals = [proposal()];
    stubActionsResolve(fake, state);
    const services = track(build({ fake }));
    const user = await mountLive(services);
    await pickGlobal(user);
    await sendAndReply(user);

    await user.click(await screen.findByRole("button", { name: "Reject: create workstream Post-MVP work" }));
    await waitFor(() => expect(pillOf(cardNamed("Proposed: Create workstream"))).toHaveTextContent("Rejected"));
    expect(within(cardNamed("Proposed: Create workstream")).queryAllByRole("button")).toHaveLength(0);
    expect(screen.getByText("You rejected this. Nothing was changed.")).toBeInTheDocument();
  });

  it("renders Approved, Superseded, Expired and an unknown state as text and a glyph without buttons", async () => {
    const fake = liveDaemon();
    const state = stubConversation(fake, "completed");
    state.proposals = [
      proposal({ id: "p-approved", state: "approved", params: { kind: "workstream.create", title: "A", summary: "" } }),
      proposal({ id: "p-superseded", state: "superseded", params: { kind: "workstream.create", title: "B", summary: "" } }),
      proposal({ id: "p-expired", state: "expired", params: { kind: "workstream.create", title: "C", summary: "" } }),
      proposal({ id: "p-unknown", state: "archived-later", params: { kind: "workstream.create", title: "D", summary: "" } }),
    ];
    const services = track(build({ fake }));
    const user = await mountLive(services);
    await pickGlobal(user);
    await sendAndReply(user);

    const cards = await screen.findAllByRole("article", { name: "Proposed: Create workstream" });
    expect(cards).toHaveLength(4);
    expect(cards.map((card) => pillOf(card).textContent)).toEqual([
      "iApproved, running",
      "–Superseded",
      "!Expired",
      "–Unknown state: archived-later",
    ]);
    for (const card of cards) expect(within(card).queryAllByRole("button")).toHaveLength(0);
  });

  it("shows a stale_revision failure in plain words", async () => {
    const fake = liveDaemon();
    const state = stubConversation(fake, "completed");
    state.proposals = [
      proposal({
        kind: "workstream.update",
        state: "failed",
        error_code: "stale_revision",
        params: { kind: "workstream.update", workstream_id: "ws-1", expected_revision: 1, title: "Renamed" },
      }),
    ];
    const services = track(build({ fake }));
    const user = await mountLive(services);
    await pickGlobal(user);
    await sendAndReply(user);

    const card = await screen.findByRole("article", { name: "Proposed: Update workstream" });
    expect(within(card).getByText("A newer change to this workstream exists. Ask the orchestrator to propose again.")).toBeInTheDocument();
  });

  it("keeps a dependent objective's Approve disabled until its workstream is created", async () => {
    const fake = liveDaemon();
    const state = stubConversation(fake, "completed");
    state.proposals = [
      proposal(),
      proposal({
        id: "proposal-2",
        index: 1,
        kind: "objective.add",
        depends_on: "proposal-1",
        params: { kind: "objective.add", workstream_ref: "proposal-1", title: "Ship it", description: "", gates: [] },
      }),
    ];
    stubActionsResolve(fake, state);
    const services = track(build({ fake }));
    const user = await mountLive(services);
    await pickGlobal(user);
    await sendAndReply(user);

    const child = await screen.findByRole("button", { name: "Approve: add objective Ship it" });
    expect(child).toBeDisabled();
    expect(screen.getByText("Approve “Create workstream: Post-MVP work” first. This objective goes into that workstream.")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Approve: create workstream Post-MVP work" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Approve: add objective Ship it" })).toBeEnabled());
  });

  it("shows proposal_not_pending and proposal_not_found on the card and reloads", async () => {
    const fake = liveDaemon();
    const state = stubConversation(fake, "completed");
    state.proposals = [proposal()];
    stubActionsResolve(fake, state);
    const services = track(build({ fake }));
    const user = await mountLive(services);
    await pickGlobal(user);
    await sendAndReply(user);

    const reads = fake.callsTo("conversations.get").length;
    fake.failNext("actions.resolve", { code: -32022, message: "proposal is not pending" });
    const card = await screen.findByRole("article", { name: "Proposed: Create workstream" });
    await user.click(within(card).getByRole("button", { name: "Approve: create workstream Post-MVP work" }));
    // The card shows the reason; the polite region announces it too.
    expect(await within(card).findByText("This proposal was already decided. Showing the daemon's current state.")).toBeInTheDocument();
    await waitFor(() => expect(fake.callsTo("conversations.get").length).toBeGreaterThan(reads));

    fake.failNext("actions.resolve", { code: -32021, message: "proposal not found" });
    await user.click(within(card).getByRole("button", { name: "Approve: create workstream Post-MVP work" }));
    expect(await within(card).findByText("This proposal no longer exists. The conversation history may have been cleared.")).toBeInTheDocument();
  });

  it("names buttons after the action, announces the outcome politely and keeps focus on the card", async () => {
    const fake = liveDaemon();
    const state = stubConversation(fake, "completed");
    state.proposals = [proposal()];
    stubActionsResolve(fake, state);
    const services = track(build({ fake }));
    const user = await mountLive(services);
    await pickGlobal(user);
    await sendAndReply(user);

    await user.click(await screen.findByRole("button", { name: "Approve: create workstream Post-MVP work" }));
    await waitFor(() => expect(document.querySelector(".ws-feedback")).toHaveTextContent("Created workstream “Post-MVP work”."));
    expect(document.querySelector(".ws-feedback")?.getAttribute("aria-live")).toBe("polite");
    await waitFor(() => expect(cardNamed("Proposed: Create workstream")).toHaveFocus());
  });

  it("shows the unreadable-block notice when the turn's action_block is rejected", async () => {
    const fake = liveDaemon();
    const state = stubConversation(fake, "running");
    const services = track(build({ fake }));
    const user = await mountLive(services);
    await pickGlobal(user);
    await user.type(await screen.findByRole("textbox", { name: "Message Global orchestrator" }), "hello");
    await user.click(screen.getByRole("button", { name: "Send message to Global orchestrator" }));
    await screen.findByText(/Waiting for the orchestrator.s reply…/);

    state.finishRunning(null, "Here is the plan.");
    state.turn = { ...state.turn!, action_block: "rejected" };
    expect(await screen.findByText(/action block in this reply could not be read/, undefined, { timeout: 5000 })).toBeInTheDocument();
    expect(screen.queryByRole("article", { name: /^Proposed:/ })).toBeNull();
  });

  it("shows no card or notice when the daemon lacks orchestrator_actions, and chat still works", async () => {
    const fake = liveDaemon();
    fake.capabilitiesOverride = ALL_CAPABILITIES.filter((capability) => capability !== "orchestrator_actions");
    const state = stubConversation(fake, "completed");
    state.proposals = [proposal()];
    const services = track(build({ fake }));
    const user = await mountLive(services);
    await pickGlobal(user);
    await sendAndReply(user);

    expect(screen.queryByRole("article", { name: /^Proposed:/ })).toBeNull();
    expect(screen.queryByText(/Proposed:/)).toBeNull();
    expect(fake.callsTo("conversations.send")).toHaveLength(1);
  });

  it("keeps working against a daemon that sends no proposals field", async () => {
    const fake = liveDaemon();
    stubConversation(fake, "completed");
    const services = track(build({ fake }));
    const user = await mountLive(services);
    await pickGlobal(user);
    await sendAndReply(user);

    expect(screen.queryByRole("article", { name: /^Proposed:/ })).toBeNull();
    expect(screen.getByText("Hello from the runtime.")).toBeInTheDocument();
  });

  it("shows the Restricted-only note at Auto, and not at Restricted or in fixture mode", async () => {
    const note = "Action proposals work only at Restricted. At Auto and Full the orchestrator can answer but cannot propose changes.";
    window.localStorage.clear();
    const fake = liveDaemon();
    stubConversation(fake, "completed");
    const services = track(build({ fake }));
    const user = await mountLive(services);
    await pickGlobal(user);
    expect(await screen.findByText(note)).toBeInTheDocument();

    const button = screen.getByRole("button", { name: /^Model settings/ });
    if (button.getAttribute("aria-expanded") !== "true") await user.click(button);
    await user.selectOptions(screen.getByRole("combobox", { name: "Permission level" }), "Restricted");
    expect(screen.queryByText(note)).toBeNull();

    cleanup();
    window.localStorage.clear();
    const fixtures = track(build());
    render(
      <ServicesProvider services={fixtures}>
        <WorkspaceView />
      </ServicesProvider>,
    );
    await screen.findByRole("article", { name: "Proposed: Create workstream" });
    expect(screen.queryByText(note)).toBeNull();
  });

  it("fixture mode renders fixture cards; Approve says Fixture data: nothing was created", async () => {
    const services = track(build());
    render(
      <ServicesProvider services={services}>
        <WorkspaceView />
      </ServicesProvider>,
    );
    const user = userEvent.setup();
    const card = await screen.findByRole("article", { name: "Proposed: Create workstream" });
    expect(within(card).getByText("Fixture data")).toBeInTheDocument();

    await user.click(within(card).getByRole("button", { name: "Approve: create workstream Post-MVP work" }));
    await waitFor(() => expect(document.querySelector(".ws-feedback")).toHaveTextContent("Fixture data: nothing was created."));
    expect(within(card).getByText("Fixture data: nothing was created.")).toBeInTheDocument();
    expect(pillOf(card)).toHaveTextContent("Pending approval");
  });
});
