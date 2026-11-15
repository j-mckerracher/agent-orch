/**
 * Live D10 conversation for Global and workstream targets (M7-24), bound to
 * `conversations.send`, `conversations.get` and `conversations.cancel`.
 *
 * What this shows, and what it does not:
 * - The daemon records the user message and starts a turn. A turn ends as
 *   completed, failed or canceled. The daemon (M7-25) routes the message to an
 *   installed, launchable and pinned agent and records its reply; when none is
 *   available the turn fails with `runtime_unavailable`. This adapter shows a
 *   failure as a status line and never produces reply text of its own.
 * - There is no push stream. Replies arrive by cursor reads; the caller polls
 *   while the latest turn is `running`.
 * - The transcript is re-read every time, with no client cursor. The log is persisted
 *   by the daemon, so a reconnect or a daemon restart needs no saved state. A very long
 *   log shows its newest entries; the oldest are cut and the result says so.
 */
import type { ConnectionManager } from "../../daemon/connection";
import { isFixtureLikeId } from "./fixtureIds";
import { DaemonError, asDaemonError } from "../../daemon/errors";
import type { IdempotencyTracker } from "../../daemon/idempotency";
import type {
  ConversationWireEntry,
  ConversationWireTarget,
  ConversationWireTurn,
} from "../../daemon/types";
import { liveRead } from "./liveSupport";
import { notOk, observation } from "./result";
import type {
  ClearReceipt,
  ConversationEntry,
  ConversationSource,
  ConversationTarget,
  ConversationTurnInfo,
  ModelChoice,
  PermissionLevel,
  Provenance,
  SendReceipt,
} from "./types";

export const MAX_LIVE_CONVERSATION_PAGES = 50;

/** Restarts of one transcript read after a clear lands mid-read. */
const MAX_HISTORY_RESTARTS = 3;

/** A clear landed between two pages of one read, so the pages read so far are stale (DPES-1105). */
export class HistoryChanged extends Error {}

/**
 * Runs one read again when a clear invalidated it: a page reported a new `clear_generation`, or the
 * cursor ran past the end of a log that was cleared (`replay-gap`). The reload starts from the
 * beginning, so a cleared log reads as empty rather than as an error.
 */
export async function restartOnHistoryChange<T>(read: () => Promise<T>): Promise<T> {
  for (let attempt = 0; ; attempt++) {
    try {
      return await read();
    } catch (error) {
      const restart =
        error instanceof HistoryChanged || (error instanceof DaemonError && error.kind === "replay-gap");
      if (!restart || attempt >= MAX_HISTORY_RESTARTS) throw error;
    }
  }
}
/** The daemon returns at most 100 entries per page, so this span fits in `MAX_LIVE_CONVERSATION_PAGES` pages. */
export const MAX_LIVE_SEQUENCE_SPAN = MAX_LIVE_CONVERSATION_PAGES * 100;

/** A window that starts mid-turn would show a reply with no message; start at the first whole turn. */
function dropPartialLeadingTurn(entries: ConversationWireEntry[]): ConversationWireEntry[] {
  const first = entries.findIndex((entry) => entry.kind === "user_message");
  return first > 0 ? entries.slice(first) : entries;
}

export type LiveTarget = Exclude<ConversationTarget, { kind: "project" }>;

export const isLiveTarget = (target: ConversationTarget): target is LiveTarget => target.kind !== "project";

export const LIVE_METHODS = [
  "conversations.get",
  "conversations.send",
  "conversations.cancel",
  "conversations.clear",
] as const;

const FAILURE_TEXT: Record<string, string> = {
  runtime_unavailable:
    "Orchestrator runtime unavailable. The daemon recorded your message, but no installed agent could be launched to answer it, so no reply was produced.",
  credentials_missing: "The orchestrator could not start: its credentials are missing. No reply was produced.",
  permission_denied: "The orchestrator was denied permission to run. No reply was produced.",
  version_mismatch: "The orchestrator runtime has an unsupported version. No reply was produced.",
  runtime_unpinned: "The orchestrator runtime is not pinned to a version. No reply was produced.",
  runtime_failed: "The orchestrator runtime failed. The reply is incomplete or missing.",
  runtime_timeout: "The orchestrator took too long to answer and was stopped. The reply is incomplete or missing.",
  runtime_busy: "The orchestrator is already answering other conversations. Try again in a moment.",
  reply_too_large: "The reply was larger than the daemon allows and was dropped.",
  interrupted_by_restart: "The daemon restarted before the reply finished. The reply is incomplete or missing.",
  // The daemon refused the model choice from the picker (DPES-1105). Each reason has its own code.
  selection_agent_unavailable: "The chosen agent is not installed or cannot be launched. Choose another agent. No reply was produced.",
  selection_authentication_failed: "The chosen agent is not signed in. Sign in to it, or choose another agent. No reply was produced.",
  selection_agent_state_unknown: "The chosen agent's state could not be checked. Try again shortly. No reply was produced.",
  selection_agent_unsupported: "The chosen agent cannot answer a message. Choose another agent. No reply was produced.",
  selection_models_unsupported: "The chosen agent does not offer model choices. Choose the default model. No reply was produced.",
  selection_models_unknown: "The chosen agent's models could not be read. Try again shortly. No reply was produced.",
  selection_unknown_model: "The chosen agent does not offer that model. Choose another model. No reply was produced.",
  selection_effort_requires_model: "That effort needs a model. Choose a model, or the default effort. No reply was produced.",
  selection_effort_unsupported: "The chosen agent does not offer that effort for this model. Choose another effort. No reply was produced.",
  selection_effort_unknown: "The chosen agent's effort levels could not be read. Try again shortly. No reply was produced.",
  // The daemon refuses a level its chat cannot run (DPES-1105). Extended is refused for chat on every agent for now.
  permission_level_unsupported: "This permission level is not available for chat with this agent yet. Choose another level. No reply was produced.",
};

export function failureText(code: string | null): string {
  if (code === null) return "The turn failed without a reason code. No reply was produced.";
  return FAILURE_TEXT[code] ?? `The turn failed (${code}). No reply was produced.`;
}

export function toWireTarget(target: LiveTarget): ConversationWireTarget {
  return target.kind === "global" ? { kind: "global" } : { kind: "workstream", workstream_id: target.workstreamId };
}

export function turnInfo(turn: ConversationWireTurn): ConversationTurnInfo {
  return {
    id: turn.id,
    state: turn.state,
    errorCode: turn.error_code,
    runtime: turn.runtime,
    permissionLevel: turn.permission_level ?? null,
  };
}

/**
 * Maps wire entries to view entries. Reply chunks of one turn become a single
 * reply that stays `streaming` until the daemon records the end of the turn;
 * a failed, canceled or empty turn becomes a `status` line, never a reply.
 */
export function mapEntries(wire: readonly ConversationWireEntry[]): ConversationEntry[] {
  const out: ConversationEntry[] = [];
  const replies = new Map<string, ConversationEntry>();
  const status = (entry: ConversationWireEntry, text: string): ConversationEntry => ({
    id: entry.id,
    kind: "status",
    role: "system",
    text,
    at: entry.created_at,
    turnId: entry.turn_id,
    ...(entry.error_code ? { errorCode: entry.error_code } : {}),
  });
  for (const entry of wire) {
    const reply = replies.get(entry.turn_id);
    switch (entry.kind) {
      case "user_message":
        out.push({ id: entry.id, kind: "message", role: "user", text: entry.body, at: entry.created_at, turnId: entry.turn_id });
        break;
      case "reply_chunk":
        if (reply) {
          reply.text += entry.body;
        } else {
          const created: ConversationEntry = {
            id: `${entry.turn_id}:reply`,
            kind: "message",
            role: "coordinator",
            text: entry.body,
            at: entry.created_at,
            turnId: entry.turn_id,
            streaming: true,
          };
          replies.set(entry.turn_id, created);
          out.push(created);
        }
        break;
      case "turn_completed":
        if (reply) reply.streaming = false;
        else out.push(status(entry, "The orchestrator finished this turn without a reply."));
        break;
      case "turn_failed":
        if (reply) reply.streaming = false;
        out.push(status(entry, failureText(entry.error_code)));
        break;
      case "turn_cancelled":
        if (reply) reply.streaming = false;
        out.push(status(entry, "This turn was canceled."));
        break;
    }
  }
  return out;
}

export const sendNotice = (turn: ConversationTurnInfo): string =>
  turn.state === "failed"
    ? `Recorded by the daemon. ${failureText(turn.errorCode)}`
    : turn.state === "running"
      ? "Recorded by the daemon. Waiting for the orchestrator's reply."
      : "Recorded by the daemon.";

const describeNotFound = (target: LiveTarget) => (error: unknown) =>
  target.kind === "workstream" && asDaemonError(error).kind === "not-found"
    ? "The daemon's workstream lookup did not find this workstream (not-found)."
    : undefined;

function refuseFixture(target: LiveTarget): void {
  if (target.kind === "workstream" && isFixtureLikeId(target.workstreamId)) {
    throw new DaemonError("invalid-params", "Fixture ids are never sent to the daemon.");
  }
}

export function createLiveConversation(
  connection: ConnectionManager,
  provenance: Provenance,
  keys: IdempotencyTracker,
  now: () => number,
): Required<Pick<ConversationSource, "cancel" | "clear">> & {
  transcript: (target: LiveTarget) => ReturnType<ConversationSource["transcript"]>;
  send: (target: LiveTarget, text: string, selection?: ModelChoice, permission?: PermissionLevel) => Promise<SendReceipt>;
} {
  const scopeOf = (target: LiveTarget) => (target.kind === "global" ? "global" : `workstream:${target.workstreamId}`);

  return {
    async transcript(target) {
      if (target.kind === "workstream" && isFixtureLikeId(target.workstreamId)) {
        return notOk(provenance, "unsupported", "Fixture ids have no daemon conversation.", observation(now()));
      }
      const readOnce = async () => {
          const api = connection.api();
          const wire = toWireTarget(target);
          let entries: ConversationWireEntry[] = [];
          let after: number | undefined;
          let epoch: string | undefined;
          let generation: number | undefined;
          let more = false;
          let turn: ConversationWireTurn | null = null;
          let jumpedTo: number | null = null;
          // The first read starts at the beginning and learns `latest_sequence`. A long log is then
          // read from `latest_sequence - MAX_LIVE_SEQUENCE_SPAN`, so the newest entries and the
          // latest turn are always the ones shown. Sequences are shared by every conversation, so
          // this is a bound on the span, not an exact entry count.
          for (let page = 0; page <= MAX_LIVE_CONVERSATION_PAGES; page++) {
            const result = await api.conversationsGet(wire, after, epoch);
            epoch ??= result.epoch;
            // A clear between pages changes the generation: this read mixes two logs, so it starts over.
            generation ??= result.clear_generation;
            if (result.clear_generation !== generation) throw new HistoryChanged();
            entries.push(...result.entries);
            turn = result.turn;
            more = result.more;
            if (!more) break;
            after = result.next_after_sequence;
            const windowStart = result.latest_sequence - MAX_LIVE_SEQUENCE_SPAN;
            if (windowStart > after) {
              entries = [];
              after = jumpedTo = windowStart;
            }
          }
          // A jump discards entries already read, so it always means older entries are not shown.
          const truncated = more || jumpedTo !== null;
          if (truncated) entries = dropPartialLeadingTurn(entries);
          return {
            target,
            entries: mapEntries(entries),
            repliesExpected: true,
            truncated,
            turn: turn === null ? null : turnInfo(turn),
          };
      };
      return liveRead(provenance, now, () => restartOnHistoryChange(readOnce), describeNotFound(target));
    },

    async send(target, text, selection, permission) {
      refuseFixture(target);
      const scope = `send:${scopeOf(target)}`;
      // A retry with another choice is another message: it must not reuse the key of the first. A request
      // without a permission keeps the key it always had.
      const fingerprint = permission === undefined ? [text, selection ?? null] : [text, selection ?? null, permission];
      const key = keys.keyFor(scope, JSON.stringify(fingerprint));
      try {
        const result = await connection.api().conversationsSend(toWireTarget(target), text, key, selection, permission);
        keys.settle(scope, null);
        const { entry } = result;
        // A replay of a message that a clear deleted has no entry body and no turn (DPES-1105).
        if (result.turn === null || "cleared" in entry) {
          return {
            simulated: false,
            provenance,
            notice: "That message was cleared before its reply was recorded.",
            at: entry.created_at ?? new Date(now()).toISOString(),
            entryId: entry.id,
          };
        }
        const turn = turnInfo(result.turn);
        return {
          simulated: false,
          provenance,
          notice: sendNotice(turn),
          at: entry.created_at,
          entryId: entry.id,
          turn,
        };
      } catch (error) {
        keys.settle(scope, asDaemonError(error));
        throw error;
      }
    },

    async clear(target): Promise<ClearReceipt> {
      if (!isLiveTarget(target)) {
        throw new DaemonError("method-unavailable", "Only Global and workstream history is cleared through conversations.clear.");
      }
      refuseFixture(target);
      const scope = `clear:${scopeOf(target)}`;
      const key = keys.keyFor(scope, scopeOf(target));
      try {
        const result = await connection.api().conversationsClear(toWireTarget(target), key);
        keys.settle(scope, null);
        return {
          simulated: false,
          provenance,
          cleared: result.cleared,
          notice: result.cleared
            ? "History cleared. The deleted messages and replies cannot be recovered."
            : "There was no history to clear.",
        };
      } catch (error) {
        keys.settle(scope, asDaemonError(error));
        throw error;
      }
    },

    async cancel(target, turnId) {
      if (!isLiveTarget(target)) throw new DaemonError("method-unavailable", "Only Global and workstream turns can be canceled.");
      refuseFixture(target);
      if (isFixtureLikeId(turnId)) throw new DaemonError("invalid-params", "Fixture turn ids are never sent to the daemon.");
      const scope = `cancel:${scopeOf(target)}:${turnId}`;
      const key = keys.keyFor(scope, turnId);
      try {
        const result = await connection.api().conversationsCancel(toWireTarget(target), turnId, key);
        keys.settle(scope, null);
        return { applied: result.cancel_applied };
      } catch (error) {
        keys.settle(scope, asDaemonError(error));
        throw error;
      }
    },
  };
}
