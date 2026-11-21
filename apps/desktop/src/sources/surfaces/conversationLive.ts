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
 * - The log is persisted by the daemon, so a reconnect or a daemon restart needs no saved state.
 *   A read starts from the beginning, except while the latest turn runs: then it continues from
 *   the last cursor and appends, and starts over when a clear or an unknown cursor says the
 *   entries read so far are stale. A very long log shows its newest entries; the oldest are cut
 *   and the result says so.
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
  LiveActivityWire,
  WakeRecordWire,
} from "../../daemon/types";
import { RESOLVE_METHOD, createLiveResolve, mapProposal } from "./actionProposals";
import { addActivity, mapLive, type ToolSlot } from "./activityItems";
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
  ModeSelection,
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
    mode: turn.mode ?? null,
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
 * reply that stays `streaming` until the daemon records the end of the turn or other activity follows it. Its thinking
 * and tool rows become `activity` entries in the order they were recorded, each `streaming` until a reply starts or the
 * turn ends; a failed, canceled or empty turn becomes a `status` line, never a reply.
 * `withProposals` holds the turns that proposed actions (DPES-1105); one of
 * those with no reply says so instead.
 */
export function mapEntries(wire: readonly ConversationWireEntry[], withProposals?: ReadonlySet<string>): ConversationEntry[] {
  const out: ConversationEntry[] = [];
  const replies = new Map<string, ConversationEntry>();
  // The entry last added for each turn. A reply chunk joins the reply only when nothing else came after it.
  const lastOf = new Map<string, ConversationEntry>();
  // The activity run of each turn that no reply has followed yet.
  const runs = new Map<string, ConversationEntry>();
  const tools = new Map<string, Map<string, ToolSlot>>();
  const endActivity = (turnId: string) => {
    const run = runs.get(turnId);
    if (run) run.streaming = false;
    runs.delete(turnId);
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
      case "thinking_chunk":
      case "tool_started":
      case "tool_finished": {
        let turnTools = tools.get(entry.turn_id);
        if (!turnTools) tools.set(entry.turn_id, (turnTools = new Map()));
        const run = runs.get(entry.turn_id);
        if (entry.kind === "tool_finished") {
          addActivity([], turnTools, entry.sequence, entry.kind, entry.body);
          break;
        }
        const items = run?.items ?? [];
        if (!addActivity(items, turnTools, entry.sequence, entry.kind, entry.body)) break;
        if (run) {
          run.sequence = entry.sequence;
          break;
        }
        // The reply before this run is done: later reply text is another bubble, below the run.
        if (reply && lastOf.get(entry.turn_id) === reply) reply.streaming = false;
        const created: ConversationEntry = {
          id: `${entry.turn_id}:activity:${entry.sequence}`,
          kind: "activity",
          role: "coordinator",
          text: "",
          at: entry.created_at,
          turnId: entry.turn_id,
          sequence: entry.sequence,
          items,
          streaming: true,
        };
        runs.set(entry.turn_id, created);
        lastOf.set(entry.turn_id, created);
        out.push(created);
        break;
      }
      case "reply_chunk":
        endActivity(entry.turn_id);
        if (reply && lastOf.get(entry.turn_id) === reply) {
          reply.text += entry.body;
          reply.sequence = entry.sequence;
        } else {
          const created: ConversationEntry = {
            id: reply ? `${entry.turn_id}:reply:${entry.sequence}` : `${entry.turn_id}:reply`,
            kind: "message",
            role: "coordinator",
            text: entry.body,
            at: entry.created_at,
            turnId: entry.turn_id,
            sequence: entry.sequence,
            streaming: true,
          };
          replies.set(entry.turn_id, created);
          lastOf.set(entry.turn_id, created);
          out.push(created);
        }
        break;
      case "turn_completed": {
        endActivity(entry.turn_id);
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
        endActivity(entry.turn_id);
        if (reply) reply.streaming = false;
        out.push(status(entry, failureText(entry.error_code, entry.runtime)));
        break;
      case "turn_cancelled":
        endActivity(entry.turn_id);
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

/** What a later read of a running turn continues from: the wire entries read so far and the cursor after them. */
interface Resume {
  entries: ConversationWireEntry[];
  epoch: string;
  generation: number | undefined;
  cursor: number;
  truncated: boolean;
}

const sameJson = (a: unknown, b: unknown): boolean => {
  if (a === b) return true;
  if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) return false;
  const keys = Object.keys(a);
  return keys.length === Object.keys(b).length && keys.every((key) => sameJson((a as Record<string, unknown>)[key], (b as Record<string, unknown>)[key]));
};

/**
 * Gives each entry of `next` the identity of the same entry in `prev` when nothing changed in it, and each unchanged
 * item of a changed activity entry its old identity, so a poll that adds one line re-renders only that line.
 */
function reuseEntries(prev: readonly ConversationEntry[] | undefined, next: ConversationEntry[]): ConversationEntry[] {
  if (!prev) return next;
  const before = new Map(prev.map((entry) => [entry.id, entry]));
  return next.map((entry) => {
    const old = before.get(entry.id);
    if (!old) return entry;
    if (entry.items && old.items) entry.items = entry.items.map((item, i) => (old.items![i] && sameJson(old.items![i], item) ? old.items![i]! : item));
    return sameJson(old, entry) ? old : entry;
  });
}

export function createLiveConversation(
  connection: ConnectionManager,
  provenance: Provenance,
  keys: IdempotencyTracker,
  now: () => number,
): Required<Pick<ConversationSource, "cancel" | "clear" | "resolve">> & {
  transcript: ConversationSource["transcript"];
  send: (target: ConversationTarget, text: string, selection?: ModelChoice, permission?: PermissionLevel, mode?: ModeSelection) => Promise<SendReceipt>;
} {
  const scopeOf = (target: ConversationTarget) => JSON.stringify(toWireTarget(target));
  const requireSupported = (target: ConversationTarget) => {
    if (target.kind === "project" && !connection.getSnapshot().capabilities.includes("project_conversations")) {
      throw new DaemonError("capability-unavailable", PROJECT_CONVERSATIONS_REQUIRED);
    }
  };
  // The unverified versions already warned about in this app session. Kept in memory only, never stored.
  const warned = new Set<string>();
  // Per conversation, kept only while its latest turn runs, and the entries last shown (for `reuseEntries`).
  const resumes = new Map<string, Resume>();
  const shown = new Map<string, ConversationEntry[]>();

  return {
    async transcript(target) {
      if (fixtureTarget(target)) {
        return notOk(provenance, "unsupported", "Fixture ids have no daemon conversation.", observation(now()));
      }
      const scope = scopeOf(target);
      const readOnce = async (resume?: Resume) => {
          requireSupported(target);
          const api = connection.api();
          const wire = toWireTarget(target);
          // A copy, so two overlapping reads never append to the same array.
          let entries: ConversationWireEntry[] = resume ? resume.entries.slice() : [];
          let after: number | undefined = resume?.cursor;
          let cursor = resume?.cursor ?? 0;
          let epoch: string | undefined = resume?.epoch;
          let generation: number | undefined = resume?.generation;
          let more = false;
          let turn: ConversationWireTurn | null = null;
          let activity: LiveActivityWire | null | undefined;
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
            activity = result.activity;
            proposals = result.proposals;
            proposalsMore = result.proposals_more;
            wakeRecords = result.wake_records;
            wakeMore = result.wake_records_more;
            routedIn = result.routed_in;
            coordinator = result.coordinator;
            more = result.more;
            cursor = result.next_after_sequence;
            if (!more) break;
            after = result.next_after_sequence;
            const windowStart = result.latest_sequence - MAX_LIVE_SEQUENCE_SPAN;
            if (windowStart > after) {
              entries = [];
              after = jumpedTo = windowStart;
            }
          }
          // A jump discards entries already read, so it always means older entries are not shown.
          const truncated = (resume?.truncated ?? false) || more || jumpedTo !== null;
          if (truncated) entries = dropPartialLeadingTurn(entries);
          if (turn?.state === "running") {
            resumes.set(scope, { entries, epoch: epoch ?? "", generation, cursor, truncated });
          } else {
            resumes.delete(scope);
          }
          const mapped = (proposals ?? []).map(mapProposal);
          const mappedEntries = reuseEntries(shown.get(scope), mapEntries(entries, new Set(mapped.map((proposal) => proposal.turnId))));
          shown.set(scope, mappedEntries);
          return {
            target,
            entries: mappedEntries,
            repliesExpected: true,
            clearGeneration: generation ?? 0,
            truncated,
            turn: turn === null ? null : turnInfo(turn),
            ...(activity ? { activity: mapLive(activity, now()) } : {}),
            proposals: mapped,
            proposalsMore: proposalsMore ?? false,
            actionBlocks: actionBlocksOf(entries, turn),
            ...(wakeRecords ? { wake: wakeRecords.map(mapWakeRecord) } : {}),
            wakeMore: wakeMore ?? false,
            ...(routedIn && routedIn.length > 0 ? { routedIn: routedIn.map(mapRoutedIn) } : {}),
            ...(coordinator ? { coordinator: mapCoordinator(coordinator) } : {}),
          };
      };
      // While the latest turn runs, continue from the last cursor. A clear or a cursor the daemon no longer knows
      // means what was read is stale, so the read starts over from the beginning.
      const read = async () => {
        const resume = resumes.get(scope);
        if (resume) {
          try {
            return await readOnce(resume);
          } catch (error) {
            resumes.delete(scope);
            if (!(error instanceof HistoryChanged || (error instanceof DaemonError && error.kind === "replay-gap"))) throw error;
          }
        }
        return restartOnHistoryChange(() => readOnce());
      };
      return liveRead(provenance, now, read, describeNotFound(target));
    },

    async send(target, text, selection, permission, mode) {
      refuseFixture(target);
      requireSupported(target);
      const scope = `send:${scopeOf(target)}`;
      // A retry with another choice is another message: it must not reuse the key of the first. A request
      // without a permission or mode keeps the key it always had.
      const fingerprint = [
        text,
        selection ?? null,
        ...(permission === undefined ? [] : [permission]),
        ...(mode === undefined ? [] : [mode]),
      ];
      const key = keys.keyFor(scope, JSON.stringify(fingerprint));
      try {
        const result = await retryWhileStarting(() =>
          connection.api().conversationsSend(toWireTarget(target), text, key, selection, permission, mode),
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
