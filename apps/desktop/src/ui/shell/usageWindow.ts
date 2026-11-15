import { calendarDay } from "../../sources/surfaces/localDay";

/**
 * Heading for the usage popover, derived from the reporting window. "Today's" is claimed only when the
 * window starts on today's calendar day in the window's own timezone; any other window is labeled
 * plainly, and the popover body states its start.
 */
export function usageHeading(windowStart: string, timezone: string, nowMs: number): string {
  const start = new Date(windowStart);
  if (Number.isNaN(start.getTime())) return "AI usage";
  const startDay = calendarDay(start, timezone);
  return startDay !== null && startDay === calendarDay(new Date(nowMs), timezone) ? "Today’s AI usage" : "AI usage";
}
