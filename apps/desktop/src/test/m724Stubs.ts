import type {
  AgentCatalogEntry,
  ConversationWireEntry,
  ConversationWireTarget,
  ConversationWireTurn,
} from "../daemon/types";
import type { FakeDaemon } from "./fakeDaemon";

/** Test doubles for the M7-24 daemon methods. The shared `testkit.ts` is left alone on purpose. */

type Outcome = "runtime_unavailable" | "running" | "completed";

const at = "2026-11-10T21:00:00Z";
const GLOBAL: ConversationWireTarget = { kind: "global" };

export interface ConversationStub {
  entries: ConversationWireEntry[];
  turn: ConversationWireTurn | null;
  outcome: Outcome;
  finishRunning(code?: string | null): void;
}

/** A daemon-shaped Global conversation: send records a turn, get pages the log, cancel ends a running turn. */
export function stubConversation(fake: FakeDaemon, outcome: Outcome = "runtime_unavailable"): ConversationStub {
  let seq = 0;
  let turns = 0;
  const state: ConversationStub = {
    entries: [],
    turn: null,
    outcome,
    finishRunning(code = null) {
      if (!this.turn || this.turn.state !== "running") return;
      const id = this.turn.id;
      this.entries.push(
        code === null
          ? entry(id, "turn_completed", "system", "", null, "end")
          : entry(id, "turn_failed", "system", "", code, "end"),
      );
      this.turn = { ...this.turn, state: code === null ? "completed" : "failed", error_code: code };
    },
  };
  const entry = (
    turnId: string,
    kind: ConversationWireEntry["kind"],
    author: ConversationWireEntry["author"],
    body: string,
    errorCode: string | null,
    suffix: string,
  ): ConversationWireEntry => ({
    sequence: ++seq,
    id: `${turnId}:${suffix}`,
    turn_id: turnId,
    kind,
    author,
    body,
    error_code: errorCode,
    created_at: at,
  });
  const base = (id: string, status: ConversationWireTurn["state"], code: string | null): ConversationWireTurn => ({
    id,
    state: status,
    error_code: code,
    runtime: { name: "unavailable", version: "none" },
    reply_bytes: 0,
    created_at: at,
    updated_at: at,
  });

  fake.stub("conversations.send", (params: Record<string, unknown>) => {
    const id = `turn-${++turns}`;
    const sent = entry(id, "user_message", "user", String(params.text), null, "user");
    state.entries.push(sent);
    state.turn = base(id, "running", null);
    if (state.outcome === "runtime_unavailable") state.finishRunning("runtime_unavailable");
    else if (state.outcome === "completed") {
      state.entries.push(entry(id, "reply_chunk", "orchestrator", "Hello from the runtime.", null, "chunk:0"));
      state.finishRunning(null);
    }
    return { authority: "none", target: GLOBAL, entry: sent, turn: state.turn, epoch: "epoch-1" };
  });
  fake.stub("conversations.get", (params: Record<string, unknown>) => {
    const after = typeof params.after_sequence === "number" ? params.after_sequence : 0;
    const entries = state.entries.filter((item) => item.sequence > after);
    return {
      authority: "none",
      target: GLOBAL,
      entries,
      more: false,
      next_after_sequence: seq,
      latest_sequence: seq,
      turn: state.turn,
      epoch: "epoch-1",
    };
  });
  fake.stub("conversations.cancel", () => {
    const running = state.turn?.state === "running";
    if (state.turn && running) {
      state.entries.push(entry(state.turn.id, "turn_cancelled", "system", "", null, "end"));
      state.turn = { ...state.turn, state: "cancelled" };
    }
    return {
      authority: "none",
      target: GLOBAL,
      turn: state.turn,
      cancel_applied: running,
      epoch: "epoch-1",
    };
  });
  return state;
}

export const CODEX_CATALOG: AgentCatalogEntry = {
  agent: "codex",
  provider: "openai",
  state: "available",
  detail: "",
  models: {
    state: "available",
    detail: "",
    items: [
      { id: "gpt-a", display_name: "GPT A", efforts: ["low", "high"], default_effort: "low" },
      { id: "gpt-b", display_name: "GPT B", efforts: ["medium"], default_effort: null },
    ],
  },
  efforts: { state: "unsupported", detail: "efforts are per model", items: [] },
};

export const CLAUDE_CATALOG: AgentCatalogEntry = {
  agent: "claude-code",
  provider: "anthropic",
  state: "available",
  detail: "",
  models: {
    state: "available",
    detail: "",
    items: [{ id: "sonnet", display_name: "Sonnet", efforts: [], default_effort: null }],
  },
  efforts: { state: "available", detail: "", items: ["low", "medium", "high"] },
};

export const PI_CATALOG: AgentCatalogEntry = {
  agent: "pi",
  provider: null,
  state: "unsupported",
  detail: "pi lists no models",
  models: { state: "unsupported", detail: "pi lists no models", items: [] },
  efforts: { state: "unsupported", detail: "pi lists no efforts", items: [] },
};

export const COLD_CATALOG = (agent: string): AgentCatalogEntry => ({
  agent,
  provider: null,
  state: "unknown",
  detail: "the catalog is still being read; retry shortly",
  models: { state: "unknown", detail: "the catalog is still being read; retry shortly", items: [] },
  efforts: { state: "unknown", detail: "the catalog is still being read; retry shortly", items: [] },
});

/** Answers `agents.validate_selection` against a catalog the way the daemon does for the cases tests need. */
export function stubValidate(fake: FakeDaemon, catalog: readonly AgentCatalogEntry[]): void {
  fake.stub("agents.validate_selection", (params: Record<string, unknown>) => {
    const entry = catalog.find((item) => item.agent === params.agent);
    const model = entry?.models.items.find((item) => item.id === params.model);
    if (!entry) return { valid: false, reason: "agent_unavailable", detail: "no such agent" };
    if (entry.state === "unknown") {
      return { valid: false, reason: "agent_state_unknown", detail: entry.detail };
    }
    if (params.model !== undefined && !model) {
      return { valid: false, reason: "unknown_model", detail: `${String(params.model)} is not offered` };
    }
    if (params.effort !== undefined) {
      const allowed = model && model.efforts.length > 0 ? model.efforts : entry.efforts.items;
      if (!allowed.includes(String(params.effort))) {
        return { valid: false, reason: "effort_unsupported", detail: `${String(params.effort)} is not offered` };
      }
    }
    return { valid: true };
  });
}
