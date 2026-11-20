import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { createFixtureSources } from "../../../sources/fixtures";
import { populate } from "../../../sources/surfaces/testkit";
import { FakeDaemon } from "../../../test/fakeDaemon";
import { stubConversation, type ConversationStub } from "../../../test/m724Stubs";
import { createServices } from "../../createServices";
import type { DraftStorage } from "../../drafts";
import { ServicesProvider, type Services } from "../../services";
import { WorkspaceView } from "./WorkspaceView";

/** Live agent activity in the orchestrator chat (DPES-1044): the status line, thinking and tool rows of a running turn. */

const memoryStorage = (): DraftStorage => {
  const data = new Map<string, string>();
  return { getItem: (key) => data.get(key) ?? null, setItem: (key, value) => void data.set(key, value) };
};

const created: Services[] = [];
beforeEach(() => window.localStorage.clear());
afterEach(() => {
  for (const services of created.splice(0)) services.connection.stop();
  window.localStorage.clear();
});

/** Sends "go" to the Global orchestrator and leaves its turn running. */
async function runningTurn(): Promise<{ convo: ConversationStub; fake: FakeDaemon; user: ReturnType<typeof userEvent.setup> }> {
  const fake = populate(new FakeDaemon());
  const convo = stubConversation(fake, "running");
  const services = createServices({
    connector: fake,
    connection: { backoffMs: [5], requestTimeoutMs: 250 },
    data: { coalesceMs: 10 },
    sources: createFixtureSources(),
    draftStorage: memoryStorage(),
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
  await screen.findByRole("heading", { name: /alpha coordinator/ });
  await user.click(screen.getByRole("button", { name: /Choose conversation target/ }));
  await user.click(screen.getByRole("radio", { name: /Global orchestrator/ }));
  await user.type(await screen.findByRole("textbox", { name: "Message Global orchestrator" }), "go");
  await user.click(screen.getByRole("button", { name: "Send message to Global orchestrator" }));
  return { convo, fake, user };
}

const statusOf = (words: string | RegExp) => screen.getByText(words, { selector: "[role=status]" }).closest("p")!;
const POLL = { timeout: 3000 };

describe("orchestrator chat live activity", () => {
  it("shows the starting status, blocks the composer with it, and keeps Cancel reply", async () => {
    const { convo } = await runningTurn();
    convo.activity = { phase: "starting", tool: null, phase_ms: 3000, idle_ms: 0 };
    await waitFor(() => expect(statusOf("Starting")).toHaveTextContent(/^Starting · [34] s$/), POLL);
    expect(screen.queryByText(/^Waiting for the orchestrator.s reply…/)).toBeNull();
    expect(screen.getByText(/Sending is disabled\./)).toHaveTextContent("still working on your last message: Starting.");
    expect(screen.getByRole("button", { name: "Cancel reply" })).toBeInTheDocument();
  });

  it("shows thinking open with the Thinking status, then collapsed with a finished tool row once the turn ends", async () => {
    const { convo } = await runningTurn();
    convo.activity = { phase: "thinking", tool: null, phase_ms: 4000, idle_ms: 0 };
    act(() => convo.think("Weigh **options**."));
    const body = await screen.findByText("options", {}, POLL);
    const block = body.closest("details") as HTMLDetailsElement;
    expect(block.open).toBe(true);
    await waitFor(() => expect(statusOf("Thinking")).toHaveTextContent(/^Thinking · [45] s$/), POLL);

    act(() => {
      convo.toolStart("a", "Read", "src/main.rs");
      convo.activity = { phase: "tool", tool: { id: "a", name: "Read", summary: "src/main.rs" }, phase_ms: 1000, idle_ms: 0 };
    });
    const row = await screen.findByRole("button", { name: /Read/ }, POLL);
    expect(row).toHaveTextContent("Running: Read");
    expect(row).toHaveTextContent("src/main.rs");
    await waitFor(() => expect(statusOf("Running Read")).toHaveTextContent(/^Running Read · [12] s$/), POLL);
    // The thinking before a tool call is done.
    await waitFor(() => expect(block.open).toBe(false), POLL);

    act(() => {
      convo.toolFinish("a", true, "120 lines");
      convo.finishRunning(null, "All done.");
    });
    await screen.findByText("All done.", {}, POLL);
    expect(screen.getByRole("button", { name: /Read/ })).toHaveTextContent("Finished: Read");
    expect(screen.queryByText(/Running Read/)).toBeNull();
    expect(screen.queryByRole("button", { name: "Cancel reply" })).toBeNull();
  });

  it("shows No activity once the daemon reports the agent idle for 20 s", async () => {
    const { convo } = await runningTurn();
    convo.activity = { phase: "thinking", tool: null, phase_ms: 40_000, idle_ms: 25_000 };
    await waitFor(() => expect(statusOf("No activity")).toHaveTextContent(/^No activity for 2[567] s$/), POLL);
  });

  it("keeps a call in progress when reply text started before it finished, and stopped only when the turn is over", async () => {
    const { convo } = await runningTurn();
    act(() => {
      convo.toolStart("a", "Read", "src/main.rs");
      convo.reply("Let me check that file.");
    });
    await screen.findByText("Let me check that file.", {}, POLL);
    expect(screen.getByRole("button", { name: /Read/ })).toHaveTextContent("Running: Read");
    expect(screen.getByRole("button", { name: /Read/ })).not.toHaveTextContent("Stopped");

    act(() => convo.toolFinish("a", true, "ok"));
    await waitFor(() => expect(screen.getByRole("button", { name: /Read/ })).toHaveTextContent("Finished: Read"), POLL);
  });

  it("cancels through the daemon and shows a call that never finished as stopped", async () => {
    const { convo, fake, user } = await runningTurn();
    act(() => {
      convo.toolStart("a", "Bash", "make test");
      convo.activity = { phase: "tool", tool: { id: "a", name: "Bash", summary: "make test" }, phase_ms: 1000, idle_ms: 0 };
    });
    const row = await screen.findByRole("button", { name: /Bash/ }, POLL);
    expect(row).toHaveTextContent("Running: Bash");
    await user.click(screen.getByRole("button", { name: "Cancel reply" }));
    await waitFor(() => expect(fake.callsTo("conversations.cancel")).toHaveLength(1));
    await waitFor(() => expect(screen.getByRole("button", { name: /Bash/ })).toHaveTextContent("Stopped: Bash"), POLL);
    expect(screen.queryByRole("button", { name: "Cancel reply" })).toBeNull();
  });

  it("keeps today's sentence, with a wait time, and the thinking block when the daemon sends no activity", async () => {
    const { convo } = await runningTurn();
    await screen.findByText(/Waiting for the orchestrator.s reply…/);
    expect(statusOf(/Waiting for the orchestrator.s reply…/)).toHaveTextContent(/ · \d+ s$/);
    expect(screen.getByText(/Sending is disabled\./)).toHaveTextContent("still working on your last message. Wait");
    act(() => convo.think("Older daemon thought."));
    expect(await screen.findByText("Older daemon thought.", {}, POLL)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Read/ })).toBeNull();
  });
});
