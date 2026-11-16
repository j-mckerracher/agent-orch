/** Format a stored instant in the device's local timezone and name that timezone. */
export function formatAt(at: string, timeZone = Intl.DateTimeFormat().resolvedOptions().timeZone): string {
  const date = new Date(at);
  if (Number.isNaN(date.getTime())) return "Unrecognized time";

  return new Intl.DateTimeFormat(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    timeZone,
    timeZoneName: "short",
  }).format(date);
}
