/** Pure parts of the Settings page's "Checks on running work" and "Automatic replies" sections (DPES-1105 Phase 3). */
import { presentError, type DaemonError } from "../../../daemon/errors";
import type { SupervisionSettings, WakeLimitSettings } from "../../../sources/surfaces/types";

export const BASE_MINUTES_MIN = 1;
export const BASE_MINUTES_MAX = 240;
export const MIN_SECONDS_FLOOR = 10;
export const WALL_MINUTES_MIN = 5;
export const WALL_MINUTES_MAX = 1440;
export const HOURLY_CAP_MAX = 1000;
export const DAILY_CAP_MAX = 10000;

export const SUPERVISION_INTRO =
  "While agents work, the orchestrator checks for tasks that look stuck. The more work is running, the more often it checks, down to the fastest interval. These checks use no tokens. A task is flagged only after it has been silent for at least 5 minutes.";
export const WAKE_INTRO =
  "Automatic replies are updates the orchestrator writes without being asked. Each one uses tokens. When a limit is reached, you still see a notice for every event, and the header says Monitoring paused: limit reached.";
export const UNAVAILABLE_TEXT = "This daemon cannot change these settings.";
export const NO_LIMIT_NOTE = "No limit means a busy fleet can spend tokens on automatic replies without a cap.";
export const SUPERVISION_SAVED_TEXT = "Saved. New checks use these settings from the next check.";
export const WAKE_SAVED_TEXT = "Saved. New limits apply from the next automatic reply.";
export const FIXTURE_SAVE_TEXT = "Fixture data: nothing was saved.";
export const CROSS_FIELD_ERROR = "The fastest interval cannot be slower than the check interval.";
export const INVALID_REASON = "Fix the fields marked as invalid before saving.";
export const UNCHANGED_REASON = "Change a value to save.";

export type SupervisionField = "baseMinutes" | "minSeconds" | "wallMinutes";
export type WakeField = "hourlyCap" | "dailyCap" | "dailyTokenBudget";
export type SupervisionTyped = Partial<Record<SupervisionField, string>>;
export type WakeTyped = Partial<Record<WakeField, string>>;

export const SUPERVISION_LABEL: Record<SupervisionField, string> = {
  baseMinutes: "Check interval with one task running (minutes)",
  minSeconds: "Fastest check interval (seconds)",
  wallMinutes: "Longest time a task may run before it is flagged (minutes)",
};
export const WAKE_LABEL: Record<WakeField, string> = {
  hourlyCap: "Replies per hour, per conversation",
  dailyCap: "Replies per day, per conversation",
  dailyTokenBudget: "Daily token budget for automatic replies",
};
export const WAKE_HINT: Record<WakeField, string> = {
  hourlyCap: "Leave empty for no limit.",
  dailyCap: "Leave empty for no limit.",
  dailyTokenBudget: "Leave empty for no limit. Empty means no budget.",
};

/** The whole number in `text`, or null for empty or anything else. */
export function parseWhole(text: string): number | null {
  const trimmed = text.trim();
  return /^\d+$/.test(trimmed) ? Number(trimmed) : null;
}

const rangeError = (min: number, max: number) => `Enter a whole number from ${min} to ${max}.`;

function inRange(text: string, min: number, max: number): number | null {
  const value = parseWhole(text);
  return value !== null && value >= min && value <= max ? value : null;
}

/** What the page shows in the fields at first: the daemon's values, minutes for the base and the wall bound. */
export function supervisionText(saved: SupervisionSettings): Record<SupervisionField, string> {
  return {
    baseMinutes: String(saved.baseIntervalSecs / 60),
    minSeconds: String(saved.minIntervalSecs),
    wallMinutes: String(saved.attemptWallMinutes),
  };
}

/** The base in seconds as typed, else as saved: the figure the minimum is checked against. */
function effectiveBaseSecs(saved: SupervisionSettings, typed: SupervisionTyped): number {
  const text = typed.baseMinutes;
  const minutes = text === undefined ? null : inRange(text, BASE_MINUTES_MIN, BASE_MINUTES_MAX);
  return minutes === null ? saved.baseIntervalSecs : minutes * 60;
}

/** The message under each touched field, or null when it is valid or untouched. */
export function supervisionErrors(saved: SupervisionSettings, typed: SupervisionTyped): Record<SupervisionField, string | null> {
  const base = typed.baseMinutes;
  const min = typed.minSeconds;
  const wall = typed.wallMinutes;
  const baseSecs = effectiveBaseSecs(saved, typed);
  let minError: string | null = null;
  if (min !== undefined) {
    const value = parseWhole(min);
    if (value === null || value < MIN_SECONDS_FLOOR) minError = rangeError(MIN_SECONDS_FLOOR, baseSecs);
    else if (value > baseSecs) minError = CROSS_FIELD_ERROR;
  } else if (base !== undefined && saved.minIntervalSecs > baseSecs && inRange(base, BASE_MINUTES_MIN, BASE_MINUTES_MAX) !== null) {
    // Lowering the base under the saved minimum is refused unless the minimum is lowered in the same save.
    minError = CROSS_FIELD_ERROR;
  }
  return {
    baseMinutes: base === undefined || inRange(base, BASE_MINUTES_MIN, BASE_MINUTES_MAX) !== null ? null : rangeError(BASE_MINUTES_MIN, BASE_MINUTES_MAX),
    minSeconds: minError,
    wallMinutes: wall === undefined || inRange(wall, WALL_MINUTES_MIN, WALL_MINUTES_MAX) !== null ? null : rangeError(WALL_MINUTES_MIN, WALL_MINUTES_MAX),
  };
}

/** What Save sends: only valid fields that differ from the saved value, with the base in seconds. */
export function supervisionPatch(saved: SupervisionSettings, typed: SupervisionTyped): Partial<SupervisionSettings> {
  const patch: Partial<SupervisionSettings> = {};
  const baseMinutes = typed.baseMinutes === undefined ? null : inRange(typed.baseMinutes, BASE_MINUTES_MIN, BASE_MINUTES_MAX);
  if (baseMinutes !== null && baseMinutes * 60 !== saved.baseIntervalSecs) patch.baseIntervalSecs = baseMinutes * 60;
  const minSeconds = typed.minSeconds === undefined ? null : parseWhole(typed.minSeconds);
  if (minSeconds !== null && minSeconds !== saved.minIntervalSecs) patch.minIntervalSecs = minSeconds;
  const wall = typed.wallMinutes === undefined ? null : inRange(typed.wallMinutes, WALL_MINUTES_MIN, WALL_MINUTES_MAX);
  if (wall !== null && wall !== saved.attemptWallMinutes) patch.attemptWallMinutes = wall;
  return patch;
}

/** The check interval for 1 to 5 running tasks, in seconds. Mirrors the daemon's `max(minimum, base / 2^(W - 1))`. */
export function previewIntervals(baseSecs: number, minSecs: number): number[] {
  return [1, 2, 3, 4, 5].map((tasks) => Math.max(minSecs, baseSecs / 2 ** (tasks - 1)));
}

/** `10 min`, `2 min 30 s`, `45 s`. Whole seconds: a fraction is advisory only. */
export function durationText(secs: number): string {
  const total = Math.round(secs);
  const minutes = Math.floor(total / 60);
  const seconds = total % 60;
  if (minutes === 0) return `${seconds} s`;
  return seconds === 0 ? `${minutes} min` : `${minutes} min ${seconds} s`;
}

/** The advisory line under the two interval fields. The daemon stays the authority. */
export function previewText(baseSecs: number, minSecs: number): string {
  if (baseSecs === minSecs) return `Checks always run every ${durationText(baseSecs)}.`;
  const [one, two, three, four, five] = previewIntervals(baseSecs, minSecs).map(durationText);
  const head = `With 1 task: every ${one}. 2 tasks: ${two}. 3: ${three}. 4: ${four}.`;
  return five === durationText(minSecs)
    ? `${head} 5 or more: ${five}.`
    : `${head} 5: ${five}. More tasks: down to ${durationText(minSecs)}.`;
}

/** The numbers the preview uses: typed where valid, else saved; null when the pair cannot be valid. */
export function previewInputs(saved: SupervisionSettings, typed: SupervisionTyped): { baseSecs: number; minSecs: number } | null {
  const baseSecs = effectiveBaseSecs(saved, typed);
  const minValue = typed.minSeconds === undefined ? saved.minIntervalSecs : parseWhole(typed.minSeconds);
  if (minValue === null || minValue < MIN_SECONDS_FLOOR || minValue > baseSecs) return null;
  return { baseSecs, minSecs: minValue };
}

const OFF_WORDS = ["no limit", "off"];

/** What a wake field holds: a number, or null for "no limit" (empty, or the words "no limit" or "off"); undefined when invalid. */
export function parseCap(text: string, max: number | null): number | null | undefined {
  const trimmed = text.trim().toLowerCase();
  if (trimmed === "" || OFF_WORDS.includes(trimmed)) return null;
  const value = parseWhole(trimmed);
  return value !== null && value >= 1 && (max === null || value <= max) ? value : undefined;
}

const WAKE_MAX: Record<WakeField, number | null> = { hourlyCap: HOURLY_CAP_MAX, dailyCap: DAILY_CAP_MAX, dailyTokenBudget: null };

export function wakeError(field: WakeField, text: string): string | null {
  if (parseCap(text, WAKE_MAX[field]) !== undefined) return null;
  const max = WAKE_MAX[field];
  const range = max === null ? "above 0" : `from 1 to ${max}`;
  return `Enter a whole number ${range}, or leave it empty for no ${field === "dailyTokenBudget" ? "budget" : "limit"}.`;
}

/** What the page shows in the wake fields at first: the number, or empty for no limit. */
export function wakeText(saved: WakeLimitSettings): Record<WakeField, string> {
  return {
    hourlyCap: saved.hourlyCap === null ? "" : String(saved.hourlyCap),
    dailyCap: saved.dailyCap === null ? "" : String(saved.dailyCap),
    dailyTokenBudget: saved.dailyTokenBudget === null ? "" : String(saved.dailyTokenBudget),
  };
}

export const WAKE_FIELDS: readonly WakeField[] = ["hourlyCap", "dailyCap", "dailyTokenBudget"];

/**
 * What Save sends: each valid field that differs from the saved value. A cleared field is `null`, which the daemon reads
 * as "no limit", so empty on screen but 6 on the daemon is a change. An untouched field is not sent.
 */
export function wakePatch(saved: WakeLimitSettings, typed: WakeTyped): Partial<WakeLimitSettings> {
  const patch: Partial<WakeLimitSettings> = {};
  for (const field of WAKE_FIELDS) {
    const text = typed[field];
    if (text === undefined) continue;
    const value = parseCap(text, WAKE_MAX[field]);
    if (value !== undefined && value !== saved[field]) patch[field] = value;
  }
  return patch;
}

/** True when any wake field, as it stands on screen, has no limit. */
export function anyUnlimited(saved: WakeLimitSettings, typed: WakeTyped): boolean {
  const shown = { ...wakeText(saved), ...typed };
  return WAKE_FIELDS.some((field) => parseCap(shown[field], WAKE_MAX[field]) === null);
}

/** Plain words for a failed save of these two sections; every other failure keeps the app's shared wording. */
export function groupsFailureText(error: DaemonError): string {
  switch (error.kind) {
    case "invalid-params":
      return "The daemon refused these values.";
    case "forbidden":
      return "Only the app can change these settings.";
    case "timeout":
      return "The daemon did not answer. Nothing was changed.";
    default: {
      const { reason, nextStep } = presentError(error);
      return `${reason} ${nextStep}`;
    }
  }
}
