/**
 * Pure parts of the coordinator panel (DPES-1108 phase 7): the scope sentence, the open work it owns and its parent
 * channel. Ids, verbs and counts are the daemon's; a coordinator note is agent text and is shown as plain text only.
 */
import { APPROVER_REQUIRED_NOTICE } from "../../../daemon/errors";
import type { CoordinatorReport, CoordinatorState, RoutedRequest } from "../../../sources/surfaces/types";
import { plainRemote } from "../review/pullsModel";
import { formatAt } from "./time";

export const COORDINATOR_HEADING = "Coordinator scope and work";
export const NO_SCOPE_TEXT = "No scope set";
export const REQUESTS_HEADING = "Open routed requests";
export const REPORTS_HEADING = "Latest reports";
export const COORDINATOR_MAX_ROWS = 3;
/** The daemon's limit, in UTF-8 bytes. */
export const MAX_SCOPE_BYTES = 300;
export const REMOVE_OPEN_WORK_TEXT =
  "This coordinator still owns open work. Finish or cancel its tasks and answer its routed requests first.";

/** Work and notes are cut to this many characters, so one row stays a few lines. */
const ROW_CHARS = 160;

/** The coordinator's state, or null when the daemon did not send one (an older daemon, or not a project target). */
export const stateFrom = (snapshot: { coordinator?: CoordinatorState } | null | undefined): CoordinatorState | null =>
  snapshot?.coordinator ?? null;

/** The newest few routed requests still waiting for an answer, in the daemon's order. */
export const openRequestsOf = (state: CoordinatorState): RoutedRequest[] =>
  state.routedRequests.filter((request) => !request.answered).slice(0, COORDINATOR_MAX_ROWS);

/** The newest few reports, in the daemon's order. */
export const reportsOf = (state: CoordinatorState): CoordinatorReport[] => state.reports.slice(0, COORDINATOR_MAX_ROWS);

/** `Open tasks: 1. Open routed requests: 2.` */
export const countsText = (state: CoordinatorState): string =>
  `Open tasks: ${state.openTasks}. Open routed requests: ${state.openRequests}.`;

/** `r-0123456789abcdef: Write the troubleshooting page`. */
export const requestLabel = (request: RoutedRequest): string => `${request.id}: ${plainRemote(request.work, ROW_CHARS)}`;

/** Who wrote a report: the app for a fact it recorded, the coordinator for its own judgment. */
export function authorText(author: string): string {
  if (author === "daemon") return "App";
  return author === "coordinator" ? "Coordinator" : "Unknown author";
}

const VERBS: Readonly<Record<string, string>> = {
  task_ready: "Task ready for review",
  report_ready: "Report ready",
  task_done: "Task done",
  task_failed: "Task failed",
  worker_blocked: "Worker blocked",
  question_raised: "Question raised",
  question_answered: "Question answered",
  merged: "Merged",
  answer: "Answer",
  caveat: "Caveat",
  blocked: "Blocked",
};

/** A report verb in plain words. An unknown verb is named as the daemon sent it. */
export function verbText(verb: string): string {
  return Object.prototype.hasOwnProperty.call(VERBS, verb) ? VERBS[verb] : `Unknown (${plainRemote(verb, 40)})`;
}

/** `Coordinator: Caveat, request r-01…, 10:04 PM EST`. The note is shown on its own, as plain text. */
export function reportLabel(report: CoordinatorReport): string {
  const ids = [
    report.requestId && `request ${report.requestId}`,
    report.taskId && `task ${report.taskId}`,
    report.key && `key ${plainRemote(report.key, 40)}`,
  ].filter(Boolean);
  return [`${authorText(report.author)}: ${verbText(report.verb)}`, ...ids, formatAt(report.createdAt)].join(", ");
}

/** A coordinator's note as one plain line; null when there is none. */
export const noteText = (report: CoordinatorReport): string | null =>
  report.note === null || report.note.trim() === "" ? null : plainRemote(report.note, ROW_CHARS);

// The daemon's `check_scope_sentence`: control, bidi and invisible characters (the zero-width joiners stay).
const HIDDEN = /[\u0000-\u001f\u007f-\u009f\u00ad\u061c\u200b\u200e\u200f\u2028\u2029\u202a-\u202e\u2060-\u2064\u2066-\u2069\ufeff\u{e0000}-\u{e007f}]/u;

/** The UTF-8 size the daemon counts. A JavaScript length counts UTF-16 units, which differs. */
export const scopeBytes = (text: string): number => new TextEncoder().encode(text.trim()).length;

/** Why a typed scope cannot be saved, or null. A blank scope is valid: it clears the sentence. The daemon decides last. */
export function scopeError(text: string): string | null {
  if (text.trim() === "") return null;
  if (/[\r\n\t]/.test(text)) return "The scope must be one line.";
  if (HIDDEN.test(text)) return "The scope has a hidden or control character. Remove it and type the text again.";
  const bytes = scopeBytes(text);
  return bytes > MAX_SCOPE_BYTES ? `The scope is too long: ${bytes} bytes, at most ${MAX_SCOPE_BYTES}.` : null;
}

/** What `coordinator.set_scope` is sent: the trimmed sentence, or null to clear it. */
export const scopeToSend = (text: string): string | null => (text.trim() === "" ? null : text.trim());

/** Why the scope cannot be changed now, or null. The method is user-only: a non-approver session is refused. */
export function scopeLockReason(state: { live: boolean; connected: boolean; supported: boolean; approver: boolean }): string | null {
  if (!state.live) return null;
  if (!state.connected) return "Offline: reconnect to the daemon to change the scope.";
  if (!state.supported) return "This daemon is older and cannot change the scope. Update the daemon to use this action.";
  return state.approver ? null : APPROVER_REQUIRED_NOTICE;
}
