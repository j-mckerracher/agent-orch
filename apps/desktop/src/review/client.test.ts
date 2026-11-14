import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { ConnectionManager } from "../daemon/connection";
import { FakeDaemon, until } from "../test/fakeDaemon";
import type { ResolveDecision } from "../daemon/types";
import { createReviewClient } from "./client";
import { COMMIT, DIFF, approval, evidenceSource, evidenceFor, taskRecord } from "./testing";
import { useReview } from "./useReview";

const stops: Array<() => void> = [];
afterEach(() => { while (stops.length) stops.pop()!(); });
async function review(commit: string) {
  const fake = new FakeDaemon();
  const a = approval({ commit });
  fake.approvals = [a];
  fake.tasks = [taskRecord()];
  fake.stub("workspaces.diff", { task_id: "task-1", workspace_id: "ws-1", diff: DIFF });
  fake.stub("approvals.resolve", (params: Record<string, unknown>) => {
    const next = { ...a, state: params.decision === "approve"
      ? { state: "approved" as const, by: { kind: "user" as const, id: "u" } }
      : { state: "rejected" as const } };
    fake.approvals = [next];
    return next;
  });
  const connection = new ConnectionManager({ connector: fake, backoffMs: [5], requestTimeoutMs: 100 });
  stops.push(() => connection.stop());
  connection.start();
  await until(() => connection.getSnapshot().status === "connected", "connected");
  const options = {
    client: createReviewClient(connection),
    evidenceSource: evidenceSource("daemon", (selected) => ({ state: "available", evidence: evidenceFor(selected) }), [a]),
    scope: { taskId: "task-1" },
  };
  const hook = renderHook(() => useReview(options));
  await waitFor(() => {
    expect(hook.result.current.loading).toBe(false);
    expect(hook.result.current.approval?.id).toBe(a.id);
  });
  return { fake, ...hook };
}

describe("A1b review candidate context before real client transport", () => {
  it.each(["approve", "reject"] as const)("refuses %s for a real approval bound to a fixture-like candidate", async (decision: ResolveDecision) => {
    for (const commit of ["fixture:commit", "fixture0001", "  FIXTURE0001  "]) {
      const { fake, result, unmount } = await review(commit);
      expect(result.current.acceptBlockers.join(" ")).toMatch(/PLACEHOLDER DATA/);
      expect(result.current.rejectBlockers.join(" ")).toMatch(/PLACEHOLDER DATA/);
      const before = fake.calls.length;
      await act(async () => { await result.current.decide(decision); });
      expect(fake.calls.length).toBe(before);
      expect(fake.callsTo("approvals.resolve")).toEqual([]);
      unmount();
    }
  });

  it.each(["approve", "reject"] as const)("sends %s for a real approval and candidate", async (decision) => {
    const { fake, result } = await review(COMMIT);
    expect(result.current.acceptBlockers).toEqual([]);
    expect(result.current.rejectBlockers).toEqual([]);
    await act(async () => { await result.current.decide(decision); });
    expect(fake.callsTo("approvals.resolve")).toHaveLength(1);
    expect(fake.callsTo("approvals.resolve")[0]?.params).toMatchObject({ approval_id: "approval-1", decision });
  });
});
