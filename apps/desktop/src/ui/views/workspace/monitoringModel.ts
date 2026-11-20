/** Pure parts of the monitoring chip, the update labels and the Global badge (DPES-1105 Phase 3). */
import type {
  ConversationEntry,
  ConversationTarget,
  LiveConversationTarget,
  MonitoringMode,
  MonitoringState,
  PausedTarget,
  WakeRecord,
} from "../../../sources/surfaces/types";
import { formatAt } from "./time";
import { parseInstant } from "../../../sources/surfaces/instant";

export const WAKE_LABEL = "Update, not a reply to you";
export const PAUSED_TEXT = "Monitoring paused: limit reached";
export const NEW_UPDATE_TEXT = "New update from the orchestrator";
export const WAKE_RUNNING_TEXT = "The orchestrator is checking an update. Sending cancels it.";
export const FIXTURE_MONITORING_TEXT = "Fixture data: nothing was saved";
export const SETTINGS_LINK_TEXT = "Checks and limits are on the Settings page";
export const CHANGE_LIMITS_TEXT = "Change limits";

export const MODE_OPTIONS: ReadonlyArray<{ mode: MonitoringMode; text: string }> = [
  { mode: "on", text: "On: tell me about updates" },
  { mode: "quiet", text: "Quiet: keep updates, do not notify me" },
  { mode: "off", text: "Off: notices only, no automatic replies" },
  {
    mode: "away",
    text: "Away: replies stay quiet, standing approvals keep working, and you get a summary when you return. It adds no permissions.",
  },
];

const isMode = (mode: string): mode is MonitoringMode => MODE_OPTIONS.some((option) => option.mode === mode);

/** The mode the menu marks as chosen, or null for a word this app does not know. */
export function knownMode(state: MonitoringState): MonitoringMode | null {
  return isMode(state.mode) ? state.mode : null;
}

/** `Monitoring: on`. A mode from a newer daemon reads as unknown, never as one of the three. */
export function chipLabel(state: MonitoringState): string {
  return `Monitoring: ${isMode(state.mode) ? state.mode : "unknown"}`;
}

const sameTarget = (a: LiveConversationTarget, b: ConversationTarget): boolean =>
  a.kind === "global" ? b.kind === "global" : b.kind === "workstream" && a.workstreamId === b.workstreamId;

/** The pause that covers `target`, or null. */
export function pausedFor(state: MonitoringState, target: ConversationTarget): PausedTarget | null {
  return state.paused.find((item) => sameTarget(item.target, target)) ?? null;
}

/** Unseen updates for `target`. */
export function unseenFor(state: MonitoringState | null, target: ConversationTarget): number {
  return state?.unseen.find((item) => sameTarget(item.target, target))?.count ?? 0;
}

/** The paused words, with the local time the pause is expected to end when it is known and still ahead. */
export function pausedText(paused: Pick<PausedTarget, "until">, now: number): string {
  const until = paused.until === null ? Number.NaN : parseInstant(paused.until);
  return Number.isNaN(until) || until <= now ? PAUSED_TEXT : `${PAUSED_TEXT}, resumes about ${formatAt(paused.until!)}`;
}

/** The badge on the Global option: `1 update`, `2 updates`. */
export function badgeLabel(count: number): string {
  return `${count} ${count === 1 ? "update" : "updates"}`;
}

/** The Global option's accessible name with its unseen count. */
export function targetNameWithBadge(label: string, count: number): string {
  return count > 0 ? `${label}, ${count} new ${count === 1 ? "update" : "updates"}` : label;
}

/** The single line a run of quiet records collapses to. */
export function quietLine(count: number): string {
  return `Checked ${count} ${count === 1 ? "update" : "updates"}. Nothing needs you.`;
}

/** What a skipped record says in words. An unknown reason is named as unknown. */
export function skipText(reason: string | null): string {
  switch (reason) {
    case "mode_off":
      return "Monitoring was off";
    case "limit_hourly":
    case "limit_daily":
    case "limit_tokens":
      return PAUSED_TEXT;
    case "agent_failing":
      return "Automatic updates are paused while the agent is failing";
    default:
      return "Not handled for a reason this app does not know";
  }
}

/** Heartbeat events. A quiet reply may stand for these, and for no other event. */
const HEARTBEAT_EVENTS: readonly string[] = ["work_check", "daemon_start"];

/**
 * Events whose subject is a task id. Their notice links to that task. `routed_request` and `coordinator_report`
 * (DPES-1108) are outcome events too, never folded as quiet, but their subject is a request or report id, so they
 * are not listed here.
 */
const TASK_EVENTS: readonly string[] = [
  "task_failed",
  "task_stalled",
  "delivery_waiting",
  "sign_in_needed",
  "scout_report_ready",
  "candidate_ready",
  "worker_needs_decision",
  "worker_blocked",
];

/**
 * True for a heartbeat record handled quietly: it shows one collapsed line and its turn's entries stay hidden.
 * An outcome event is never folded, whatever its reply says.
 */
export const isQuiet = (record: WakeRecord): boolean =>
  record.state === "handled" && record.outcome === "quiet" && HEARTBEAT_EVENTS.includes(record.event);

/** True for a record whose subject is a task, so its notice can link to that task. */
export const isTaskEvent = (record: WakeRecord): boolean => TASK_EVENTS.includes(record.event);

/** How many records count toward the badge and the live region. */
export const notifyingCount = (wake: readonly WakeRecord[]): number => wake.filter((record) => record.notify).length;

/** The turn ids of quiet records; their opening and reply entries are not shown. */
export function quietTurnIds(wake: readonly WakeRecord[]): Set<string> {
  return new Set(wake.flatMap((record) => (isQuiet(record) && record.turnId !== null ? [record.turnId] : [])));
}

export interface WakePlacement {
  /** Records whose anchor is older than every entry in the window. */
  before: WakeRecord[];
  /** Records to show after the entry with this id, oldest first. */
  after: Map<string, WakeRecord[]>;
}

/**
 * Puts each record after the last entry whose sequence is at most its anchor. An anchor beyond the window lands after
 * the last entry. Entries without a sequence (simulated sends) are skipped, so they never hold a record.
 */
export function placeWake(entries: readonly ConversationEntry[], wake: readonly WakeRecord[]): WakePlacement {
  const placement: WakePlacement = { before: [], after: new Map() };
  for (const record of wake) {
    let anchor: string | null = null;
    for (const entry of entries) {
      if (entry.sequence !== undefined && entry.sequence <= record.anchorSequence) anchor = entry.id;
    }
    if (anchor === null) placement.before.push(record);
    else placement.after.set(anchor, [...(placement.after.get(anchor) ?? []), record]);
  }
  return placement;
}
