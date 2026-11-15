/**
 * Calendar-day helpers in a named IANA timezone, via `Intl` (no date library needed). Used by the
 * usage fixture (its window is local midnight of the current day) and by the usage popover heading.
 */

const PARTS: Intl.DateTimeFormatOptions = {
  year: "numeric",
  month: "2-digit",
  day: "2-digit",
  hour: "2-digit",
  minute: "2-digit",
  second: "2-digit",
  hourCycle: "h23",
};

/** Calendar day (YYYY-MM-DD) `instant` falls on in `timeZone`, or null when the runtime cannot resolve the zone. */
export function calendarDay(instant: Date, timeZone: string): string | null {
  try {
    return new Intl.DateTimeFormat("en-CA", { timeZone, year: "numeric", month: "2-digit", day: "2-digit" }).format(instant);
  } catch {
    return null;
  }
}

/** Wall-clock reading of `instant` in `timeZone`, expressed as milliseconds as if that reading were UTC. */
export function wallClockMs(instant: number, timeZone: string): number {
  const fields = new Intl.DateTimeFormat("en-US", { ...PARTS, timeZone }).formatToParts(new Date(instant));
  const value = (type: string) => Number(fields.find((part) => part.type === type)?.value);
  return Date.UTC(value("year"), value("month") - 1, value("day"), value("hour"), value("minute"), value("second"));
}

/**
 * Instant of local midnight at the start of the calendar day `nowMs` falls on in `timeZone`, or null
 * when the zone is unknown. Two passes settle the offset at DST transitions.
 */
export function zoneDayStart(nowMs: number, timeZone: string): Date | null {
  const day = calendarDay(new Date(nowMs), timeZone);
  if (day === null) return null;
  const [year, month, date] = day.split("-").map(Number);
  const midnightAsUtc = Date.UTC(year!, month! - 1, date!);
  let instant = midnightAsUtc;
  for (let pass = 0; pass < 2; pass++) {
    instant = midnightAsUtc - (wallClockMs(instant, timeZone) - instant);
  }
  return new Date(instant);
}
