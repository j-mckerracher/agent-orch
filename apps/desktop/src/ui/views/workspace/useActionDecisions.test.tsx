import { act, renderHook } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { DaemonError } from "../../../daemon/errors";
import type { ActionProposal, ConversationSource, ResolveReceipt } from "../../../sources/surfaces/types";
import { FakeDaemon, until } from "../../../test/fakeDaemon";
import { createServices } from "../../createServices";
import { ServicesProvider } from "../../services";
import { useActionDecisions } from "./useActionDecisions";

const pending: ActionProposal = {
  id: "proposal-1",
  turnId: "turn-1",
  index: 0,
  kind: "task.propose",
  params: { project: "p-1", objective: "Add unit tests", agent: "codex" },
  digest: "d".repeat(64),
  dependsOn: null,
  state: "pending",
  resultRef: null,
  errorCode: null,
  denial: null,
  target: null,
  updatedAt: "2026-11-10T21:00:00Z",
};

const answer = (proposal: ActionProposal): ResolveReceipt => ({
  simulated: false,
  provenance: { kind: "daemon", id: "D10" },
  notice: null,
  proposal,
});

const stops: Array<() => void> = [];
afterEach(() => {
  while (stops.length) stops.pop()!();
});

async function connectedHook(resolve: ConversationSource["resolve"]) {
  const fake = new FakeDaemon();
  fake.stub("actions.pending", { proposals: [], more: false });
  const services = createServices({ connector: fake, connection: { backoffMs: [5], requestTimeoutMs: 250 }, data: { coalesceMs: 10 } });
  stops.push(() => {
    services.connection.stop();
    services.data.dispose();
  });
  services.connection.start();
  // The connect itself reads the pending list; a decision must read it again.
  await until(() => fake.callsTo("actions.pending").length >= 1, "first actions.pending");
  const before = fake.callsTo("actions.pending").length;
  const wrapper = ({ children }: { children: ReactNode }) => <ServicesProvider services={services}>{children}</ServicesProvider>;
  const reload = vi.fn();
  const hook = renderHook(() => useActionDecisions({ scope: "global", resolve, announce: vi.fn(), reload }), { wrapper });
  return { fake, hook, reload, before };
}

describe("useActionDecisions pending list", () => {
  it("a_decision_that_the_daemon_executed_reads_the_pending_proposals_again", async () => {
    const { fake, hook, reload, before } = await connectedHook(() => Promise.resolve(answer({ ...pending, state: "executed", resultRef: "task-1" })));
    await act(async () => hook.result.current.decide(pending, "approve", null, new Map()));
    expect(reload).toHaveBeenCalledTimes(1);
    await until(() => fake.callsTo("actions.pending").length > before, "another actions.pending");
  });

  it("a_decision_that_failed_reads_the_pending_proposals_again_because_the_outcome_is_unknown", async () => {
    const { fake, hook, before } = await connectedHook(() => Promise.reject(new DaemonError("timeout", "actions.resolve timed out")));
    await act(async () => hook.result.current.decide(pending, "reject", null, new Map()));
    expect(hook.result.current.errors.get(pending.id)?.kind).toBe("timeout");
    await until(() => fake.callsTo("actions.pending").length > before, "another actions.pending");
  });
});
