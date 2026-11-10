import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { createServices } from "../../createServices";
import { ALPHA, BETA, populate } from "../../../sources/surfaces/testkit";
import { createFixtureSources } from "../../../sources/fixtures";
import { FakeDaemon } from "../../../test/fakeDaemon";
import { CODEX_CATALOG, COLD_CATALOG, stubConversation, stubValidate } from "../../../test/m724Stubs";
import type { DraftStorage } from "../../drafts";
import { ServicesProvider, type Services } from "../../services";
import { workspaceRegistration } from "./register";
import { WorkspaceView } from "./WorkspaceView";

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

const liveDaemon = () => populate(new FakeDaemon());
const created: Services[] = [];
const track = (services: Services) => (created.push(services), services);

afterEach(() => {
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
    expect(screen.getByText(/does not create tasks or approvals/)).toBeInTheDocument();
  });

  it("sends through coordinator.send_message with an idempotency key, then reloads the recorded note", async () => {
    const fake = liveDaemon();
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    const box = await screen.findByRole("textbox", { name: "Message alpha coordinator" });
    await user.type(box, "ship it");
    await user.click(screen.getByRole("button", { name: "Send message to alpha coordinator" }));
    await waitFor(() => expect(fake.callsTo("coordinator.send_message")).toHaveLength(1));
    const params = fake.callsTo("coordinator.send_message")[0].params as Record<string, unknown>;
    expect(params).toMatchObject({ project_id: ALPHA, text: "ship it" });
    expect(String(params.idempotency_key)).not.toBe("");
    expect(await screen.findByText("Recorded by the daemon. The coordinator does not reply.")).toBeInTheDocument();
    expect(box).toHaveValue("");
    expect(services.drafts.workspaceDraft(`project:${ALPHA}`)).toBe("");
    // The transcript was re-read after the send.
    await waitFor(() => expect(fake.callsTo("coordinator.get").length).toBeGreaterThanOrEqual(2));
  });

  it("keeps the draft after a failed send and reuses the idempotency key when an indeterminate send is retried", async () => {
    const fake = liveDaemon();
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    const box = await screen.findByRole("textbox", { name: "Message alpha coordinator" });
    await user.type(box, "retry me");
    fake.failNext("coordinator.send_message", "hang");
    await user.click(screen.getByRole("button", { name: "Send message to alpha coordinator" }));
    expect(await screen.findByRole("alert", {}, { timeout: 2000 })).toHaveTextContent(/Sending the message failed/);
    expect(screen.getByText(/The daemon may have recorded it/)).toBeInTheDocument();
    expect(box).toHaveValue("retry me");
    await user.click(screen.getByRole("button", { name: "Send message to alpha coordinator" }));
    await waitFor(() => expect(fake.callsTo("coordinator.send_message")).toHaveLength(2));
    const [first, second] = fake.callsTo("coordinator.send_message").map((call) => (call.params as Record<string, unknown>).idempotency_key);
    expect(second).toBe(first);
    await waitFor(() => expect(box).toHaveValue(""));
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
    const banner = await screen.findAllByText(/Orchestrator runtime unavailable/);
    expect(banner.length).toBeGreaterThan(0);
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
    expect(await screen.findByText(/Cancelled the reply/)).toBeInTheDocument();
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
    await waitFor(() => expect(fake.callsTo("coordinator.send_message")).toHaveLength(1));
    for (const call of [...fake.callsTo("coordinator.send_message"), ...fake.callsTo("coordinator.get")]) {
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
    expect(within(model).getAllByRole("option").map((o) => o.textContent)).toEqual(["GPT A", "GPT B"]);
    const effort = screen.getByRole("combobox", { name: "Effort" });
    expect(within(effort).getAllByRole("option").map((o) => o.textContent)).toEqual(["Default effort", "low", "high"]);
    expect(screen.queryByText(/PLACEHOLDER/)).toBeNull();
    expect(await screen.findByText("The daemon supports this choice.")).toBeInTheDocument();
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
    expect(screen.getByText(/task submission only/)).toBeInTheDocument();
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
    await waitFor(() => expect(within(model).getAllByRole("option").map((o) => o.textContent)).toEqual(["GPT A", "GPT B"]), {
      timeout: 5000,
    });
    expect(fake.callsTo("agents.catalog").length).toBeGreaterThanOrEqual(2);
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

  it("labels project targets 'notes, no reply' in the header, the picker and the note copy", async () => {
    const services = track(build({ fake: liveDaemon() }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    const heading = await screen.findByRole("heading", { name: "Talking to: alpha coordinator", level: 2 });
    expect(within(heading.parentElement as HTMLElement).getByText("Notes, no reply")).toBeInTheDocument();
    expect(screen.getByText(/Notes only, no reply/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Choose conversation target/ }));
    expect(screen.getAllByText(/notes, no reply/).length).toBeGreaterThan(0);
  });

  it("keeps the send hint in persistent text, not in the placeholder", async () => {
    const fake = liveDaemon();
    const services = track(build({ fake }));
    const { user: _user } = await mountView(services, false);
    await waitLive(services);
    const box = await screen.findByRole("textbox", { name: "Message alpha coordinator" });
    expect(box.getAttribute("placeholder")).toBe("Message alpha coordinator");
    expect(screen.getByText("Press Ctrl or Cmd + Enter to send.")).toBeInTheDocument();
  });

  it("shows an unavailable transcript with Retry", async () => {
    const fake = liveDaemon();
    fake.failNext("coordinator.get", { code: -32603, message: "store down" });
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    expect(await screen.findByText(/The conversation is unavailable/)).toBeInTheDocument();
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
});

describe("workspace view, fixture mode", () => {
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

  it("shows chips for project targets: the dependency plan is navigation, the rest are disabled with a reason", async () => {
    const services = build();
    const { user } = await mountView(services);
    await screen.findByRole("heading", { name: /Global orchestrator/ });
    expect(screen.queryByRole("link", { name: "Show dependency plan" })).toBeNull();
    await pickTarget(user, /Docs refresh coordinator/);
    expect(await screen.findByRole("button", { name: "Escalate to Global" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Request review" })).toBeDisabled();
    expect(screen.getByText(/not available: the daemon has no operation for them/)).toBeInTheDocument();
    await user.click(screen.getByRole("link", { name: "Show dependency plan" }));
    expect(services.router.getSnapshot()).toEqual({ name: "global", view: "workstreams" });
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
});

describe("accessibility structure", () => {
  it("exposes a heading, a labelled conversation region, a log and a labelled composer", async () => {
    const services = build();
    await mountView(services);
    expect(await screen.findByRole("heading", { name: "Orchestrator Workspace", level: 1 })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: /Talking to: Global orchestrator/ })).toBeInTheDocument();
    expect(screen.getByRole("log")).toHaveAttribute("tabindex", "0");
    expect(screen.getByRole("form", { name: "Message Global orchestrator" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Send message to Global orchestrator" })).toBeDisabled();
  });

  it("sends on Ctrl+Enter in fixture mode", async () => {
    const services = build();
    const { user } = await mountView(services);
    const box = await screen.findByRole("textbox", { name: "Message Global orchestrator" });
    await user.type(box, "hi{Control>}{Enter}{/Control}");
    expect(await screen.findByText(/SIMULATED:/)).toBeInTheDocument();
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
