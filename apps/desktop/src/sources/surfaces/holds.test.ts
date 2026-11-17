import { afterEach, describe, expect, it } from "vitest";
import { FakeDaemon } from "../../test/fakeDaemon";
import { createFixtureSurfaces } from "./fixtures";
import { FIXTURE_HOLD_NOTICE, heldSets, mapHold } from "./holds";
import { connect, populate } from "./testkit";
import type { HoldItem } from "./types";

const stops: Array<() => void> = [];
afterEach(() => {
  while (stops.length) stops.pop()!();
});

const WIRE = { id: "hold-1", subject_kind: "approval", subject_id: "t-run", until_date: "2026-11-21", state: "active" };

async function live(fake: FakeDaemon) {
  const connected = await connect(fake);
  stops.push(connected.stop);
  return connected.surfaces;
}

describe("live holds source", () => {
  it("holds_live_list_maps_the_holds_in_force", async () => {
    const fake = populate(new FakeDaemon());
    fake.stub("holds.list", { holds: [WIRE, { ...WIRE, id: "hold-2", subject_kind: "proposal", subject_id: "proposal-9" }] });
    const result = await (await live(fake)).holds.list();
    expect(result.status === "ok" && result.data).toEqual([
      { id: "hold-1", subjectKind: "approval", subjectId: "t-run", untilDate: "2026-11-21", state: "active" },
      { id: "hold-2", subjectKind: "proposal", subjectId: "proposal-9", untilDate: "2026-11-21", state: "active" },
    ]);
    expect(fake.callsTo("holds.list")[0]!.params).toEqual({});
  });

  it("holds_live_list_is_unsupported_when_the_daemon_has_no_holds_method", async () => {
    const fake = populate(new FakeDaemon());
    fake.failNext("holds.list", { code: -32601, message: "method not found" });
    const result = await (await live(fake)).holds.list();
    expect(result.status).toBe("unsupported");
  });

  it("holds_live_set_sends_the_kind_the_subject_the_day_and_a_key", async () => {
    const fake = populate(new FakeDaemon());
    fake.stub("holds.set", { hold: WIRE });
    const receipt = await (await live(fake)).holds.set("approval", "t-run", "2026-11-21");
    expect(receipt).toMatchObject({ simulated: false, notice: null, hold: { id: "hold-1", untilDate: "2026-11-21" } });
    const [call] = fake.callsTo("holds.set");
    expect(Object.keys(call!.params as object).sort()).toEqual(["idempotency_key", "subject_id", "subject_kind", "until"]);
    expect(call!.params).toMatchObject({ subject_kind: "approval", subject_id: "t-run", until: "2026-11-21" });
  });

  it("holds_live_release_sends_the_hold_id_and_a_key", async () => {
    const fake = populate(new FakeDaemon());
    fake.stub("holds.release", { hold: { ...WIRE, state: "released" } });
    const receipt = await (await live(fake)).holds.release("hold-1");
    expect(receipt.hold.state).toBe("released");
    const [call] = fake.callsTo("holds.release");
    expect(Object.keys(call!.params as object).sort()).toEqual(["hold_id", "idempotency_key"]);
  });

  it("holds_live_set_rejects_a_daemon_refusal_and_never_answers_with_a_receipt", async () => {
    const fake = populate(new FakeDaemon());
    fake.failNext("holds.set", { code: -32602, message: "invalid params" });
    const surfaces = await live(fake);
    await expect(surfaces.holds.set("approval", "t-run", "2030-01-01")).rejects.toMatchObject({ kind: "invalid-params" });
  });

  it("holds_live_set_refuses_a_fixture_id_before_any_request", async () => {
    const fake = populate(new FakeDaemon());
    const surfaces = await live(fake);
    await expect(surfaces.holds.set("approval", "fixture:task-1", "2026-11-21")).rejects.toMatchObject({ kind: "invalid-params" });
    expect(fake.callsTo("holds.set")).toEqual([]);
  });
});

describe("holds helpers", () => {
  it("holds_map_hold_keeps_unknown_kind_and_state_as_strings", () => {
    expect(mapHold({ ...WIRE, subject_kind: "future", state: "paused" })).toMatchObject({ subjectKind: "future", state: "paused" });
  });

  it("holds_held_sets_split_tasks_from_proposals_and_ignore_a_hold_that_is_not_active", () => {
    const item = (over: Partial<HoldItem>): HoldItem => ({ ...mapHold(WIRE), ...over });
    const sets = heldSets([
      item({ subjectId: "t-1" }),
      item({ id: "h2", subjectKind: "proposal", subjectId: "p-1" }),
      item({ id: "h3", subjectId: "t-2", state: "released" }),
      item({ id: "h4", subjectKind: "future", subjectId: "x" }),
    ]);
    expect([...sets.tasks]).toEqual(["t-1"]);
    expect([...sets.proposals]).toEqual(["p-1"]);
  });
});

describe("fixture holds source", () => {
  it("holds_fixture_lists_one_held_decision_and_never_calls_a_daemon", async () => {
    const result = await createFixtureSurfaces().holds.list();
    expect(result.status === "ok" && result.data).toHaveLength(1);
    expect(result.provenance.kind).toBe("fixture");
  });

  it("holds_fixture_set_and_release_change_memory_only_and_say_so", async () => {
    const holds = createFixtureSurfaces().holds;
    const set = await holds.set("approval", "fixture:task-auth-1", "2026-01-15");
    expect(set).toMatchObject({ simulated: true, notice: FIXTURE_HOLD_NOTICE, hold: { subjectId: "fixture:task-auth-1" } });
    const listed = await holds.list();
    expect(listed.status === "ok" && listed.data.map((hold) => hold.subjectId)).toContain("fixture:task-auth-1");
    const released = await holds.release(set.hold.id);
    expect(released).toMatchObject({ simulated: true, notice: "Fixture data: nothing was changed." });
    const after = await holds.list();
    expect(after.status === "ok" && after.data.map((hold) => hold.id)).not.toContain(set.hold.id);
  });

  it("holds_fixture_empty_scenario_lists_nothing_and_a_missing_hold_is_not_found", async () => {
    const holds = createFixtureSurfaces({ scenario: "empty" }).holds;
    const result = await holds.list();
    expect(result.status === "ok" && result.data).toEqual([]);
    await expect(holds.release("fixture:missing")).rejects.toMatchObject({ kind: "not-found" });
  });
});
