import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import type { ConversationWireEntry, ConversationWireTurn, MonitoringWire, WakeRecordWire } from "../../../daemon/types";
import { createFixtureSources } from "../../../sources/fixtures";
import { populate } from "../../../sources/surfaces/testkit";
import { FakeDaemon } from "../../../test/fakeDaemon";
import { stubConversation } from "../../../test/m724Stubs";
import { createServices } from "../../createServices";
import { ServicesProvider, type Services } from "../../services";
import { WorkspaceView } from "./WorkspaceView";

const AT = "2026-11-16T10:00:00Z";
const created: Services[] = [];
afterEach(() => {
  cleanup();
  for (const services of created.splice(0)) services.connection.stop();
});

const entry = (sequence: number, turn: string, kind: ConversationWireEntry["kind"], author: ConversationWireEntry["author"], body: string, origin?: string): ConversationWireEntry => ({
  sequence,
  id: `${turn}:${kind}:${sequence}`,
  turn_id: turn,
  kind,
  author,
  body,
  error_code: null,
  created_at: AT,
  ...(origin ? { origin } : {}),
});

const turn = (id: string, trigger: string): ConversationWireTurn => ({
  id,
  state: "running",
  error_code: null,
  runtime: { name: "codex", version: "1" },
  reply_bytes: 0,
  created_at: AT,
  updated_at: AT,
  trigger,
});

const wakeRecord = (id: string, extra: Partial<WakeRecordWire> = {}): WakeRecordWire => ({
  id,
  event: "task_failed",
  subject: "t-1",
  notice: `Task ${id} failed.`,
  state: "handled",
  outcome: "replied",
  skip_reason: null,
  turn_id: null,
  anchor_sequence: 1,
  notify: true,
  seen: false,
  created_at: AT,
  ...extra,
});

interface Scenario {
  entries: ConversationWireEntry[];
  turn: ConversationWireTurn | null;
  wake: WakeRecordWire[];
  monitoring: MonitoringWire;
}

const MONITORING: MonitoringWire = { authority: "none", mode: "on", paused: [], unseen: [], epoch: "e1" };

/** A live daemon whose Global conversation and monitoring answers come from `scenario`, which a test may change. */
function daemon(scenario: Partial<Scenario> = {}) {
  const state: Scenario = { entries: [], turn: null, wake: [], monitoring: MONITORING, ...scenario };
  const fake = populate(new FakeDaemon());
  stubConversation(fake, "completed");
  fake.stub("conversations.get", () => ({
    authority: "none",
    target: { kind: "global" },
    entries: state.entries,
    more: false,
    next_after_sequence: state.entries.at(-1)?.sequence ?? 0,
    latest_sequence: state.entries.at(-1)?.sequence ?? 0,
    clear_generation: 0,
    turn: state.turn,
    epoch: "epoch-1",
    wake_records: state.wake,
    wake_records_more: false,
  }));
  fake.stub("monitoring.get", () => state.monitoring);
  fake.stub("monitoring.ack", { authority: "none", acked: 1 });
  return { fake, state };
}

async function mount(fake: FakeDaemon, target: RegExp | null = /Global orchestrator/) {
  const services = createServices({
    connector: fake,
    connection: { backoffMs: [5], requestTimeoutMs: 250 },
    data: { coalesceMs: 10 },
    sources: createFixtureSources(),
  });
  created.push(services);
  const user = userEvent.setup();
  render(
    <ServicesProvider services={services}>
      <WorkspaceView />
    </ServicesProvider>,
  );
  services.connection.start();
  await waitFor(() => expect(services.connection.getSnapshot().status).toBe("connected"));
  if (target) {
    const button = await screen.findByRole("button", { name: /Choose conversation target/ });
    await user.click(button);
    await user.click(screen.getByRole("radio", { name: target }));
  }
  return user;
}

const composer = () => screen.findByRole("textbox", { name: "Message Global orchestrator" });
const send = () => screen.getByRole("button", { name: "Send message to Global orchestrator" });

describe("Workspace monitoring", () => {
  it("workspace_monitoring_composer_is_not_blocked_while_a_wake_turn_runs", async () => {
    const { fake } = daemon({ turn: turn("w1", "wake"), entries: [entry(1, "w1", "user_message", "user", "Task t-1 failed.", "wake")] });
    const user = await mount(fake);
    await user.type(await composer(), "hello");
    expect(send()).toBeEnabled();
    expect(screen.getByText("The orchestrator is checking an update. Sending cancels it.")).toBeInTheDocument();
    expect(screen.queryByText(/Sending is disabled/)).not.toBeInTheDocument();
  });

  it("workspace_monitoring_composer_stays_blocked_while_a_user_turn_runs", async () => {
    const { fake } = daemon({ turn: turn("u1", "user"), entries: [entry(1, "u1", "user_message", "user", "hi")] });
    const user = await mount(fake);
    await user.type(await composer(), "hello");
    expect(send()).toBeDisabled();
    expect(screen.getByText(/Sending is disabled\. The orchestrator is still working on your last message/)).toBeInTheDocument();
  });

  it("workspace_monitoring_the_wake_opening_entry_is_event_lines_not_a_you_bubble", async () => {
    const { fake } = daemon({
      entries: [entry(1, "w1", "user_message", "user", "Task t-9 failed.", "wake"), entry(2, "w1", "reply_chunk", "orchestrator", "Retry it."), entry(3, "w1", "turn_completed", "system", "")],
    });
    await mount(fake);
    expect(await screen.findByText("Task t-9 failed.")).toBeInTheDocument();
    expect(screen.queryByText("You")).not.toBeInTheDocument();
    expect(screen.getAllByText("Update, not a reply to you").length).toBeGreaterThanOrEqual(2);
    expect(screen.getByText("Retry it.")).toBeInTheDocument();
  });

  it("workspace_monitoring_global_option_shows_the_badge_text_and_the_accessible_name", async () => {
    const { fake } = daemon({ monitoring: { ...MONITORING, unseen: [{ target: { kind: "global" }, count: 2 }] } });
    const user = await mount(fake, null);
    await waitFor(() => expect(fake.callsTo("monitoring.get").length).toBeGreaterThan(0));
    await user.click(await screen.findByRole("button", { name: /Choose conversation target/ }));
    const option = await screen.findByRole("radio", { name: "Global orchestrator, 2 new updates" });
    expect(option.closest("label")).toHaveTextContent("2 updates");
  });

  it("workspace_monitoring_a_new_notifying_record_announces_once_and_not_on_first_load", async () => {
    const { fake, state } = daemon({ wake: [wakeRecord("wake-1")] });
    const user = await mount(fake);
    await screen.findByText("Task wake-1 failed.");
    expect(screen.queryByText("New update from the orchestrator")).not.toBeInTheDocument();
    state.wake = [...state.wake, wakeRecord("wake-2")];
    await user.type(await composer(), "hello");
    await user.click(send());
    expect(await screen.findByText("New update from the orchestrator")).toBeInTheDocument();
    expect(screen.getAllByText("New update from the orchestrator")).toHaveLength(1);
  });

  it("workspace_monitoring_acks_once_when_the_conversation_is_visible_with_unseen_updates", async () => {
    const { fake } = daemon({ wake: [wakeRecord("wake-1")], monitoring: { ...MONITORING, unseen: [{ target: { kind: "global" }, count: 1 }] } });
    await mount(fake);
    await screen.findByText("Task wake-1 failed.");
    await waitFor(() => expect(fake.callsTo("monitoring.ack")).toHaveLength(1), { timeout: 3000 });
    expect(fake.callsTo("monitoring.ack")[0]!.params).toMatchObject({ target: { kind: "global" } });
  });

  it("workspace_monitoring_shows_a_chip_for_global_and_none_for_a_project_coordinator", async () => {
    const { fake } = daemon();
    const user = await mount(fake, null);
    await screen.findByRole("textbox", { name: /Message / });
    await waitFor(() => expect(fake.callsTo("monitoring.get").length).toBeGreaterThan(0));
    expect(screen.queryByRole("button", { name: /Monitoring:/ })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Choose conversation target/ }));
    await user.click(screen.getByRole("radio", { name: /Global orchestrator/ }));
    expect(await screen.findByRole("button", { name: "Monitoring: on" })).toBeInTheDocument();
  });
});
