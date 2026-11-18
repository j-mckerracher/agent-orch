import { afterEach, describe, expect, it } from "vitest";
import { FakeDaemon } from "../../test/fakeDaemon";
import { stubConversation } from "../../test/m724Stubs";
import type { ConversationWireEntry } from "../../daemon/types";
import { mapEntries, mapWakeRecord, turnInfo, unverifiedWarning } from "./conversationLive";
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

  it("names the agent, its version and the pin that accepts it when a version is refused (DPES-1106)", () => {
    const refused = { ...wire(2, "t1", "turn_failed", "", "version_mismatch"), runtime: { name: "codex", version: "0.169.0" } };
    const entries = mapEntries([wire(1, "t1", "user_message", "hi"), refused]);
    expect(entries[1]).toMatchObject({ kind: "status", errorCode: "version_mismatch" });
    expect(entries[1]!.text).toBe(
      "Codex 0.169.0 is not a supported version yet. Set AGENT_ORCH_CONVERSATION_PINS=codex@0.169.0 and restart the daemon to try it. No reply was produced.",
    );
  });

  it("uses the agent's display name for a refused Claude Code version", () => {
    const refused = { ...wire(2, "t1", "turn_failed", "", "version_mismatch"), runtime: { name: "claude-code", version: "2.2.0" } };
    const entries = mapEntries([wire(1, "t1", "user_message", "hi"), refused]);
    expect(entries[1]!.text).toMatch(/^Claude Code 2\.2\.0 is not a supported version yet\. Set AGENT_ORCH_CONVERSATION_PINS=claude-code@2\.2\.0 /);
  });

  it("keeps the generic unsupported-version text when the daemon names no runtime", () => {
    const entries = mapEntries([wire(1, "t1", "user_message", "hi"), wire(2, "t1", "turn_failed", "", "version_mismatch")]);
    expect(entries[1]!.text).toBe("The orchestrator runtime has an unsupported version. No reply was produced.");
  });

  it("says the agent changed before the send when the selection moved mid-turn", () => {
    const entries = mapEntries([wire(1, "t1", "user_message", "hi"), wire(2, "t1", "turn_failed", "", "runtime_changed")]);
    expect(entries[1]!.text).toMatch(/agent changed before this message was sent/);
  });

  it("names the M7-25 runtime failures instead of showing a generic message", () => {
    for (const [code, pattern] of [
      ["runtime_timeout", /too long/],
      ["runtime_busy", /already answering/],
      ["selection_unknown_model", /does not offer that model/],
      ["selection_agent_unsupported", /cannot answer a message/],
    ] as const) {
      const entries = mapEntries([wire(1, "t1", "user_message", "hi"), wire(2, "t1", "turn_failed", "", code)]);
      expect(entries[1]).toMatchObject({ kind: "status", role: "system", errorCode: code });
      expect(entries[1]!.text).toMatch(pattern);
    }
  });

  it("says the agent's usage or spending limit refused the turn, and offers the way out", () => {
    const entries = mapEntries([wire(1, "t1", "user_message", "hi"), wire(2, "t1", "turn_failed", "", "runtime_provider_limit")]);
    expect(entries[1]).toMatchObject({ kind: "status", role: "system", errorCode: "runtime_provider_limit" });
    expect(entries[1]!.text).toBe(
      "The orchestrator's agent refused the turn: its usage or spending limit is reached. Raise the limit or choose another agent in Settings. No reply was produced.",
    );
  });

  it("marks a completed turn without a reply and a canceled turn as statuses", () => {
    const entries = mapEntries([wire(1, "t1", "turn_completed"), wire(2, "t2", "turn_cancelled")]);
    expect(entries.map((e) => e.kind)).toEqual(["status", "status"]);
    expect(entries[0]!.text).toMatch(/without a reply/);
    expect(entries[1]!.text).toMatch(/canceled/);
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

/**
 * A daemon-shaped paged log. `turns` turns of two entries each; `gap` is how many sequence numbers
 * other conversations use between two of this conversation's entries (sequences are store-wide).
 * The last turn is left `running` when `lastRunning` is set.
 */
function stubPagedLog(fake: FakeDaemon, turns: number, options: { gap?: number; lastRunning?: boolean } = {}) {
  const gap = options.gap ?? 1;
  const log: ConversationWireEntry[] = [];
  for (let n = 1; n <= turns; n++) {
    const running = options.lastRunning === true && n === turns;
    log.push(wire(log.length * gap + 1, `t${n}`, "user_message", `m${n}`));
    if (!running) log.push(wire(log.length * gap + 1, `t${n}`, "reply_chunk", `r${n}`));
    if (!running) log.push(wire(log.length * gap + 1, `t${n}`, "turn_completed"));
  }
  const latest = log.at(-1)!.sequence;
  const turnFor = (id: string, state: "running" | "completed") => ({
    id,
    state,
    error_code: null,
    runtime: { name: "stub", version: "1" },
    reply_bytes: 0,
    created_at: "2026-11-10T21:00:00Z",
    updated_at: "2026-11-10T21:00:00Z",
  });
  const lastTurn = turnFor(`t${turns}`, options.lastRunning ? "running" : "completed");
  fake.stub("conversations.get", (params: Record<string, unknown>) => {
    const after = typeof params.after_sequence === "number" ? params.after_sequence : 0;
    const rest = log.filter((entry) => entry.sequence > after);
    const entries = rest.slice(0, 100);
    return {
      authority: "none",
      target: { kind: "global" },
      entries,
      more: rest.length > 100,
      next_after_sequence: entries.at(-1)?.sequence ?? after,
      latest_sequence: latest,
      turn: lastTurn,
      epoch: "epoch-1",
    };
  });
  return { log, afters: () => fake.callsTo("conversations.get").map((c) => (c.params as Record<string, unknown>).after_sequence) };
}

describe("live transcript of a very long log", () => {
  it("shows the newest entries and the latest turn instead of the oldest 5000 entries", async () => {
    const fake = populate(new FakeDaemon());
    const { afters } = stubPagedLog(fake, 3000, { lastRunning: true });
    const result = await (await live(fake)).conversation.transcript({ kind: "global" });
    expect(result.status).toBe("ok");
    if (result.status !== "ok") return;
    const messages = result.data.entries.filter((e) => e.role === "user");
    expect(messages.at(-1)!.text).toBe("m3000");
    expect(result.data.entries.some((e) => e.text === "m1")).toBe(false);
    expect(result.data.entries[0]).toMatchObject({ role: "user" });
    expect(result.data.entries.length).toBeLessThanOrEqual(5000);
    expect(result.data.truncated).toBe(true);
    expect(result.data.turn).toMatchObject({ id: "t3000", state: "running" });
    expect(afters().length).toBeLessThanOrEqual(51);
    expect(afters()[0]).toBeUndefined();
    expect(afters()[1]).toBeGreaterThan(0);
    for (const call of fake.callsTo("conversations.get").slice(1)) {
      expect((call.params as Record<string, unknown>).epoch).toBe("epoch-1");
    }
  });

  it("shows a log that fits the window whole, without truncation", async () => {
    const fake = populate(new FakeDaemon());
    stubPagedLog(fake, 1600);
    const result = await (await live(fake)).conversation.transcript({ kind: "global" });
    expect(result.status === "ok" && [result.data.truncated, result.data.entries.filter((e) => e.role === "user").length]).toEqual([
      false,
      1600,
    ]);
  });

  it("does not report truncation when other conversations only stretch the sequence span", async () => {
    const fake = populate(new FakeDaemon());
    const { afters } = stubPagedLog(fake, 40, { gap: 200 });
    const result = await (await live(fake)).conversation.transcript({ kind: "global" });
    expect(result.status === "ok" && result.data.truncated).toBe(false);
    expect(result.status === "ok" && result.data.entries.filter((e) => e.role === "user")).toHaveLength(40);
    expect(afters().length).toBeGreaterThan(1);
  });

  it("reports truncation when the window skips real older entries across a stretched span", async () => {
    const fake = populate(new FakeDaemon());
    stubPagedLog(fake, 200, { gap: 200 });
    const result = await (await live(fake)).conversation.transcript({ kind: "global" });
    expect(result.status).toBe("ok");
    if (result.status !== "ok") return;
    expect(result.data.truncated).toBe(true);
    expect(result.data.entries.some((e) => e.text === "m1")).toBe(false);
    expect(result.data.entries.at(-1)).toMatchObject({ role: "coordinator", text: "r200", streaming: false });
    expect(result.data.entries.filter((e) => e.role === "user").at(-1)!.text).toBe("m200");
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

  it("sends the chosen selection with the message, and a retry with another choice is a new message", async () => {
    const fake = populate(new FakeDaemon());
    stubConversation(fake);
    fake.failNext("conversations.send", "hang");
    const s = await live(fake);
    const choice = { agent: "codex", model: "gpt-a", effort: "high" };
    await expect(s.conversation.send({ kind: "global" }, "same", choice)).rejects.toMatchObject({ kind: "timeout" });
    await s.conversation.send({ kind: "global" }, "same");
    const [first, second] = fake.callsTo("conversations.send").map((c) => c.params as Record<string, unknown>);
    expect(first!.selection).toEqual(choice);
    // Without a choice the request has no `selection` key at all, as before the picker sent one.
    expect(second).not.toHaveProperty("selection");
    expect(second!.idempotency_key).not.toBe(first!.idempotency_key);
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

  it("reads legacy project notes and the shared runtime transcript", async () => {
    const fake = populate(new FakeDaemon());
    stubConversation(fake);
    const s = await live(fake);
    const result = await s.conversation.transcript({ kind: "project", projectId: "/repos/alpha" });
    expect(result.status).toBe("ok");
    expect(fake.callsTo("conversations.get")[0]!.params).toMatchObject({
      target: { kind: "project", project_id: "/repos/alpha" },
    });
    expect(fake.callsTo("coordinator.get")).toHaveLength(1);
  });
});

describe("unverified-version warning (DPES-1106)", () => {
  const turn = (verified: boolean | undefined, name = "codex", version = "0.169.0") =>
    turnInfo({
      id: "t1",
      state: "completed",
      error_code: null,
      runtime: { name, version },
      ...(verified === undefined ? {} : { verified }),
      reply_bytes: 0,
      created_at: "2026-11-10T21:00:00Z",
      updated_at: "2026-11-10T21:00:00Z",
    });

  it("says a version has not been verified, in plain words, and names the agent and version", () => {
    const warned = new Set<string>();
    expect(unverifiedWarning(turn(false), warned)).toBe(
      "Codex 0.169.0 has not been verified with MCAO. Replies may fail.",
    );
  });

  it("shows the warning once per agent and version for the session, not on every message", () => {
    const warned = new Set<string>();
    expect(unverifiedWarning(turn(false), warned)).toBeDefined();
    expect(unverifiedWarning(turn(false), warned)).toBeUndefined();
    expect(unverifiedWarning(turn(false, "codex", "0.170.0"), warned)).toBeDefined();
    expect(unverifiedWarning(turn(false, "claude-code", "2.2.0"), warned)).toMatch(/^Claude Code 2\.2\.0 /);
  });

  it("is never shown for a verified turn, or for a daemon that does not say", () => {
    const warned = new Set<string>();
    expect(unverifiedWarning(turn(true), warned)).toBeUndefined();
    expect(unverifiedWarning(turn(undefined), warned)).toBeUndefined();
    expect(warned.size).toBe(0);
  });

  it("is sent with the notice of a send, without blocking it, and only the first send of a version carries it", async () => {
    const fake = populate(new FakeDaemon());
    let seq = 0;
    fake.stub("conversations.send", (params: Record<string, unknown>) => {
      seq += 1;
      const entry = {
        sequence: seq,
        id: `e${seq}`,
        turn_id: `turn-${seq}`,
        kind: "user_message",
        author: "user",
        body: String(params.text),
        error_code: null,
        created_at: "2026-11-10T21:00:00Z",
      };
      return {
        authority: "none",
        target: params.target,
        entry,
        turn: {
          id: `turn-${seq}`,
          state: "running",
          error_code: null,
          runtime: { name: "codex", version: "0.169.0" },
          verified: false,
          reply_bytes: 0,
          created_at: "2026-11-10T21:00:00Z",
          updated_at: "2026-11-10T21:00:00Z",
        },
        epoch: "epoch-1",
      };
    });
    const s = await live(fake);
    const first = await s.conversation.send({ kind: "global" }, "one");
    const second = await s.conversation.send({ kind: "global" }, "two");
    expect(first.simulated).toBe(false);
    expect(first.notice).toMatch(/Recorded by the daemon\. .*Codex 0\.169\.0 has not been verified with MCAO\. Replies may fail\./);
    expect(first.warning).toBe("Codex 0.169.0 has not been verified with MCAO. Replies may fail.");
    expect(second.notice).not.toMatch(/has not been verified/);
    expect(second.warning).toBeUndefined();
  });
});

describe("wake mapping", () => {
  it("conversation_live_maps_wake_records_trigger_and_origin_and_unknown_strings_survive", () => {
    const opening = { ...wire(3, "w1", "user_message", "Task t-1 failed."), origin: "wake" };
    const [entry] = mapEntries([opening]);
    expect(entry).toMatchObject({ origin: "wake", sequence: 3, role: "user" });
    expect(mapEntries([wire(1, "t1", "user_message", "hi")])[0]!.origin).toBeUndefined();
    const turn = {
      id: "w1",
      state: "running" as const,
      error_code: null,
      runtime: { name: "codex", version: "1" },
      reply_bytes: 0,
      created_at: "x",
      updated_at: "x",
    };
    expect(turnInfo({ ...turn, trigger: "wake" }).trigger).toBe("wake");
    expect(turnInfo(turn).trigger).toBeUndefined();
    expect(
      mapWakeRecord({
        id: "wake-1",
        event: "brand_new_event",
        subject: "t-1",
        notice: "n",
        state: "waiting_room",
        outcome: null,
        skip_reason: "limit_weekly",
        turn_id: null,
        anchor_sequence: 41,
        notify: true,
        seen: false,
        created_at: "2026-11-16 02:10:03",
      }),
    ).toMatchObject({ event: "brand_new_event", state: "waiting_room", skipReason: "limit_weekly", anchorSequence: 41 });
  });
});
