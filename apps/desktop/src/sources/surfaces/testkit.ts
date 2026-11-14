/** Test-only helpers shared by the D9-D16 test files. Not imported by app code. */
import { ConnectionManager } from "../../daemon/connection";
import { FakeDaemon, mkApproval, mkTask, until } from "../../test/fakeDaemon";
import { installFakeWorkstreams, type FakeWorkstreams } from "../../test/fakeWorkstreams";
import { createDerivedSurfaces } from "./derived";
import type { SurfaceSources } from "./types";

export const NOW = Date.parse("2026-01-01T12:00:00Z");
export const clock = () => NOW;

export const ALPHA = "/repos/alpha";
export const BETA = "/repos/beta";

/** The stateful `workstreams.*` fake installed by `populate`, for tests that inspect or seed it. */
export const WORKSTREAM_STATE = new WeakMap<FakeDaemon, FakeWorkstreams>();

/**
 * Two projects, four tasks (one unknown-project task is added by tests that need it), two approvals. */
export function populate(fake: FakeDaemon): FakeDaemon {
  fake.projects = [{ id: ALPHA }, { id: BETA }];
  fake.tasks = [
    mkTask("t-run", ALPHA, { state: "Running" }),
    mkTask("t-wait", ALPHA, { state: "Queued", contract: { dependencies: ["t-run"], objective: "Document the session model" } }),
    mkTask("t-free", BETA, { state: "Queued" }),
    mkTask("t-done", BETA, { state: "Succeeded" }),
  ];
  fake.approvals = [mkApproval("a-1", "t-run"), mkApproval("a-2", "t-done", "approved")];
  fake.stub("host.metrics", HOST_METRICS_WIRE);
  // ws-1 spans both projects and has one objective with a verified and a pending gate;
  // ws-2 is suspended and empty; t-free is linked to nothing.
  WORKSTREAM_STATE.set(
    fake,
    installFakeWorkstreams(fake, {
      workstreams: [
        { id: "ws-1", title: "Session hardening", summary: "Make sessions durable", state: "active", revision: 2 },
        { id: "ws-2", title: "Docs refresh", summary: "", state: "suspended", revision: 1 },
      ],
      objectives: [
        {
          id: "obj-1",
          workstream_id: "ws-1",
          title: "Durable sessions",
          description: "Sessions survive restarts",
          revision: 3,
          gates: [
            { id: "gate-1", description: "Restart test passes", state: "verified", evidence_task_id: "t-done" },
            { id: "gate-2", description: "Docs updated", state: "pending", evidence_task_id: null },
          ],
        },
      ],
      links: [
        { task_id: "t-run", workstream_id: "ws-1", objective_id: "obj-1" },
        { task_id: "t-wait", workstream_id: "ws-1", objective_id: "obj-1" },
        { task_id: "t-done", workstream_id: "ws-1", objective_id: null },
      ],
    }),
  );
  fake.stub("agents.capabilities", [
    {
      agent: "codex",
      installed: true,
      launchable: true,
      capabilities: {
        streaming: true,
        interactive_input: false,
        structured_output: true,
        cancellation: true,
        session_resumption: false,
        approvals: false,
        model_selection: true,
      },
    },
    { agent: "pi", installed: false, launchable: false, capabilities: null },
  ]);
  fake.stub("coordinator.get", (params: Record<string, unknown>) => ({
    project_id: params.project_id,
    entries: [
      {
        sequence: 1,
        id: "c-1",
        project_id: params.project_id,
        kind: "message",
        author: "user",
        body: "hello",
        validation: null,
        created_at: "2026-01-01T09:00:00Z",
      },
    ],
    more: false,
    next_after_sequence: 1,
  }));
  fake.stub("coordinator.send_message", (params: Record<string, unknown>) => ({
    sequence: 2,
    id: "c-2",
    project_id: params.project_id,
    kind: "message",
    author: "user",
    body: params.text,
    validation: null,
    created_at: "2026-01-01T09:05:00Z",
  }));
  return fake;
}

/** A `host.metrics` result observed 2 s before NOW: every meter ok except GPU (unsupported). */
export const HOST_METRICS_WIRE = {
  schema_version: 1,
  sample_interval_ms: 2000,
  meters: [
    { meter: "cpu", state: "ok", kind: "utilization", unit: "percent", scope: "all logical CPUs", value: 12.5, total: null, observed_at: "2026-01-01T11:59:58.000Z", sample_interval_ms: 2000, reason: null },
    { meter: "memory", state: "ok", kind: "capacity", unit: "bytes", scope: "physical memory", value: 8e9, total: 32e9, observed_at: "2026-01-01T11:59:58.000Z", sample_interval_ms: 2000, reason: null },
    { meter: "disk", state: "ok", kind: "capacity", unit: "bytes", scope: "/", value: 100e9, total: 500e9, observed_at: "2026-01-01T11:59:58.000Z", sample_interval_ms: 2000, reason: null },
    { meter: "gpu", state: "unsupported", kind: "utilization", unit: "percent", scope: null, value: null, total: null, observed_at: null, sample_interval_ms: 2000, reason: "No vetted GPU source on this platform." },
  ],
};

export const READ_METHODS = [
  "workstreams.list",
  "tasks.list",
  "host.metrics",
  "approvals.list",
  "queue.snapshot",
  "agents.capabilities",
  "coordinator.get",
] as const;

export async function connect(fake: FakeDaemon, start = true) {
  const connection = new ConnectionManager({ connector: fake, backoffMs: [5], requestTimeoutMs: 100 });
  const surfaces: SurfaceSources = createDerivedSurfaces(connection, clock);
  if (start) {
    connection.start();
    await until(() => connection.getSnapshot().status === "connected", "connected");
  }
  return { connection, surfaces, stop: () => connection.stop() };
}
