import { describe, expect, it } from "vitest";
import derivedSource from "./derived.ts?raw";
import fixturesSource from "./fixtures.ts?raw";
import resultSource from "./result.ts?raw";
import searchSource from "./search.ts?raw";
import { createFixtureSurfaces } from "./fixtures";
import { clock } from "./testkit";
import type { SurfaceFixtureScenario } from "./fixtures";

/** Module specifiers of non-type imports; type-only imports are erased and reach nothing at runtime. */
function runtimeImports(source: string): string[] {
  return [...source.matchAll(/^import\s+(?!type\b)[^;]*?from\s+["']([^"']+)["']/gm)].map((match) => match[1]!);
}

describe("fixture surfaces (D9-D16)", () => {
  it("labels every source fixture and keeps wireframe sample numbers in fixture mode only", async () => {
    const s = createFixtureSurfaces({ now: clock });
    for (const source of [s.workstreams, s.conversation, s.activity, s.usage, s.metrics, s.models, s.search, s.decisions]) {
      expect(source.provenance.kind).toBe("fixture");
    }
    const activity = await s.activity.activity();
    expect(activity.status === "ok" && activity.data.slots).toMatchObject({ status: "ok", value: { used: 20, limit: 24 } });
    const usage = await s.usage.usage();
    expect(usage.status === "ok" && [usage.data.reported, usage.data.estimated]).toMatchObject([
      { status: "ok", value: 26 },
      { status: "ok", value: 6 },
    ]);
    const metrics = await s.metrics.metrics();
    expect(metrics.status === "ok" && metrics.data.cpu).toMatchObject({ status: "ok", value: { value: 44 } });
    expect(metrics.status === "ok" && metrics.data.gpu.status).toBe("unsupported");
  });

  it("stamps observedAt from the injected clock minus the fixture age", async () => {
    const s = createFixtureSurfaces({ now: clock });
    const usage = await s.usage.usage();
    expect(usage).toMatchObject({ status: "ok", ageMs: 60_000, observedAt: new Date(clock() - 60_000).toISOString() });
    if (usage.status === "ok" && usage.data.reported.status === "ok") {
      expect(usage.data.reported.observedAt).toBe(new Date(clock() - 60_000).toISOString());
    }
  });

  it("mints only fixture: ids for records and receipts", async () => {
    const s = createFixtureSurfaces({ now: clock });
    const ids: string[] = [];
    const ws = await s.workstreams.workstreams();
    if (ws.status === "ok") for (const w of ws.data) ids.push(w.id, ...w.projectIds, ...w.tasks.map((t) => t.id));
    const dq = await s.decisions.queue();
    if (dq.status === "ok") for (const item of dq.data.items) ids.push(item.id, item.taskId, item.projectId!);
    const act = await s.activity.activity();
    if (act.status === "ok") {
      for (const e of [...act.data.running, ...act.data.queued]) ids.push(e.taskId, e.projectId);
      for (const a of act.data.attention) ids.push(a.approvalId, a.taskId, a.projectId!);
    }
    const receipts = [
      await s.workstreams.simulateCreate("x"),
      await s.activity.simulatePauseDispatch(),
      await s.activity.simulateStop("fixture:task-auth-2"),
      await s.decisions.simulatePostpone("fixture:approval-1"),
      await s.decisions.simulateDismiss("fixture:approval-1"),
    ];
    for (const receipt of receipts) {
      expect(receipt).toMatchObject({ simulated: true, provenance: { kind: "fixture" } });
      ids.push(receipt.id);
    }
    const send = await s.conversation.send({ kind: "global" }, "hi");
    expect(send).toMatchObject({ simulated: true, provenance: { kind: "fixture", id: "D10" } });
    ids.push(send.entryId);
    expect(ids.length).toBeGreaterThan(20);
    for (const id of ids) expect(id.startsWith("fixture:"), id).toBe(true);
  });

  it("serves the global and project transcripts separately", async () => {
    const s = createFixtureSurfaces({ now: clock });
    const global = await s.conversation.transcript({ kind: "global" });
    const project = await s.conversation.transcript({ kind: "project", projectId: "fixture:project-alpha" });
    expect(global.status === "ok" && global.data.entries.map((e) => e.id)).toEqual(["fixture:msg-1", "fixture:msg-2"]);
    expect(project.status === "ok" && project.data.entries.map((e) => e.kind)).toEqual(["message", "proposal"]);
  });

  it("shows an unknown objective and an unsupported GPU rather than inventing values", async () => {
    const s = createFixtureSurfaces({ now: clock });
    const ws = await s.workstreams.workstreams();
    expect(ws.status === "ok" && ws.data.find((w) => w.id === "fixture:ws-infra")?.objective.status).toBe("unknown");
    const models = await s.models.catalog();
    expect(models.status === "ok" && models.data.providers.map((p) => p.modelSelection)).toEqual(["supported", "unknown"]);
  });

  it.each(["unavailable", "unsupported", "unknown"] as SurfaceFixtureScenario[])(
    "reports the %s scenario with a reason and no data",
    async (scenario) => {
      const s = createFixtureSurfaces({ scenario, now: clock });
      const results = await Promise.all([s.workstreams.workstreams(), s.activity.activity(), s.decisions.queue()]);
      for (const result of results) {
        expect(result).toMatchObject({ status: scenario, provenance: { kind: "fixture" } });
        expect(result).not.toHaveProperty("data");
      }
    },
  );

  it("never resolves in the loading scenario", async () => {
    const s = createFixtureSurfaces({ scenario: "loading", now: clock });
    const outcome = await Promise.race([
      s.workstreams.workstreams().then(() => "resolved"),
      s.search.search("auth").then(() => "resolved"),
      new Promise((resolve) => setTimeout(() => resolve("pending"), 30)),
    ]);
    expect(outcome).toBe("pending");
  });

  it("imports no daemon client at runtime", () => {
    for (const source of [fixturesSource, searchSource, resultSource]) {
      for (const specifier of runtimeImports(source)) expect(specifier).not.toMatch(/daemon/);
    }
  });

  it("derived adapters import no fixtures at runtime", () => {
    const specifiers = runtimeImports(derivedSource);
    expect(specifiers.length).toBeGreaterThan(0);
    for (const specifier of specifiers) {
      expect(specifier).not.toMatch(/fixtures/);
      expect(specifier).not.toMatch(/\.json$/);
    }
  });
});
