import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ALL_CAPABILITIES } from "../../../daemon/types";
import { createServices } from "../../createServices";
import { populate } from "../../../sources/surfaces/testkit";
import { createFixtureSources } from "../../../sources/fixtures";
import { createFixtureSurfaces } from "../../../sources/surfaces/fixtures";
import { FakeDaemon } from "../../../test/fakeDaemon";
import { stubConversation } from "../../../test/m724Stubs";
import type { DraftStorage } from "../../drafts";
import { ServicesProvider, type Services } from "../../services";
import { WorkspaceView } from "./WorkspaceView";
import { OLDER_DAEMON_NOTICE, RESTRICTED_PREFERENCE_KEY } from "./permissionLevel";

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
});

beforeEach(() => {
  window.localStorage.clear();
});

afterEach(() => {
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

/** Opens the model picker, where the level is chosen, and returns the level control. */
async function levelSelect(user: ReturnType<typeof userEvent.setup>) {
  const button = screen.getByRole("button", { name: /^Model settings/ });
  if (button.getAttribute("aria-expanded") !== "true") await user.click(button);
  return screen.getByRole("combobox", { name: "Permission level" });
}

/** A Global conversation whose daemon reports `daemonLevel` for the turn, and nothing before the first send. */
function stubReportedLevel(fake: FakeDaemon, daemonLevel: string | null) {
  let sent = false;
  const turn = {
    id: "turn-1",
    state: "completed",
    error_code: null,
    runtime: { name: "stub", version: "1" },
    reply_bytes: 0,
    created_at: "2026-11-15T12:00:00Z",
    updated_at: "2026-11-15T12:00:01Z",
    permission_level: daemonLevel,
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

/** A Global conversation whose latest turn is an automatic update the user did not send, at `daemonLevel`. */
function stubLatestWakeTurn(fake: FakeDaemon, daemonLevel: string) {
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
      permission_level: daemonLevel,
      trigger: "wake",
    },
    epoch: "epoch-1",
  }));
}

describe("permission level in the chat composer, live daemon", () => {
  it("sends Auto with each message by default, on a daemon that advertises permission_levels", async () => {
    const fake = liveDaemon();
    stubConversation(fake, "completed");
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await pickGlobal(user);
    expect(await levelSelect(user)).toHaveValue("auto");
    await sendFromGlobal(user, "hello");
    await waitFor(() => expect(fake.callsTo("conversations.send")).toHaveLength(1));
    expect(fake.callsTo("conversations.send")[0]!.params).toMatchObject({ text: "hello", permission: "auto" });
  });

  it("sends the level the user chose, and Restricted goes as restricted", async () => {
    const fake = liveDaemon();
    stubConversation(fake, "completed");
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await pickGlobal(user);
    await user.selectOptions(await levelSelect(user), "Restricted");
    await sendFromGlobal(user, "stay read-only");
    await waitFor(() => expect(fake.callsTo("conversations.send")).toHaveLength(1));
    expect(fake.callsTo("conversations.send")[0]!.params).toMatchObject({ text: "stay read-only", permission: "restricted" });
  });

  it("sends nothing at Full until the user confirms it, then sends full", async () => {
    const fake = liveDaemon();
    stubConversation(fake, "completed");
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await pickGlobal(user);
    await user.selectOptions(await levelSelect(user), "Full");
    await user.click(await screen.findByRole("button", { name: "Use Full" }));
    expect(await levelSelect(user)).toHaveValue("full");
    expect(screen.getByRole("button", { name: /^Model settings: .* · Full$/ })).toBeInTheDocument();
    await sendFromGlobal(user, "run it");
    await waitFor(() => expect(fake.callsTo("conversations.send")).toHaveLength(1));
    expect(fake.callsTo("conversations.send")[0]!.params).toMatchObject({ text: "run it", permission: "full" });
  });

  it("an older daemon gets no permission field, shows Restricted with the notice, and cannot choose Auto", async () => {
    const fake = liveDaemon();
    fake.capabilitiesOverride = ALL_CAPABILITIES.filter((capability) => capability !== "permission_levels");
    stubConversation(fake, "completed");
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await pickGlobal(user);
    expect(await levelSelect(user)).toHaveValue("restricted");
    expect(screen.getByText(OLDER_DAEMON_NOTICE)).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "Auto (needs an updated daemon)" })).toBeDisabled();
    await sendFromGlobal(user, "old daemon");
    await waitFor(() => expect(fake.callsTo("conversations.send")).toHaveLength(1));
    expect(fake.callsTo("conversations.send")[0]!.params).not.toHaveProperty("permission");
  });

  it("shows the level in the model picker chip, and no indicator or one-click switch below the composer", async () => {
    const fake = liveDaemon();
    stubConversation(fake, "completed");
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await pickGlobal(user);
    expect(screen.getByRole("button", { name: /^Model settings: .* · Auto$/ })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Switch to Restricted" })).toBeNull();
    expect(screen.queryByText(/What automatic review does/)).toBeNull();
    expect((await levelSelect(user)).closest(".ws-model-panel")).not.toBeNull();
  });

  it("warns when the daemon reports a different level from the one sent", async () => {
    const fake = liveDaemon();
    stubReportedLevel(fake, "restricted");
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await pickGlobal(user);
    await sendFromGlobal(user, "hello");
    expect(await screen.findByRole("alert")).toHaveTextContent("ran at Restricted, not Auto.");
  });

  it("shows the daemon's level without a warning when it matches the level sent", async () => {
    const fake = liveDaemon();
    stubReportedLevel(fake, "auto");
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await pickGlobal(user);
    await sendFromGlobal(user, "hello");
    expect(await screen.findByText("The daemon reports the last reply ran at Auto.")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("describes a last automatic update in its own words, with no mismatch alert", async () => {
    const fake = liveDaemon();
    stubLatestWakeTurn(fake, "restricted");
    const services = track(build({ fake }));
    const { user } = await mountView(services, false);
    await waitLive(services);
    await pickGlobal(user);
    expect(await screen.findByText("The last automatic update ran at Restricted, as automatic updates always do.")).toBeInTheDocument();
    expect(screen.queryByText(/last reply ran at/)).toBeNull();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("saves only the Restricted preference, never Auto, Extended or Full", async () => {
    const fake = liveDaemon();
    stubConversation(fake, "completed");
    const setItem = vi.spyOn(Storage.prototype, "setItem");
    const removeItem = vi.spyOn(Storage.prototype, "removeItem");
    try {
      const services = track(build({ fake }));
      const { user } = await mountView(services, false);
      await waitLive(services);
      await pickGlobal(user);
      await user.selectOptions(await levelSelect(user), "Restricted");
      expect(window.localStorage.getItem(RESTRICTED_PREFERENCE_KEY)).toBe("restricted");
      await user.selectOptions(await levelSelect(user), "Auto");
      expect(window.localStorage.getItem(RESTRICTED_PREFERENCE_KEY)).toBeNull();
      await user.selectOptions(await levelSelect(user), "Full");
      await user.click(await screen.findByRole("button", { name: "Use Full" }));
      expect(window.localStorage.getItem(RESTRICTED_PREFERENCE_KEY)).toBeNull();
      expect(setItem.mock.calls).toEqual([[RESTRICTED_PREFERENCE_KEY, "restricted"]]);
      expect(removeItem.mock.calls.every(([key]) => key === RESTRICTED_PREFERENCE_KEY)).toBe(true);
    } finally {
      setItem.mockRestore();
      removeItem.mockRestore();
    }
  });

  it("restores a saved Restricted after a reload, and never restores Full", async () => {
    const fake = liveDaemon();
    stubConversation(fake, "completed");
    window.localStorage.setItem(RESTRICTED_PREFERENCE_KEY, "restricted");
    const firstServices = track(build({ fake }));
    const first = await mountView(firstServices, false);
    await waitLive(firstServices);
    await pickGlobal(first.user);
    expect(await levelSelect(first.user)).toHaveValue("restricted");
    first.view.unmount();

    window.localStorage.clear();
    const secondServices = track(build({ fake }));
    const second = await mountView(secondServices, false);
    await waitLive(secondServices);
    await pickGlobal(second.user);
    await second.user.selectOptions(await levelSelect(second.user), "Full");
    await second.user.click(await screen.findByRole("button", { name: "Use Full" }));
    second.view.unmount();

    const thirdServices = track(build({ fake }));
    const third = await mountView(thirdServices, false);
    await waitLive(thirdServices);
    await pickGlobal(third.user);
    expect(await levelSelect(third.user)).toHaveValue("auto");
    third.view.unmount();
  });
});

describe("permission level in the chat composer, fixture mode", () => {
  it("sends nothing to a connected daemon at any level, and says the message was simulated", async () => {
    const fake = liveDaemon();
    const services = track(build({ fake, fixtures: true }));
    const { user } = await mountView(services);
    const before = fake.calls.length;
    await user.selectOptions(await levelSelect(user), "Restricted");
    await sendFromGlobal(user, "first");
    expect(await screen.findByText(/SIMULATED: this message was not sent or recorded anywhere/)).toBeInTheDocument();
    await user.selectOptions(await levelSelect(user), "Auto");
    await user.selectOptions(await levelSelect(user), "Full");
    await user.click(await screen.findByRole("button", { name: "Use Full" }));
    await sendFromGlobal(user, "second");
    await waitFor(() => expect(screen.getAllByText(/SIMULATED/).length).toBeGreaterThan(1));
    expect(fake.calls.slice(before).filter((call) => /^conversations\./.test(call.method))).toEqual([]);
    expect(screen.queryByText(OLDER_DAEMON_NOTICE)).toBeNull();
  });
});
