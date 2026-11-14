// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { DaemonApi } from "../src/daemon/api";
import { DaemonSession } from "../src/daemon/session";
import { M5_03_CAPABILITIES } from "../src/daemon/types";
import { createServices } from "../src/ui/createServices";
import { ServicesProvider } from "../src/ui/services";
import { WorkspaceView } from "../src/ui/views/workspace/WorkspaceView";
import { NodeConnector, fakeAgentEnv, noAgentsEnv, startDaemon } from "./harness";

afterEach(cleanup);

async function until<T>(read: () => T | undefined | false | Promise<T | undefined | false>, label: string, ms = 15000): Promise<T> {
  const deadline = Date.now() + ms;
  for (;;) {
    const v = await read();
    if (v) return v as T;
    if (Date.now() > deadline) throw new Error(`timeout ${label}`);
    await new Promise((r) => setTimeout(r, 50));
  }
}

async function prepare(env: Record<string, string> = noAgentsEnv()) {
  const daemon = await startDaemon(env);
  const connector = new NodeConnector(daemon.dataDir);
  const direct = async <T,>(fn: (api: DaemonApi) => Promise<T>): Promise<T> => {
    const s = new DaemonSession(await connector.open(M5_03_CAPABILITIES));
    try {
      return await fn(new DaemonApi(s));
    } finally {
      s.close();
    }
  };
  const services = createServices({ connector, connection: { backoffMs: [50], requestTimeoutMs: 8000 } });
  services.connection.start();
  await until(() => services.connection.getSnapshot().status === "connected", "connected");
  const stop = async () => {
    cleanup();
    services.connection.stop();
    await daemon.stop();
  };
  return { daemon, services, direct, stop };
}

// With no installed agent the daemon's runtime fails closed with `runtime_unavailable`. These
// tests prove the UI reports exactly that and never produces a reply of its own. The tests after
// them use the deterministic fake agent (`examples/fake_agent`) as the only installed agent.
describe("Workspace conversation and model selector over the real daemon", () => {
  it("sends to Global, records it, and shows the runtime-unavailable state with no reply", async () => {
    const t = await prepare();
    try {
      render(
        <ServicesProvider services={t.services}>
          <WorkspaceView />
        </ServicesProvider>,
      );
      const heading = await screen.findByRole("heading", { name: "Talking to: Global orchestrator" }, { timeout: 15000 });
      expect(heading.parentElement?.textContent).not.toContain("PLACEHOLDER");
      const box = screen.getByRole("textbox", { name: "Message Global orchestrator" });
      await userEvent.type(box, "hello real daemon");
      const sendButton = () => screen.getByRole("button", { name: "Send message to Global orchestrator" });
      await waitFor(() => expect((sendButton() as HTMLButtonElement).disabled).toBe(false), { timeout: 15000 });
      await userEvent.click(sendButton());

      expect((await screen.findAllByText(/Orchestrator runtime unavailable/, undefined, { timeout: 15000 })).length).toBeGreaterThan(0);
      expect(await screen.findByText("hello real daemon")).toBeTruthy();
      expect(screen.queryByText(/SIMULATED/)).toBeNull();

      const page = await t.direct((api) => api.conversationsGet({ kind: "global" }));
      expect(page.entries.map((e) => e.kind)).toEqual(["user_message", "turn_failed"]);
      expect(page.entries[1]).toMatchObject({ error_code: "runtime_unavailable", body: "" });
      expect(page.entries.some((e) => e.kind === "reply_chunk")).toBe(false);
      expect(page.turn).toMatchObject({ state: "failed", error_code: "runtime_unavailable" });
      await waitFor(() => expect((box as HTMLTextAreaElement).value).toBe(""));
    } finally {
      await t.stop();
    }
  }, 60000);

  it("keeps the recorded transcript across a daemon restart and sends only real ids", async () => {
    const t = await prepare();
    try {
      const { surfaces } = t.services;
      const receipt = await surfaces.conversation.send({ kind: "global" }, "persist me");
      expect(receipt).toMatchObject({ simulated: false, turn: { state: "failed", errorCode: "runtime_unavailable" } });
      await t.daemon.restart();
      const result = await until(async () => {
        const r = await surfaces.conversation.transcript({ kind: "global" });
        return r.status === "ok" ? r : undefined;
      }, "transcript after restart");
      expect(result.data.entries.map((e) => [e.kind, e.role])).toEqual([
        ["message", "user"],
        ["status", "system"],
      ]);
      expect(result.data.entries[0]!.text).toBe("persist me");
      expect(result.data.turn).toMatchObject({ state: "failed", errorCode: "runtime_unavailable" });
      await expect(
        surfaces.conversation.send({ kind: "workstream", workstreamId: "fixture:ws-auth" }, "nope"),
      ).rejects.toMatchObject({ kind: "invalid-params" });
    } finally {
      await t.stop();
    }
  }, 60000);

  it("reports an unknown workstream as unavailable, not as an empty conversation", async () => {
    const t = await prepare();
    try {
      const result = await t.services.surfaces.conversation.transcript({ kind: "workstream", workstreamId: "ws-not-there" });
      expect(result.status).toBe("unavailable");
      if (result.status === "unavailable") expect(result.reason).toMatch(/workstream lookup/);
      await expect(
        t.services.surfaces.conversation.send({ kind: "workstream", workstreamId: "ws-not-there" }, "x"),
      ).rejects.toMatchObject({ kind: "not-found" });
    } finally {
      await t.stop();
    }
  }, 60000);

  it("binds the real catalog and checks selections with agents.validate_selection", async () => {
    const t = await prepare({});
    try {
      const catalog = await until(async () => {
        const r = await t.services.surfaces.models.catalog();
        return r.status === "ok" ? r : undefined;
      }, "catalog");
      expect(catalog.provenance).toMatchObject({ kind: "daemon", id: "D14" });
      const ids = catalog.data.providers.map((p) => p.id);
      expect(ids).toEqual(expect.arrayContaining(["codex", "claude-code", "pi"]));
      // The catalog is cold at first ("retry shortly") and depends on which agents are installed
      // here, so only the honest-state contract is asserted: every facet is a measured value or a
      // named non-ok status, never an invented list.
      for (const provider of catalog.data.providers) {
        for (const facet of [provider.models, provider.efforts]) {
          if (facet.status === "ok") expect(Array.isArray(facet.value)).toBe(true);
          else expect(facet.reason.length).toBeGreaterThan(0);
        }
      }
      const pi = catalog.data.providers.find((p) => p.id === "pi")!;
      expect(pi.models.status).not.toBe("ok");

      // The daemon rejects an empty selection; the adapter never asks.
      expect((await t.services.surfaces.models.validate!("codex", {})).status).toBe("unknown");
      const verdict = await t.services.surfaces.models.validate!("codex", { model: "definitely-not-a-model" });
      expect(verdict.status).toBe("ok");
      if (verdict.status === "ok") expect(verdict.data.valid).toBe(false);
      const raw = await t.direct((api) => api.agentsCatalog());
      expect(raw.map((r) => r.agent)).toEqual(expect.arrayContaining(["codex", "claude-code", "pi"]));
    } finally {
      await t.stop();
    }
  }, 60000);
});

// M7-25: the fake agent is the only installed agent, pinned at its own version.
describe("Workspace conversation with a launchable agent (deterministic fake)", () => {
  /** The first send can race the daemon's cold agent catalog and is then refused as a conflict. */
  async function sendWhenReady(t: Awaited<ReturnType<typeof prepare>>, target: Parameters<typeof t.services.surfaces.conversation.send>[0], text: string) {
    return until(async () => {
      try {
        return await t.services.surfaces.conversation.send(target, text);
      } catch (error) {
        if ((error as { kind?: string }).kind === "conflict") return undefined;
        throw error;
      }
    }, "send accepted");
  }

  async function settled(t: Awaited<ReturnType<typeof prepare>>, target: Parameters<typeof t.services.surfaces.conversation.transcript>[0]) {
    return until(async () => {
      const r = await t.services.surfaces.conversation.transcript(target);
      return r.status === "ok" && r.data.turn && r.data.turn.state !== "running" ? r : undefined;
    }, "turn settled");
  }

  it("shows the agent's reply in the UI, with no task, approval or simulated text", async () => {
    const t = await prepare(fakeAgentEnv());
    try {
      // A cold agent catalog makes the first send a retryable conflict; warm it first.
      await until(async () => (await t.direct((api) => api.agentsCatalog())).some((c) => c.agent === "codex" && c.state === "available"), "catalog warm");
      render(
        <ServicesProvider services={t.services}>
          <WorkspaceView />
        </ServicesProvider>,
      );
      await screen.findByRole("heading", { name: "Talking to: Global orchestrator" }, { timeout: 15000 });
      const box = screen.getByRole("textbox", { name: "Message Global orchestrator" });
      await userEvent.type(box, "hello fake agent");
      const sendButton = () => screen.getByRole("button", { name: "Send message to Global orchestrator" });
      await waitFor(() => expect((sendButton() as HTMLButtonElement).disabled).toBe(false), { timeout: 15000 });
      await userEvent.click(sendButton());

      expect(await screen.findByText("fake reply to: hello fake agent", undefined, { timeout: 20000 })).toBeTruthy();
      expect(screen.queryByText(/Orchestrator runtime unavailable/)).toBeNull();
      expect(screen.queryByText(/SIMULATED/)).toBeNull();

      const page = await until(async () => {
        const current = await t.direct((api) => api.conversationsGet({ kind: "global" }));
        return current.turn?.state === "completed" && current;
      }, "completed conversation turn");
      expect(page.entries.map((e) => e.kind)).toEqual(["user_message", "reply_chunk", "turn_completed"]);
      expect(page.turn).toMatchObject({ state: "completed", runtime: { name: "codex", version: "0.0.0-fake" } });
      expect(await t.direct((api) => api.tasksList())).toEqual([]);
      expect(await t.direct((api) => api.approvalsList())).toEqual([]);
    } finally {
      await t.stop();
    }
  }, 90000);

  it("keeps ordered replies across a daemon restart and answers a workstream conversation", async () => {
    const t = await prepare(fakeAgentEnv());
    try {
      const { conversation } = t.services.surfaces;
      const global = { kind: "global" } as const;
      await sendWhenReady(t, global, "one");
      await settled(t, global);
      await conversation.send(global, "two");
      const before = await settled(t, global);
      expect(before.data.entries.map((e) => [e.role, e.text])).toEqual([
        ["user", "one"],
        ["coordinator", "fake reply to: one"],
        ["user", "two"],
        ["coordinator", "fake reply to: two"],
      ]);

      await t.daemon.restart();
      const after = await until(async () => {
        const r = await conversation.transcript(global);
        return r.status === "ok" ? r : undefined;
      }, "transcript after restart");
      expect(after.data.entries.map((e) => [e.role, e.text])).toEqual(before.data.entries.map((e) => [e.role, e.text]));
      expect(after.data.turn).toMatchObject({ state: "completed" });

      const workstream = await t.direct((api) => api.workstreamsCreate("Auth rewrite", "Replace session cookies", "e2e-ws-1"));
      const target = { kind: "workstream", workstreamId: workstream.id } as const;
      await sendWhenReady(t, target, "[fake:echo]");
      const echoed = await settled(t, target);
      const reply = echoed.data.entries.find((e) => e.role === "coordinator")!.text;
      expect(reply).toContain("Scope: one workstream.");
      expect(reply).toContain("Auth rewrite");
      const globalAgain = await settled(t, global);
      expect(globalAgain.data.entries).toHaveLength(4);
    } finally {
      await t.stop();
    }
  }, 120000);

  it("turns an agent failure into a named status, never reply text", async () => {
    const t = await prepare(fakeAgentEnv());
    try {
      const global = { kind: "global" } as const;
      await sendWhenReady(t, global, "[fake:fail]");
      const result = await settled(t, global);
      expect(result.data.turn).toMatchObject({ state: "failed", errorCode: "runtime_failed" });
      expect(result.data.entries.map((e) => e.kind)).toEqual(["message", "status"]);
      expect(result.data.entries.some((e) => e.role === "coordinator")).toBe(false);
    } finally {
      await t.stop();
    }
  }, 90000);
});
