const RFC_3339 = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$/;
const SQLITE_UTC = /^(\d{4}-\d{2}-\d{2}) (\d{2}:\d{2}:\d{2})$/;

/**
 * Epoch milliseconds of a daemon instant, or `NaN`. Accepts RFC 3339 with `Z` or an offset, and
 * SQLite's zone-less `YYYY-MM-DD HH:MM:SS`, which is UTC by construction. Any other zone-less
 * text is refused: `Date` would read it as local time and shift it by the UTC offset.
 */
export function parseInstant(at: string): number {
  const legacy = SQLITE_UTC.exec(at);
  const text = legacy ? `${legacy[1]}T${legacy[2]}Z` : at;
  return RFC_3339.test(text) ? Date.parse(text) : Number.NaN;
}
