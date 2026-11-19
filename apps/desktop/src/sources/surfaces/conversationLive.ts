/**
 * Live D10 conversation for Global, project and workstream targets, bound to
 * `conversations.send`, `conversations.get`, `conversations.cancel` and `actions.resolve` (DPES-1105).
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
import { agentDisplayName } from "../../daemon/agentDisplayNames";
import { isFixtureLikeId } from "./fixtureIds";
import { DaemonError, asDaemonError } from "../../daemon/errors";
import type { IdempotencyTracker } from "../../daemon/idempotency";
import type {
  ActionProposalWire,
  ConversationCoordinatorWire,
  ConversationRoutedInWire,
  ConversationWireEntry,
  ConversationWireTarget,
  ConversationWireTurn,
  WakeRecordWire,
} from "../../daemon/types";
import { RESOLVE_METHOD, createLiveResolve, mapProposal } from "./actionProposals";
import { liveRead } from "./liveSupport";
import { notOk, observation } from "./result";
import type {
  ActionBlock,
  ClearReceipt,
  ConversationEntry,
  ConversationSource,
  ConversationTarget,
  ConversationTurnInfo,
  CoordinatorState,
  ModelChoice,
  PermissionLevel,
  Provenance,
  PullRequestLink,
  RoutedIn,
  SendReceipt,
  WakeRecord,
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

export const PROJECT_CONVERSATIONS_REQUIRED =
  "Update the daemon to enable project coordinator replies. Your message has not been sent.";

export const LIVE_METHODS = [
  "conversations.get",
  "conversations.send",
  "conversations.cancel",
  "conversations.clear",
  RESOLVE_METHOD,
] as const;

const FAILURE_TEXT: Record<string, string> = {
  runtime_unavailable:
    "Orchestrator runtime unavailable. The daemon recorded your message, but no installed agent could be launched to answer it, so no reply was produced.",
  credentials_missing: "The orchestrator could not start: its credentials are missing. No reply was produced.",
  permission_denied: "The orchestrator was denied permission to run. No reply was produced.",
  version_mismatch: "The orchestrator runtime has an unsupported version. No reply was produced.",
  runtime_changed: "The orchestrator's agent changed before this message was sent, so no reply was produced. Send the message again.",
  runtime_compatibility_unconfirmed: "The daemon could not confirm CLI compatibility. No agent ran. Retry the message.",
  runtime_unpinned: "The orchestrator runtime is not pinned to a version. No reply was produced.",
  runtime_failed: "The orchestrator runtime failed. The reply is incomplete or missing.",
  runtime_provider_limit:
    "The orchestrator's agent refused the turn: its usage or spending limit is reached. Raise the limit or choose another agent in Settings. No reply was produced.",
  runtime_timeout: "The orchestrator took too long to answer and was stopped. The reply is incomplete or missing.",
  runtime_busy: "The orchestrator is already answering other conversations. Try again in a moment.",
  reply_too_large: "The reply was larger than the daemon allows and was dropped.",
  interrupted_by_restart: "The daemon restarted before the reply finished. The reply is incomplete or missing.",
  // The daemon refused the model choice from the picker (DPES-1105). Each reason has its own code.
  selection_agent_unavailable: "The chosen agent cannot run: it is not installed, cannot be launched, or its account has no access to it. Choose another agent. No reply was produced.",
  selection_authentication_failed: "The chosen agent is not signed in. Sign in to it (for GitHub Copilot, run copilot login), or choose another agent. No reply was produced.",
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

/** The agent and version a turn refused because they are not pinned (DPES-1106). */
export interface RefusedRuntime {
  name: string;
  version: string;
}

/** Names the refused agent and the pin that would accept it. `runtime` is what the daemon recorded for the turn. */
function unsupportedVersionText({ name, version }: RefusedRuntime): string {
  return `${agentDisplayName(name)} ${version} is not a supported version yet. Set AGENT_ORCH_CONVERSATION_PINS=${name}@${version} and restart the daemon to try it. No reply was produced.`;
}

export function failureText(code: string | null, runtime?: RefusedRuntime): string {
  if (code === null) return "The turn failed without a reason code. No reply was produced.";
  if (code === "version_mismatch" && runtime) return unsupportedVersionText(runtime);
  return FAILURE_TEXT[code] ?? `The turn failed (${code}). No reply was produced.`;
}

export function toWireTarget(target: ConversationTarget): ConversationWireTarget {
  if (target.kind === "project") return { kind: "project", project_id: target.projectId };
  return target.kind === "global" ? { kind: "global" } : { kind: "workstream", workstream_id: target.workstreamId };
}

/** Copies a routed-in row from the wire. The state stays the daemon's string, so an unknown one shows as unknown. */
export function mapRoutedIn(wire: ConversationRoutedInWire): RoutedIn {
  return { taskId: wire.task_id, title: wire.title, routedAt: wire.routed_at, state: wire.state };
}

/** Copies a coordinator's state from the wire (DPES-1108). Verbs and authors stay the daemon's strings. */
export function mapCoordinator(wire: ConversationCoordinatorWire): CoordinatorState {
  return {
    scopeSentence: wire.scope_sentence,
    openTasks: wire.open_tasks,
    openRequests: wire.open_requests,
    routedRequests: wire.routed_requests.map((request) => ({
      id: request.id,
      work: request.work,
      answered: request.answered,
      createdAt: request.created_at,
    })),
    reports: wire.reports.map((report) => ({
      id: report.id,
      author: report.author,
      verb: report.verb,
      requestId: report.request_id,
      taskId: report.task_id,
      key: report.key,
      note: report.note,
      createdAt: report.created_at,
    })),
  };
}

/** Copies a wake record from the wire. State, outcome and reason stay the daemon's strings, so an unknown one shows as unknown. */
export function mapWakeRecord(wire: WakeRecordWire): WakeRecord {
  return {
    id: wire.id,
    event: wire.event,
    subject: wire.subject,
    notice: wire.notice,
    state: wire.state,
    outcome: wire.outcome,
    skipReason: wire.skip_reason,
    turnId: wire.turn_id,
    anchorSequence: wire.anchor_sequence,
    notify: wire.notify,
    seen: wire.seen,
    createdAt: wire.created_at,
  };
}

export function turnInfo(turn: ConversationWireTurn): ConversationTurnInfo {
  return {
    id: turn.id,
    state: turn.state,
    errorCode: turn.error_code,
    runtime: turn.runtime,
    permissionLevel: turn.permission_level ?? null,
    // A daemon from before the verified field (DPES-1106) is not warned about.
    verified: turn.verified ?? true,
    actionBlock: turn.action_block ?? null,
    // An older daemon has no wake turns; an unknown word is not a wake turn either.
    ...(turn.trigger === "wake" ? { trigger: "wake" as const } : turn.trigger === "user" ? { trigger: "user" as const } : {}),
  };
}

/**
 * The action block of each turn in the window (DPES-1105). A turn's `turn_completed` entry carries it, so older turns
 * keep their notice; the latest turn's field covers a daemon that sent it only there.
 */
function actionBlocksOf(entries: readonly ConversationWireEntry[], latest: ConversationWireTurn | null): Record<string, ActionBlock> {
  const blocks: Record<string, ActionBlock> = {};
  for (const entry of entries) {
    if (entry.kind === "turn_completed" && entry.action_block) blocks[entry.turn_id] = entry.action_block;
  }
  if (latest?.action_block && !(latest.id in blocks)) blocks[latest.id] = latest.action_block;
  return blocks;
}

/** Exactly the shape the daemon builds. Anything else is dropped, never shown. */
const PULL_REQUEST_URL = /^https:\/\/github\.com\/[A-Za-z0-9._-]{1,100}\/[A-Za-z0-9._-]{1,100}\/pull\/([1-9][0-9]{0,5})$/;

/** The most links one turn shows; the daemon keeps at most this many. */
const MAX_PULL_REQUEST_LINKS = 5;

/**
 * The pull request links of a `turn_completed` entry (DPES-1108). Each URL must match the daemon's form and name the
 * entry's own number; any other entry, or an older daemon, gives none.
 */
export function pullRequestLinks(entry: ConversationWireEntry): PullRequestLink[] {
  if (entry.kind !== "turn_completed" || !Array.isArray(entry.pull_requests)) return [];
  const links: PullRequestLink[] = [];
  for (const wire of entry.pull_requests) {
    if (links.length === MAX_PULL_REQUEST_LINKS) break;
    const match = typeof wire?.url === "string" ? PULL_REQUEST_URL.exec(wire.url) : null;
    if (!match || Number(match[1]) !== wire.number || typeof wire.task_id !== "string") continue;
    links.push({ taskId: wire.task_id, number: wire.number, url: wire.url });
  }
  return links;
}

/**
 * Maps wire entries to view entries. Reply chunks of one turn become a single
 * reply that stays `streaming` until the daemon records the end of the turn;
 * its thinking chunks become one `thinking` entry before the reply, `streaming` until the reply starts or the turn ends;
 * a failed, canceled or empty turn becomes a `status` line, never a reply.
 * `withProposals` holds the turns that proposed actions (DPES-1105); one of
 * those with no reply says so instead.
 */
export function mapEntries(wire: readonly ConversationWireEntry[], withProposals?: ReadonlySet<string>): ConversationEntry[] {
  const out: ConversationEntry[] = [];
  const replies = new Map<string, ConversationEntry>();
  const thoughts = new Map<string, ConversationEntry>();
  const endThinking = (turnId: string) => {
    const thought = thoughts.get(turnId);
    if (thought) thought.streaming = false;
  };
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
        out.push({
          id: entry.id,
          kind: "message",
          role: "user",
          text: entry.body,
          at: entry.created_at,
          turnId: entry.turn_id,
          sequence: entry.sequence,
          ...(entry.origin === "wake" ? { origin: "wake" as const } : {}),
        });
        break;
      case "thinking_chunk": {
        const thought = thoughts.get(entry.turn_id);
        if (thought) {
          thought.text += entry.body;
          thought.sequence = entry.sequence;
        } else {
          const created: ConversationEntry = {
            id: `${entry.turn_id}:thinking`,
            kind: "thinking",
            role: "coordinator",
            text: entry.body,
            at: entry.created_at,
            turnId: entry.turn_id,
            sequence: entry.sequence,
            streaming: true,
          };
          thoughts.set(entry.turn_id, created);
          out.push(created);
        }
        break;
      }
      case "reply_chunk":
        endThinking(entry.turn_id);
        if (reply) {
          reply.text += entry.body;
          reply.sequence = entry.sequence;
        } else {
          const created: ConversationEntry = {
            id: `${entry.turn_id}:reply`,
            kind: "message",
            role: "coordinator",
            text: entry.body,
            at: entry.created_at,
            turnId: entry.turn_id,
            sequence: entry.sequence,
            streaming: true,
          };
          replies.set(entry.turn_id, created);
          out.push(created);
        }
        break;
      case "turn_completed": {
        endThinking(entry.turn_id);
        let ended: ConversationEntry;
        if (reply) {
          reply.streaming = false;
          ended = reply;
        } else {
          ended = status(
            entry,
            withProposals?.has(entry.turn_id)
              ? "The orchestrator proposed actions without a written reply."
              : "The orchestrator finished this turn without a reply.",
          );
          out.push(ended);
        }
        const links = pullRequestLinks(entry);
        if (links.length > 0) ended.pullRequests = links;
        break;
      }
      case "turn_failed":
        endThinking(entry.turn_id);
        if (reply) reply.streaming = false;
        out.push(status(entry, failureText(entry.error_code, entry.runtime)));
        break;
      case "turn_cancelled":
        endThinking(entry.turn_id);
        if (reply) reply.streaming = false;
        out.push(status(entry, "This turn was canceled."));
        break;
    }
  }
  return out;
}

/**
 * The informational line for a turn that ran a version the daemon has not verified (DPES-1106). It does not block a
 * send and needs no action. Shown once per agent and version per app session: `warned` is that session's memory and
 * is never stored.
 */
export function unverifiedWarning(turn: ConversationTurnInfo, warned: Set<string>): string | undefined {
  if (turn.verified) return undefined;
  const key = `${turn.runtime.name}@${turn.runtime.version}`;
  if (warned.has(key)) return undefined;
  warned.add(key);
  return `${agentDisplayName(turn.runtime.name)} ${turn.runtime.version} has not been verified with MCAO. Replies may fail.`;
}

export const sendNotice = (turn: ConversationTurnInfo): string =>
  turn.state === "failed"
    ? "Recorded by the daemon."
    : turn.state === "running"
      ? "Recorded by the daemon. Waiting for the orchestrator's reply."
      : "Recorded by the daemon.";

const describeNotFound = (target: ConversationTarget) => (error: unknown) =>
  target.kind !== "global" && asDaemonError(error).kind === "not-found"
    ? `The daemon's ${target.kind} lookup did not find this ${target.kind} (not-found).`
    : undefined;

function fixtureTarget(target: ConversationTarget): boolean {
  return target.kind !== "global" && isFixtureLikeId(target.kind === "project" ? target.projectId : target.workstreamId);
}

function refuseFixture(target: ConversationTarget): void {
  if (fixtureTarget(target)) {
    throw new DaemonError("invalid-params", "Fixture ids are never sent to the daemon.");
  }
}

// The daemon answers a send with conflict while the chosen agent CLI is still starting. Nothing is recorded,
// so the same key is sent again. The daemon answers conflict only within its 15 s retry window; after that it
// records a failed turn instead. These delays sum to 13.5 s, so the last attempt still lands inside that window.
const SEND_RETRY_DELAYS_MS = [500, 1000, 2000, 4000, 6000];

async function retryWhileStarting<T>(attempt: () => Promise<T>): Promise<T> {
  for (let retry = 0; ; retry++) {
    try {
      return await attempt();
    } catch (error) {
      if (!(error instanceof DaemonError) || error.kind !== "conflict") throw error;
      if (retry === SEND_RETRY_DELAYS_MS.length) throw new DaemonError("agent-starting", error.message, error.code);
      await new Promise((resolve) => setTimeout(resolve, SEND_RETRY_DELAYS_MS[retry]));
    }
  }
}

export function createLiveConversation(
  connection: ConnectionManager,
  provenance: Provenance,
  keys: IdempotencyTracker,
  now: () => number,
): Required<Pick<ConversationSource, "cancel" | "clear" | "resolve">> & {
  transcript: ConversationSource["transcript"];
  send: (target: ConversationTarget, text: string, selection?: ModelChoice, permission?: PermissionLevel) => Promise<SendReceipt>;
} {
  const scopeOf = (target: ConversationTarget) => JSON.stringify(toWireTarget(target));
  const requireSupported = (target: ConversationTarget) => {
    if (target.kind === "project" && !connection.getSnapshot().capabilities.includes("project_conversations")) {
      throw new DaemonError("capability-unavailable", PROJECT_CONVERSATIONS_REQUIRED);
    }
  };
  // The unverified versions already warned about in this app session. Kept in memory only, never stored.
  const warned = new Set<string>();

  return {
    async transcript(target) {
      if (fixtureTarget(target)) {
        return notOk(provenance, "unsupported", "Fixture ids have no daemon conversation.", observation(now()));
      }
      const readOnce = async () => {
          requireSupported(target);
          const api = connection.api();
          const wire = toWireTarget(target);
          let entries: ConversationWireEntry[] = [];
          let after: number | undefined;
          let epoch: string | undefined;
          let generation: number | undefined;
          let more = false;
          let turn: ConversationWireTurn | null = null;
          // Every page carries the scope's proposals; the last page read is the newest (DPES-1105).
          let proposals: ActionProposalWire[] | undefined;
          let proposalsMore: boolean | undefined;
          let wakeRecords: WakeRecordWire[] | undefined;
          let wakeMore: boolean | undefined;
          let routedIn: ConversationRoutedInWire[] | undefined;
          let coordinator: ConversationCoordinatorWire | undefined;
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
            proposals = result.proposals;
            proposalsMore = result.proposals_more;
            wakeRecords = result.wake_records;
            wakeMore = result.wake_records_more;
            routedIn = result.routed_in;
            coordinator = result.coordinator;
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
          const mapped = (proposals ?? []).map(mapProposal);
          return {
            target,
            entries: mapEntries(entries, new Set(mapped.map((proposal) => proposal.turnId))),
            repliesExpected: true,
            clearGeneration: generation ?? 0,
            truncated,
            turn: turn === null ? null : turnInfo(turn),
            proposals: mapped,
            proposalsMore: proposalsMore ?? false,
            actionBlocks: actionBlocksOf(entries, turn),
            ...(wakeRecords ? { wake: wakeRecords.map(mapWakeRecord) } : {}),
            wakeMore: wakeMore ?? false,
            ...(routedIn && routedIn.length > 0 ? { routedIn: routedIn.map(mapRoutedIn) } : {}),
            ...(coordinator ? { coordinator: mapCoordinator(coordinator) } : {}),
          };
      };
      return liveRead(provenance, now, () => restartOnHistoryChange(readOnce), describeNotFound(target));
    },

    async send(target, text, selection, permission) {
      refuseFixture(target);
      requireSupported(target);
      const scope = `send:${scopeOf(target)}`;
      // A retry with another choice is another message: it must not reuse the key of the first. A request
      // without a permission keeps the key it always had.
      const fingerprint = permission === undefined ? [text, selection ?? null] : [text, selection ?? null, permission];
      const key = keys.keyFor(scope, JSON.stringify(fingerprint));
      try {
        const result = await retryWhileStarting(() =>
          connection.api().conversationsSend(toWireTarget(target), text, key, selection, permission),
        );
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
        const warning = unverifiedWarning(turn, warned);
        return {
          simulated: false,
          provenance,
          // The warning joins the notice, so every view that shows a send's notice shows it.
          notice: warning ? `${sendNotice(turn)} ${warning}` : sendNotice(turn),
          at: entry.created_at,
          entryId: entry.id,
          turn,
          ...(warning ? { warning } : {}),
        };
      } catch (error) {
        keys.settle(scope, asDaemonError(error));
        throw error;
      }
    },

    async clear(target): Promise<ClearReceipt> {
      refuseFixture(target);
      requireSupported(target);
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

    resolve: createLiveResolve(connection, provenance, keys),

    async cancel(target, turnId) {
      refuseFixture(target);
      requireSupported(target);
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
