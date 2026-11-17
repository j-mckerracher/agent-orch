import { afterEach, describe, expect, it } from "vitest";
import { FakeDaemon } from "../../test/fakeDaemon";
import { mapMonitoring } from "./monitoringLive";
import { connect, populate } from "./testkit";

const stops: Array<() => void> = [];
afterEach(() => {
  while (stops.length) stops.pop()!();
});

const WIRE = {
  authority: "none" as const,
  mode: "on",
  paused: [{ target: { kind: "global" as const }, reason: "hourly", until: "2026-11-16T10:20:00Z" }],
  unseen: [
    { target: { kind: "global" as const }, count: 2 },
    { target: { kind: "workstream" as const, workstream_id: "ws-1" }, count: 1 },
  ],
  epoch: "e1",
};

async function live(fake: FakeDaemon) {
  const connected = await connect(fake);
  stops.push(connected.stop);
  return connected.surfaces;
}

describe("live monitoring source", () => {
  it("monitoring_live_parses_monitoring_get_into_modes_pauses_and_unseen_counts", async () => {
    const fake = populate(new FakeDaemon());
    fake.stub("monitoring.get", WIRE);
    const surfaces = await live(fake);
    const result = await surfaces.monitoring.get();
    expect(result.status === "ok" && result.data).toEqual({
      mode: "on",
      paused: [{ target: { kind: "global" }, reason: "hourly", until: "2026-11-16T10:20:00Z" }],
      unseen: [
        { target: { kind: "global" }, count: 2 },
        { target: { kind: "workstream", workstreamId: "ws-1" }, count: 1 },
      ],
    });
  });

  it("monitoring_live_set_sends_only_the_mode_and_a_key", async () => {
    const fake = populate(new FakeDaemon());
    fake.stub("monitoring.set", (params: Record<string, unknown>) => ({ ...WIRE, mode: params.mode }));
    const surfaces = await live(fake);
    const receipt = await surfaces.monitoring.set("quiet");
    expect(receipt).toMatchObject({ simulated: false, state: { mode: "quiet" } });
    const [call] = fake.callsTo("monitoring.set");
    expect(Object.keys(call!.params as object).sort()).toEqual(["idempotency_key", "mode"]);
    expect(call!.params).toMatchObject({ mode: "quiet" });
  });

  it("monitoring_live_keeps_an_unknown_mode_and_reason_as_strings", () => {
    const state = mapMonitoring({ ...WIRE, mode: "away", paused: [{ target: { kind: "global" }, reason: "weekly", until: null }] });
    expect(state.mode).toBe("away");
    expect(state.paused[0]).toMatchObject({ reason: "weekly", until: null });
  });

  it("monitoring_live_ack_sends_the_target", async () => {
    const fake = populate(new FakeDaemon());
    fake.stub("monitoring.ack", { authority: "none", acked: 2 });
    const surfaces = await live(fake);
    await surfaces.monitoring.ack({ kind: "workstream", workstreamId: "ws-1" });
    expect(fake.callsTo("monitoring.ack")[0]!.params).toMatchObject({ target: { kind: "workstream", workstream_id: "ws-1" } });
  });

  it("brief_live_reads_the_brief_and_acks_it_with_a_key", async () => {
    const fake = populate(new FakeDaemon());
    fake.stub("monitoring.brief", () => ({ authority: "none", brief: null }));
    fake.stub("monitoring.brief_ack", { authority: "none", brief_pending: false });
    const surfaces = await live(fake);
    const result = await surfaces.monitoring.brief();
    expect(result.status === "ok" && result.data).toBeNull();
    await expect(surfaces.monitoring.briefAck()).resolves.toEqual({ simulated: false });
    expect(Object.keys(fake.callsTo("monitoring.brief_ack")[0]!.params as object)).toEqual(["idempotency_key"]);
  });
});
