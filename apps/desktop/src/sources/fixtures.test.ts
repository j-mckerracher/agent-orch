import { describe, expect, it } from "vitest";
import fixturesSource from "./fixtures.ts?raw";
import { createFixtureSources, type FixtureScenario } from "./fixtures";
import type { TaskRef } from "./types";

const real = (state: Parameters<typeof Object>[0]): TaskRef => ({ origin: "real", taskId: "task-1", state: state as never });

describe("fixture sources (D1-D6)", () => {
  it("returns labelled placeholder data for every source", async () => {
    const s = createFixtureSources();
    const sources = [s.agents, s.execution, s.recovery, s.retry, s.coordinator, s.journal];
    expect(sources.map((x) => x.provenance.id)).toEqual(["D1", "D2", "D3", "D4", "D5", "D6"]);
    for (const source of sources) expect(source.provenance.kind).toBe("fixture");
    const agents = await s.agents.listAgents();
    expect(agents.state).toBe("ready");
    expect(agents.state === "ready" && agents.data.map((a) => a.availability)).toEqual([
      "installed",
      "available",
      "unsupported",
      "unknown",
    ]);
  });

  it.each(["empty", "unavailable", "error"] as FixtureScenario[])("honours the %s scenario", async (scenario) => {
    const s = createFixtureSources({ scenario });
    const results = await Promise.all([
      s.agents.listAgents(),
      s.execution.availability(),
      s.recovery.recoveryFor(real("Queued")),
      s.retry.retryFor(real("Failed")),
      s.coordinator.conversation("/p"),
      s.journal.identity(),
    ]);
    for (const result of results) expect(result.state).toBe(scenario);
  });

  it("never resolves in the loading scenario", async () => {
    const s = createFixtureSources({ scenario: "loading" });
    const outcome = await Promise.race([
      s.agents.listAgents().then(() => "resolved"),
      new Promise((resolve) => setTimeout(() => resolve("pending"), 30)),
    ]);
    expect(outcome).toBe("pending");
  });

  it("mints only fixture-prefixed ids", async () => {
    const s = createFixtureSources();
    const list = await s.execution.listPreviewTasks();
    expect(list.state === "ready" && list.data.every((t) => t.id.startsWith("fixture:"))).toBe(true);
    const receipt = await s.retry.simulateRetry({ origin: "fixture", id: "fixture:task-failed" });
    expect(receipt.newAttemptId.startsWith("fixture:")).toBe(true);
  });

  it("simulated actions report that nothing happened", async () => {
    const s = createFixtureSources({ now: () => "T" });
    const launch = await s.execution.simulateLaunch("fixture:task-queued");
    expect(launch).toMatchObject({ simulated: true, at: "T" });
    expect(launch.notice).toMatch(/No agent was started/);
    expect((await s.execution.simulateCancel("fixture:task-running")).notice).toMatch(/No process was stopped/);
    expect((await s.coordinator.previewProposal("/p", "")).proposal.validation).toBe("invalid");
  });

  it("derives outcome and recovery from the real task state without calling the daemon", async () => {
    const s = createFixtureSources();
    const outcome = await s.execution.outcomeFor(real("Queued"));
    expect(outcome.state).toBe("ready");
    const recovery = await s.recovery.recoveryFor(real("Running"));
    expect(recovery.state).toBe("ready");
  });

  it("keeps the fixture layer free of daemon-client imports", () => {
    expect(fixturesSource).not.toMatch(/from "\.\.\/daemon\//);
  });
});
