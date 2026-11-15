import { describe, expect, it } from "vitest";
import localToolsFixture from "../../fixtures/ui/local-tools.json";
import { LOCAL_TOOLS_FIXTURE_STATES, createFixtureSurfaces, type LocalToolsFixtureState } from "./fixtures";
import { clock } from "./testkit";
import type { LocalToolsSnapshot } from "./types";

const UNAVAILABLE_READER = [
  "not_installed",
  "node_missing",
  "node_too_old",
  "unsupported_version",
  "integrity_mismatch",
  "timeout",
  "failed",
  "bad_output",
  "output_too_large",
  "wrong_day",
  "not_bundled",
] as const;

async function snapshotFor(state: LocalToolsFixtureState): Promise<LocalToolsSnapshot> {
  const result = await createFixtureSurfaces({ localTools: state, now: clock }).localTools.local();
  if (result.status !== "ok") throw new Error(`expected ok for ${state}, got ${result.status}`);
  return result.data;
}

describe("local-tools fixtures", () => {
  it("has one fixture per listed state, and the list matches the file", () => {
    expect(Object.keys(localToolsFixture.states).sort()).toEqual([...LOCAL_TOOLS_FIXTURE_STATES].sort());
  });

  it("is labelled PLACEHOLDER DATA and holds no real paths", () => {
    expect(localToolsFixture.note).toContain("PLACEHOLDER DATA");
    expect(JSON.stringify(localToolsFixture)).not.toMatch(/\/Users\/|\/home\/|[A-Za-z]:\\\\/);
  });

  it.each(LOCAL_TOOLS_FIXTURE_STATES.map((state) => [state]))("serves %s with fixture provenance, through the live parser", async (state) => {
    const result = await createFixtureSurfaces({ localTools: state, now: clock }).localTools.local();
    expect(result).toMatchObject({ status: "ok", provenance: { kind: "fixture", id: "D12", fixture: "local-tools" } });
  });

  it("covers the local day of its own zone, starting at that zone's midnight", async () => {
    const off = await snapshotFor("ok");
    expect(off.window.timezone).toBe("America/Denver");
    const clockInZone = new Intl.DateTimeFormat("en-US", { timeZone: "America/Denver", hour: "2-digit", minute: "2-digit", hourCycle: "h23" });
    expect(clockInZone.format(new Date(off.window.startMs))).toBe("00:00");
    expect(off.window.endMs - off.window.startMs).toBeGreaterThanOrEqual(23 * 3_600_000);
    expect(off.window.endMs - off.window.startMs).toBeLessThanOrEqual(25 * 3_600_000);
  });

  it("hides every figure when opted out", async () => {
    const off = await snapshotFor("off");
    expect(off).toMatchObject({ enabled: false, figures: null, observedAtMs: null });
    expect(off.providers.every((provider) => provider.state === "not_run")).toBe(true);
  });

  it("shows not_started as checking, with no figures yet", async () => {
    const started = await snapshotFor("not_started");
    expect(started).toMatchObject({ enabled: true, figures: null, reader: { state: "not_started" } });
  });

  it("gives every provider a figure in the ok state, all priced", async () => {
    const ok = await snapshotFor("ok");
    expect(ok.figures).not.toBeNull();
    expect(ok.providers.map((provider) => provider.state)).toEqual(["ok", "ok", "ok"]);
    expect(ok.providers.every((provider) => provider.costComplete && provider.unpricedModels === 0)).toBe(true);
  });

  it("names one unpriced model in ok_unpriced, and keeps it out of the estimate", async () => {
    const unpriced = await snapshotFor("ok_unpriced");
    const copilot = unpriced.providers.find((provider) => provider.id === "github-copilot")!;
    expect(copilot).toMatchObject({ unpricedModels: 1, costComplete: false, estimatedUsd: 0.005 });
    expect(unpriced.providers.filter((provider) => provider.costComplete).map((provider) => provider.id)).toEqual(["claude-code", "codex"]);
  });

  it("reports calls as none, not as a dollar figure, in no_calls", async () => {
    const none = await snapshotFor("no_calls");
    expect(none.providers.every((provider) => provider.state === "no_calls" && provider.estimatedUsd === null)).toBe(true);
  });

  it("marks stale figures with their age", async () => {
    const stale = await snapshotFor("stale");
    expect(stale.stale).toBe(true);
    expect(stale.observedAtMs).toBe(clock() - 1_380_000);
    expect(stale.figures).not.toBeNull();
  });

  it.each(UNAVAILABLE_READER.map((state) => [state]))("gives %s no figures, since none were read", async (state) => {
    const snapshot = await snapshotFor(state);
    expect(snapshot).toMatchObject({ enabled: true, figures: null, observedAtMs: null, reader: { state } });
    expect(snapshot.providers.every((provider) => provider.state === "not_run")).toBe(true);
  });

  it("turns on to ok_unpriced, and off hides every figure at once", async () => {
    const surfaces = createFixtureSurfaces({ now: clock });
    const on = await surfaces.localTools.setEnabled(true);
    expect(on.status === "ok" && on.data).toMatchObject({ enabled: true, reader: { state: "ok" } });
    expect(on.status === "ok" && on.data.providers.find((p) => p.id === "github-copilot")!.unpricedModels).toBe(1);
    const off = await surfaces.localTools.setEnabled(false);
    expect(off.status === "ok" && off.data).toMatchObject({ enabled: false, figures: null });
    expect(await surfaces.localTools.local()).toMatchObject({ status: "ok", data: { enabled: false } });
  });

  it("starts from the state it is given, so a screenshot can open any state", async () => {
    const surfaces = createFixtureSurfaces({ localTools: "wrong_day", now: clock });
    expect(await surfaces.localTools.local()).toMatchObject({ status: "ok", data: { enabled: true, reader: { state: "wrong_day" } } });
  });

  it("gives each reader its source: PATH for the development install, none when missing, the bundle for release", async () => {
    expect((await snapshotFor("not_installed")).reader).toMatchObject({ source: null });
    expect((await snapshotFor("unsupported_version")).reader).toMatchObject({ source: "path" });
    expect((await snapshotFor("ok")).reader).toMatchObject({ source: "bundled" });
    expect((await snapshotFor("not_bundled")).reader).toMatchObject({ state: "not_bundled", source: null });
  });

  it("reports an unavailable scenario as unavailable, never with figures", async () => {
    const result = await createFixtureSurfaces({ scenario: "unavailable", now: clock }).localTools.local();
    expect(result).toMatchObject({ status: "unavailable", provenance: { kind: "fixture" } });
    expect(result).not.toHaveProperty("data");
  });
});
