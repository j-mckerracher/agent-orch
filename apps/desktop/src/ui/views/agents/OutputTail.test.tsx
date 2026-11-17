import { act, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { OutputLine } from "../../../daemon/types";
import { FakeDaemon, mkSession, mkTask } from "../../../test/fakeDaemon";
import { createServices } from "../../createServices";
import { ServicesProvider, type Services } from "../../services";
import { OutputTail } from "./OutputTail";
import { OUTPUT_POLL_MS, OUTPUT_UI_LINES } from "./useOutputTail";

const REPO = "/work/repo";
const line = (sequence: number, text: string, extra: Partial<OutputLine> = {}): OutputLine => ({
  sequence,
  stream: "stdout",
  text,
  line_truncated: false,
  ...extra,
});

function setup(running = true) {
  const fake = new FakeDaemon();
  fake.projects = [{ id: REPO }];
  fake.tasks = [mkTask("t1", REPO, { state: "Running", sessions: [mkSession("s1")] })];
  const services: Services = createServices({ connector: fake, connection: { backoffMs: [5], requestTimeoutMs: 250 } });
  const ui = (isRunning: boolean) => (
    <ServicesProvider services={services}>
      <OutputTail taskId="t1" sessionId="s1" running={isRunning} />
    </ServicesProvider>
  );
  const view = render(ui(running));
  return { fake, services, view, ui };
}

const tailCalls = (fake: FakeDaemon) => fake.calls.filter((c) => c.method === "sessions.output_tail");
const settle = (ms = 0) => act(() => vi.advanceTimersByTimeAsync(ms));
const start = async (services: Services) => {
  services.connection.start();
  await settle(50);
};

describe("OutputTail", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it("shows lines labeled stdout or stderr as plain text in a read-only panel", async () => {
    const { fake, services } = setup();
    fake.outputs.set("s1", { lines: [line(1, "building"), line(2, "<b>warn</b> **x**", { stream: "stderr" })] });
    await start(services);
    const panel = screen.getByRole("region", { name: "Recent output" });
    const log = within(panel).getByRole("log");
    expect(within(log).getByText("stdout")).toBeInTheDocument();
    expect(within(log).getByText("stderr")).toBeInTheDocument();
    expect(within(log).getByText("building")).toBeInTheDocument();
    expect(within(log).getByText("<b>warn</b> **x**")).toBeInTheDocument();
    expect(log.querySelector("b")).toBeNull();
    expect(panel).toHaveTextContent(/may contain secrets/);
    expect(within(panel).queryByRole("button")).toBeNull();
    expect(within(panel).queryByRole("textbox")).toBeNull();
  });

  it("marks truncated lines and dropped older lines", async () => {
    const { fake, services } = setup();
    fake.outputs.set("s1", { lines: [line(5, "long", { line_truncated: true })], lastSequence: 5 });
    await start(services);
    expect(screen.getByText("[line truncated]")).toBeInTheDocument();
    expect(screen.getByText("Older lines were dropped and are not shown.")).toBeInTheDocument();
  });

  it("explains available:false", async () => {
    const { services } = setup();
    await start(services);
    expect(screen.getByText(/No output is available for this session/)).toBeInTheDocument();
    expect(screen.getByText(/daemon restarted/)).toBeInTheDocument();
    expect(screen.queryByRole("log")).toBeNull();
  });

  it("explains ended:true, with and without output", async () => {
    const first = setup(false);
    first.fake.outputs.set("s1", { lines: [], ended: true });
    await start(first.services);
    expect(screen.getByText("The session ended without producing output.")).toBeInTheDocument();
    first.view.unmount();

    const second = setup(false);
    second.fake.outputs.set("s1", { lines: [line(1, "done")], ended: true });
    await start(second.services);
    expect(screen.getByText("done")).toBeInTheDocument();
    expect(screen.getByText("The session ended; this is its final retained output.")).toBeInTheDocument();
  });

  it("polls with after_sequence, appends new lines and never refetches the old ones", async () => {
    const { fake, services } = setup();
    fake.outputs.set("s1", { lines: [line(1, "one"), line(2, "two")] });
    await start(services);
    expect(tailCalls(fake)[0]!.params).not.toHaveProperty("after_sequence");
    fake.outputs.set("s1", { lines: [line(1, "one"), line(2, "two"), line(3, "three")] });
    await settle(OUTPUT_POLL_MS);
    expect(tailCalls(fake)).toHaveLength(2);
    expect(tailCalls(fake)[1]!.params).toMatchObject({ task_id: "t1", session_id: "s1", after_sequence: 2 });
    expect(screen.getAllByText(/^(one|two|three)$/).map((n) => n.textContent)).toEqual(["one", "two", "three"]);
    await settle(OUTPUT_POLL_MS);
    expect(tailCalls(fake)[2]!.params).toMatchObject({ after_sequence: 3 });
  });

  it("stops polling on unmount", async () => {
    const { fake, services, view } = setup();
    await start(services);
    await settle(OUTPUT_POLL_MS);
    const before = tailCalls(fake).length;
    expect(before).toBeGreaterThanOrEqual(2);
    view.unmount();
    await settle(OUTPUT_POLL_MS * 5);
    expect(tailCalls(fake)).toHaveLength(before);
  });

  it("stops polling once the daemon reports the session ended", async () => {
    const { fake, services } = setup();
    fake.outputs.set("s1", { lines: [line(1, "one")] });
    await start(services);
    fake.outputs.set("s1", { lines: [line(1, "one"), line(2, "bye")], ended: true });
    await settle(OUTPUT_POLL_MS);
    const seen = tailCalls(fake).length;
    expect(screen.getByText("bye")).toBeInTheDocument();
    await settle(OUTPUT_POLL_MS * 5);
    expect(tailCalls(fake)).toHaveLength(seen);
  });

  it("stops polling when the session is no longer running, after one last read", async () => {
    const { fake, services, view, ui } = setup();
    fake.outputs.set("s1", { lines: [line(1, "one")] });
    await start(services);
    view.rerender(ui(false));
    await settle(0);
    const seen = tailCalls(fake).length;
    await settle(OUTPUT_POLL_MS * 5);
    expect(tailCalls(fake)).toHaveLength(seen);
  });

  it("pauses while the page is hidden and resumes when it is visible", async () => {
    const { fake, services } = setup();
    await start(services);
    const visibility = vi.spyOn(document, "visibilityState", "get").mockReturnValue("hidden");
    await settle(OUTPUT_POLL_MS);
    const seen = tailCalls(fake).length;
    await settle(OUTPUT_POLL_MS * 3);
    expect(tailCalls(fake)).toHaveLength(seen);
    visibility.mockReturnValue("visible");
    await act(async () => {
      document.dispatchEvent(new Event("visibilitychange"));
    });
    await settle(0);
    expect(tailCalls(fake).length).toBeGreaterThan(seen);
  });

  it("shows a non-fatal error, keeps the lines, and recovers on the next poll", async () => {
    const { fake, services } = setup();
    fake.outputs.set("s1", { lines: [line(1, "kept")] });
    await start(services);
    const tasks = fake.tasks;
    fake.tasks = [];
    await settle(OUTPUT_POLL_MS);
    expect(screen.getByRole("alert")).toHaveTextContent(/Refreshing recent output failed.*not-found/);
    expect(screen.getByText("kept")).toBeInTheDocument();
    const failed = tailCalls(fake).length;
    fake.tasks = tasks;
    fake.outputs.set("s1", { lines: [line(1, "kept"), line(2, "after")] });
    await settle(OUTPUT_POLL_MS);
    expect(tailCalls(fake).length).toBeGreaterThan(failed);
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.getByText("after")).toBeInTheDocument();
  });

  it(`keeps at most ${OUTPUT_UI_LINES} lines and says older ones were dropped`, async () => {
    const { fake, services } = setup();
    const batch = (from: number, to: number) => Array.from({ length: to - from + 1 }, (_, i) => line(from + i, `row-${from + i}`));
    fake.outputs.set("s1", { lines: batch(1, 100) });
    await start(services);
    for (const to of [200, 300]) {
      fake.outputs.set("s1", { lines: batch(1, to) });
      await settle(OUTPUT_POLL_MS);
    }
    const log = screen.getByRole("log");
    const rows = within(log).getAllByText(/^row-\d+$/);
    expect(rows).toHaveLength(OUTPUT_UI_LINES);
    expect(rows[0]).toHaveTextContent("row-101");
    expect(rows[rows.length - 1]).toHaveTextContent("row-300");
    expect(screen.getByText("Older lines were dropped and are not shown.")).toBeInTheDocument();
  });
});
