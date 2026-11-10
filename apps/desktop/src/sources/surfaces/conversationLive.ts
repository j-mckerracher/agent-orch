/**
 * Live D10 conversation for Global and workstream targets (M7-24), bound to
 * `conversations.send`, `conversations.get` and `conversations.cancel`.
 *
 * What this shows, and what it does not:
 * - The daemon records the user message and starts a turn. A turn ends as
 *   completed, failed or cancelled. The shipped daemon has no model runtime
 *   yet, so every turn fails with `runtime_unavailable`; this adapter shows
 *   that as a status line and never produces reply text of its own.
 * - There is no push stream. Replies arrive by cursor reads; the caller polls
 *   while the latest turn is `running`.
 * - The transcript is re-read from the start every time. The log is persisted
 *   by the daemon, so a reconnect or a daemon restart needs no client cursor.
 */
import type { ConnectionManager } from "../../daemon/connection";
import { isFixtureId } from "../../daemon/api";
import { DaemonError, asDaemonError } from "../../daemon/errors";
import type { IdempotencyTracker } from "../../daemon/idempotency";
import type {
  ConversationWireEntry,
  ConversationWireTarget,
  ConversationWireTurn,
} from "../../daemon/types";
import { liveRead } from "./liveSupport";
import { notOk } from "./result";
import type {
  ConversationEntry,
  ConversationSource,
  ConversationTarget,
  ConversationTurnInfo,
  Provenance,
  SendReceipt,
} from "./types";

export const MAX_LIVE_CONVERSATION_PAGES = 50;

export type LiveTarget = Exclude<ConversationTarget, { kind: "project" }>;

export const isLiveTarget = (target: ConversationTarget): target is LiveTarget => target.kind !== "project";

export const LIVE_METHODS = ["conversations.get", "conversations.send", "conversations.cancel"] as const;

const FAILURE_TEXT: Record<string, string> = {
  runtime_unavailable:
    "Orchestrator runtime unavailable. The daemon recorded your message, but no conversation runtime is attached, so no reply was produced.",
  credentials_missing: "The orchestrator could not start: its credentials are missing. No reply was produced.",
  permission_denied: "The orchestrator was denied permission to run. No reply was produced.",
  version_mismatch: "The orchestrator runtime has an unsupported version. No reply was produced.",
  runtime_unpinned: "The orchestrator runtime is not pinned to a version. No reply was produced.",
  runtime_failed: "The orchestrator runtime failed. The reply is incomplete or missing.",
  reply_too_large: "The reply was larger than the daemon allows and was dropped.",
  interrupted_by_restart: "The daemon restarted before the reply finished. The reply is incomplete or missing.",
};

export function failureText(code: string | null): string {
  if (code === null) return "The turn failed without a reason code. No reply was produced.";
  return FAILURE_TEXT[code] ?? `The turn failed (${code}). No reply was produced.`;
}

export function toWireTarget(target: LiveTarget): ConversationWireTarget {
  return target.kind === "global" ? { kind: "global" } : { kind: "workstream", workstream_id: target.workstreamId };
}

export function turnInfo(turn: ConversationWireTurn): ConversationTurnInfo {
  return { id: turn.id, state: turn.state, errorCode: turn.error_code, runtime: turn.runtime };
}

/**
 * Maps wire entries to view entries. Reply chunks of one turn become a single
 * reply that stays `streaming` until the daemon records the end of the turn;
 * a failed, cancelled or empty turn becomes a `status` line, never a reply.
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
        out.push(status(entry, "This turn was cancelled."));
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
    ? "The daemon does not know this workstream (not-found). Workstream conversations need the daemon's workstream lookup, which is not wired yet."
    : undefined;

function refuseFixture(target: LiveTarget): void {
  if (target.kind === "workstream" && isFixtureId(target.workstreamId)) {
    throw new DaemonError("invalid-params", "Fixture ids are never sent to the daemon.");
  }
}

export function createLiveConversation(
  connection: ConnectionManager,
  provenance: Provenance,
  keys: IdempotencyTracker,
  now: () => number,
): Required<Pick<ConversationSource, "cancel">> & {
  transcript: (target: LiveTarget) => ReturnType<ConversationSource["transcript"]>;
  send: (target: LiveTarget, text: string) => Promise<SendReceipt>;
} {
  const scopeOf = (target: LiveTarget) => (target.kind === "global" ? "global" : `workstream:${target.workstreamId}`);

  return {
    async transcript(target) {
      if (target.kind === "workstream" && isFixtureId(target.workstreamId)) {
        return notOk(provenance, "unsupported", "Fixture ids have no daemon conversation.");
      }
      return liveRead(
        provenance,
        now,
        async () => {
          const api = connection.api();
          const wire = toWireTarget(target);
          const entries: ConversationWireEntry[] = [];
          let after: number | undefined;
          let more = false;
          let turn: ConversationWireTurn | null = null;
          for (let page = 0; page < MAX_LIVE_CONVERSATION_PAGES; page++) {
            const result = await api.conversationsGet(wire, after);
            entries.push(...result.entries);
            turn = result.turn;
            more = result.more;
            if (!more) break;
            after = result.next_after_sequence;
          }
          return {
            target,
            entries: mapEntries(entries),
            repliesExpected: true,
            truncated: more,
            turn: turn === null ? null : turnInfo(turn),
          };
        },
        describeNotFound(target),
      );
    },

    async send(target, text) {
      refuseFixture(target);
      const scope = `send:${scopeOf(target)}`;
      const key = keys.keyFor(scope, text);
      try {
        const result = await connection.api().conversationsSend(toWireTarget(target), text, key);
        keys.settle(scope, null);
        const turn = turnInfo(result.turn);
        return {
          simulated: false,
          provenance,
          notice: sendNotice(turn),
          at: result.entry.created_at,
          entryId: result.entry.id,
          turn,
        };
      } catch (error) {
        keys.settle(scope, asDaemonError(error));
        throw error;
      }
    },

    async cancel(target, turnId) {
      if (!isLiveTarget(target)) throw new DaemonError("method-unavailable", "Only Global and workstream turns can be cancelled.");
      refuseFixture(target);
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
