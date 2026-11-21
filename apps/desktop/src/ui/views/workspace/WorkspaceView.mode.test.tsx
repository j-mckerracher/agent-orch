import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { ALL_CAPABILITIES } from "../../../daemon/types";
import { createServices } from "../../createServices";
import { populate } from "../../../sources/surfaces/testkit";
import { createFixtureSources } from "../../../sources/fixtures";
import { createFixtureSurfaces } from "../../../sources/surfaces/fixtures";
import { FakeDaemon } from "../../../test/fakeDaemon";
import { CODEX_CATALOG, stubConversation } from "../../../test/m724Stubs";
import type { DraftStorage } from "../../drafts";
import { ServicesProvider, type Services } from "../../services";
import { CHOICE_PREFERENCE_KEY } from "./modelChoice";
import { WorkspaceView } from "./WorkspaceView";

const MODE_KEY = "agent-orch.chat.mode.codex";

function memoryStorage(): DraftStorage {
  const data = new Map<string, string>();
  return { getItem: (key) => data.get(key) ?? null, setItem: (key, value) => void data.set(key, value) };
}

function build(options: { fake?: FakeDaemon; fixtures?: boolean } = {}): Services {
  return createServices({
    connector: options.fake,
    connection: { backoffMs: [5], requestTimeoutMs: 250 },
    data: { coalesceMs: 10 },
    sources: createFixtureSources(),
    ...(options.fixtures ? { surfaces: createFixtureSurfaces() } : {}),
    draftStorage: memoryStorage(),
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
  window.localStorage.clear();
});

beforeEach(() => {
  window.localStorage.clear();
});

async function waitLive(services: Services) {
  await waitFor(() => expect(services.connection.getSnapshot().status).toBe("connected"));
}

async function pickGlobal(user: ReturnType<typeof userEvent.setup>) {
  const button = screen.getByRole("button", { name: /Choose conversation target/ });
  if (button.getAttribute("aria-expanded") !== "true") await user.click(button);
  await user.click(screen.getByRole("radio", { name: /Global orchestrator/ }));
}

async function sendFromGlobal(user: ReturnType<typeof userEvent.setup>, text: string) {
  const box = await screen.findByRole("textbox", { name: "Message Global orchestrator" });
  await user.type(box, text);
  await user.click(screen.getByRole("button", { name: "Send message to Global orchestrator" }));
}

/** Opens the mode dropdown, which is where a chat's mode is chosen. */
async function openModes(user: ReturnType<typeof userEvent.setup>) {
  const button = screen.getByRole("button", { name: /Chat mode:/ });
  if (button.getAttribute("aria-expanded") !== "true") await user.click(button);
}

/** Picks an option in one group of the open mode dropdown, by its label. */
async function chooseMode(user: ReturnType<typeof userEvent.setup>, group: string, label: string) {
  await openModes(user);
  await user.click(within(screen.getByRole("group", { name: group })).getByRole("radio", { name: new RegExp(`^${label}`) }));
}

/** A chat with Codex chosen, as a saved choice restores it on mount. */
function restoreCodex(extra: Record<string, string> = {}) {
  window.localStorage.setItem(CHOICE_PREFERENCE_KEY, JSON.stringify({ agent: "codex" }));
  for (const [key, value] of Object.entries(extra)) window.localStorage.setItem(key, value);
}

/** A Global conversation whose daemon reports `mode` for the turn, and nothing before the first send. */
function stubReportedMode(fake: FakeDaemon, mode: Record<string, string> | null) {
  let sent = false;
  const turn = {
    id: "turn-1",
    state: "completed",
    error_code: null,
    runtime: { name: "stub", version: "1" },
    reply_bytes: 0,
    created_at: "2026-11-15T12:00:00Z",
    updated_at: "2026-11-15T12:00:01Z",
    mode,
  };
  fake.stub("conversations.send", (params: Record<string, unknown>) => {
    sent = true;
    return {
      authority: "none",
      target: { kind: "global" },
      entry: {
        sequence: 1,
        id: "entry-1",
        turn_id: "turn-1",
        kind: "user_message",
        author: "user",
        body: String(params.text),
        error_code: null,
        created_at: "2026-11-15T12:00:00Z",
      },
      turn,
      epoch: "epoch-1",
    };
  });
  fake.stub("conversations.get", () => ({
    authority: "none",
    target: { kind: "global" },
    entries: sent
      ? [
          {
            sequence: 1,
            id: "entry-1",
            turn_id: "turn-1",
            kind: "user_message",
            author: "user",
            body: "hello",
            error_code: null,
            created_at: "2026-11-15T12:00:00Z",
          },
        ]
      : [],
    more: false,
    next_after_sequence: 0,
    latest_sequence: sent ? 1 : 0,
    clear_generation: 0,
    turn: sent ? turn : null,
    epoch: "epoch-1",
  }));
}

/** A Global conversation whose latest turn is an automatic update the user did not send, in `mode`. */
function stubLatestWakeTurn(fake: FakeDaemon, mode: Record<string, string>) {
  fake.stub("conversations.get", () => ({
    authority: "none",
    target: { kind: "global" },
    entries: [],
    more: false,
    next_after_sequence: 0,
    latest_sequence: 0,
    clear_generation: 0,
    turn: {
      id: "wake-1",
      state: "completed",
      error_code: null,
      runtime: { name: "stub", version: "1" },
      reply_bytes: 0,
      created_at: "2026-11-15T12:00:00Z",
      updated_at: "2026-11-15T12:00:01Z",
      mode,
      trigger: "wake",
    },
    epoch: "epoch-1",
  }));
}

/** A live daemon with Codex in the catalog, so its modes can be read. */
function codexDaemon() {
  const fake = liveDaemon();
  fake.stub("agents.catalog", [CODEX_CATALOG]);
  stubConversation(fake, "completed");
  return fake;
}

describe("chat modes in the composer, live daemon", () => {
  it("a routed message names no agent, so it sends an empty mode and no permission level", async () => {
    const fake = codexDaemon();
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await pickGlobal(user);
    expect(screen.getByRole("button", { name: /Chat mode:/ })).toHaveTextContent("Default mode");
    await sendFromGlobal(user, "hello");
    await waitFor(() => expect(fake.callsTo("conversations.send")).toHaveLength(1));
    const params = fake.callsTo("conversations.send")[0]!.params;
    expect(params).toMatchObject({ text: "hello" });
    expect(params).toHaveProperty("mode", {});
    expect(params).not.toHaveProperty("permission");
  });

  it("with an agent chosen, the dropdown lists its groups with the defaults selected, and says why Plan cannot be chosen", async () => {
    const fake = codexDaemon();
    restoreCodex();
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await pickGlobal(user);
    await waitFor(() => expect(screen.getByRole("button", { name: /Chat mode:/ })).toHaveAttribute("aria-label", "Chat mode: Auto · Standard"));
    await openModes(user);
    expect(within(screen.getByRole("group", { name: "Permission" })).getByRole("radio", { name: /^Auto/ })).toBeChecked();
    expect(within(screen.getByRole("group", { name: "Work" })).getByRole("radio", { name: /^Standard/ })).toBeChecked();
    expect(within(screen.getByRole("group", { name: "Work" })).getByRole("radio", { name: /^Plan/ })).toBeDisabled();
    // Plan and Goal both say why they cannot run under codex exec.
    expect(within(screen.getByRole("group", { name: "Work" })).getAllByText("Not available through codex exec.")).toHaveLength(2);
  });

  it("sends the modes the user chose, with the other group at its default", async () => {
    const fake = codexDaemon();
    restoreCodex();
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await pickGlobal(user);
    await chooseMode(user, "Permission", "Read only");
    await sendFromGlobal(user, "stay read-only");
    await waitFor(() => expect(fake.callsTo("conversations.send")).toHaveLength(1));
    const params = fake.callsTo("conversations.send")[0]!.params;
    expect(params).toMatchObject({ text: "stay read-only", mode: { permission: "read-only", work: "standard" } });
    expect(params).not.toHaveProperty("permission");
  });

  it("sends nothing at Full access until the user confirms it, then sends it", async () => {
    const fake = codexDaemon();
    restoreCodex();
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await pickGlobal(user);
    await chooseMode(user, "Permission", "Full access");
    await user.click(await screen.findByRole("button", { name: "Use Full" }));
    expect(screen.getByRole("button", { name: /Chat mode:/ })).toHaveAttribute("aria-label", "Chat mode: Full access · Standard");
    await sendFromGlobal(user, "run it");
    await waitFor(() => expect(fake.callsTo("conversations.send")).toHaveLength(1));
    expect(fake.callsTo("conversations.send")[0]!.params).toMatchObject({
      text: "run it",
      mode: { permission: "full-access", work: "standard" },
    });
  });

  it("sends the mode it had before when the user cancels Full access", async () => {
    const fake = codexDaemon();
    restoreCodex();
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await pickGlobal(user);
    await chooseMode(user, "Permission", "Full access");
    await user.click(await screen.findByRole("button", { name: "Cancel" }));
    await sendFromGlobal(user, "still auto");
    await waitFor(() => expect(fake.callsTo("conversations.send")).toHaveLength(1));
    expect(fake.callsTo("conversations.send")[0]!.params).toMatchObject({ mode: { permission: "auto", work: "standard" } });
  });

  it("an older daemon without chat modes gets the legacy permission level, and the control says it needs an update", async () => {
    const fake = codexDaemon();
    fake.capabilitiesOverride = ALL_CAPABILITIES.filter((capability) => capability !== "chat_modes");
    restoreCodex();
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await pickGlobal(user);
    expect(screen.getByRole("button", { name: /Chat mode:/ })).toHaveTextContent("Needs an updated daemon");
    await sendFromGlobal(user, "old daemon");
    await waitFor(() => expect(fake.callsTo("conversations.send")).toHaveLength(1));
    const params = fake.callsTo("conversations.send")[0]!.params;
    expect(params).toMatchObject({ text: "old daemon", permission: "auto" });
    expect(params).not.toHaveProperty("mode");
  });

  it("an older daemon with neither chat modes nor permission levels gets neither field, and the control says it needs an update", async () => {
    const fake = codexDaemon();
    fake.capabilitiesOverride = ALL_CAPABILITIES.filter((capability) => capability !== "chat_modes" && capability !== "permission_levels");
    restoreCodex();
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await pickGlobal(user);
    expect(screen.getByRole("button", { name: /Chat mode:/ })).toHaveTextContent("Needs an updated daemon");
    await sendFromGlobal(user, "oldest daemon");
    await waitFor(() => expect(fake.callsTo("conversations.send")).toHaveLength(1));
    const params = fake.callsTo("conversations.send")[0]!.params;
    expect(params).not.toHaveProperty("permission");
    expect(params).not.toHaveProperty("mode");
  });

  it("warns when the daemon reports a different mode from the one sent", async () => {
    const fake = codexDaemon();
    stubReportedMode(fake, { permission: "read-only", work: "standard" });
    restoreCodex();
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await pickGlobal(user);
    await sendFromGlobal(user, "hello");
    expect(await screen.findByRole("alert")).toHaveTextContent("ran in Read only · Standard, not Auto · Standard.");
  });

  it("shows the mode the daemon reports for the last reply, without a warning when it matches", async () => {
    const fake = codexDaemon();
    stubReportedMode(fake, { permission: "auto", work: "standard" });
    restoreCodex();
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await pickGlobal(user);
    await sendFromGlobal(user, "hello");
    expect(await screen.findByText("Last reply ran in Auto · Standard.")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("shows no mode note for a last automatic update, and no mismatch alert", async () => {
    const fake = codexDaemon();
    stubLatestWakeTurn(fake, { permission: "read-only", work: "standard" });
    restoreCodex();
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await pickGlobal(user);
    expect(await screen.findByText("No messages yet")).toBeInTheDocument();
    expect(screen.queryByText(/Last reply ran in/)).toBeNull();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("saves the chosen mode for the agent, and never saves Full access", async () => {
    const fake = codexDaemon();
    restoreCodex();
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await pickGlobal(user);
    await chooseMode(user, "Permission", "Read only");
    expect(window.localStorage.getItem(MODE_KEY)).toBe(JSON.stringify({ permission: "read-only" }));
    await chooseMode(user, "Permission", "Full access");
    await user.click(await screen.findByRole("button", { name: "Use Full" }));
    expect(window.localStorage.getItem(MODE_KEY)).toBe("{}");
  });

  it("restores a saved mode after a reload, and sends it", async () => {
    const fake = codexDaemon();
    restoreCodex({ [MODE_KEY]: JSON.stringify({ permission: "read-only" }) });
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await pickGlobal(user);
    expect(screen.getByRole("button", { name: /Chat mode:/ })).toHaveAttribute("aria-label", "Chat mode: Read only · Standard");
    await sendFromGlobal(user, "restored");
    await waitFor(() => expect(fake.callsTo("conversations.send")).toHaveLength(1));
    expect(fake.callsTo("conversations.send")[0]!.params).toMatchObject({ mode: { permission: "read-only", work: "standard" } });
  });
});

describe("chat modes in the composer, fixture mode", () => {
  it("sends nothing to a connected daemon, and says the message was simulated", async () => {
    const fake = liveDaemon();
    const services = track(build({ fake, fixtures: true }));
    const { user } = await mountView(services);
    const before = fake.calls.length;
    await sendFromGlobal(user, "first");
    expect(await screen.findByText(/SIMULATED: this message was not sent or recorded anywhere/)).toBeInTheDocument();
    expect(fake.calls.slice(before).filter((call) => /^conversations\./.test(call.method))).toEqual([]);
  });
});
