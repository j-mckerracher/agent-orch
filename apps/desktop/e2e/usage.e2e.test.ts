import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { chmodSync, mkdtempSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { DaemonApi } from "../src/daemon/api";
import { ConnectionManager } from "../src/daemon/connection";
import { DaemonSession } from "../src/daemon/session";
import { createLiveUsageSource, type LocalDay } from "../src/sources/surfaces/usageLive";
import type { TaskSubmission } from "../src/daemon/types";
import { NodeConnector, startDaemon, workRoot, type RunningDaemon } from "./harness";

/** A recorded real Claude Code stream whose result reported `total_cost_usd` 0.0086388. */
const CLAUDE_STREAM = resolve(__dirname, "../../../docs/evidence/m4-02-macos/20261107T215309-0700.stdout.jsonl");
const CODEX_USAGE_TURN =
  '{"type":"turn.completed","usage":{"input_tokens":1000,"cached_input_tokens":400,"output_tokens":50,"reasoning_output_tokens":20}}';

const sources: ConnectionManager[] = [];
const daemons: RunningDaemon[] = [];
const scriptDirs: string[] = [];

function script(dir: string, name: string, body: string): string {
  const path = join(dir, name);
  writeFileSync(path, `#!/bin/sh\n${body}\n`);
  chmodSync(path, 0o755);
  return path;
}

const submission = (projectId: string, agent: string): TaskSubmission => ({
  project_id: projectId,
  parent_id: null,
  objective: `usage e2e ${agent}`,
  kind: "Implement",
  agent,
  dependencies: [],
  verification: "None",
  delivery: "LocalReview",
  autonomy: "ApprovalRequired",
  max_retries: 0,
  max_repairs: 0,
});

async function until<T>(read: () => Promise<T | undefined | false>, label: string, ms = 25_000): Promise<T> {
  const deadline = Date.now() + ms;
  for (;;) {
    const value = await read();
    if (value) return value;
    if (Date.now() > deadline) throw new Error(`timed out waiting for ${label}`);
    await new Promise((r) => setTimeout(r, 100));
  }
}

async function liveSource(daemon: RunningDaemon, window: LocalDay) {
  const manager = new ConnectionManager({
    connector: new NodeConnector(daemon.dataDir),
    backoffMs: [50],
    requestTimeoutMs: 5000,
  });
  sources.push(manager);
  manager.start();
  await until(async () => manager.getSnapshot().status === "connected", "connected");
  return createLiveUsageSource(manager, Date.now, () => window);
}

/** A UTC window that brackets "now", so the test does not depend on the host's midnight. */
function aroundNow(): LocalDay {
  const now = Date.now();
  return { timezone: "UTC", utcOffsetSeconds: 0, startMs: now - 3_600_000, endMs: now + 3_600_000 };
}

afterAll(async () => {
  for (const manager of sources) manager.stop();
  for (const daemon of daemons) await daemon.stop();
  for (const dir of scriptDirs) rmSync(dir, { recursive: true, force: true });
  delete process.env.AGENT_ORCH_CODEX_PROGRAM;
  delete process.env.AGENT_ORCH_CLAUDE_PROGRAM;
});

describe("real daemon: usage.summary through the live D12 adapter", () => {
  let reporting: RunningDaemon;
  let idle: RunningDaemon;

  beforeAll(async () => {
    // The idle daemon starts first, so it never sees the fake agents.
    idle = await startDaemon();
    daemons.push(idle);

    const dir = mkdtempSync(join(workRoot(), "s-"));
    scriptDirs.push(dir);
    const codex = script(dir, "codex-agent", [
      `printf '{"type":"thread.started","thread_id":"01a1b721-1f91-7393-ae1d-6aace8e4646a"}\\n'`,
      `printf '{"type":"turn.started"}\\n'`,
      `printf '%s\\n' '${CODEX_USAGE_TURN}'`,
    ].join("\n"));
    const claude = script(dir, "claude-agent", `cat '${CLAUDE_STREAM}'`);
    process.env.AGENT_ORCH_CODEX_PROGRAM = codex;
    process.env.AGENT_ORCH_CLAUDE_PROGRAM = claude;
    reporting = await startDaemon();
    daemons.push(reporting);

    const session = new DaemonSession(await new NodeConnector(reporting.dataDir).open(["projects", "tasks", "agents"]));
    const api = new DaemonApi(session);
    const projectId = realpathSync(reporting.repo);
    await api.projectsRegister(reporting.repo);
    for (const agent of ["codex", "claude-code"]) {
      const task = await api.tasksSubmit(`usage-e2e-${agent}`, submission(projectId, agent));
      await until(async () => {
        const state = (await api.tasksGet(task.contract.id)).state;
        if (state === "Failed") throw new Error(`${agent} task failed: ${reporting.stderr()}`);
        return state === "Verifying";
      }, `${agent} task to finish`);
    }
    session.close();
  });

  it("reports unknown, never zero, when the daemon has recorded nothing", async () => {
    const source = await liveSource(idle, aroundNow());
    const result = await source.usage();
    expect(result).toMatchObject({ status: "ok", provenance: { kind: "daemon", methods: ["usage.summary"] } });
    if (result.status !== "ok") return;
    expect(result.data.reported.status).toBe("unknown");
    expect(result.data.estimated.status).toBe("unknown");
    expect(result.data.budget.status).toBe("unsupported");
    expect(result.data.coverage).toEqual([]);
    expect(result.data.currency).toBe("unknown");
    expect(JSON.stringify(result)).not.toMatch(/26\.00|fixture/i);
  });

  it("maps a provider-reported Claude cost and a Codex token-only session honestly", async () => {
    const source = await liveSource(reporting, aroundNow());
    const result = await source.usage();
    expect(result.status).toBe("ok");
    if (result.status !== "ok") return;
    const { data } = result;
    expect(data.timezone).toBe("UTC");
    expect(data.currency).toBe("USD");
    expect(data.reported).toMatchObject({ status: "ok" });
    if (data.reported.status === "ok") expect(data.reported.value).toBeCloseTo(0.0086388, 7);
    // The daemon produces no estimates, and Codex reports tokens but never a cost.
    expect(data.estimated.status).toBe("unknown");
    expect(data.budget.status).toBe("unsupported");
    const coverage = Object.fromEntries(data.coverage.map((c) => [c.provider, c.reporting]));
    expect(coverage["claude-code"]).toBe("provider-reported");
    expect(coverage["codex"]).toBe("tokens-only");
    expect(JSON.stringify(result)).not.toMatch(/26\.00|fixture/i);
  });

  it("never serves fixtures when the daemon is gone", async () => {
    const source = await liveSource(idle, aroundNow());
    await idle.halt();
    daemons.splice(daemons.indexOf(idle), 1);
    rmSync(idle.workDir, { recursive: true, force: true });
    const result = await source.usage();
    expect(result.status).not.toBe("ok");
    expect(result.provenance.kind).toBe("daemon");
  });
});
