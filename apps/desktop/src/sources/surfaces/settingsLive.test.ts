import { afterEach, describe, expect, it } from "vitest";
import { ALL_CAPABILITIES, type SettingsWire } from "../../daemon/types";
import { FakeDaemon } from "../../test/fakeDaemon";
import { connect } from "./testkit";

const stops: Array<() => void> = [];
afterEach(() => {
  while (stops.length) stops.pop()!();
});

const WIRE: SettingsWire = {
  load: { cpu_percent: 80, memory_percent: 85, disk_percent: 100 },
  gate: {
    state: "closed",
    hold: { meter: "cpu", mean_percent: 91, limit_percent: 80, reopen_percent: 70 },
    meters: [
      { meter: "cpu", state: "mean", mean_percent: 91 },
      { meter: "memory", state: "unknown", reason: "warming_up" },
      { meter: "disk", state: "off" },
    ],
  },
};

async function settingsOf(fake: FakeDaemon, start = true) {
  const connected = await connect(fake, start);
  stops.push(connected.stop);
  return connected.surfaces.settings;
}

describe("live D17 settings adapter", () => {
  it("maps dispatch defaults and sends a group replacement without changing unrelated settings", async () => {
    const fake = new FakeDaemon();
    const wire = { ...WIRE, dispatch_defaults: { default_agent: "codex", providers: { codex: { model: "m1", effort: "high" } } } };
    fake.stub("settings.get", wire);
    fake.stub("settings.set", wire);
    const source = await settingsOf(fake);
    const defaults = { defaultAgent: "codex", providers: { codex: { model: "m1", effort: "high" } } };
    expect(await source.get()).toMatchObject({ status: "ok", data: { dispatchDefaults: defaults } });
    await source.setGroups({ dispatchDefaults: defaults });
    expect(fake.callsTo("settings.set")[0]?.params).toEqual({
      idempotency_key: expect.any(String), dispatch_defaults: wire.dispatch_defaults,
    });
  });

  it("settings_live_maps_the_wire_to_limits_readings_and_a_hold_and_defaults_the_feed_to_the_sampler", async () => {
    const fake = new FakeDaemon();
    fake.stub("settings.get", WIRE);
    const result = await (await settingsOf(fake)).get();
    expect(result).toMatchObject({ status: "ok", provenance: { kind: "daemon", id: "D17", methods: ["settings.get", "settings.set"] } });
    expect(result.status === "ok" && result.data).toEqual({
      limits: { cpu: 80, memory: 85, disk: 100 },
      gate: {
        state: "closed",
        hold: { meter: "cpu", meanPercent: 91, limitPercent: 80, reopenPercent: 70 },
        readings: {
          cpu: { state: "mean", meanPercent: 91 },
          memory: { state: "unknown", reason: "warming_up" },
          disk: { state: "off" },
        },
      },
      feed: "sampler",
    });
  });

  it("settings_live_reads_a_meter_the_daemon_did_not_list_as_unknown_never_as_zero", async () => {
    const fake = new FakeDaemon();
    fake.stub("settings.get", { ...WIRE, gate: { state: "open", hold: null, meters: [{ meter: "cpu", state: "mean", mean_percent: 10 }] }, feed: "disabled_by_environment" });
    const result = await (await settingsOf(fake)).get();
    expect(result.status === "ok" && result.data.gate).toMatchObject({
      hold: null,
      readings: { memory: { state: "unknown", reason: "unavailable" }, disk: { state: "unknown", reason: "unavailable" } },
    });
    expect(result.status === "ok" && result.data.feed).toBe("disabled_by_environment");
  });

  it("settings_live_reads_a_daemon_without_the_settings_capability_as_unsupported_and_sends_nothing", async () => {
    const fake = new FakeDaemon();
    fake.capabilitiesOverride = ALL_CAPABILITIES.filter((capability) => capability !== "settings");
    const result = await (await settingsOf(fake)).get();
    expect(result).toMatchObject({ status: "unsupported", provenance: { kind: "daemon", id: "D17" } });
    expect(fake.callsTo("settings.get")).toEqual([]);
  });

  it("settings_live_reads_a_lost_connection_as_unavailable_and_a_failed_call_as_unavailable", async () => {
    const down = await (await settingsOf(new FakeDaemon(), false)).get();
    expect(down).toMatchObject({ status: "unavailable", provenance: { kind: "daemon", id: "D17" } });
    const fake = new FakeDaemon();
    fake.failNext("settings.get", { code: -32603, message: "store busy" });
    expect(await (await settingsOf(fake)).get()).toMatchObject({ status: "unavailable" });
  });

  it("settings_live_save_sends_only_the_given_limits_and_returns_the_snapshot_the_daemon_answers_with", async () => {
    const fake = new FakeDaemon();
    fake.stub("settings.set", WIRE);
    const receipt = await (await settingsOf(fake)).set({ cpu: 85, memory: 90 });
    expect(receipt).toMatchObject({ simulated: false, provenance: { kind: "daemon", id: "D17" }, snapshot: { limits: { cpu: 80, memory: 85, disk: 100 } } });
    expect(fake.callsTo("settings.set")).toEqual([
      { method: "settings.set", params: { idempotency_key: expect.any(String), load: { cpu_percent: 85, memory_percent: 90 } } },
    ]);
  });

  it("settings_live_save_rejects_when_the_daemon_refuses_it_and_mints_a_new_key_for_the_next_try", async () => {
    const fake = new FakeDaemon();
    fake.failNext("settings.set", { code: -32012, message: "approver required" });
    fake.stub("settings.set", WIRE);
    const settings = await settingsOf(fake);
    await expect(settings.set({ cpu: 85 })).rejects.toMatchObject({ kind: "forbidden" });
    await settings.set({ cpu: 85 });
    const keys = fake.callsTo("settings.set").map((call) => (call.params as { idempotency_key: string }).idempotency_key);
    expect(keys).toHaveLength(2);
    expect(keys[0]).not.toBe(keys[1]);
  });
});
