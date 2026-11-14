import { afterEach, describe, expect, it } from "vitest";
import { FakeDaemon } from "../../test/fakeDaemon";
import { CLAUDE_CATALOG, CODEX_CATALOG, COLD_CATALOG, PI_CATALOG, stubValidate } from "../../test/m724Stubs";
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

function withAgents(fake: FakeDaemon) {
  const caps = (agent: string, installed: boolean) => ({
    agent,
    installed,
    launchable: installed,
    capabilities: installed
      ? {
          streaming: true,
          interactive_input: false,
          structured_output: true,
          cancellation: true,
          session_resumption: false,
          approvals: false,
          model_selection: agent !== "pi",
        }
      : null,
  });
  fake.stub("agents.capabilities", [caps("codex", true), caps("claude-code", true), caps("pi", false)]);
  return fake;
}

describe("live model catalog", () => {
  it("keeps an absent catalog row unknown after a successful catalog read", async () => {
    const fake = withAgents(populate(new FakeDaemon()));
    fake.stub("agents.catalog", []);
    const result = await (await live(fake)).models.catalog();
    expect(result.status).toBe("ok");
    if (result.status !== "ok") return;
    expect(result.data.providers.length).toBeGreaterThan(0);
    for (const provider of result.data.providers) {
      expect(provider.models).toMatchObject({ status: "unknown", reason: expect.stringContaining("no row") });
      expect(provider.efforts).toMatchObject({ status: "unknown" });
    }
  });

  it("binds per-model efforts for Codex, adapter-wide efforts for Claude, and Pi as unsupported", async () => {
    const fake = withAgents(populate(new FakeDaemon()));
    fake.stub("agents.catalog", [CODEX_CATALOG, CLAUDE_CATALOG, PI_CATALOG]);
    const result = await (await live(fake)).models.catalog();
    expect(result).toMatchObject({
      status: "ok",
      provenance: { kind: "daemon", id: "D14", methods: ["agents.capabilities", "agents.catalog", "agents.validate_selection"] },
    });
    if (result.status !== "ok") return;
    const [codex, claude, pi] = result.data.providers;
    expect(codex!.models).toMatchObject({
      status: "ok",
      value: [
        { id: "gpt-a", label: "GPT A", efforts: ["low", "high"], defaultEffort: "low" },
        { id: "gpt-b", label: "GPT B", efforts: ["medium"], defaultEffort: null },
      ],
    });
    expect(codex!.efforts.status).toBe("unsupported");
    expect(claude!.models).toMatchObject({ status: "ok", value: [{ id: "sonnet", efforts: [] }] });
    expect(claude!.efforts).toMatchObject({ status: "ok", value: ["low", "medium", "high"] });
    expect(pi!.models.status).toBe("unsupported");
    expect(pi!.efforts.status).toBe("unsupported");
    expect(pi).toMatchObject({ installed: false, launchable: false });
  });

  it("reports a cold catalog as unknown with the retry-shortly hint, not an error", async () => {
    const fake = withAgents(populate(new FakeDaemon()));
    fake.stub("agents.catalog", [COLD_CATALOG("codex"), COLD_CATALOG("claude-code"), COLD_CATALOG("pi")]);
    const result = await (await live(fake)).models.catalog();
    expect(result.status).toBe("ok");
    if (result.status !== "ok") return;
    for (const provider of result.data.providers) {
      expect(provider.models).toMatchObject({ status: "unknown" });
      expect(provider.models.status === "unknown" && provider.models.reason).toMatch(/retry shortly/);
    }
  });

  it("treats a refused catalog read (Conflict) as unknown, retry shortly", async () => {
    const fake = withAgents(populate(new FakeDaemon()));
    fake.stub("agents.catalog", [CODEX_CATALOG]);
    fake.failNext("agents.catalog", { code: -32011, message: "catalog is busy" });
    const result = await (await live(fake)).models.catalog();
    expect(result.status).toBe("ok");
    if (result.status !== "ok") return;
    expect(result.data.providers[0]!.models).toMatchObject({ status: "unknown" });
    expect(result.data.providers[0]!.efforts).toMatchObject({ status: "unknown" });
  });

  it("shows an authentication failure as sign-in required (unavailable)", async () => {
    const fake = withAgents(populate(new FakeDaemon()));
    const auth = {
      ...CODEX_CATALOG,
      state: "authentication_failed" as const,
      detail: "not logged in",
      models: { state: "authentication_failed" as const, detail: "not logged in", items: [] },
    };
    fake.stub("agents.catalog", [auth]);
    const result = await (await live(fake)).models.catalog();
    if (result.status !== "ok") throw new Error("expected ok");
    const models = result.data.providers[0]!.models;
    expect(models.status === "unavailable" && models.reason).toMatch(/Sign-in required/);
  });

  it("falls back to unsupported models and efforts when the daemon has no catalog method", async () => {
    const result = await (await live(withAgents(populate(new FakeDaemon())))).models.catalog();
    if (result.status !== "ok") throw new Error("expected ok");
    for (const provider of result.data.providers) {
      expect(provider.models.status).toBe("unsupported");
      expect(provider.efforts.status).toBe("unsupported");
    }
  });

  it("does not invent a provider for a catalog row without a capabilities row", async () => {
    const fake = withAgents(populate(new FakeDaemon()));
    fake.stub("agents.catalog", [CODEX_CATALOG, { ...CLAUDE_CATALOG, agent: "mystery" }]);
    const result = await (await live(fake)).models.catalog();
    if (result.status !== "ok") throw new Error("expected ok");
    expect(result.data.providers.map((p) => p.id)).not.toContain("mystery");
  });
});

describe("live selection check", () => {
  it("asks agents.validate_selection with the agent, model and effort", async () => {
    const fake = withAgents(populate(new FakeDaemon()));
    stubValidate(fake, [CODEX_CATALOG]);
    const s = await live(fake);
    const ok = await s.models.validate!("codex", { model: "gpt-a", effort: "high" });
    expect(ok).toMatchObject({ status: "ok", data: { valid: true } });
    expect(fake.callsTo("agents.validate_selection")[0]!.params).toEqual({ agent: "codex", model: "gpt-a", effort: "high" });
  });

  it("returns the daemon's reason for an invalid choice", async () => {
    const fake = withAgents(populate(new FakeDaemon()));
    stubValidate(fake, [CODEX_CATALOG]);
    const bad = await (await live(fake)).models.validate!("codex", { model: "gpt-b", effort: "high" });
    expect(bad).toMatchObject({ status: "ok", data: { valid: false, reason: "effort_unsupported" } });
  });

  it("does not call the daemon with neither a model nor an effort", async () => {
    const fake = withAgents(populate(new FakeDaemon()));
    stubValidate(fake, [CODEX_CATALOG]);
    const result = await (await live(fake)).models.validate!("codex", {});
    expect(result.status).toBe("unknown");
    expect(fake.callsTo("agents.validate_selection")).toHaveLength(0);
  });

  it("never sends a fixture agent id", async () => {
    const fake = withAgents(populate(new FakeDaemon()));
    stubValidate(fake, [CODEX_CATALOG]);
    const result = await (await live(fake)).models.validate!("fixture:codex", { model: "x" });
    expect(result.status).not.toBe("ok");
    expect(fake.callsTo("agents.validate_selection")).toHaveLength(0);
  });
});
