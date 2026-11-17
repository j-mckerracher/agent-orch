import type {
  ActionProposalWire,
  ActionsResolveWire,
  AgentCatalogEntry,
  ConversationWireEntry,
  ConversationWireTarget,
  ConversationWireTurn,
} from "../daemon/types";
import { StubRpcError, type FakeDaemon } from "./fakeDaemon";

/** Test doubles for the M7-24 daemon methods. The shared `testkit.ts` is left alone on purpose. */

type Outcome = "runtime_unavailable" | "running" | "completed";

const at = "2026-11-10T21:00:00Z";
const GLOBAL: ConversationWireTarget = { kind: "global" };
const PROPOSAL_NOT_FOUND = -32021;
const PROPOSAL_NOT_PENDING = -32022;
const PROPOSAL_INVALID = -32023;
const INVALID_PARAMS = -32602;

export interface ConversationStub {
  entries: ConversationWireEntry[];
  turn: ConversationWireTurn | null;
  outcome: Outcome;
  /** Set by a test to send `proposals` on `conversations.get`; left out by default, like an older daemon. */
  proposals?: ActionProposalWire[];
  finishRunning(code?: string | null, reply?: string): void;
}

/** A daemon-shaped live conversation: send records a turn, get pages the log, cancel ends a running turn. */
export function stubConversation(fake: FakeDaemon, outcome: Outcome = "runtime_unavailable"): ConversationStub {
  let seq = 0;
  let turns = 0;
  const state: ConversationStub = {
    entries: [],
    turn: null,
    outcome,
    finishRunning(code = null, reply) {
      if (!this.turn || this.turn.state !== "running") return;
      const id = this.turn.id;
      if (reply !== undefined) this.entries.push(entry(id, "reply_chunk", "orchestrator", reply, null, `chunk:${seq}`));
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
    return { authority: "none", target: conversationTarget(params), entry: sent, turn: state.turn, epoch: "epoch-1" };
  });
  fake.stub("conversations.get", (params: Record<string, unknown>) => {
    const after = typeof params.after_sequence === "number" ? params.after_sequence : 0;
    const entries = state.entries.filter((item) => item.sequence > after);
    return {
      authority: "none",
      target: conversationTarget(params),
      entries,
      more: false,
      next_after_sequence: seq,
      latest_sequence: seq,
      turn: state.turn,
      epoch: "epoch-1",
      ...(state.proposals ? { proposals: state.proposals } : {}),
    };
  });
  fake.stub("conversations.cancel",(params: Record<string, unknown>) => {
    const running = state.turn?.state === "running";
    if (state.turn && running) {
      state.entries.push(entry(state.turn.id, "turn_cancelled", "system", "", null, "end"));
      state.turn = { ...state.turn, state: "cancelled" };
    }
    return {
      authority: "none",
      target: conversationTarget(params),
      turn: state.turn,
      cancel_applied: running,
      epoch: "epoch-1",
    };
  });
  return state;
}

/** The result a daemon gives an executed proposal; the prefix follows the kind (`objective-` for objectives). */
const resultRefOf = (proposal: ActionProposalWire): string =>
  `${proposal.kind.startsWith("objective") ? "objective" : "workstream"}-${proposal.id}`;

/** Fails every pending proposal that depends on `parentId`, and the ones that depend on those, as the daemon does. */
function failDependents(list: ActionProposalWire[], parentId: string): ActionProposalWire[] {
  const dependents = list.filter((item) => item.depends_on === parentId && item.state === "pending");
  return dependents.reduce(
    (acc, dependent) =>
      failDependents(
        acc.map((item): ActionProposalWire => (item.id === dependent.id ? { ...item, state: "failed", error_code: "dependency_not_executed", updated_at: at } : item)),
        dependent.id,
      ),
    list,
  );
}

/**
 * Answers `actions.resolve` for the cases tests need, against `state.proposals`. The decision must carry the proposal's
 * digest. Approve executes the proposal and sets its `result_ref`. Reject ends it and fails its pending dependents. A dependent needs an executed parent first, and only
 * a pending proposal can be decided.
 */
export function stubActionsResolve(fake: FakeDaemon, state: ConversationStub): void {
  fake.stub("actions.resolve", (params: Record<string, unknown>): ActionsResolveWire => {
    const proposals = state.proposals ?? [];
    const target = proposals.find((item) => item.id === params.proposal_id);
    if (!target) throw new StubRpcError(PROPOSAL_NOT_FOUND, "action proposal was not found");
    // A missing digest is refused as invalid params; a stale one is a proposal the daemon cannot approve (-32023).
    if (params.digest === undefined) throw new StubRpcError(INVALID_PARAMS, "digest is required");
    if (params.digest !== target.digest) throw new StubRpcError(PROPOSAL_INVALID, "digest does not match the proposal");
    if (target.state !== "pending") throw new StubRpcError(PROPOSAL_NOT_PENDING, "action proposal is no longer pending");
    if (params.decision === "approve") {
      const parent = proposals.find((item) => item.id === target.depends_on);
      if (target.depends_on !== null && parent?.state !== "executed") {
        throw new StubRpcError(PROPOSAL_INVALID, "action proposal cannot be approved now");
      }
      const executed: ActionProposalWire = { ...target, state: "executed", result_ref: resultRefOf(target), updated_at: at };
      state.proposals = proposals.map((item) => (item.id === target.id ? executed : item));
      return { proposal: executed };
    }
    const rejected: ActionProposalWire = { ...target, state: "rejected", updated_at: at };
    state.proposals = failDependents(
      proposals.map((item) => (item.id === target.id ? rejected : item)),
      target.id,
    );
    return { proposal: rejected };
  });
}

function conversationTarget(params: Record<string, unknown>): ConversationWireTarget {
  const target = params.target;
  if (typeof target !== "object" || target === null) return GLOBAL;
  const candidate = target as Partial<ConversationWireTarget>;
  if (candidate.kind === "global") return GLOBAL;
  if (candidate.kind === "workstream" && typeof candidate.workstream_id === "string") {
    return { kind: "workstream", workstream_id: candidate.workstream_id };
  }
  return GLOBAL;
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
    detail:
      "Aliases documented by Claude Code. The CLI cannot list the models your account can use, so availability depends on your account.",
    items: [
      { id: "opus", display_name: "Opus (alias)", efforts: [], default_effort: null },
      { id: "sonnet", display_name: "Sonnet (alias)", efforts: [], default_effort: null },
      { id: "haiku", display_name: "Haiku (alias)", efforts: [], default_effort: null },
    ],
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

/** GitHub Copilot before any sign-in check: installed and pinned, sign-in not yet confirmed (DPES-1105). */
export const COPILOT_CATALOG: AgentCatalogEntry = {
  agent: "github-copilot",
  provider: "github",
  state: "unknown",
  detail: "Not confirmed yet: sign-in is checked on your first Copilot use. If you have not signed in, run `copilot login`.",
  models: {
    state: "unsupported",
    detail: "The Copilot CLI cannot list models without a billable call. Runs use the default model from your Copilot settings.",
    items: [],
  },
  efforts: { state: "unsupported", detail: "This provider exposes no effort setting.", items: [] },
};

/** GitHub Copilot when its CLI is not installed: launch is unavailable (DPES-1105). */
export const COPILOT_UNAVAILABLE_CATALOG: AgentCatalogEntry = {
  agent: "github-copilot",
  provider: "github",
  state: "unavailable",
  detail: "the copilot executable was not found",
  models: {
    state: "unavailable",
    detail: "the copilot executable was not found",
    items: [],
  },
  efforts: { state: "unsupported", detail: "This provider exposes no effort setting.", items: [] },
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
