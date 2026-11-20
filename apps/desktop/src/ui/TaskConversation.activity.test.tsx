import { screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { LiveActivityWire } from "../daemon/types";
import { FakeDaemon, StubRpcError, mkSession, mkTask } from "../test/fakeDaemon";
import { REPO, mount } from "../test/render";

/** Live agent activity in the task conversation (DPES-1044). */

const taskRoute = (taskId: string) => ({ name: "task", projectId: REPO, taskId }) as const;
const POLL = { timeout: 3000 };
type Row = { sequence: number; kind: string; body: string };

const started = (sequence: number, id: string, name: string, summary = ""): Row => ({ sequence, kind: "tool_started", body: JSON.stringify({ id, name, summary }) });
const finished = (sequence: number, id: string, ok: boolean, summary = ""): Row => ({ sequence, kind: "tool_finished", body: JSON.stringify({ id, ok, summary }) });

function daemon(state: "Running" | "Failed") {
  const fake = new FakeDaemon();
  fake.projects = [{ id: REPO }];
  fake.tasks = [mkTask("t-r", REPO, { state, sessions: [mkSession("s1", { state: state === "Running" ? "Running" : "Exited" })] })];
  return fake;
}

function stubActivity(fake: FakeDaemon, entries: Row[], live: LiveActivityWire | null) {
  fake.stub("sessions.activity", (params: Record<string, unknown>) => {
    const after = typeof params.after === "number" ? params.after : 0;
    const page = entries.filter((e) => e.sequence > after);
    return { entries: page, next_cursor: page.length > 0 ? page[page.length - 1]!.sequence : after, truncated: false, live };
  });
}

const thread = async () => within(await screen.findByRole("log", { name: /^Conversation with/ }));

describe("task conversation live activity", () => {
  it("shows thinking, a running tool row and the status line of a running session", async () => {
    const fake = daemon("Running");
    stubActivity(
      fake,
      [{ sequence: 1, kind: "thinking", body: "Reading the module first." }, started(2, "a", "Read", "src/lib.rs")],
      { phase: "tool", tool: { id: "a", name: "Read", summary: "src/lib.rs" }, phase_ms: 12_000, idle_ms: 300 },
    );
    await mount(fake, { route: taskRoute("t-r") });
    const log = await thread();
    expect(await log.findByText("Reading the module first.")).toBeInTheDocument();
    const row = await log.findByRole("button", { name: /Read/ });
    expect(row).toHaveTextContent("Running: Read");
    await waitFor(() => expect(screen.getByText("Running Read").closest("p")).toHaveTextContent(/^Running Read · 1[234] s$/), POLL);
  });

  it("shows No activity once the daemon reports the session idle for 20 s", async () => {
    const fake = daemon("Running");
    stubActivity(fake, [started(1, "a", "Bash", "make")], { phase: "tool", tool: { id: "a", name: "Bash", summary: "make" }, phase_ms: 60_000, idle_ms: 45_000 });
    await mount(fake, { route: taskRoute("t-r") });
    await waitFor(() => expect(screen.getByText("No activity").closest("p")).toHaveTextContent(/^No activity for 4[567] s$/), POLL);
  });

  it("keeps a finished session's rows as static history, a call that never finished stopped, and no status line", async () => {
    const fake = daemon("Failed");
    stubActivity(fake, [started(1, "a", "Read", "a.rs"), finished(2, "a", true, "10 lines"), started(3, "b", "Bash", "make")], null);
    await mount(fake, { route: taskRoute("t-r") });
    const log = await thread();
    expect(await log.findByRole("button", { name: /Read/ })).toHaveTextContent("Finished: Read");
    expect(log.getByRole("button", { name: /Bash/ })).toHaveTextContent("Stopped: Bash");
    expect(document.querySelector(".act-status")).toBeNull();
  });

  it("shows only the thinking text, with no status line, when the daemon has no sessions.activity", async () => {
    const fake = daemon("Running");
    fake.stub("sessions.activity", () => {
      throw new StubRpcError(-32601, "method unavailable: sessions.activity");
    });
    fake.stub("sessions.thinking", { chunks: [{ sequence: 1, body: "Older daemon thought." }], next_cursor: 1, truncated: false });
    await mount(fake, { route: taskRoute("t-r") });
    const log = await thread();
    expect(await log.findByText("Older daemon thought.")).toBeInTheDocument();
    expect(log.queryByRole("button", { name: /Read/ })).toBeNull();
    expect(document.querySelector(".act-status")).toBeNull();
  });
});
