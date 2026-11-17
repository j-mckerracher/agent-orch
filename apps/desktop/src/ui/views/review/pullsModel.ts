// Words for watched pull requests and the merge card (DPES-1105 Phase 4c), as plain functions. No React.
// Titles, branch names and check names come from the remote host. They are copied as text, capped, and never become
// markup, a link or an attribute other than a label read aloud.
import { presentError, type DaemonError } from "../../../daemon/errors";
import type { PullFacts, PullItem } from "../../../sources/surfaces/types";
import type { Tone } from "../../theme";

const CONTROL = /[\u0000-\u001f\u007f-\u009f\u2028\u2029]/g;

/** One line of at most `max` characters: control characters become spaces, runs of space collapse, the cut is on a character boundary. */
export function plainRemote(value: string, max = 120): string {
  const line = value.replace(CONTROL, " ").replace(/\s+/g, " ").trim();
  const chars = Array.from(line);
  return chars.length > max ? `${chars.slice(0, max).join("").trimEnd()}…` : line;
}

/** The first 12 characters of a commit, as the card and the button name show it. */
export function shortSha(sha: string): string {
  return Array.from(plainRemote(sha, 200)).slice(0, 12).join("");
}

export function prStateText(state: string): string {
  if (state === "open") return "Open";
  if (state === "merged") return "Merged";
  if (state === "closed") return "Closed, not merged";
  return `State: ${plainRemote(state, 30)}`;
}

export function prStateTone(state: string): Tone {
  if (state === "open") return "info";
  return state === "merged" ? "ok" : "neutral";
}

/** A check's outcome in words. The host's own word is read; an unknown one is said not to be known. */
export function checkResultText(result: string): string {
  switch (result.trim().toLowerCase()) {
    case "pass":
    case "passed":
    case "success":
    case "successful":
      return "Passed";
    case "fail":
    case "failed":
    case "failure":
    case "error":
    case "cancel":
    case "cancelled":
    case "timed_out":
      return "Failed";
    case "pending":
    case "queued":
    case "waiting":
    case "running":
    case "in_progress":
      return "Not finished";
    default:
      return "Result not known";
  }
}

export function checkLine(check: { name: string; result: string }): string {
  return `${plainRemote(check.name, 80)}: ${checkResultText(check.result)}`;
}

/** "Required checks 3 of 3 passed", with the failed and unfinished ones named when there are any. */
export function checksSummary(facts: Pick<PullFacts, "requiredTotal" | "requiredPassed" | "requiredFailed">): string {
  if (facts.requiredTotal === 0) return "No required checks were reported";
  const waiting = facts.requiredTotal - facts.requiredPassed - facts.requiredFailed;
  const parts = [`Required checks ${facts.requiredPassed} of ${facts.requiredTotal} passed`];
  if (facts.requiredFailed > 0) parts.push(`${facts.requiredFailed} failed`);
  if (waiting > 0) parts.push(`${waiting} not finished`);
  return parts.join(", ");
}

const plural = (count: number, unit: string) => `${count} ${unit}${count === 1 ? "" : "s"}`;

/** "40 seconds ago", the part after "Seen". */
export function agoText(secs: number): string {
  if (!Number.isFinite(secs) || secs < 0) return "at an unknown time";
  if (secs < 5) return "just now";
  if (secs < 60) return `${plural(Math.floor(secs), "second")} ago`;
  if (secs < 3600) return `${plural(Math.floor(secs / 60), "minute")} ago`;
  return `${plural(Math.floor(secs / 3600), "hour")} ago`;
}

/** "Seen 40 seconds ago". */
export function ageText(secs: number): string {
  return `Seen ${agoText(secs)}`;
}

export function mergeableText(mergeable: PullItem["mergeable"]): string {
  return `Mergeable: ${mergeable === "unknown" ? "not known" : mergeable}`;
}

/** Why a merge would be refused, by the daemon's refusal code, as a clause that follows "Not ready: ". */
const REFUSAL_WORDS: Readonly<Record<string, string>> = {
  checks_red: "a required check failed",
  checks_pending: "a required check has not finished",
  checks_missing: "no required check was reported, so the app does not merge",
  head_moved: "the pull request changed after you were shown it",
  not_open: "the pull request is no longer open",
  not_mergeable: "GitHub says it cannot be merged now",
  stale: "the app last looked at it too long ago",
  project_mode: "this project's mode is local-only",
  unconfirmed: "the app could not confirm that it merged",
};

export function refusalWords(code: string): string {
  return Object.prototype.hasOwnProperty.call(REFUSAL_WORDS, code) ? REFUSAL_WORDS[code] : `the app refused it (${plainRemote(code, 40)})`;
}

/** The refusal the counts alone give, in the order the daemon checks; null when every required check passed. */
function refusalFromCounts(facts: PullFacts): string | null {
  if (facts.requiredTotal === 0) return "checks_missing";
  if (facts.requiredFailed > 0) return "checks_red";
  return facts.requiredPassed < facts.requiredTotal ? "checks_pending" : null;
}

/**
 * The merge card's check line. The daemon's own refusal wins (a denial the card was stored with, then the refusal it
 * reports now); the counts are the fallback. Null when the card carries no facts to judge by, so the card shows
 * nothing it cannot back up.
 */
export function mergeVerdict(facts: PullFacts | undefined, denial: string | null): { ready: boolean; text: string } | null {
  if (facts === undefined && denial === null) return null;
  const code = denial ?? facts?.refusal ?? (facts === undefined ? null : refusalFromCounts(facts));
  return code === null ? { ready: true, text: "All required checks passed" } : { ready: false, text: `Not ready: ${refusalWords(code)}` };
}

export function mergeMethodText(method: string): string {
  if (method === "merge") return "Merge commit";
  if (method === "squash") return "Squash";
  return method === "rebase" ? "Rebase" : plainRemote(method, 30);
}

/** The line for a task's merge request on a list row; null when none was made or it is already shown as Merged. */
export function mergeStateText(merge: PullItem["merge"]): string | null {
  if (merge === null) return null;
  switch (merge.state) {
    case "claimed":
      return "Merge approved, not started yet";
    case "issued":
    case "unknown":
      return "Checking whether it merged";
    case "refused":
      return merge.refusal === null ? "Merge refused" : `Merge refused: ${refusalWords(merge.refusal)}`;
    default:
      return null;
  }
}

/** What the polite region says when a refresh failed. The daemon allows one fresh read per task every 10 seconds. */
export function pullsErrorText(error: DaemonError): string {
  switch (error.kind) {
    case "conflict":
      return "The pull request was read a moment ago. Wait about 10 seconds and try again.";
    case "not-found":
      return "That task has no watched pull request any more. Nothing was changed.";
    case "timeout":
    case "disconnected":
      return "The daemon did not answer. Try again in a moment.";
    default:
      return /rate/i.test(error.message)
        ? "The pull request was read a moment ago. Wait about 10 seconds and try again."
        : presentError(error).reason;
  }
}
