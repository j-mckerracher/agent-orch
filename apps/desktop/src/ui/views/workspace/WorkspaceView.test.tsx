import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { createHash } from "node:crypto";
import { afterEach, describe, expect, it, vi } from "vitest";
import { APPROVER_REQUIRED_NOTICE, DaemonError } from "../../../daemon/errors";
import { ALL_CAPABILITIES, type ActionProposalWire } from "../../../daemon/types";
import { createServices } from "../../createServices";
import { ALPHA, BETA, WORKSTREAM_STATE, populate } from "../../../sources/surfaces/testkit";
import { createFixtureSources } from "../../../sources/fixtures";
import { createFixtureSurfaces } from "../../../sources/surfaces/fixtures";
import { FakeDaemon, StubRpcError } from "../../../test/fakeDaemon";
import {
  CLAUDE_CATALOG,
  CODEX_CATALOG,
  COLD_CATALOG,
  COPILOT_UNAVAILABLE_CATALOG,
  PI_CATALOG,
  stubConversation,
  stubValidate,
} from "../../../test/m724Stubs";
import type { DraftStorage } from "../../drafts";
import { ServicesProvider, type Services } from "../../services";
import { CHOICE_PREFERENCE_KEY } from "./modelChoice";
import { RESTRICTED_PREFERENCE_KEY } from "./permissionLevel";
import { workspaceRegistration } from "./register";
import { COORDINATOR_HEADING, REMOVE_OPEN_WORK_TEXT } from "./coordinatorModel";
import { formatAt } from "./time";
import { GLOBAL_TASKS_WORKSTREAM_ID, WorkspaceView } from "./WorkspaceView";

function memoryStorage(): DraftStorage & { data: Map<string, string> } {
  const data = new Map<string, string>();
  return { data, getItem: (key) => data.get(key) ?? null, setItem: (key, value) => void data.set(key, value) };
}

function build(options: { fake?: FakeDaemon; storage?: DraftStorage } = {}): Services {
  return createServices({
    connector: options.fake,
    connection: { backoffMs: [5], requestTimeoutMs: 250 },
    data: { coalesceMs: 10 },
    sources: createFixtureSources(),
    draftStorage: options.storage,
  });
}

async function mountView(services: Services, waitConnected = true) {
  const user = userEvent.setup();
  const view = render(
    <ServicesProvider services={services}>
      <WorkspaceView />
    </ServicesProvider>,
  );
  if (services.bridgeBound) {
    services.connection.start();
    if (waitConnected) await waitLive(services);
  }
  return { user, view };
}

const liveDaemon = () => {
  const fake = populate(new FakeDaemon());
  stubConversation(fake, "completed");
  return fake;
};
const created: Services[] = [];
const track = (services: Services) => (created.push(services), services);

afterEach(() => {
  vi.useRealTimers();
  for (const services of created.splice(0)) services.connection.stop();
});

/** The connection status element lives in the shell, so tests wait on the store instead. */
async function waitLive(services: Services) {
  await waitFor(() => expect(services.connection.getSnapshot().status).toBe("connected"));
}

async function pickTarget(user: ReturnType<typeof userEvent.setup>, name: RegExp | string) {
  const button = screen.getByRole("button", { name: /Choose conversation target/ });
  if (button.getAttribute("aria-expanded") !== "true") await user.click(button);
  await user.click(screen.getByRole("radio", { name }));
}

/** The agents `agents.capabilities` lists for the picker, each launchable or not (DPES-1105). */
function stubAgents(fake: FakeDaemon, launchable: Record<string, boolean>) {
  fake.stub(
    "agents.capabilities",
    Object.entries(launchable).map(([agent, ok]) => ({
      agent,
      installed: true,
      launchable: ok,
      capabilities: {
        streaming: true,
        interactive_input: false,
        structured_output: true,
        cancellation: true,
        session_resumption: false,
        approvals: false,
        model_selection: true,
      },
    })),
  );
}

/** One pending workstream creation in the Global conversation, as the daemon lists it. */
function pendingCreate(): ActionProposalWire {
  const at = "2026-11-10T21:00:00Z";
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
  };
}

/** Mounts live Global with one proposal under the reply. The welcome's `approver` is `approver`; null omits it. */
async function globalCard(approver: boolean | null): Promise<HTMLElement> {
  const fake = liveDaemon();
  fake.approver = approver;
  const state = stubConversation(fake, "completed");
  state.proposals = [pendingCreate()];
  const services = track(build({ fake }));
  const { user } = await mountView(services, false);
  await waitLive(services);
  await pickTarget(user, /Global orchestrator/);
  await user.type(await screen.findByRole("textbox", { name: "Message Global orchestrator" }), "hello");
  await user.click(screen.getByRole("button", { name: "Send message to Global orchestrator" }));
  await screen.findByText("Hello from the runtime.");
  return screen.getByRole("article", { name: "Proposed: Create workstream" });
}

describe("approval cards need the approver role (DPES-1105)", () => {
  it("a live card is not decidable without the approver role and shows the plain-words notice", async () => {
    const card = await globalCard(false);
    expect(within(card).getByRole("button", { name: "Approve: create workstream Post-MVP work" })).toBeDisabled();
    expect(within(card).getByRole("button", { name: "Reject: create workstream Post-MVP work" })).toBeDisabled();
    expect(card).toHaveTextContent(APPROVER_REQUIRED_NOTICE);
    expect(card).not.toHaveTextContent(/forbidden/i);
  });

  it("a live card is decidable with the approver role", async () => {
    const card = await globalCard(true);
    expect(within(card).getByRole("button", { name: "Approve: create workstream Post-MVP work" })).toBeEnabled();
    expect(within(card).getByRole("button", { name: "Reject: create workstream Post-MVP work" })).toBeEnabled();
  });
});

describe("saved model choice across a remount, live (DPES-1105)", () => {
  it("keeps Claude Code after the view is left and reopened, and sends it with the next message", async () => {
    const fake = liveDaemon();
    const catalog = [CODEX_CATALOG, CLAUDE_CATALOG];
    stubAgents(fake, { codex: true, "claude-code": true });
    fake.stub("agents.catalog", catalog);
    stubValidate(fake, catalog);
    stubConversation(fake, "completed");
    const before = track(build({ fake }));
    const first = await mountView(before, false);
    await waitLive(before);
    await pickTarget(first.user, /Global orchestrator/);
    await first.user.click(screen.getByRole("button", { name: /^Model settings/ }));
    await first.user.selectOptions(await screen.findByRole("combobox", { name: "Provider" }), "claude-code");
    first.view.unmount();

    const after = track(build({ fake }));
    const second = await mountView(after, false);
    await waitLive(after);
    await pickTarget(second.user, /Global orchestrator/);
    expect(screen.getByRole("button", { name: /^Model settings: Claude Code/ })).toBeInTheDocument();
    await second.user.type(await screen.findByRole("textbox", { name: "Message Global orchestrator" }), "hello");
    await second.user.click(screen.getByRole("button", { name: "Send message to Global orchestrator" }));
    await waitFor(() => expect(fake.callsTo("conversations.send")).toHaveLength(1));
    expect(fake.callsTo("conversations.send")[0]!.params).toMatchObject({
      text: "hello",
      selection: { agent: "claude-code" },
    });
  });

  it("blocks sending when the saved provider is no longer launchable, and preserves the choice", async () => {
    window.localStorage.setItem(CHOICE_PREFERENCE_KEY, JSON.stringify({ agent: "claude-code" }));
    const fake = liveDaemon();
    stubAgents(fake, { codex: true, "claude-code": false });
    fake.stub("agents.catalog", [CODEX_CATALOG, { ...CLAUDE_CATALOG, state: "unavailable", detail: "the claude executable was not found" }]);
    stubConversation(fake, "completed");
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await pickTarget(user, /Global orchestrator/);
    expect(screen.getByRole("button", { name: /^Model settings: Claude Code · unavailable/ })).toBeInTheDocument();
    await user.type(await screen.findByRole("textbox", { name: "Message Global orchestrator" }), "hello");
    const send = screen.getByRole("button", { name: "Send message to Global orchestrator" });
    expect(send).toBeDisabled();
    expect(screen.getByText(/Your agent choice cannot be used.*claude executable was not found/)).toBeInTheDocument();
    await user.click(send);
    expect(fake.callsTo("conversations.send")).toHaveLength(0);
    expect(window.localStorage.getItem(CHOICE_PREFERENCE_KEY)).toBe(JSON.stringify({ agent: "claude-code" }));
  });

  it("blocks a saved choice on a daemon without conversation_selection, names the control, and keeps the saved value", async () => {
    window.localStorage.setItem(CHOICE_PREFERENCE_KEY, JSON.stringify({ agent: "codex", model: "gpt-a" }));
    const fake = liveDaemon();
    fake.capabilitiesOverride = ALL_CAPABILITIES.filter((capability) => capability !== "conversation_selection");
    fake.stub("agents.catalog", [CODEX_CATALOG]);
    stubValidate(fake, [CODEX_CATALOG]);
    stubConversation(fake, "completed");
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await pickTarget(user, /Global orchestrator/);
    await user.type(await screen.findByRole("textbox", { name: "Message Global orchestrator" }), "hello");
    expect(screen.getByRole("button", { name: "Send message to Global orchestrator" })).toBeDisabled();
    expect(
      screen.getByText(/Open model settings and choose "Use the daemon default", or update the daemon\./),
    ).toBeInTheDocument();
    expect(screen.queryByText(/choose the default in model settings/i)).toBeNull();
    await user.click(screen.getByRole("button", { name: /^Model settings/ }));
    expect(screen.getByText(/The connected daemon cannot take a model choice, so a message/)).toBeInTheDocument();
    expect(window.localStorage.getItem(CHOICE_PREFERENCE_KEY)).toBe(JSON.stringify({ agent: "codex", model: "gpt-a" }));
    expect(fake.callsTo("conversations.send")).toHaveLength(0);
  });

  it("Use the daemon default clears the saved choice, and the next send goes without a selection", async () => {
    window.localStorage.setItem(CHOICE_PREFERENCE_KEY, JSON.stringify({ agent: "codex", model: "gpt-a" }));
    const fake = liveDaemon();
    fake.capabilitiesOverride = ALL_CAPABILITIES.filter((capability) => capability !== "conversation_selection");
    fake.stub("agents.catalog", [CODEX_CATALOG]);
    stubValidate(fake, [CODEX_CATALOG]);
    stubConversation(fake, "completed");
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await pickTarget(user, /Global orchestrator/);
    await user.click(screen.getByRole("button", { name: /^Model settings/ }));
    await user.click(screen.getByRole("button", { name: "Use the daemon default" }));
    expect(window.localStorage.getItem(CHOICE_PREFERENCE_KEY)).toBeNull();
    await user.type(screen.getByRole("textbox", { name: "Message Global orchestrator" }), "hello");
    const send = screen.getByRole("button", { name: "Send message to Global orchestrator" });
    expect(send).toBeEnabled();
    await user.click(send);
    await waitFor(() => expect(fake.callsTo("conversations.send")).toHaveLength(1));
    expect(fake.callsTo("conversations.send")[0]!.params).toMatchObject({ text: "hello" });
    expect(fake.callsTo("conversations.send")[0]!.params).not.toHaveProperty("selection");
  });

  it("keeps a saved choice the daemon cannot take, and sends it after a reconnect to a daemon that can", async () => {
    window.localStorage.setItem(CHOICE_PREFERENCE_KEY, JSON.stringify({ agent: "codex", model: "gpt-a" }));
    const fake = liveDaemon();
    fake.capabilitiesOverride = ALL_CAPABILITIES.filter((capability) => capability !== "conversation_selection");
    fake.stub("agents.catalog", [CODEX_CATALOG]);
    stubValidate(fake, [CODEX_CATALOG]);
    stubConversation(fake, "completed");
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await pickTarget(user, /Global orchestrator/);
    await user.type(await screen.findByRole("textbox", { name: "Message Global orchestrator" }), "hello");
    const send = screen.getByRole("button", { name: "Send message to Global orchestrator" });
    expect(send).toBeDisabled();
    // The window reconnects, now to a daemon that takes a model choice.
    services.connection.stop();
    fake.capabilitiesOverride = null;
    services.connection.start();
    await waitLive(services);
    await waitFor(() => expect(send).toBeEnabled());
    expect(window.localStorage.getItem(CHOICE_PREFERENCE_KEY)).toBe(JSON.stringify({ agent: "codex", model: "gpt-a" }));
    await user.click(send);
    await waitFor(() => expect(fake.callsTo("conversations.send")).toHaveLength(1));
    expect(fake.callsTo("conversations.send")[0]!.params).toMatchObject({
      text: "hello",
      selection: { agent: "codex", model: "gpt-a" },
    });
  });

  it("keeps the chosen target and an unsent draft across a remount, in live mode", async () => {
    const fake = liveDaemon();
    stubConversation(fake, "completed");
    const storage = memoryStorage();
    const before = track(build({ fake, storage }));
    const first = await mountView(before, false);
    await waitLive(before);
    await pickTarget(first.user, /Global orchestrator/);
    await first.user.type(await screen.findByRole("textbox", { name: "Message Global orchestrator" }), "half a thought");
    first.view.unmount();

    const after = track(build({ fake, storage }));
    await mountView(after, false);
    await waitLive(after);
    expect(await screen.findByRole("heading", { name: "Talking to: Global orchestrator" })).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Message Global orchestrator" })).toHaveValue("half a thought");
  });
});

describe("workspace view, live (daemon-derived) mode", () => {
  it("defaults to the first project coordinator and shows the recorded transcript, with no placeholder badge", async () => {
    const fake = liveDaemon();
    const services = track(build({ fake }));
    await mountView(services, false);
    await waitLive(services);
    const heading = await screen.findByRole("heading", { name: "Talking to: alpha coordinator", level: 2 });
    const log = await screen.findByRole("log", { name: "Conversation with alpha coordinator" });
    expect(await within(log).findByText("hello")).toBeInTheDocument();
    expect(within(heading.parentElement as HTMLElement).queryByText(/PLACEHOLDER DATA/)).toBeNull();
    expect(screen.queryByText(/does not create tasks or approvals/)).toBeNull();
  });

  it("sends to the project conversation runtime with an idempotency key and shows its reply", async () => {
    const fake = liveDaemon();
    stubConversation(fake, "completed");
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    const box = await screen.findByRole("textbox", { name: "Message alpha coordinator" });
    await user.type(box, "ship it");
    await user.click(screen.getByRole("button", { name: "Send message to alpha coordinator" }));
    await waitFor(() => expect(fake.callsTo("conversations.send")).toHaveLength(1));
    const params = fake.callsTo("conversations.send")[0].params as Record<string, unknown>;
    expect(params).toMatchObject({ target: { kind: "project", project_id: ALPHA }, text: "ship it" });
    expect(String(params.idempotency_key)).not.toBe("");
    expect(await screen.findByText("Hello from the runtime.")).toBeInTheDocument();
    expect(screen.queryByText("Notes, no reply")).toBeNull();
    expect(fake.callsTo("coordinator.send_message")).toHaveLength(0);
    expect(box).toHaveValue("");
    expect(services.drafts.workspaceDraft(`project:${ALPHA}`)).toBe("");
    // The transcript was re-read after the send.
    await waitFor(() => expect(fake.callsTo("conversations.get").length).toBeGreaterThanOrEqual(2));
  });

  it("keeps the draft after a failed send and reuses the idempotency key when an indeterminate send is retried", async () => {
    const fake = liveDaemon();
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    const box = await screen.findByRole("textbox", { name: "Message alpha coordinator" });
    await user.type(box, "retry me");
    fake.failNext("conversations.send", "hang");
    await user.click(screen.getByRole("button", { name: "Send message to alpha coordinator" }));
    expect(await screen.findByRole("alert", {}, { timeout: 2000 })).toHaveTextContent(/Sending the message failed/);
    expect(screen.getByText(/The daemon may have recorded it/)).toBeInTheDocument();
    expect(box).toHaveValue("retry me");
    await user.click(screen.getByRole("button", { name: "Send message to alpha coordinator" }));
    await waitFor(() => expect(fake.callsTo("conversations.send")).toHaveLength(2));
    const [first, second] = fake.callsTo("conversations.send").map((call) => (call.params as Record<string, unknown>).idempotency_key);
    expect(second).toBe(first);
    await waitFor(() => expect(box).toHaveValue(""));
  });

  it("says the agent is still starting when every send attempt conflicts, not that the record conflicts (DPES-1106)", async () => {
    const fake = liveDaemon();
    fake.failNext("conversations.send", { code: -32011, message: "operation conflicts with current state" }, 6);
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    const box = await screen.findByRole("textbox", { name: "Message alpha coordinator" });
    await user.type(box, "still starting");
    // The retries wait about 13.5 s. The clock is faked only for the send, so typing is not affected.
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    fireEvent.click(screen.getByRole("button", { name: "Send message to alpha coordinator" }));
    await act(() => vi.runAllTimersAsync());
    expect(fake.callsTo("conversations.send")).toHaveLength(6);
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Sending the message failed. The orchestrator's agent is still starting. Your message was not sent. Try again in a moment.",
    );
    expect(screen.queryByText(/not in a state that allows this action/)).toBeNull();
    expect(box).toHaveValue("still starting");
  });

  it("sends to the Global target through the daemon and shows the runtime-unavailable state, never a reply", async () => {
    const fake = liveDaemon();
    stubConversation(fake, "runtime_unavailable");
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await screen.findByRole("heading", { name: /alpha coordinator/ });
    await pickTarget(user, /Global orchestrator/);
    const heading = await screen.findByRole("heading", { name: "Talking to: Global orchestrator" });
    expect(within(heading.parentElement as HTMLElement).queryByText(/PLACEHOLDER/)).toBeNull();
    expect(screen.queryByText(/Sending is disabled/)).toBeNull();
    const box = screen.getByRole("textbox", { name: "Message Global orchestrator" });
    await user.type(box, "hello global");
    await user.click(screen.getByRole("button", { name: "Send message to Global orchestrator" }));
    await waitFor(() => expect(fake.callsTo("conversations.send")).toHaveLength(1));
    const params = fake.callsTo("conversations.send")[0]!.params as Record<string, unknown>;
    expect(params.target).toEqual({ kind: "global" });
    expect(params.text).toBe("hello global");
    expect(await screen.findByText("hello global")).toBeInTheDocument();
    await waitFor(() => expect(screen.getAllByText(/Orchestrator runtime unavailable/)).toHaveLength(1));
    expect(screen.getByText("Recorded by the daemon.")).toBeInTheDocument();
    expect(screen.queryByText(/SIMULATED/)).toBeNull();
    expect(fake.callsTo("coordinator.send_message")).toHaveLength(0);
    expect(screen.queryByText(/Hello from the runtime/)).toBeNull();
    await waitFor(() => expect(box).toHaveValue(""));
  });

  it("shows a real reply only when the daemon records one", async () => {
    const fake = liveDaemon();
    stubConversation(fake, "completed");
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await screen.findByRole("heading", { name: /alpha coordinator/ });
    await pickTarget(user, /Global orchestrator/);
    await user.type(await screen.findByRole("textbox", { name: "Message Global orchestrator" }), "hi");
    await user.click(screen.getByRole("button", { name: "Send message to Global orchestrator" }));
    expect(await screen.findByText("Hello from the runtime.")).toBeInTheDocument();
    expect(screen.queryByText(/runtime unavailable/i)).toBeNull();
  });

  it("renders a recorded reply's markdown and copies its source with one click", async () => {
    const fake = liveDaemon();
    const convo = stubConversation(fake, "running");
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await screen.findByRole("heading", { name: /alpha coordinator/ });
    await pickTarget(user, /Global orchestrator/);
    await user.type(await screen.findByRole("textbox", { name: "Message Global orchestrator" }), "plan it");
    await user.click(screen.getByRole("button", { name: "Send message to Global orchestrator" }));
    await screen.findByText(/Waiting for the orchestrator.s reply…/);

    act(() => convo.finishRunning(null, "**Done**: shipped `v2`."));
    const done = await screen.findByText("Done", {}, { timeout: 3000 });
    expect(done.tagName).toBe("STRONG");
    expect(screen.getByRole("log").textContent).not.toContain("**");
    await user.click(within(done.closest("article") as HTMLElement).getByRole("button", { name: "Copy message" }));
    expect(await navigator.clipboard.readText()).toBe("**Done**: shipped `v2`.");
  });

  it("shows a turn's thinking in a Thinking disclosure: open while it runs, collapsed once the reply lands", async () => {
    const fake = liveDaemon();
    const convo = stubConversation(fake, "running");
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await screen.findByRole("heading", { name: /alpha coordinator/ });
    await pickTarget(user, /Global orchestrator/);
    await user.type(await screen.findByRole("textbox", { name: "Message Global orchestrator" }), "plan it");
    await user.click(screen.getByRole("button", { name: "Send message to Global orchestrator" }));
    await screen.findByText(/Waiting for the orchestrator.s reply…/);

    act(() => convo.think("First **idea**."));
    const body = await screen.findByText("idea", {}, { timeout: 3000 });
    const block = body.closest("details") as HTMLDetailsElement;
    expect(block).toHaveTextContent("Thinking");
    expect(block).toHaveTextContent("Streaming");
    expect(block.open).toBe(true);
    expect(block.closest("article")).toBeNull();

    act(() => convo.finishRunning(null, "Done."));
    await screen.findByText("Done.", {}, { timeout: 3000 });
    await waitFor(() => expect(block.open).toBe(false));
    expect(block).not.toHaveTextContent("Streaming");
    expect(block).toHaveTextContent("First idea.");
  });

  it.each([
    { label: "Global orchestrator", target: /Global orchestrator/ },
    { label: "Session hardening orchestrator", target: /Session hardening orchestrator/ },
    { label: "alpha coordinator", target: /alpha coordinator/ },
  ])("clears the waiting status after a successful $label turn", async ({ target }) => {
    const fake = liveDaemon();
    const convo = stubConversation(fake, "running");
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await screen.findByRole("heading", { name: /alpha coordinator/ });
    await pickTarget(user, target);

    const composer = screen.getByRole("textbox", { name: /^Message / });
    await user.type(composer, "hello");
    await user.keyboard("{Enter}");
    expect(await screen.findByText(/Waiting for the orchestrator.s reply…/)).toBeInTheDocument();
    expect(await screen.findByText("Recorded by the daemon. Waiting for the orchestrator's reply.")).toBeInTheDocument();
    expect(fake.callsTo("conversations.send")).toHaveLength(1);

    act(() => convo.finishRunning(null, "Hello from the runtime."));
    await waitFor(() => expect(screen.getByText("Hello from the runtime.")).toBeInTheDocument(), { timeout: 3000 });
    await waitFor(() => {
      expect(screen.queryByText(/Waiting for the orchestrator.s reply/)).toBeNull();
      expect(screen.queryByRole("button", { name: "Cancel reply" })).toBeNull();
      expect(screen.queryByText("Recorded by the daemon. Waiting for the orchestrator's reply.")).toBeNull();
    });
    expect(fake.callsTo("conversations.send")).toHaveLength(1);
  });

  it("replaces a running notice with the recorded failure after a turn fails", async () => {
    const fake = liveDaemon();
    const convo = stubConversation(fake, "running");
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await screen.findByRole("heading", { name: /alpha coordinator/ });
    await pickTarget(user, /Global orchestrator/);
    const composer = screen.getByRole("textbox", { name: "Message Global orchestrator" });
    await user.type(composer, "fail this turn");
    await user.keyboard("{Enter}");
    await screen.findByText(/Waiting for the orchestrator.s reply…/);
    expect(screen.getByRole("button", { name: "Cancel reply" })).toBeInTheDocument();

    act(() => convo.finishRunning("runtime_failed"));
    expect(await screen.findByText("The orchestrator runtime failed. The reply is incomplete or missing.", {}, { timeout: 3000 })).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.queryByText(/Waiting for the orchestrator.s reply/)).toBeNull();
      expect(screen.queryByRole("button", { name: "Cancel reply" })).toBeNull();
      expect(screen.queryByText("Recorded by the daemon. Waiting for the orchestrator's reply.")).toBeNull();
    });
  });

  it("polls a running turn, blocks a second send, and cancels through the daemon", async () => {
    const fake = liveDaemon();
    const convo = stubConversation(fake, "running");
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await screen.findByRole("heading", { name: /alpha coordinator/ });
    await pickTarget(user, /Global orchestrator/);
    await user.type(await screen.findByRole("textbox", { name: "Message Global orchestrator" }), "slow");
    await user.click(screen.getByRole("button", { name: "Send message to Global orchestrator" }));
    await screen.findByText(/Waiting for the orchestrator.s reply…/);
    await user.type(screen.getByRole("textbox", { name: "Message Global orchestrator" }), "more");
    expect(screen.getByRole("button", { name: "Send message to Global orchestrator" })).toBeDisabled();
    expect(screen.getByText(/Sending is disabled\./)).toHaveTextContent(/still working/);
    const reads = fake.callsTo("conversations.get").length;
    await waitFor(() => expect(fake.callsTo("conversations.get").length).toBeGreaterThan(reads), { timeout: 3000 });
    await user.click(screen.getByRole("button", { name: "Cancel reply" }));
    await waitFor(() => expect(fake.callsTo("conversations.cancel")).toHaveLength(1));
    const cancelParams = fake.callsTo("conversations.cancel")[0]!.params as Record<string, unknown>;
    expect(cancelParams.turn_id).toBe(convo.turn?.id);
    expect(await screen.findByText(/Canceled the reply/)).toBeInTheDocument();
    await waitFor(() => expect(screen.queryByText(/Waiting for the orchestrator.s reply…/)).toBeNull());
  });

  it("never sends a fixture id to the daemon", async () => {
    const fake = liveDaemon();
    const services = track(build({ fake }));
    services.drafts.setWorkspaceTarget("project:fixture:ws-auth");
    services.drafts.setWorkspaceDraft("project:fixture:ws-auth", "stale");
    const { user } = await mountView(services, false);
    await waitLive(services);
    // The stored fixture target is not in the live list, so the first real project is shown instead.
    expect(await screen.findByRole("heading", { name: "Talking to: alpha coordinator" })).toBeInTheDocument();
    await user.type(screen.getByRole("textbox", { name: "Message alpha coordinator" }), "x");
    await user.click(screen.getByRole("button", { name: "Send message to alpha coordinator" }));
    await waitFor(() => expect(fake.callsTo("conversations.send")).toHaveLength(1));
    for (const call of [...fake.callsTo("conversations.send"), ...fake.callsTo("coordinator.get")]) {
      expect(JSON.stringify(call.params)).not.toContain("fixture:");
    }
  });

  it("lists real catalog models and per-model efforts and checks the choice with the daemon", async () => {
    const fake = liveDaemon();
    const catalog = [CODEX_CATALOG];
    fake.stub("agents.catalog", catalog);
    stubValidate(fake, catalog);
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await screen.findByRole("heading", { name: /alpha coordinator/ });
    await user.click(screen.getByRole("button", { name: /^Model settings/ }));
    const model = await screen.findByRole("combobox", { name: "Model" });
    await waitFor(() => expect(model).toBeEnabled());
    expect(within(model).getAllByRole("option").map((o) => o.textContent)).toEqual(["Default model", "GPT A", "GPT B"]);
    expect(screen.getByRole("button", { name: /^Model settings: Codex · default model · default effort/ })).toBeInTheDocument();
    // With no model chosen, Codex's per-model levels are not known yet: the effort control says why.
    expect(screen.getByRole("combobox", { name: "Effort" })).toBeDisabled();
    expect(screen.getByText("Effort levels are listed per model. Choose a model to see its levels.")).toBeInTheDocument();
    expect(screen.queryByText(/PLACEHOLDER/)).toBeNull();
    await user.selectOptions(model, "gpt-a");
    const effort = screen.getByRole("combobox", { name: "Effort" });
    await waitFor(() =>
      expect(within(effort).getAllByRole("option").map((o) => o.textContent)).toEqual(["Default effort", "low", "high"]),
    );
    expect(await screen.findByText("The daemon accepts this choice.")).toBeInTheDocument();
    await user.selectOptions(model, "gpt-b");
    await waitFor(() =>
      expect(within(screen.getByRole("combobox", { name: "Effort" })).getAllByRole("option").map((o) => o.textContent)).toEqual([
        "Default effort",
        "medium",
      ]),
    );
    await user.selectOptions(screen.getByRole("combobox", { name: "Effort" }), "medium");
    await waitFor(() => {
      const last = fake.callsTo("agents.validate_selection").at(-1)!.params as Record<string, unknown>;
      expect(last).toEqual({ agent: "codex", model: "gpt-b", effort: "medium" });
    });
    fake.failNext("agents.validate_selection", { code: -32603, message: "validation unavailable" });
    await user.selectOptions(model, "gpt-a");
    expect(await screen.findByText(/This choice could not be checked/)).toHaveTextContent(/observed \d+ s ago/);
    expect(screen.getByText(/A changed choice is sent with each message/)).toBeInTheDocument();
  });

  it("sends the picked agent, model and effort with the next message, and nothing while the picker is untouched", async () => {
    const fake = liveDaemon();
    const catalog = [CODEX_CATALOG];
    fake.stub("agents.catalog", catalog);
    stubValidate(fake, catalog);
    stubConversation(fake, "completed");
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await pickTarget(user, /Global orchestrator/);
    const box = await screen.findByRole("textbox", { name: "Message Global orchestrator" });
    const send = screen.getByRole("button", { name: "Send message to Global orchestrator" });
    await user.type(box, "first");
    await user.click(send);
    await waitFor(() => expect(fake.callsTo("conversations.send")).toHaveLength(1));
    expect(fake.callsTo("conversations.send")[0]!.params).not.toHaveProperty("selection");
    await user.click(screen.getByRole("button", { name: /^Model settings/ }));
    await user.selectOptions(await screen.findByRole("combobox", { name: "Model" }), "gpt-a");
    const effort = screen.getByRole("combobox", { name: "Effort" });
    await waitFor(() => expect(effort).toBeEnabled());
    await user.selectOptions(effort, "high");
    await user.type(box, "second");
    await user.click(send);
    await waitFor(() => expect(fake.callsTo("conversations.send")).toHaveLength(2));
    expect(fake.callsTo("conversations.send")[1]!.params).toMatchObject({
      text: "second",
      selection: { agent: "codex", model: "gpt-a", effort: "high" },
    });
  });

  it("blocks a chosen model on a daemon without conversation_selection, and sends nothing without the choice", async () => {
    const fake = liveDaemon();
    fake.capabilitiesOverride = ALL_CAPABILITIES.filter((capability) => capability !== "conversation_selection");
    const catalog = [CODEX_CATALOG];
    fake.stub("agents.catalog", catalog);
    stubValidate(fake, catalog);
    stubConversation(fake, "completed");
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await pickTarget(user, /Global orchestrator/);
    const box = await screen.findByRole("textbox", { name: "Message Global orchestrator" });
    const send = screen.getByRole("button", { name: "Send message to Global orchestrator" });
    // An untouched picker sends exactly the request an older desktop always sent.
    await user.type(box, "first");
    await user.click(send);
    await waitFor(() => expect(fake.callsTo("conversations.send")).toHaveLength(1));
    expect(fake.callsTo("conversations.send")[0]!.params).not.toHaveProperty("selection");
    await user.click(screen.getByRole("button", { name: /^Model settings/ }));
    await user.selectOptions(await screen.findByRole("combobox", { name: "Model" }), "gpt-a");
    await user.type(box, "second");
    expect(send).toBeDisabled();
    expect(screen.getByText(/This daemon cannot use a chosen model or effort/)).toBeInTheDocument();
    expect(fake.callsTo("conversations.send")).toHaveLength(1);
  });

  it("sends the chosen model with a project coordinator message and keeps the choice", async () => {
    const fake = liveDaemon();
    const catalog = [CODEX_CATALOG];
    fake.stub("agents.catalog", catalog);
    stubValidate(fake, catalog);
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await screen.findByRole("heading", { name: /alpha coordinator/ });
    await user.click(screen.getByRole("button", { name: /^Model settings/ }));
    await user.selectOptions(await screen.findByRole("combobox", { name: "Model" }), "gpt-a");
    await user.type(screen.getByRole("textbox", { name: "Message alpha coordinator" }), "a note");
    expect(screen.queryByText(/Coordinator notes take no model/)).toBeNull();
    await user.click(screen.getByRole("button", { name: "Send message to alpha coordinator" }));
    await waitFor(() => expect(fake.callsTo("conversations.send")).toHaveLength(1));
    expect(fake.callsTo("conversations.send")[0]!.params).toMatchObject({ selection: { agent: "codex", model: "gpt-a" } });
    expect(window.localStorage.getItem(CHOICE_PREFERENCE_KEY)).toBe(JSON.stringify({ agent: "codex", model: "gpt-a" }));
    expect(screen.getByRole("button", { name: /^Model settings: Codex · GPT A/ })).toBeInTheDocument();
  });

  it("sends a project coordinator message with the saved model, effort and permission level", async () => {
    window.localStorage.setItem(CHOICE_PREFERENCE_KEY, JSON.stringify({ agent: "codex", model: "gpt-a", effort: "high" }));
    window.localStorage.setItem(RESTRICTED_PREFERENCE_KEY, "restricted");
    const fake = liveDaemon();
    const catalog = [CODEX_CATALOG];
    fake.stub("agents.catalog", catalog);
    stubValidate(fake, catalog);
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await screen.findByRole("heading", { name: /alpha coordinator/ });
    // The picker label arrives with the catalog read, which a slow runner can finish after the heading.
    expect(await screen.findByRole("button", { name: /^Model settings: Codex · GPT A · high/ })).toBeInTheDocument();
    await user.type(screen.getByRole("textbox", { name: "Message alpha coordinator" }), "a note");
    expect(screen.queryByText(/Coordinator notes take no model/)).toBeNull();
    await user.click(screen.getByRole("button", { name: "Send message to alpha coordinator" }));
    await waitFor(() => expect(fake.callsTo("conversations.send")).toHaveLength(1));
    const params = fake.callsTo("conversations.send")[0]!.params as Record<string, unknown>;
    expect(params).toMatchObject({
      target: { kind: "project", project_id: ALPHA }, text: "a note",
      selection: { agent: "codex", model: "gpt-a", effort: "high" }, permission: "restricted",
    });
    expect(window.localStorage.getItem(CHOICE_PREFERENCE_KEY)).toBe(
      JSON.stringify({ agent: "codex", model: "gpt-a", effort: "high" }),
    );
    expect(window.localStorage.getItem(RESTRICTED_PREFERENCE_KEY)).toBe("restricted");
  });

  it("keeps Enter on the model selector from submitting the unsent draft in either direction", async () => {
    const fake = liveDaemon();
    const catalog = [CODEX_CATALOG];
    fake.stub("agents.catalog", catalog);
    stubValidate(fake, catalog);
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await screen.findByRole("heading", { name: /alpha coordinator/ });
    const draft = await screen.findByRole("textbox", { name: "Message alpha coordinator" });
    await user.type(draft, "keep this draft");
    await user.click(screen.getByRole("button", { name: /^Model settings/ }));
    const model = await screen.findByRole("combobox", { name: "Model" });
    await waitFor(() => expect(model).toBeEnabled());

    await user.selectOptions(model, "gpt-b");
    model.focus();
    expect(model).toHaveValue("gpt-b");
    await user.keyboard("{Enter}");
    fireEvent.submit(screen.getByRole("form", { name: "Message alpha coordinator" }));
    await user.keyboard("{Enter}");
    fireEvent.submit(screen.getByRole("form", { name: "Message alpha coordinator" }));
    expect(model).toHaveValue("gpt-b");
    expect(draft).toHaveValue("keep this draft");
    expect(fake.callsTo("conversations.send")).toHaveLength(0);

    await user.selectOptions(model, "gpt-a");
    model.focus();
    expect(model).toHaveValue("gpt-a");
    await user.keyboard("{Enter}");
    fireEvent.submit(screen.getByRole("form", { name: "Message alpha coordinator" }));
    await user.keyboard("{Enter}");
    fireEvent.submit(screen.getByRole("form", { name: "Message alpha coordinator" }));
    expect(model).toHaveValue("gpt-a");
    expect(draft).toHaveValue("keep this draft");
    expect(fake.callsTo("conversations.send")).toHaveLength(0);
  });

  it("disables a non-launchable provider and never defaults to it", async () => {
    const fake = liveDaemon();
    const caps = (agent: string, launchable: boolean) => ({
      agent,
      installed: launchable,
      launchable,
      capabilities: launchable
        ? { streaming: true, interactive_input: false, structured_output: true, cancellation: true, session_resumption: false, approvals: false, model_selection: true }
        : null,
    });
    fake.stub("agents.capabilities", [caps("pi", false), caps("codex", true)]);
    fake.stub("agents.catalog", [PI_CATALOG, CODEX_CATALOG]);
    stubValidate(fake, [PI_CATALOG, CODEX_CATALOG]);
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await screen.findByRole("heading", { name: /alpha coordinator/ });
    await user.click(screen.getByRole("button", { name: /^Model settings/ }));
    const provider = await screen.findByRole("combobox", { name: "Provider" });
    const pi = within(provider).getByRole("option", { name: /Pi \(not launchable\)/ });
    expect(pi).toBeDisabled();
    expect(within(provider).getByRole("option", { name: "Codex" })).toBeEnabled();
    await waitFor(() => expect(provider).toHaveValue("codex"));
    expect(screen.getByRole("button", { name: /^Model settings: Codex · default model/ })).toBeInTheDocument();
    await user.selectOptions(provider, "pi");
    expect(provider).toHaveValue("codex");
  });

  it("lists GitHub Copilot by its display name, disables it when its CLI is not installed, and never defaults to it", async () => {
    const fake = liveDaemon();
    fake.stub("agents.capabilities", [
      {
        agent: "codex",
        installed: true,
        launchable: true,
        capabilities: { streaming: true, interactive_input: false, structured_output: true, cancellation: true, session_resumption: false, approvals: false, model_selection: true },
      },
      { agent: "github-copilot", installed: true, launchable: false, capabilities: null },
    ]);
    fake.stub("agents.catalog", [CODEX_CATALOG, COPILOT_UNAVAILABLE_CATALOG]);
    stubValidate(fake, [CODEX_CATALOG]);
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await screen.findByRole("heading", { name: /alpha coordinator/ });
    await user.click(screen.getByRole("button", { name: /^Model settings/ }));
    const provider = await screen.findByRole("combobox", { name: "Provider" });
    const copilot = within(provider).getByRole("option", { name: /GitHub Copilot \(not launchable\)/ });
    expect(copilot).toBeDisabled();
    await waitFor(() => expect(provider).toHaveValue("codex"));
    expect(screen.getByRole("button", { name: /^Model settings: Codex · default model/ })).toBeInTheDocument();
  });

  it("does not offer model settings when no provider is launchable", async () => {
    const fake = liveDaemon();
    fake.stub("agents.capabilities", [
      { agent: "codex", installed: false, launchable: false, capabilities: null },
    ]);
    fake.stub("agents.catalog", [CODEX_CATALOG]);
    stubValidate(fake, [CODEX_CATALOG]);
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await screen.findByRole("heading", { name: /alpha coordinator/ });
    await user.click(screen.getByRole("button", { name: /^Model settings/ }));

    const provider = await screen.findByRole("combobox", { name: "Provider" });
    expect(provider).toBeDisabled();
    expect(provider).toHaveValue("");
    expect(within(provider).getByRole("option", { name: "No launchable providers" })).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Model" })).toBeDisabled();
    expect(screen.getByRole("combobox", { name: "Effort" })).toBeDisabled();
    expect(screen.getByText("No launchable providers are reported.")).toBeInTheDocument();
    expect(fake.callsTo("agents.validate_selection")).toHaveLength(0);
  });

  it("retries a cold catalog shortly and then lists the models", async () => {
    const fake = liveDaemon();
    let reads = 0;
    fake.stub("agents.catalog", () => (++reads === 1 ? [COLD_CATALOG("codex")] : [CODEX_CATALOG]));
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await screen.findByRole("heading", { name: /alpha coordinator/ });
    await user.click(screen.getByRole("button", { name: /^Model settings/ }));
    expect((await screen.findAllByText(/retry shortly/)).length).toBeGreaterThan(0);
    const model = screen.getByRole("combobox", { name: "Model" });
    await waitFor(
      () => expect(within(model).getAllByRole("option").map((o) => o.textContent)).toEqual(["Default model", "GPT A", "GPT B"]),
      { timeout: 5000 },
    );
    expect(fake.callsTo("agents.catalog").length).toBeGreaterThanOrEqual(2);
  });

  it("offers Claude Code's documented aliases, labels them as documented, and checks the chosen one", async () => {
    const fake = liveDaemon();
    fake.stub("agents.capabilities", [
      { agent: "claude-code", installed: true, launchable: true, capabilities: { streaming: true, interactive_input: false, structured_output: true, cancellation: true, session_resumption: false, approvals: false, model_selection: true } },
    ]);
    fake.stub("agents.catalog", [CLAUDE_CATALOG]);
    stubValidate(fake, [CLAUDE_CATALOG]);
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await screen.findByRole("heading", { name: /alpha coordinator/ });
    await user.click(screen.getByRole("button", { name: /^Model settings/ }));
    const model = await screen.findByRole("combobox", { name: "Model" });
    await waitFor(() => expect(model).toBeEnabled());
    expect(within(model).getAllByRole("option").map((o) => o.textContent)).toEqual([
      "Default model",
      "Opus (alias)",
      "Sonnet (alias)",
      "Haiku (alias)",
    ]);
    expect(screen.getByText(/Aliases documented by Claude Code/)).toBeInTheDocument();
    expect(model).toHaveAccessibleDescription(/availability depends on your account/);
    expect(screen.getByRole("button", { name: /^Model settings: Claude Code · default model · default effort/ })).toBeInTheDocument();
    await user.selectOptions(model, "sonnet");
    await waitFor(() => {
      const last = fake.callsTo("agents.validate_selection").at(-1)!.params as Record<string, unknown>;
      expect(last).toEqual({ agent: "claude-code", model: "sonnet" });
    });
    expect(await screen.findByText("The daemon accepts this choice.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^Model settings: Claude Code · Sonnet \(alias\) · default effort/ })).toBeInTheDocument();
  });

  it("shows a provider with no model list as a disabled Default model, with its reason under the control", async () => {
    const fake = liveDaemon();
    fake.stub("agents.capabilities", [
      { agent: "claude-code", installed: true, launchable: true, capabilities: { streaming: true, interactive_input: false, structured_output: true, cancellation: true, session_resumption: false, approvals: false, model_selection: true } },
    ]);
    fake.stub("agents.catalog", [
      {
        ...CLAUDE_CATALOG,
        models: { state: "unsupported", detail: "the CLI cannot enumerate the models available to the account", items: [] },
      },
    ]);
    stubValidate(fake, [CLAUDE_CATALOG]);
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await screen.findByRole("heading", { name: /alpha coordinator/ });
    await user.click(screen.getByRole("button", { name: /^Model settings/ }));
    const model = await screen.findByRole("combobox", { name: "Model" });
    expect(model).toBeDisabled();
    expect(within(model).getAllByRole("option").map((o) => o.textContent)).toEqual(["Default model"]);
    const reason = screen.getByText("The CLI cannot enumerate the models available to the account.");
    expect(reason.tagName).toBe("P");
    expect(model).toHaveAccessibleDescription(/cannot enumerate the models available to the account/);
    expect(screen.getByRole("button", { name: /^Model settings: Claude Code · default model · default effort/ })).toBeInTheDocument();
  });

  it("keeps the model menu honest when the daemon has no catalog method", async () => {
    const fake = liveDaemon();
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await screen.findByRole("heading", { name: /alpha coordinator/ });
    await user.click(screen.getByRole("button", { name: /^Model settings/ }));
    for (const name of ["Model", "Effort"]) expect(screen.getByRole("combobox", { name })).toBeDisabled();
    expect(screen.queryByText(/Codex \(placeholder\)/)).toBeNull();
    expect(screen.queryByText(/PLACEHOLDER/)).toBeNull();
  });

  it("keeps legacy notes readable but blocks unanswered sends on older daemons", async () => {
    const fake = liveDaemon();
    fake.capabilitiesOverride = ALL_CAPABILITIES.filter((capability) => capability !== "project_conversations");
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await screen.findByRole("heading", { name: "Talking to: alpha coordinator", level: 2 });
    expect(await screen.findByText("hello")).toBeInTheDocument();
    const box = screen.getByRole("textbox", { name: "Message alpha coordinator" });
    await user.type(box, "keep my question");
    expect(screen.getByRole("button", { name: "Send message to alpha coordinator" })).toBeDisabled();
    expect(screen.getByText(/Update the daemon to enable project coordinator replies/)).toBeInTheDocument();
    expect(box).toHaveValue("keep my question");
    expect(fake.callsTo("conversations.send")).toHaveLength(0);
    expect(fake.callsTo("coordinator.send_message")).toHaveLength(0);
  });

  it("names the composer for screen readers with a fixed placeholder and shows no visible label or key hint", async () => {
    const fake = liveDaemon();
    const services = track(build({ fake }));
    const { user: _user } = await mountView(services, false);
    await waitLive(services);
    const box = await screen.findByRole("textbox", { name: "Message alpha coordinator" });
    expect(box.getAttribute("placeholder")).toBe("Ask a question, redirect work, or discuss the plan…");
    expect(box).toHaveAttribute("rows", "1");
    expect(screen.queryByText("Message alpha coordinator", { selector: "label" })).toBeNull();
    expect(screen.queryByText(/Press Enter to send, Shift\+Enter for a new line\./)).toBeNull();
  });

  it("shows an unavailable transcript with Retry", async () => {
    const fake = liveDaemon();
    fake.failNext("coordinator.get", { code: -32603, message: "store down" });
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    expect(await screen.findByText(/The conversation is unavailable/)).toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent(/The conversation is unavailable:.*observed \d+ s ago/);
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByText("hello")).toBeInTheDocument();
  });

  it("renders an empty transcript as empty, not as an error", async () => {
    const fake = liveDaemon();
    fake.stub("coordinator.get", { project_id: ALPHA, entries: [], more: false, next_after_sequence: 0 });
    const services = track(build({ fake }));
    await mountView(services, false);
    await waitLive(services);
    expect(await screen.findByText("No messages yet")).toBeInTheDocument();
  });

  it("does not show loading conversations once the daemon is unreachable", async () => {
    const fake = liveDaemon();
    fake.openError = new DaemonError("disconnected", "down");
    const services = track(build({ fake }));
    await mountView(services, false);
    await waitFor(() => expect(services.connection.getSnapshot().failedAttempts).toBeGreaterThan(0));
    // The failure is recorded before the view re-renders, so wait for the rendered state to settle.
    await waitFor(() => expect(screen.queryByText("Loading conversations…")).not.toBeInTheDocument());
  });
});

describe("workspace view, fixture mode", () => {
  it("sends nothing to a connected daemon when the fixture surfaces are active (every target, placeholder ids included)", async () => {
    const fake = liveDaemon();
    const services = track(
      createServices({
        connector: fake,
        connection: { backoffMs: [5], requestTimeoutMs: 250 },
        data: { coalesceMs: 10 },
        sources: createFixtureSources(),
        surfaces: createFixtureSurfaces(),
      }),
    );
    const { user } = await mountView(services);
    const before = fake.calls.length;
    await user.type(await screen.findByRole("textbox", { name: "Message Global orchestrator" }), "plan it");
    await user.click(screen.getByRole("button", { name: "Send message to Global orchestrator" }));
    expect(await screen.findByText(/SIMULATED: this message was not sent or recorded anywhere/)).toBeInTheDocument();
    await pickTarget(user, /Auth hardening/);
    await user.type(await screen.findByRole("textbox", { name: /^Message / }), "and this");
    await user.click(screen.getByRole("button", { name: /^Send message to / }));
    await screen.findAllByText(/SIMULATED/);
    expect(fake.calls.slice(before).filter((call) => /^(conversations|coordinator)\./.test(call.method) && call.method !== "coordinator.get")).toEqual([]);
    expect(fake.calls.slice(before).filter((call) => /fixture:/.test(JSON.stringify(call.params)))).toEqual([]);
  });

  it("defaults to the PLACEHOLDER Global conversation and simulates a send without any daemon", async () => {
    const services = build();
    const { user } = await mountView(services);
    expect(await screen.findByRole("heading", { name: "Talking to: Global orchestrator" })).toBeInTheDocument();
    expect(screen.getAllByText(/PLACEHOLDER DATA \(D10\)/).length).toBeGreaterThan(0);
    await user.type(screen.getByRole("textbox", { name: "Message Global orchestrator" }), "plan the sprint");
    await user.click(screen.getByRole("button", { name: "Send message to Global orchestrator" }));
    expect(await screen.findByText(/SIMULATED: this message was not sent or recorded anywhere/)).toBeInTheDocument();
    const log = screen.getByRole("log");
    expect(within(log).getByText("plan the sprint")).toBeInTheDocument();
    expect(within(log).getByText(/SIMULATED, not recorded/)).toBeInTheDocument();
  });

  it("keeps simulated entries per target while switching", async () => {
    const services = build();
    const { user } = await mountView(services);
    await user.type(await screen.findByRole("textbox", { name: "Message Global orchestrator" }), "one");
    await user.click(screen.getByRole("button", { name: "Send message to Global orchestrator" }));
    await screen.findByText(/SIMULATED:/);
    await pickTarget(user, /Auth hardening coordinator/);
    expect(await screen.findByRole("heading", { name: "Talking to: Auth hardening coordinator" })).toBeInTheDocument();
    expect(within(screen.getByRole("log")).queryByText("one")).toBeNull();
    await pickTarget(user, /Global orchestrator/);
    expect(within(screen.getByRole("log")).getByText("one")).toBeInTheDocument();
  });

  it("enables the model menu locally and labels it PLACEHOLDER", async () => {
    const services = build();
    const { user } = await mountView(services);
    await screen.findByRole("heading", { name: /Global orchestrator/ });
    const trigger = screen.getByRole("button", { name: /^Model settings/ });
    expect(within(trigger).getByText("PLACEHOLDER (D14)")).toBeInTheDocument();
    await user.click(trigger);
    expect(screen.getByRole("combobox", { name: "Provider" })).toBeEnabled();
    expect(screen.getByText(/never sent anywhere/)).toBeInTheDocument();
  });

  it("shows chips for project targets: Escalate is disabled with a reason, the dependency plan chip navigates", async () => {
    const services = build();
    const { user } = await mountView(services);
    await screen.findByRole("heading", { name: /Global orchestrator/ });
    expect(screen.queryByRole("link", { name: "Show dependency plan" })).toBeNull();
    await pickTarget(user, /Docs refresh coordinator/);
    expect(await screen.findByRole("button", { name: "Escalate to Global" })).toBeDisabled();
    expect(screen.getByText("Escalate to Global is not available: the daemon has no operation for it.")).toBeInTheDocument();
    await user.click(screen.getByRole("link", { name: "Show dependency plan" }));
    expect(services.router.getSnapshot()).toEqual({ name: "global", view: "workstreams" });
  });

  it("opens Review & Decisions from the Request review chip", async () => {
    const services = build();
    const { user } = await mountView(services);
    await screen.findByRole("heading", { name: /Global orchestrator/ });
    await pickTarget(user, /Docs refresh coordinator/);
    const review = await screen.findByRole("link", { name: "Request review" });
    expect(review).toHaveAttribute("href", "#/review");
    await user.click(review);
    expect(services.router.getSnapshot()).toEqual({ name: "global", view: "review" });
  });

  it("gives the Global orchestrator the wireframe scope line", async () => {
    const services = build();
    await mountView(services);
    expect(
      await screen.findByText("All workstreams · Cross-workstream coordination", { selector: "p.ws-scope" }),
    ).toBeInTheDocument();
  });
});

describe("drafts and target selection", () => {
  it("retains each target's draft while switching and across a reload from storage", async () => {
    const storage = memoryStorage();
    const fake = liveDaemon();
    const first = track(build({ fake, storage }));
    const { user, view } = await mountView(first, false);
    await waitLive(first);
    await user.type(await screen.findByRole("textbox", { name: "Message alpha coordinator" }), "alpha draft");
    await pickTarget(user, /beta coordinator/);
    await user.type(await screen.findByRole("textbox", { name: "Message beta coordinator" }), "beta draft");
    await pickTarget(user, /alpha coordinator/);
    expect(await screen.findByRole("textbox", { name: "Message alpha coordinator" })).toHaveValue("alpha draft");

    view.unmount();
    first.connection.stop();
    const second = track(build({ fake: liveDaemon(), storage }));
    await mountView(second, false);
    await waitLive(second);
    expect(await screen.findByRole("heading", { name: "Talking to: alpha coordinator" })).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Message alpha coordinator" })).toHaveValue("alpha draft");
    await pickTarget(userEvent.setup(), /beta coordinator/);
    expect(await screen.findByRole("textbox", { name: "Message beta coordinator" })).toHaveValue("beta draft");
  });

  it("keeps each target's recorded transcript distinct while switching and across a reload", async () => {
    const storage = memoryStorage();
    const fake = liveDaemon();
    const transcripts = new Map([
      [ALPHA, "alpha recorded note"],
      [BETA, "beta recorded note"],
    ]);
    fake.stub("coordinator.get", (params: Record<string, unknown>) => {
      const projectId = String(params.project_id);
      return {
        project_id: projectId,
        entries: [
          {
            sequence: 1,
            id: `note-${projectId}`,
            project_id: projectId,
            kind: "message",
            author: "user",
            body: transcripts.get(projectId) ?? "",
            validation: null,
            created_at: "2026-01-01T09:00:00Z",
          },
        ],
        more: false,
        next_after_sequence: 1,
      };
    });

    const first = track(build({ fake, storage }));
    const { user, view } = await mountView(first, false);
    await waitLive(first);
    const alphaLog = await screen.findByRole("log", { name: "Conversation with alpha coordinator" });
    expect(await within(alphaLog).findByText("alpha recorded note")).toBeInTheDocument();

    await pickTarget(user, /beta coordinator/);
    const betaLog = await screen.findByRole("log", { name: "Conversation with beta coordinator" });
    expect(await within(betaLog).findByText("beta recorded note")).toBeInTheDocument();
    expect(within(betaLog).queryByText("alpha recorded note")).toBeNull();
    view.unmount();
    first.connection.stop();

    const second = track(build({ fake, storage }));
    const { user: secondUser } = await mountView(second, false);
    await waitLive(second);
    expect(await screen.findByRole("heading", { name: "Talking to: beta coordinator" })).toBeInTheDocument();
    const restoredBetaLog = await screen.findByRole("log", { name: "Conversation with beta coordinator" });
    expect(await within(restoredBetaLog).findByText("beta recorded note")).toBeInTheDocument();
    expect(within(restoredBetaLog).queryByText("alpha recorded note")).toBeNull();

    await pickTarget(secondUser, /alpha coordinator/);
    const restoredAlphaLog = await screen.findByRole("log", { name: "Conversation with alpha coordinator" });
    expect(await within(restoredAlphaLog).findByText("alpha recorded note")).toBeInTheDocument();
    expect(within(restoredAlphaLog).queryByText("beta recorded note")).toBeNull();
  });

  it("restores the stored target on reload", async () => {
    const storage = memoryStorage();
    const first = build({ fake: liveDaemon(), storage });
    first.drafts.setWorkspaceTarget(`project:${BETA}`);
    first.connection.stop();
    const second = track(build({ fake: liveDaemon(), storage }));
    await mountView(second, false);
    await waitLive(second);
    expect(await screen.findByRole("heading", { name: "Talking to: beta coordinator" })).toBeInTheDocument();
  });

  it("still loads a draft snapshot that predates the workspace fields", () => {
    const storage = memoryStorage();
    storage.setItem("agent-orch.drafts.v1", JSON.stringify({ registerPath: "/x", submit: {}, coordinator: {} }));
    const services = build({ storage });
    expect(services.drafts.workspaceDraft("global")).toBe("");
    expect(services.drafts.workspace().target).toBeNull();
    expect(services.drafts.getSnapshot().registerPath).toBe("/x");
  });
});

describe("conversation target dropdown", () => {
  it("opens and closes from the keyboard and returns focus to its button", async () => {
    const services = build();
    const { user } = await mountView(services);
    await screen.findByRole("heading", { name: /Global orchestrator/ });
    const button = screen.getByRole("button", { name: /Choose conversation target/ });
    expect(button).toHaveAttribute("aria-expanded", "false");
    button.focus();
    await user.keyboard("{Enter}");
    expect(button).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("searchbox", { name: "Search conversations" })).toHaveFocus();
    await user.keyboard("{ArrowDown}");
    expect(screen.getByRole("radio", { name: /Global orchestrator/ })).toHaveFocus();
    await user.keyboard("{ArrowDown}");
    expect(screen.getByRole("radio", { name: /Auth hardening coordinator/ })).toBeChecked();
    await user.keyboard("{Escape}");
    expect(button).toHaveAttribute("aria-expanded", "false");
    expect(button).toHaveFocus();
  });

  it("announces matches and an empty search in the status region", async () => {
    const services = build();
    const { user } = await mountView(services);
    await screen.findByRole("heading", { name: /Global orchestrator/ });
    await user.click(screen.getByRole("button", { name: /Choose conversation target/ }));
    const panel = screen.getByRole("searchbox", { name: "Search conversations" }).closest(".ws-target-panel") as HTMLElement;
    const status = within(panel).getByRole("status");
    await user.type(screen.getByRole("searchbox", { name: "Search conversations" }), "docs");
    expect(status).toHaveTextContent("1 matching conversation.");
    await user.clear(screen.getByRole("searchbox", { name: "Search conversations" }));
    await user.type(screen.getByRole("searchbox", { name: "Search conversations" }), "zzz");
    expect(status).toHaveTextContent("No matching conversations.");
    expect(within(panel).queryAllByRole("radio")).toHaveLength(0);
  });

  it("closes on a press outside", async () => {
    const services = build();
    const { user } = await mountView(services);
    await screen.findByRole("heading", { name: /Global orchestrator/ });
    const button = screen.getByRole("button", { name: /Choose conversation target/ });
    await user.click(button);
    await user.click(document.body);
    expect(button).toHaveAttribute("aria-expanded", "false");
  });

  it("Enter in the search box neither sends the draft nor closes the panel", async () => {
    const services = build();
    const { user } = await mountView(services);
    await screen.findByRole("heading", { name: /Global orchestrator/ });
    const box = await screen.findByRole("textbox", { name: "Message Global orchestrator" });
    await user.type(box, "plan it");
    const button = screen.getByRole("button", { name: /Choose conversation target/ });
    await user.click(button);
    const search = screen.getByRole("searchbox", { name: "Search conversations" });
    await user.type(search, "auth{Enter}");
    expect(search).toHaveFocus();
    expect(button).toHaveAttribute("aria-expanded", "true");
    expect(box).toHaveValue("plan it");
    expect(screen.queryByText(/SIMULATED/)).toBeNull();
  });
});

describe("accessibility structure", () => {
  it("exposes a heading, a labeled conversation region, a log and a labeled composer", async () => {
    const services = build();
    await mountView(services);
    expect(await screen.findByRole("heading", { name: "Orchestrator Workspace", level: 1 })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: /Talking to: Global orchestrator/ })).toBeInTheDocument();
    expect(screen.getByRole("log")).toHaveAttribute("tabindex", "0");
    expect(screen.getByRole("form", { name: "Message Global orchestrator" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Send message to Global orchestrator" })).toBeDisabled();
  });

  it("sends on Enter and inserts a newline on Shift+Enter", async () => {
    const fake = liveDaemon();
    const convo = stubConversation(fake, "completed");
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await screen.findByRole("heading", { name: /alpha coordinator/ });
    await pickTarget(user, /Global orchestrator/);
    const box = await screen.findByRole("textbox", { name: "Message Global orchestrator" });
    await user.type(box, "line one");
    await user.keyboard("{Shift>}{Enter}{/Shift}");
    expect(box).toHaveValue("line one\n");
    expect(fake.callsTo("conversations.send")).toHaveLength(0);

    await user.keyboard("{Enter}");
    await waitFor(() => expect(fake.callsTo("conversations.send")).toHaveLength(1));
    expect(fake.callsTo("conversations.send")[0]!.params).toMatchObject({ text: "line one\n" });
    expect(box).toHaveValue("");
    expect(await screen.findByText("Hello from the runtime.")).toBeInTheDocument();
    expect(convo.turn?.state).toBe("completed");
  });
});

describe("conversation timestamps", () => {
  it("shows local time with its zone across UTC and the Boise daylight-saving boundary", () => {
    const boise = (at: string) => formatAt(at, "America/Boise").replace(/\s/g, "");
    const utc = (at: string) => formatAt(at, "UTC").replace(/\s/g, "");

    expect(boise("2026-11-10T21:00:00Z")).toContain("02:00PMMST");
    expect(utc("2026-11-10T21:00:00Z")).toContain("09:00PMUTC");
    expect(boise("2026-03-08T08:59:00Z")).toContain("01:59AMMST");
    expect(boise("2026-03-08T09:00:00Z")).toContain("03:00AMMDT");
    expect(formatAt("not-a-time", "America/Boise")).toBe("Unrecognized time");
  });
});

describe("composer keys", () => {
  it("sends on Enter in fixture mode, shows the notice and clears the draft without a newline", async () => {
    const services = build();
    const { user } = await mountView(services);
    const box = await screen.findByRole("textbox", { name: "Message Global orchestrator" });
    await user.type(box, "plan it{Enter}");
    expect(await screen.findByText(/SIMULATED: this message was not sent or recorded anywhere/)).toBeInTheDocument();
    expect(within(screen.getByRole("log")).getByText("plan it")).toBeInTheDocument();
    await waitFor(() => expect(box).toHaveValue(""));
  });

  it("still sends on Cmd+Enter (Meta)", async () => {
    const services = build();
    const { user } = await mountView(services);
    const box = await screen.findByRole("textbox", { name: "Message Global orchestrator" });
    await user.type(box, "hi{Meta>}{Enter}{/Meta}");
    expect(await screen.findByText(/SIMULATED:/)).toBeInTheDocument();
  });

  it("inserts a newline on Shift+Enter and does not send", async () => {
    const services = build();
    const { user } = await mountView(services);
    const box = await screen.findByRole("textbox", { name: "Message Global orchestrator" });
    await user.type(box, "line one{Shift>}{Enter}{/Shift}line two");
    expect(box).toHaveValue("line one\nline two");
    expect(screen.queryByText(/SIMULATED/)).toBeNull();
  });

  it("leaves Enter that confirms an IME composition to the IME, with no send", async () => {
    const services = build();
    const { user } = await mountView(services);
    const box = await screen.findByRole("textbox", { name: "Message Global orchestrator" });
    await user.type(box, "nihon");
    // A handled Enter calls preventDefault, which fireEvent reports as false. The IME needs the event.
    expect(fireEvent.keyDown(box, { key: "Enter", isComposing: true })).toBe(true);
    expect(fireEvent.keyDown(box, { key: "Enter", keyCode: 229 })).toBe(true);
    expect(box).toHaveValue("nihon");
    expect(screen.queryByText(/SIMULATED/)).toBeNull();
  });

  it("does not send an empty or whitespace-only draft, and adds no newline", async () => {
    const services = build();
    const { user } = await mountView(services);
    const box = await screen.findByRole("textbox", { name: "Message Global orchestrator" });
    await user.type(box, "{Enter}");
    expect(box).toHaveValue("");
    await user.type(box, "   {Enter}");
    expect(box).toHaveValue("   ");
    expect(screen.queryByText(/SIMULATED/)).toBeNull();
  });

  it("does not send or add a newline on Enter while the orchestrator is still replying", async () => {
    const fake = liveDaemon();
    stubConversation(fake, "running");
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await screen.findByRole("heading", { name: /alpha coordinator/ });
    await pickTarget(user, /Global orchestrator/);
    const box = await screen.findByRole("textbox", { name: "Message Global orchestrator" });
    await user.type(box, "slow{Enter}");
    await waitFor(() => expect(fake.callsTo("conversations.send")).toHaveLength(1));
    await screen.findByText(/Waiting for the orchestrator.s reply…/);
    await user.type(box, "more{Enter}");
    expect(fake.callsTo("conversations.send")).toHaveLength(1);
    expect(box).toHaveValue("more");
  });

  it("sends on Enter through the daemon, as the send button does", async () => {
    const fake = liveDaemon();
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    const box = await screen.findByRole("textbox", { name: "Message alpha coordinator" });
    await user.type(box, "ship it{Enter}");
    await waitFor(() => expect(fake.callsTo("conversations.send")).toHaveLength(1));
    const params = fake.callsTo("conversations.send")[0]!.params as Record<string, unknown>;
    expect(params).toMatchObject({ target: { kind: "project", project_id: ALPHA }, text: "ship it" });
    await waitFor(() => expect(box).toHaveValue(""));
  });

  it("Enter on a model setting neither sends the draft nor changes the composer", async () => {
    const services = build();
    const { user } = await mountView(services);
    await screen.findByRole("heading", { name: /Global orchestrator/ });
    const box = await screen.findByRole("textbox", { name: "Message Global orchestrator" });
    await user.type(box, "plan it");
    await user.click(screen.getByRole("button", { name: /^Model settings/ }));
    const provider = await screen.findByRole("combobox", { name: "Provider" });
    provider.focus();
    await user.keyboard("{Enter}");
    expect(provider).toHaveFocus();
    expect(box).toHaveValue("plan it");
    expect(screen.queryByText(/SIMULATED/)).toBeNull();
  });
});

describe("registration", () => {
  it("registers the view for the workspace route only", () => {
    expect(Object.keys(workspaceRegistration.globalViews)).toEqual(["workspace"]);
    const services = build();
    const { unmount } = render(<ServicesProvider services={services}>{workspaceRegistration.globalViews.workspace()}</ServicesProvider>);
    expect(screen.getByRole("heading", { name: "Orchestrator Workspace", level: 1 })).toBeInTheDocument();
    act(() => unmount());
  });
});

// DPES-1105: clearing history and removing or adding project coordinators, as the user sees them.
describe("workspace view, history actions", () => {
  it("removing a coordinator deletes its conversation, drops it from the picker, switches to Global and discards its draft", async () => {
    const fake = liveDaemon();
    fake.projects = [
      { id: ALPHA, coordinator_active: true },
      { id: BETA, coordinator_active: true },
    ];
    fake.stub("coordinator.remove", () => {
      fake.projects = fake.projects.map((p) => (p.id === ALPHA ? { ...p, coordinator_active: false } : p));
      return { project_id: ALPHA, changed: true, removed: true, entries_deleted: 1, clear_generation: 1 };
    });
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await screen.findByRole("heading", { name: "Talking to: alpha coordinator", level: 2 });
    await user.type(screen.getByRole("textbox", { name: "Message alpha coordinator" }), "unsent words");

    await user.click(screen.getByRole("button", { name: "Conversation actions for alpha coordinator" }));
    await user.click(screen.getByRole("menuitem", { name: "Remove coordinator…" }));
    await user.click(screen.getByRole("button", { name: "Remove coordinator permanently" }));

    expect(await screen.findByRole("heading", { name: "Talking to: Global orchestrator", level: 2 })).toBeInTheDocument();
    expect(fake.callsTo("coordinator.remove")[0].params).toMatchObject({ project_id: ALPHA });
    expect(await screen.findByText(/Coordinator removed\. Its conversation was deleted/)).toBeInTheDocument();
    expect(services.drafts.workspaceDraft(`project:${ALPHA}`)).toBe("");
    await user.click(screen.getByRole("button", { name: /Choose conversation target/ }));
    await waitFor(() => expect(screen.queryByRole("radio", { name: /alpha coordinator/ })).toBeNull());
    expect(screen.getByRole("radio", { name: /beta coordinator/ })).toBeInTheDocument();
  });

  it("adds a removed coordinator back from the picker and selects it", async () => {
    const fake = liveDaemon();
    fake.projects = [
      { id: ALPHA, coordinator_active: false },
      { id: BETA, coordinator_active: true },
    ];
    fake.stub("coordinator.restore", () => {
      fake.projects = fake.projects.map((p) => (p.id === ALPHA ? { ...p, coordinator_active: true } : p));
      return { project_id: ALPHA, changed: true, removed: false, entries_deleted: 0, clear_generation: 1 };
    });
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await screen.findByRole("heading", { name: "Talking to: beta coordinator", level: 2 });

    await user.click(screen.getByRole("button", { name: /Choose conversation target/ }));
    await user.click(screen.getByRole("button", { name: "Add coordinator…" }));
    const dialog = screen.getByRole("dialog", { name: "Add a coordinator" });
    await user.click(within(dialog).getByRole("button", { name: "Add alpha coordinator" }));

    expect(fake.callsTo("coordinator.restore")[0].params).toMatchObject({ project_id: ALPHA });
    expect(await screen.findByRole("heading", { name: "Talking to: alpha coordinator", level: 2 })).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("removing_a_coordinator_that_owns_open_work_says_so_in_plain_words", async () => {
    const fake = liveDaemon();
    fake.stub("coordinator.remove", () => {
      throw new StubRpcError(-32011, "conflict");
    });
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await screen.findByRole("heading", { name: "Talking to: alpha coordinator", level: 2 });

    await user.click(screen.getByRole("button", { name: "Conversation actions for alpha coordinator" }));
    await user.click(screen.getByRole("menuitem", { name: "Remove coordinator…" }));
    await user.click(screen.getByRole("button", { name: "Remove coordinator permanently" }));

    await waitFor(() => expect(screen.getAllByRole("alert").some((item) => item.textContent?.includes(REMOVE_OPEN_WORK_TEXT))).toBe(true));
    expect(screen.queryByText(/Removing the coordinator failed/)).toBeNull();
    expect(screen.getByRole("heading", { name: "Talking to: alpha coordinator", level: 2 })).toBeInTheDocument();
  });

  it("coordinator_panel_shows_the_daemon_state_and_saves_the_scope_through_the_daemon", async () => {
    const fake = populate(new FakeDaemon());
    const conversation = stubConversation(fake, "completed");
    conversation.coordinator = {
      scope_sentence: null,
      open_tasks: 1,
      open_requests: 1,
      routed_requests: [{ id: "r-0123456789abcdef", work: "Write the troubleshooting page.", answered: false, created_at: "2026-11-16T21:04:11Z" }],
      reports: [],
    };
    fake.stub("coordinator.set_scope", (params: Record<string, unknown>) => {
      conversation.coordinator = { ...conversation.coordinator!, scope_sentence: params.scope_sentence as string | null };
      return { project_id: params.project_id, scope_sentence: params.scope_sentence };
    });
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    const region = await screen.findByRole("region", { name: COORDINATOR_HEADING });
    expect(region.textContent).toContain("Scope: No scope set");
    expect(region.textContent).toContain("r-0123456789abcdef: Write the troubleshooting page.");

    await user.type(within(region).getByRole("textbox", { name: "Scope sentence" }), "Owns the user documentation.");
    await user.click(within(region).getByRole("button", { name: "Save scope" }));
    await waitFor(() => expect(fake.callsTo("coordinator.set_scope")).toHaveLength(1));
    expect(fake.callsTo("coordinator.set_scope")[0]!.params).toMatchObject({
      project_id: ALPHA,
      scope_sentence: "Owns the user documentation.",
      idempotency_key: expect.any(String),
    });
    await waitFor(() => expect(region.textContent).toContain("Scope: Owns the user documentation."));
  });

  it("coordinator_panel_scope_editor_is_off_for_a_session_that_is_not_the_approver", async () => {
    const fake = populate(new FakeDaemon());
    fake.approver = false;
    const conversation = stubConversation(fake, "completed");
    conversation.coordinator = { scope_sentence: "Owns docs.", open_tasks: 0, open_requests: 0, routed_requests: [], reports: [] };
    const services = track(build({ fake }));
    await mountView(services, false);
    await waitLive(services);
    const region = await screen.findByRole("region", { name: COORDINATOR_HEADING });
    expect(within(region).getByRole("textbox", { name: "Scope sentence" })).toBeDisabled();
    expect(within(region).getByRole("button", { name: "Save scope" })).toBeDisabled();
    expect(within(region).getByText(APPROVER_REQUIRED_NOTICE)).toBeInTheDocument();
  });

  it("clearing a coordinator's history asks first, empties the transcript and keeps the unsent draft", async () => {
    const fake = liveDaemon();
    let cleared = false;
    const note = {
      sequence: 1,
      id: "c-1",
      project_id: ALPHA,
      kind: "message",
      author: "user",
      body: "hello",
      validation: null,
      created_at: "2026-11-01T10:00:00Z",
    };
    fake.stub("coordinator.get", () => ({
      project_id: ALPHA,
      entries: cleared ? [] : [note],
      more: false,
      next_after_sequence: cleared ? 0 : 1,
      clear_generation: cleared ? 1 : 0,
    }));
    fake.stub("coordinator.clear", () => {
      cleared = true;
      return { project_id: ALPHA, cleared: true, entries_deleted: 1, clear_generation: 1 };
    });
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    const log = await screen.findByRole("log", { name: "Conversation with alpha coordinator" });
    expect(await within(log).findByText("hello")).toBeInTheDocument();
    const box = screen.getByRole("textbox", { name: "Message alpha coordinator" });
    await user.type(box, "still here");

    await user.click(screen.getByRole("button", { name: "Conversation actions for alpha coordinator" }));
    await user.click(screen.getByRole("menuitem", { name: "Clear history…" }));
    expect(screen.getByRole("dialog", { name: "Clear history for alpha coordinator?" })).toBeInTheDocument();
    expect(fake.callsTo("coordinator.clear")).toHaveLength(0);
    await user.click(screen.getByRole("button", { name: "Clear history permanently" }));

    expect(fake.callsTo("coordinator.clear")[0].params).toMatchObject({ project_id: ALPHA });
    expect(await screen.findByText("Coordinator history cleared. The deleted messages and replies cannot be recovered.")).toBeInTheDocument();
    await waitFor(() => expect(within(log).queryByText("hello")).toBeNull());
    expect(box).toHaveValue("still here");
  });

  it("in fixture mode, clearing is simulated, labeled as placeholder data, and reaches no daemon", async () => {
    const services = build();
    const { user } = await mountView(services);
    await user.click(await screen.findByRole("button", { name: "Conversation actions for Global orchestrator" }));
    await user.click(screen.getByRole("menuitem", { name: "Clear history…" }));
    expect(screen.getByRole("dialog")).toHaveTextContent("PLACEHOLDER DATA");
    await user.click(screen.getByRole("button", { name: "Clear history permanently" }));
    expect(await screen.findByText(/Simulated history clear: placeholder data, nothing was changed\./)).toBeInTheDocument();
  });
});

describe("Global tasks workstream is not a target (DPES-1106)", () => {
  /** The daemon's workstream for tasks Global proposes without naming one. It must not sit beside Global. */
  function seedGlobalTasks(fake: FakeDaemon) {
    WORKSTREAM_STATE.get(fake)!.workstreams.push({
      id: GLOBAL_TASKS_WORKSTREAM_ID,
      title: "Global tasks",
      summary: "Tasks the global orchestrator proposed without naming a workstream.",
      state: "active",
      revision: 1,
    });
  }

  it("lists Global orchestrator once and does not offer the Global tasks workstream", async () => {
    const fake = liveDaemon();
    seedGlobalTasks(fake);
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await user.click(await screen.findByRole("button", { name: /Choose conversation target/ }));
    expect(await screen.findByRole("radio", { name: /alpha coordinator/ })).toBeInTheDocument();
    expect(screen.getAllByRole("radio", { name: /Global orchestrator/ })).toHaveLength(1);
    expect(screen.queryByRole("radio", { name: /Global tasks/ })).toBeNull();
  });

  it("opens Global, not the first project, when the stored selection is the Global tasks workstream", async () => {
    const fake = liveDaemon();
    seedGlobalTasks(fake);
    const services = track(build({ fake, storage: memoryStorage() }));
    services.drafts.setWorkspaceTarget(`workstream:${GLOBAL_TASKS_WORKSTREAM_ID}`);
    const { user } = await mountView(services, false);
    await waitLive(services);
    await user.click(await screen.findByRole("button", { name: /Choose conversation target/ }));
    expect(await screen.findByRole("radio", { name: /alpha coordinator/ })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Talking to: Global orchestrator" })).toBeInTheDocument();
  });

  it("pins the id to the daemon's seed: workstream- plus sha256 of default:global-tasks", () => {
    expect(GLOBAL_TASKS_WORKSTREAM_ID).toBe(`workstream-${createHash("sha256").update("default:global-tasks").digest("hex")}`);
  });
});
