/** Pure parts of the routed-in strip (DPES-1105 Phase 6a). Titles and state words only: no path, token or session output. */
import type { RoutedIn } from "../../../sources/surfaces/types";
import { formatAt } from "./time";

export const ROUTED_IN_HEADING = "Routed here by the global orchestrator";
export const ROUTED_IN_HELP = "This list is built from approved proposals. Clearing the global conversation removes its rows.";
export const ROUTED_IN_MAX_ROWS = 3;

/** The rows to show: the newest few, in the daemon's order. An absent or empty field shows nothing. */
export const rowsFrom = (snapshot: { routedIn?: RoutedIn[] } | null | undefined): RoutedIn[] =>
  (snapshot?.routedIn ?? []).slice(0, ROUTED_IN_MAX_ROWS);

/** `t-12ab (Fix the login bug), queued, 10:04 PM EST`. The state is text, never only a colour. */
export const labelFor = (row: RoutedIn): string => `${row.taskId} (${row.title}), ${row.state}, ${formatAt(row.routedAt)}`;

/**
 * What to say when a row arrives. The first read (`prev` null) says nothing, so opening a workstream does not
 * announce its existing rows. Only the newest arrival is spoken; the rest stay visible.
 */
export function announcement(prev: RoutedIn[] | null, next: RoutedIn[]): string | null {
  if (prev === null) return null;
  const seen = new Set(prev.map((row) => row.taskId));
  const arrived = next.find((row) => !seen.has(row.taskId));
  return arrived ? `Routed here: ${labelFor(arrived)}` : null;
}
