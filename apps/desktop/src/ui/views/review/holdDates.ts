// Date and failure text for held decisions (DPES-1105 Phase 4d). A hold date is a plain `YYYY-MM-DD` day with no time
// zone, so it is read from its text and never through `Date`, which would shift it by the machine's offset.
import { APPROVER_REQUIRED_NOTICE, presentError, type DaemonError } from "../../../daemon/errors";

/** The longest hold the daemon accepts, in days from today. The daemon checks it with its own clock. */
export const MAX_HOLD_DAYS = 60;

const MONTHS = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
];

const DAY = /^(\d{4})-(\d{2})-(\d{2})$/;

/** "21 November" or "21 November 2026". A value that is not a day is returned as it came. */
export function formatHoldDay(day: string, withYear: boolean): string {
  const match = DAY.exec(day);
  const month = match === null ? undefined : MONTHS[Number(match[2]) - 1];
  if (match === null || month === undefined) return day;
  const text = `${Number(match[3])} ${month}`;
  return withYear ? `${text} ${match[1]}` : text;
}

function localDay(date: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

/**
 * The first and last day the date input offers, from the local date. They only narrow the picker: the daemon decides
 * what is allowed, with its own clock.
 */
export function holdRange(now: Date): { min: string; max: string } {
  const last = new Date(now.getFullYear(), now.getMonth(), now.getDate() + MAX_HOLD_DAYS);
  return { min: localDay(now), max: localDay(last) };
}

/** "Held until 21 November": the text of a held row. */
export function heldUntilText(day: string): string {
  return `Held until ${formatHoldDay(day, false)}`;
}

/** Plain words for a hold or release the daemon refused, by error kind. Each says nothing was changed. */
export function holdErrorText(error: DaemonError, operation: "hold" | "release"): string {
  switch (error.kind) {
    case "invalid-params":
      return "The daemon did not accept that date. Pick a day from today to 60 days from today. Nothing was changed.";
    case "not-found":
      return operation === "hold"
        ? "There is nothing left to hold: the decision may already be resolved. Nothing was changed."
        : "That hold has already ended. Nothing was changed.";
    case "conflict":
      return "The decision changed while you were working on it. Nothing was changed.";
    case "forbidden":
      return APPROVER_REQUIRED_NOTICE;
    case "timeout":
    case "disconnected":
      return "The daemon did not answer. Your change may have been recorded. Trying again reuses the same request.";
    default:
      return presentError(error).reason;
  }
}
