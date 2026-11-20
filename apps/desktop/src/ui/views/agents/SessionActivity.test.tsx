import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { FakeDaemon, StubRpcError } from "../../../test/fakeDaemon";
import type { LiveActivityWire } from "../../../daemon/types";
import { createServices } from "../../createServices";
import { ServicesProvider, type Services } from "../../services";
import { SessionActivity } from "./SessionActivity";
import { ACTIVITY_POLL_MS } from "./useSessionActivity";

type Row = { sequence: number; kind: string; body: string };

function setup(running = true) {
  const fake = new FakeDaemon();
  const log = { entries: [] as Row[], truncated: false, live: null as LiveActivityWire | null };
  // Answers like the daemon: only entries after the cursor, and the cursor to continue from.
  fake.stub("sessions.activity", (params: Record<string, unknown>) => {
    const after = typeof params.after === "number" ? params.after : 0;
    const entries = log.entries.filter((e) => e.sequence > after);
    return { entries, next_cursor: entries.length > 0 ? entries[entries.length - 1]!.sequence : after, truncated: log.truncated, live: log.live };
  });
  const services: Services = createServices({ connector: fake, connection: { backoffMs: [5], requestTimeoutMs: 250 } });
  const ui = (isRunning: boolean) => (
    <ServicesProvider services={services}>
      <SessionActivity sessionId="s1" running={isRunning} />
    </ServicesProvider>
  );
  const view = render(ui(running));
  return { fake, log, services, view, ui };
}

const calls = (fake: FakeDaemon, method = "sessions.activity") => fake.calls.filter((c) => c.method === method);
const settle = (ms = 0) => act(() => vi.advanceTimersByTimeAsync(ms));
const start = async (services: Services) => {
  services.connection.start();
  await settle(50);
};
const think = (sequence: number, body: string): Row => ({ sequence, kind: "thinking", body });
const started = (sequence: number, id: string, name: string, summary = ""): Row => ({ sequence, kind: "tool_started", body: JSON.stringify({ id, name, summary }) });
const finished = (sequence: number, id: string, ok: boolean, summary = ""): Row => ({ sequence, kind: "tool_finished", body: JSON.stringify({ id, ok, summary }) });

describe("SessionActivity", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("says no activity was recorded when the log is empty", async () => {
    const { services } = setup();
    await start(services);
    expect(screen.getByText("No activity recorded")).toBeInTheDocument();
    expect(screen.queryByText("Thinking log hit its size limit.")).toBeNull();
  });

  it("notes when the thinking log hit its size limit, and still shows what was kept", async () => {
    const { log, services } = setup();
    log.entries = [think(1, "kept")];
    log.truncated = true;
    await start(services);
    expect(screen.getByText("kept")).toBeInTheDocument();
    expect(screen.getByText("Thinking log hit its size limit.")).toBeInTheDocument();
  });

  it("accumulates entries across 1 s polls, advancing the cursor and never refetching old entries", async () => {
    const { fake, log, services } = setup();
    log.entries = [think(1, "one "), think(2, "two ")];
    await start(services);
    expect(calls(fake)[0]!.params).toEqual({ session_id: "s1", after: 0 });
    // The first read drains: a second request returns the empty page after the cursor.
    expect(calls(fake)[1]!.params).toEqual({ session_id: "s1", after: 2 });
    expect(screen.getByText("one two")).toBeInTheDocument();

    log.entries.push(think(3, "three"));
    await settle(ACTIVITY_POLL_MS);
    expect(calls(fake).slice(2).map((c) => (c.params as { after: number }).after)).toEqual([2, 3]);
    expect(screen.getByText("one two three")).toBeInTheDocument();
  });

  it("shows tool rows in order with their state, and a finish joins its start in a later poll", async () => {
    const { log, services } = setup();
    log.entries = [think(1, "plan"), started(2, "a", "Read", "src/x.ts")];
    await start(services);
    const row = screen.getByRole("button", { name: /Read/ });
    expect(row).toHaveTextContent("Running: Read");
    expect(row).toHaveTextContent("src/x.ts");

    log.entries.push(finished(3, "a", true, "40 lines"));
    await settle(ACTIVITY_POLL_MS);
    expect(screen.getByRole("button", { name: /Read/ })).toHaveTextContent("Finished: Read");
    expect(screen.getAllByRole("button", { name: /Read/ })).toHaveLength(1);
  });

  it("expands a tool row by keyboard to its result preview", async () => {
    const { log, services } = setup();
    log.entries = [started(1, "a", "Bash", "make"), finished(2, "a", false, "exit 2")];
    await start(services);
    vi.useRealTimers();
    const user = userEvent.setup();
    const row = screen.getByRole("button", { name: /Bash/ });
    expect(row).toHaveAttribute("aria-expanded", "false");
    row.focus();
    await user.keyboard("{Enter}");
    expect(row).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByText("exit 2")).not.toHaveAttribute("hidden");
    await user.keyboard(" ");
    expect(row).toHaveAttribute("aria-expanded", "false");
  });

  it("shows a call that never finished as stopped once the session is not running", async () => {
    const { log, services, view, ui } = setup();
    log.entries = [started(1, "a", "Bash", "make")];
    await start(services);
    expect(screen.getByRole("button", { name: /Bash/ })).toHaveTextContent("Running: Bash");
    view.rerender(ui(false));
    await settle(0);
    expect(screen.getByRole("button", { name: /Bash/ })).toHaveTextContent("Stopped: Bash");
  });

  it("shows the live status and 'No activity' once the daemon reports it idle for 20 s", async () => {
    const { log, services } = setup();
    log.live = { phase: "tool", tool: { id: "a", name: "Read", summary: "" }, phase_ms: 12_000, idle_ms: 500 };
    await start(services);
    expect(screen.getByText("Running Read").closest("p")).toHaveTextContent("Running Read · 12 s");

    log.live = { phase: "thinking", tool: null, phase_ms: 30_000, idle_ms: 25_000 };
    await settle(ACTIVITY_POLL_MS);
    expect(screen.getByText("No activity").closest("p")).toHaveTextContent("No activity for 25 s");
  });

  it("drops the live status when the connection is lost", async () => {
    const { fake, log, services } = setup();
    log.live = { phase: "thinking", tool: null, phase_ms: 2000, idle_ms: 0 };
    await start(services);
    expect(screen.getByText("Thinking")).toBeInTheDocument();
    act(() => fake.drop());
    await settle(0);
    expect(screen.queryByText("Thinking")).toBeNull();
    expect(document.querySelector(".act-status")).toBeNull();
  });

  it("falls back to sessions.thinking when the daemon has no sessions.activity", async () => {
    const { fake, services } = setup();
    fake.stub("sessions.activity", () => {
      throw new StubRpcError(-32601, "method unavailable: sessions.activity");
    });
    fake.stub("sessions.thinking", (params: Record<string, unknown>) => {
      const after = typeof params.after === "number" ? params.after : 0;
      return after === 0 ? { chunks: [{ sequence: 1, body: "old daemon text" }], next_cursor: 1, truncated: false } : { chunks: [], next_cursor: after, truncated: false };
    });
    await start(services);
    expect(screen.getByText("old daemon text")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Read/ })).toBeNull();
    expect(document.querySelector(".act-status")).toBeNull();
    const asked = calls(fake, "sessions.thinking").length;
    await settle(ACTIVITY_POLL_MS * 2);
    expect(calls(fake, "sessions.thinking").length).toBeGreaterThan(asked);
    // Once it learned the method is missing it never asks for it again.
    expect(calls(fake)).toHaveLength(1);
  });

  it("reads once more when the session stops running, then stops polling", async () => {
    const { fake, log, services, view, ui } = setup();
    log.entries = [think(1, "early ")];
    await start(services);
    log.entries.push(think(2, "last"));
    view.rerender(ui(false));
    await settle(0);
    expect(screen.getByText("early last")).toBeInTheDocument();
    const seen = calls(fake).length;
    await settle(ACTIVITY_POLL_MS * 5);
    expect(calls(fake)).toHaveLength(seen);
  });

  it("stops polling on unmount", async () => {
    const { fake, services, view } = setup();
    await start(services);
    await settle(ACTIVITY_POLL_MS);
    const before = calls(fake).length;
    expect(before).toBeGreaterThanOrEqual(2);
    view.unmount();
    await settle(ACTIVITY_POLL_MS * 5);
    expect(calls(fake)).toHaveLength(before);
  });

  it("caps the rendered tool rows to the newest 200 and says how many earlier ones are left out", async () => {
    const { log, services } = setup();
    log.entries = Array.from({ length: 230 }, (_, i) => started(i + 1, `id${i}`, `Tool${i}`));
    await start(services);
    expect(screen.getByText("30 earlier tool calls")).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /Tool\d+/ })).toHaveLength(200);
    expect(screen.queryByRole("button", { name: /Tool29$/ })).toBeNull();
    expect(screen.getByRole("button", { name: /Tool30$/ })).toBeInTheDocument();
  });
});
