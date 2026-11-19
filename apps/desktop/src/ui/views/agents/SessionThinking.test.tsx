import { act, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { FakeDaemon } from "../../../test/fakeDaemon";
import { createServices } from "../../createServices";
import { ServicesProvider, type Services } from "../../services";
import { SessionThinking } from "./SessionThinking";
import { THINKING_POLL_MS } from "./useSessionThinking";

type Chunk = { sequence: number; body: string };

function setup(running = true) {
  const fake = new FakeDaemon();
  const log = { chunks: [] as Chunk[], truncated: false };
  // Answers like the daemon: only chunks after the cursor, and the cursor to continue from.
  fake.stub("sessions.thinking", (params: Record<string, unknown>) => {
    const after = typeof params.after === "number" ? params.after : 0;
    const chunks = log.chunks.filter((c) => c.sequence > after);
    return { chunks, next_cursor: chunks.length > 0 ? chunks[chunks.length - 1]!.sequence : after, truncated: log.truncated };
  });
  const services: Services = createServices({ connector: fake, connection: { backoffMs: [5], requestTimeoutMs: 250 } });
  const ui = (isRunning: boolean) => (
    <ServicesProvider services={services}>
      <SessionThinking sessionId="s1" running={isRunning} />
    </ServicesProvider>
  );
  const view = render(ui(running));
  return { fake, log, services, view, ui };
}

const calls = (fake: FakeDaemon) => fake.calls.filter((c) => c.method === "sessions.thinking");
const settle = (ms = 0) => act(() => vi.advanceTimersByTimeAsync(ms));
const start = async (services: Services) => {
  services.connection.start();
  await settle(50);
};

describe("SessionThinking", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("says no thinking was recorded when the log is empty", async () => {
    const { services } = setup();
    await start(services);
    expect(screen.getByText("No thinking recorded")).toBeInTheDocument();
    expect(screen.queryByText("Thinking log hit its size limit.")).toBeNull();
  });

  it("notes when the log hit its size limit, and still shows what was kept", async () => {
    const { log, services } = setup();
    log.chunks = [{ sequence: 1, body: "kept" }];
    log.truncated = true;
    await start(services);
    expect(screen.getByText("kept")).toBeInTheDocument();
    expect(screen.getByText("Thinking log hit its size limit.")).toBeInTheDocument();
  });

  it("accumulates chunks across polls, advancing the cursor and never refetching old chunks", async () => {
    const { fake, log, services } = setup();
    log.chunks = [
      { sequence: 1, body: "one " },
      { sequence: 2, body: "two " },
    ];
    await start(services);
    expect(calls(fake)[0]!.params).toEqual({ session_id: "s1", after: 0 });
    // The first read drains: a second request returns the empty page after the cursor.
    expect(calls(fake)[1]!.params).toEqual({ session_id: "s1", after: 2 });
    expect(screen.getByText("one two")).toBeInTheDocument();

    log.chunks.push({ sequence: 3, body: "three" });
    await settle(THINKING_POLL_MS);
    expect(calls(fake)[2]!.params).toEqual({ session_id: "s1", after: 2 });
    expect(screen.getByText("one two three")).toBeInTheDocument();
    expect(calls(fake).slice(2).map((c) => (c.params as { after: number }).after)).toEqual([2, 3]);
  });

  it("keeps the size-limit flag from a final page that adds no chunks", async () => {
    const { fake, log, services } = setup();
    log.chunks = [{ sequence: 1, body: "x" }];
    let first = true;
    fake.stub("sessions.thinking", () => {
      const page = first
        ? { chunks: log.chunks, next_cursor: 1, truncated: false }
        : { chunks: [], next_cursor: 1, truncated: true };
      first = false;
      return page;
    });
    await start(services);
    expect(screen.getByText("Thinking log hit its size limit.")).toBeInTheDocument();
  });

  it("shows all of a very long log with no cap", async () => {
    const { log, services } = setup();
    log.chunks = Array.from({ length: 5000 }, (_, i) => ({ sequence: i + 1, body: `w${i} ` }));
    await start(services);
    const text = screen.getByText(/^w0 w1 /).textContent!;
    expect(text.endsWith("w4999")).toBe(true);
    expect(text.split(" ")).toHaveLength(5000);
  });

  it("reads once more when the session stops running, then stops polling", async () => {
    const { fake, log, services, view, ui } = setup();
    log.chunks = [{ sequence: 1, body: "early " }];
    await start(services);
    log.chunks.push({ sequence: 2, body: "last" });
    view.rerender(ui(false));
    await settle(0);
    expect(screen.getByText("early last")).toBeInTheDocument();
    const seen = calls(fake).length;
    await settle(THINKING_POLL_MS * 5);
    expect(calls(fake)).toHaveLength(seen);
  });

  it("stops polling on unmount", async () => {
    const { fake, services, view } = setup();
    await start(services);
    await settle(THINKING_POLL_MS);
    const before = calls(fake).length;
    expect(before).toBeGreaterThanOrEqual(2);
    view.unmount();
    await settle(THINKING_POLL_MS * 5);
    expect(calls(fake)).toHaveLength(before);
  });
});
