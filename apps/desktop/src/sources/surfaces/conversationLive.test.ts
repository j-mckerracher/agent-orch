import { afterEach, describe, expect, it } from "vitest";
import { FakeDaemon } from "../../test/fakeDaemon";
import { stubConversation } from "../../test/m724Stubs";
import type { ConversationWireEntry } from "../../daemon/types";
import { mapEntries } from "./conversationLive";
import { connect, populate } from "./testkit";

const stops: Array<() => void> = [];
afterEach(() => {
  while (stops.length) stops.pop()!();
});

async function live(fake: FakeDaemon) {
  const connected = await connect(fake);
  stops.push(connected.stop);
  return connected.surfaces;
}

const wire = (
  sequence: number,
  turn: string,
  kind: ConversationWireEntry["kind"],
  body = "",
  code: string | null = null,
): ConversationWireEntry => ({
  sequence,
  id: `${turn}:${kind}:${sequence}`,
  turn_id: turn,
  kind,
  author: kind === "user_message" ? "user" : kind === "reply_chunk" ? "orchestrator" : "system",
  body,
  error_code: code,
  created_at: "2026-11-10T21:00:00Z",
});

describe("mapEntries", () => {
  it("coalesces reply chunks into one reply that stops streaming at the turn end", () => {
    const entries = mapEntries([
      wire(1, "t1", "user_message", "hi"),
      wire(2, "t1", "reply_chunk", "Hel"),
      wire(3, "t1", "reply_chunk", "lo"),
    ]);
    expect(entries.map((e) => [e.kind, e.role, e.text, e.streaming])).toEqual([
      ["message", "user", "hi", undefined],
      ["message", "coordinator", "Hello", true],
    ]);
    const done = mapEntries([
      wire(1, "t1", "user_message", "hi"),
      wire(2, "t1", "reply_chunk", "Hello"),
      wire(3, "t1", "turn_completed"),
    ]);
    expect(done).toHaveLength(2);
    expect(done[1]).toMatchObject({ text: "Hello", streaming: false });
  });

  it("turns a runtime_unavailable failure into a system status, never reply text", () => {
    const entries = mapEntries([wire(1, "t1", "user_message", "hi"), wire(2, "t1", "turn_failed", "", "runtime_unavailable")]);
    expect(entries[1]).toMatchObject({ kind: "status", role: "system", errorCode: "runtime_unavailable" });
    expect(entries[1]!.text).toMatch(/Orchestrator runtime unavailable/);
    expect(entries.some((e) => e.role === "coordinator")).toBe(false);
  });

  it("marks a completed turn without a reply and a cancelled turn as statuses", () => {
    const entries = mapEntries([wire(1, "t1", "turn_completed"), wire(2, "t2", "turn_cancelled")]);
    expect(entries.map((e) => e.kind)).toEqual(["status", "status"]);
    expect(entries[0]!.text).toMatch(/without a reply/);
    expect(entries[1]!.text).toMatch(/cancelled/);
  });

  it("keeps a partial reply and adds the failure status when a turn fails mid-reply", () => {
    const entries = mapEntries([wire(1, "t1", "reply_chunk", "part"), wire(2, "t1", "turn_failed", "", "interrupted_by_restart")]);
    expect(entries[0]).toMatchObject({ text: "part", streaming: false });
    expect(entries[1]!.text).toMatch(/restarted/);
  });

  it("names an unknown failure code without inventing a reply", () => {
    const entries = mapEntries([wire(1, "t1", "turn_failed", "", "brand_new")]);
    expect(entries[0]!.text).toContain("brand_new");
  });
});

describe("live conversation adapter", () => {
  it("reads every page and reports the latest turn", async () => {
    const fake = populate(new FakeDaemon());
    const pages = [
      { entries: [wire(1, "t1", "user_message", "a")], more: true, next: 1 },
      { entries: [wire(2, "t1", "turn_failed", "", "runtime_unavailable")], more: false, next: 2 },
    ];
    const turn = {
      id: "t1",
      state: "failed",
      error_code: "runtime_unavailable",
      runtime: { name: "unavailable", version: "none" },
      reply_bytes: 0,
      created_at: "2026-11-10T21:00:00Z",
      updated_at: "2026-11-10T21:00:00Z",
    };
    fake.stub("conversations.get", (params: Record<string, unknown>) => {
      const page = pages[params.after_sequence === undefined ? 0 : 1]!;
      return {
        authority: "none",
        target: { kind: "global" },
        entries: page.entries,
        more: page.more,
        next_after_sequence: page.next,
        latest_sequence: 2,
        turn,
        epoch: "e",
      };
    });
    const result = await (await live(fake)).conversation.transcript({ kind: "global" });
    expect(result.status).toBe("ok");
    if (result.status !== "ok") return;
    expect(result.data.entries.map((e) => e.kind)).toEqual(["message", "status"]);
    expect(result.data.turn).toMatchObject({ state: "failed", errorCode: "runtime_unavailable" });
    expect(result.data.truncated).toBe(false);
    expect(fake.callsTo("conversations.get").map((c) => (c.params as Record<string, unknown>).after_sequence)).toEqual([undefined, 1]);
  });

  it("sends the wire target and a key, and shows the runtime-unavailable outcome as recorded, not replied", async () => {
    const fake = populate(new FakeDaemon());
    stubConversation(fake);
    const s = await live(fake);
    const receipt = await s.conversation.send({ kind: "global" }, "hello");
    const [call] = fake.callsTo("conversations.send");
    expect(call!.params).toMatchObject({ target: { kind: "global" }, text: "hello" });
    expect(typeof (call!.params as Record<string, unknown>).idempotency_key).toBe("string");
    expect(receipt.simulated).toBe(false);
    expect(receipt.turn).toMatchObject({ state: "failed", errorCode: "runtime_unavailable" });
    const transcript = await s.conversation.transcript({ kind: "global" });
    expect(transcript.status === "ok" && transcript.data.entries.map((e) => e.kind)).toEqual(["message", "status"]);
  });

  it("reuses the key after an indeterminate failure and uses a new one after a definitive result", async () => {
    const fake = populate(new FakeDaemon());
    stubConversation(fake);
    fake.failNext("conversations.send", "hang");
    const s = await live(fake);
    await expect(s.conversation.send({ kind: "global" }, "same")).rejects.toMatchObject({ kind: "timeout" });
    await s.conversation.send({ kind: "global" }, "same");
    await s.conversation.send({ kind: "global" }, "same");
    const keys = fake.callsTo("conversations.send").map((c) => (c.params as Record<string, string>).idempotency_key);
    expect(keys[1]).toBe(keys[0]);
    expect(keys[2]).not.toBe(keys[1]);
  });

  it("refuses a fixture workstream id before any request", async () => {
    const fake = populate(new FakeDaemon());
    stubConversation(fake);
    const s = await live(fake);
    const target = { kind: "workstream", workstreamId: "fixture:ws-auth" } as const;
    await expect(s.conversation.send(target, "x")).rejects.toMatchObject({ kind: "invalid-params" });
    await expect(s.conversation.cancel!(target, "t1")).rejects.toMatchObject({ kind: "invalid-params" });
    expect((await s.conversation.transcript(target)).status).toBe("unsupported");
    for (const method of ["conversations.send", "conversations.get", "conversations.cancel"]) {
      expect(fake.callsTo(method)).toHaveLength(0);
    }
  });

  it("reports an unknown workstream honestly as unavailable with the lookup reason", async () => {
    const fake = populate(new FakeDaemon());
    stubConversation(fake);
    fake.failNext("conversations.get", { code: -32010, message: "workstream not found" });
    const result = await (await live(fake)).conversation.transcript({ kind: "workstream", workstreamId: "ws-real" });
    expect(result.status).toBe("unavailable");
    if (result.status === "unavailable") expect(result.reason).toMatch(/workstream lookup/);
    expect(JSON.stringify(fake.callsTo("conversations.get")[0]!.params)).toContain('"workstream_id":"ws-real"');
  });

  it("cancels a running turn through conversations.cancel with a key", async () => {
    const fake = populate(new FakeDaemon());
    const convo = stubConversation(fake, "running");
    const s = await live(fake);
    const sent = await s.conversation.send({ kind: "global" }, "slow");
    expect(sent.turn?.state).toBe("running");
    const outcome = await s.conversation.cancel!({ kind: "global" }, sent.turn!.id);
    expect(outcome).toEqual({ applied: true });
    expect(convo.turn?.state).toBe("cancelled");
    expect(fake.callsTo("conversations.cancel")[0]!.params).toMatchObject({
      target: { kind: "global" },
      turn_id: sent.turn!.id,
    });
    const transcript = await s.conversation.transcript({ kind: "global" });
    expect(transcript.status === "ok" && transcript.data.entries.at(-1)).toMatchObject({ kind: "status" });
  });

  it("leaves the project coordinator path untouched", async () => {
    const fake = populate(new FakeDaemon());
    const s = await live(fake);
    const result = await s.conversation.transcript({ kind: "project", projectId: "/repos/alpha" });
    expect(result.status).toBe("ok");
    expect(fake.callsTo("conversations.get")).toHaveLength(0);
  });
});
