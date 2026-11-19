import { afterEach, describe, expect, it } from "vitest";
import { FakeDaemon } from "../../test/fakeDaemon";
import { stubActionsResolve, stubConversation } from "../../test/m724Stubs";
import type { ActionProposalWire } from "../../daemon/types";
import { mapProposal } from "./actionProposals";
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

const at = "2026-11-10T21:00:00Z";
const DIGEST = "d".repeat(64);
const proposal = (over: Partial<ActionProposalWire> = {}): ActionProposalWire => ({
  id: "proposal-1",
  turn_id: "turn-1",
  index: 0,
  kind: "workstream.create",
  params: { kind: "workstream.create", title: "Post-MVP work", summary: "" },
  digest: DIGEST,
  depends_on: null,
  state: "pending",
  result_ref: null,
  error_code: null,
  created_at: at,
  updated_at: at,
  ...over,
});

describe("action proposals (DPES-1105)", () => {
  it("maps proposals without their kind key, and treats a daemon that sends none as having none", async () => {
    const fake = populate(new FakeDaemon());
    const stub = stubConversation(fake);
    const s = await live(fake);
    expect(await s.conversation.transcript({ kind: "global" })).toMatchObject({ status: "ok", data: { proposals: [] } });
    stub.proposals = [proposal()];
    const transcript = await s.conversation.transcript({ kind: "global" });
    expect(transcript.status === "ok" && transcript.data.proposals).toEqual([
      {
        id: "proposal-1",
        turnId: "turn-1",
        index: 0,
        kind: "workstream.create",
        params: { title: "Post-MVP work", summary: "" },
        digest: DIGEST,
        dependsOn: null,
        state: "pending",
        resultRef: null,
        errorCode: null,
        denial: null,
        target: null,
        updatedAt: at,
      },
    ]);
  });

  it("reads the proposals from the last page, which is the newest", async () => {
    const fake = populate(new FakeDaemon());
    let pages = 0;
    fake.stub("conversations.get", () => {
      pages += 1;
      const last = pages > 1;
      return {
        authority: "none",
        target: { kind: "global" },
        entries: [],
        more: !last,
        next_after_sequence: 0,
        latest_sequence: 0,
        clear_generation: 0,
        turn: null,
        epoch: "epoch-1",
        proposals: [proposal({ id: last ? "proposal-2" : "proposal-1" })],
      };
    });
    const s = await live(fake);
    const transcript = await s.conversation.transcript({ kind: "global" });
    expect(pages).toBe(2);
    expect(transcript.status === "ok" && transcript.data.proposals?.map((item) => item.id)).toEqual(["proposal-2"]);
  });

  it("carries proposals_more into the transcript, and reads an absent flag as false", async () => {
    const fake = populate(new FakeDaemon());
    stubConversation(fake, "completed");
    const s = await live(fake);
    expect(await s.conversation.transcript({ kind: "global" })).toMatchObject({ status: "ok", data: { proposalsMore: false } });
    fake.stub("conversations.get", () => ({
      authority: "none",
      target: { kind: "global" },
      entries: [],
      more: false,
      next_after_sequence: 0,
      latest_sequence: 0,
      clear_generation: 0,
      turn: null,
      epoch: "epoch-1",
      proposals: [proposal()],
      proposals_more: true,
    }));
    const capped = await s.conversation.transcript({ kind: "global" });
    expect(capped.status === "ok" && capped.data.proposalsMore).toBe(true);
  });

  it("keeps each older turn's action block from its completion entry, and reads the latest turn's from the turn", async () => {
    const fake = populate(new FakeDaemon());
    const stub = stubConversation(fake, "completed");
    const s = await live(fake);
    await s.conversation.send({ kind: "global" }, "first");
    stub.entries.find((item) => item.kind === "turn_completed")!.action_block = "rejected";
    await s.conversation.send({ kind: "global" }, "second");
    const older = await s.conversation.transcript({ kind: "global" });
    expect(older.status === "ok" && older.data.actionBlocks).toEqual({ "turn-1": "rejected" });
    expect(older.status === "ok" && older.data.turn).toMatchObject({ actionBlock: null });

    stub.turn = { ...stub.turn!, action_block: "accepted" };
    const latest = await s.conversation.transcript({ kind: "global" });
    expect(latest.status === "ok" && latest.data.turn).toMatchObject({ actionBlock: "accepted" });
    expect(latest.status === "ok" && latest.data.actionBlocks).toEqual({ "turn-1": "rejected", "turn-2": "accepted" });
  });

  it("says a turn that proposed actions without a written reply did not finish with a reply", async () => {
    const fake = populate(new FakeDaemon());
    const stub = stubConversation(fake, "completed");
    stub.proposals = [proposal()];
    const s = await live(fake);
    await s.conversation.send({ kind: "global" }, "make a workstream");
    stub.entries = stub.entries.filter((item) => item.kind !== "reply_chunk");
    const transcript = await s.conversation.transcript({ kind: "global" });
    expect(transcript.status === "ok" && transcript.data.entries.map((item) => item.text)).toContain(
      "The orchestrator proposed actions without a written reply.",
    );
  });

  it("refuses a fixture proposal id without sending anything", async () => {
    const fake = populate(new FakeDaemon());
    stubConversation(fake);
    const s = await live(fake);
    await expect(s.conversation.resolve("fixture:proposal-1", "approve", DIGEST)).rejects.toMatchObject({ kind: "invalid-params" });
    expect(fake.callsTo("actions.resolve")).toEqual([]);
  });

  it("sends the proposal's digest with the decision", async () => {
    const fake = populate(new FakeDaemon());
    const stub = stubConversation(fake, "completed");
    stub.proposals = [proposal()];
    stubActionsResolve(fake, stub);
    const s = await live(fake);
    await s.conversation.resolve("proposal-1", "approve", DIGEST);
    const [call] = fake.callsTo("actions.resolve");
    expect(call!.params).toMatchObject({ proposal_id: "proposal-1", decision: "approve", digest: DIGEST });
  });

  it("approves through actions.resolve and returns the state the daemon confirmed", async () => {
    const fake = populate(new FakeDaemon());
    const stub = stubConversation(fake, "completed");
    stub.proposals = [proposal()];
    stubActionsResolve(fake, stub);
    const s = await live(fake);
    const receipt = await s.conversation.resolve("proposal-1", "approve", DIGEST);
    expect(receipt).toMatchObject({ simulated: false, proposal: { state: "executed", resultRef: "workstream-proposal-1" } });
    const [call] = fake.callsTo("actions.resolve");
    expect(call!.params).toMatchObject({ proposal_id: "proposal-1", decision: "approve" });
    expect(typeof (call!.params as Record<string, unknown>).idempotency_key).toBe("string");
  });

  it("refuses a dependent until its parent has executed, then executes the dependent", async () => {
    const fake = populate(new FakeDaemon());
    const stub = stubConversation(fake, "completed");
    stub.proposals = [
      proposal(),
      proposal({
        id: "proposal-2",
        index: 1,
        kind: "objective.add",
        params: { workstream_ref: "proposal-1", title: "Draft the checklist" },
        depends_on: "proposal-1",
      }),
    ];
    stubActionsResolve(fake, stub);
    const s = await live(fake);
    await expect(s.conversation.resolve("proposal-2", "approve", DIGEST)).rejects.toMatchObject({ kind: "proposal-invalid" });
    await s.conversation.resolve("proposal-1", "approve", DIGEST);
    await expect(s.conversation.resolve("proposal-2", "approve", DIGEST)).resolves.toMatchObject({
      proposal: { state: "executed", resultRef: "objective-proposal-2" },
    });
  });

  it("marks a rejected parent's pending dependents as failed", async () => {
    const fake = populate(new FakeDaemon());
    const stub = stubConversation(fake, "completed");
    stub.proposals = [
      proposal(),
      proposal({ id: "proposal-2", index: 1, kind: "objective.add", params: { workstream_ref: "proposal-1" }, depends_on: "proposal-1" }),
    ];
    stubActionsResolve(fake, stub);
    const s = await live(fake);
    await s.conversation.resolve("proposal-1", "reject", DIGEST);
    const transcript = await s.conversation.transcript({ kind: "global" });
    expect(transcript.status === "ok" && transcript.data.proposals?.map((item) => [item.id, item.state, item.errorCode])).toEqual([
      ["proposal-1", "rejected", null],
      ["proposal-2", "failed", "dependency_not_executed"],
    ]);
  });

  it("reuses the key after a timeout, and mints a new one after the daemon has answered", async () => {
    const fake = populate(new FakeDaemon());
    const stub = stubConversation(fake, "completed");
    stub.proposals = [proposal()];
    stubActionsResolve(fake, stub);
    fake.failNext("actions.resolve", "hang");
    const s = await live(fake);
    await expect(s.conversation.resolve("proposal-1", "approve", DIGEST)).rejects.toMatchObject({ kind: "timeout" });
    await s.conversation.resolve("proposal-1", "approve", DIGEST);
    await expect(s.conversation.resolve("proposal-1", "approve", DIGEST)).rejects.toMatchObject({ kind: "proposal-not-pending" });
    const keys = fake.callsTo("actions.resolve").map((c) => (c.params as Record<string, string>).idempotency_key);
    expect(keys[1]).toBe(keys[0]);
    expect(keys[2]).not.toBe(keys[1]);
  });

  it("mints a new key after idempotency-in-progress, because the old key cannot finish", async () => {
    const fake = populate(new FakeDaemon());
    const stub = stubConversation(fake, "completed");
    stub.proposals = [proposal()];
    stubActionsResolve(fake, stub);
    fake.failNext("actions.resolve", { code: -32014, message: "the same request is still being processed" });
    const s = await live(fake);
    await expect(s.conversation.resolve("proposal-1", "approve", DIGEST)).rejects.toMatchObject({ kind: "idempotency-in-progress" });
    await s.conversation.resolve("proposal-1", "approve", DIGEST);
    const keys = fake.callsTo("actions.resolve").map((c) => (c.params as Record<string, string>).idempotency_key);
    expect(keys[1]).not.toBe(keys[0]);
  });
});

describe("mapProposal", () => {
  it("grants_the_daemons_top_level_grant_id_reads_as_an_approval_by_a_standing_grant", () => {
    expect(mapProposal(proposal({ grant_id: "grant-7" })).approvedBy).toEqual({ kind: "grant", grantId: "grant-7" });
    expect(mapProposal(proposal({ grant_id: null })).approvedBy).toBeUndefined();
  });

  it("authority_reads_autonomous_and_overrides_and_treats_an_older_daemon_as_neither", () => {
    const older = mapProposal(proposal());
    expect(older.autonomous).toBeUndefined();
    expect(older.overrides).toBeUndefined();
    expect(mapProposal(proposal({ autonomous: false, overrides: [] }))).toEqual(older);
    const mapped = mapProposal(
      proposal({
        autonomous: true,
        overrides: [
          { field: "delivery", project_default: "LocalReview", chosen: "ReportOnly" },
          { field: "delivery", chosen: "ReportOnly" } as never,
        ],
      }),
    );
    expect(mapped.autonomous).toBe(true);
    expect(mapped.overrides).toEqual([{ field: "delivery", projectDefault: "LocalReview", chosen: "ReportOnly" }]);
  });

  it("copies every field and leaves an unknown kind or state as it arrived", () => {
    const mapped = mapProposal(proposal({ kind: "future.kind", state: "paused", depends_on: "proposal-0", result_ref: "ref-1", error_code: "x" }));
    expect(mapped).toEqual({
      id: "proposal-1",
      turnId: "turn-1",
      index: 0,
      kind: "future.kind",
      params: { title: "Post-MVP work", summary: "" },
      digest: DIGEST,
      dependsOn: "proposal-0",
      state: "paused",
      resultRef: "ref-1",
      errorCode: "x",
      denial: null,
      target: null,
      updatedAt: at,
    });
  });

  it("reads a denial and the display facts a daemon sends with a task proposal", () => {
    const denied = mapProposal(
      proposal({
        kind: "task.propose",
        state: "failed",
        error_code: "action_denied",
        denial: "delivery_not_authorized",
        target: { project_name: "alpha" },
      }),
    );
    expect(denied).toMatchObject({
      kind: "task.propose",
      errorCode: "action_denied",
      denial: "delivery_not_authorized",
      target: { projectName: "alpha" },
    });

    const cancel = mapProposal(
      proposal({ kind: "task.cancel", target: { task_title: "Add refresh-token expiry", task_state: "Running" } }),
    );
    expect(cancel.target).toEqual({ taskTitle: "Add refresh-token expiry", taskState: "Running" });
    expect(mapProposal(proposal({ kind: "task.retry", target: null })).target).toBeNull();
  });
});
