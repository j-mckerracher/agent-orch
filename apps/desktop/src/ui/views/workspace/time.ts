import { parseInstant } from "../../../sources/surfaces/instant";

/** Format a daemon instant in the device's local timezone and name that timezone. */
export function formatAt(at: string, timeZone = Intl.DateTimeFormat().resolvedOptions().timeZone): string {
  const ms = parseInstant(at);
  if (Number.isNaN(ms)) return "Unrecognized time";
  const date = new Date(ms);

  return new Intl.DateTimeFormat(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    timeZone,
    timeZoneName: "short",
  }).format(date);
}
