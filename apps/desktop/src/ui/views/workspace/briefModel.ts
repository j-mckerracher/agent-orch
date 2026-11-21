/** Pure parts of the return brief panel (DPES-1105 Phase 5b). Every line is built from counts and state words; no agent text. */
import type { Brief, BriefGrant, BriefItem } from "../../../sources/surfaces/types";

export const BRIEF_HEADING = "While you were away";
export const BRIEF_ACK_TEXT = "Dismiss";
export const BRIEF_FIXTURE_TEXT = "Fixture data: nothing was saved";
export const BRIEF_ACKED_TEXT = "Return brief acknowledged";
export const BRIEF_READY_TEXT = "Return brief ready";
export const BRIEF_ACK_FAILED_TEXT = "The return brief could not be acknowledged.";
export const BRIEF_WAITING_HEADING = "Decisions waiting for you";
export const BRIEF_ATTENTION_HEADING = "Work that needs attention";
export const BRIEF_GRANTS_HEADING = "Standing approvals";

/** `1 task`, `2 tasks`. */
export const plural = (count: number, one: string, many = `${one}s`): string => `${count} ${count === 1 ? one : many}`;

const when = (ms: number): string =>
  new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(new Date(ms));

export const briefHeading = (brief: Brief): string => (brief.open ? `${BRIEF_HEADING}, so far` : BRIEF_HEADING);

/** The window as words. An open window has no end yet, so it says "so far". */
export const briefWindowText = (brief: Brief): string =>
  brief.open ? `Since ${when(brief.fromMs)}, so far` : `From ${when(brief.fromMs)} to ${when(brief.toMs)}`;

const tasksLine = (brief: Brief): string | null => {
  const c = brief.counts;
  const parts = [
    { n: c.tasksSucceeded, text: "finished" },
    { n: c.tasksFailed, text: "failed" },
    { n: c.tasksStalled, text: "stalled" },
    { n: c.tasksRunning, text: "still running" },
  ].filter((part) => part.n > 0);
  if (parts.length === 0) return null;
  return parts.map((part, at) => (at === 0 ? `${plural(part.n, "task")} ${part.text}` : `${part.n} ${part.text}`)).join(", ");
};

const grantLine = (brief: Brief): string | null => {
  const c = brief.counts;
  if (c.proposalsApprovedByGrant === 0 && c.executedByGrant === 0 && c.failedUnderGrant === 0) return null;
  const parts = [`${plural(c.proposalsApprovedByGrant, "action")} approved by standing approvals`, `${c.executedByGrant} completed`];
  if (c.failedUnderGrant > 0) parts.push(`${c.failedUnderGrant} failed`);
  return parts.join(", ");
};

/** The count lines in reading order. A zero count adds no line. A brief with nothing in it gets one line saying so. */
export function briefLines(brief: Brief): string[] {
  const c = brief.counts;
  const lines: Array<string | null> = [
    tasksLine(brief),
    grantLine(brief),
    c.proposalsApprovedByUser > 0 ? `${plural(c.proposalsApprovedByUser, "action")} approved by you` : null,
    c.proposalsWaiting > 0 ? `${plural(c.proposalsWaiting, "decision")} ${c.proposalsWaiting === 1 ? "is" : "are"} waiting for you` : null,
    c.deliveriesWaiting > 0
      ? `${plural(c.deliveriesWaiting, "delivery", "deliveries")} ${c.deliveriesWaiting === 1 ? "is" : "are"} waiting for approval`
      : null,
    c.grantsExpired > 0 ? `${plural(c.grantsExpired, "standing approval")} expired` : null,
    c.quietUpdates > 0 ? `${plural(c.quietUpdates, "automatic update")} ${c.quietUpdates === 1 ? "was" : "were"} quiet` : null,
    c.repliesPausedByLimit > 0
      ? `${plural(c.repliesPausedByLimit, "automatic reply", "automatic replies")} ${c.repliesPausedByLimit === 1 ? "was" : "were"} paused by a limit`
      : null,
  ];
  const present = lines.filter((line): line is string => line !== null);
  if (present.length > 0) return present;
  return [brief.open ? "Nothing has happened so far." : "Nothing happened while you were away."];
}

const KIND_TEXT: Readonly<Record<string, string>> = {
  proposal_waiting: "Decision waiting",
  delivery_waiting: "Delivery waiting",
  task_failed: "Task failed",
  task_stalled: "Task stalled",
  sign_in_needed: "Sign-in needed",
};

/** `Task failed: Fix the login form`. Only the kind label and the daemon's short title; the id and project code are not shown. */
export const itemText = (item: BriefItem): string => `${KIND_TEXT[item.kind] ?? "Item"}: ${item.title}`;

const GRANT_STATUS_TEXT: Readonly<Record<string, string>> = {
  active: "Active",
  expired: "Expired",
  revoked: "Revoked",
  exhausted: "Used up",
  budget_exhausted: "Out of budget",
};

export const grantStatusText = (grant: BriefGrant): string => GRANT_STATUS_TEXT[grant.status] ?? "Unknown status";

export const grantUsesText = (grant: BriefGrant): string =>
  `${plural(grant.usesInWindow, "use")} in this window, ${grant.usesUsed} of ${grant.maxUses} used in all`;

/** What the panel's polite region says once the user acknowledges. */
export const ackedText = (simulated: boolean): string => (simulated ? BRIEF_FIXTURE_TEXT : BRIEF_ACKED_TEXT);
